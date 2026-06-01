//! Auto-action 三條規則引擎：
//!   1. quota_low       — 任一 runner 剩餘 < threshold% → 推 embed（只報不動）
//!   2. session_idle    — session 閒置超過 trigger_secs → 推確認訊息 + reaction polling，
//!      ✅=kill / ❌=取消 / timeout=取消
//!   3. hook_failure_burst — 同 provider 窗口內 ≥ count 次失敗 → 推 embed + openab_restart_command
//!
//! 每 10 秒跑一次 tick。去重用 dedup_key → last_fired_at hashmap。

use crate::config::AutoActionsConfig;
use crate::discord;
use crate::session::SessionManager;
use chrono::Timelike;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 一則 pending confirmation（session_idle 發出後等用戶回 ✅/❌）。
#[derive(Clone)]
pub struct PendingConfirm {
    pub kind: &'static str, // "session_idle" for now
    pub session_id: String,
    pub message_id: String,
    pub posted_at: Instant,
}

#[derive(Default)]
pub struct AutoRuleState {
    /// dedup_key → 最後一次觸發的 epoch secs。key 格式：`{rule}:{provider}:{extra}`
    pub last_fired: HashMap<String, i64>,
    pub pending_confirms: Vec<PendingConfirm>,
    /// 最後一次推 daily summary 的日期 "YYYY-MM-DD"（本地時區）——避免一天推多次
    pub last_summary_date: String,
    /// 最後一次推週摘要的 ISO 週碼 "YYYY-W##"——避免一週多次
    pub last_weekly_key: String,
    /// 最後一次處理過的 command message id（避免重複執行）
    pub last_cmd_msg_id: String,
    /// session_idle 規則專用：以「通知當下 session.last_event_time」為錨點，
    /// 同一個 idle 週期內（同樣的 last_event_time）只 fire 一次。
    /// 當 session 再次 active（last_event_time 推進），會自動失配 → 允許下輪 idle 再 fire。
    /// 純 toast 模式（無 Discord 成功路徑）下，這是避免永久 spam 的唯一手段。
    pub last_session_idle_event_ts: HashMap<String, i64>,
}

pub type SharedAutoState = Arc<Mutex<AutoRuleState>>;
type ProviderBurstCounts = Vec<(String, u32)>;
type LastFailureByProvider = HashMap<String, (String, Option<String>)>;

pub struct NotifyChannels<'a> {
    pub discord_token: &'a str,
    pub discord_channel: &'a str,
    pub app: Option<tauri::AppHandle>,
    pub toast_enabled: bool,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn prefix_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = 0usize;
    for (idx, ch) in s.char_indices() {
        let next = idx + ch.len_utf8();
        if next > max_bytes {
            break;
        }
        end = next;
    }
    s[..end].to_string()
}

/// 統一格式化 Discord 呼叫失敗的 log 字串。
/// 修前 14 處 `let _ = discord::xxx(...)` 直接吞 error，R2 修了 curl `-f` 讓 4xx/5xx 不再 silent，
/// 但呼叫端仍 swallow → log 系統看不到。本 helper 集中格式，便於 log filter / unit test 鎖定 prefix。
pub(crate) fn discord_err_msg(ctx: &str, err: &str) -> String {
    format!("[auto_rules] discord {ctx} failed: {err}")
}

/// 統一格式化 `!lp pause` / `!lp resume` save_config 失敗的 log 字串。
/// 修前 2 處 `let _ = crate::config::save_config(&c)` 直接吞 error，使用者主動改設定時
/// （磁碟滿 / 權限拒絕 / path 鎖住）UI 顯示「成功」但下次啟動 revert，operator 無 log 可查。
/// 對齊 R6 `discord_err_msg`：集中 prefix + ctx，便於 log filter / unit test 鎖定。
pub(crate) fn config_persist_warn_msg(action: &str, err: &str) -> String {
    format!(
        "[auto_rules] !lp {action}: save_config failed: {err} — \
         setting will revert on next launch"
    )
}

/// 統一格式化 `auto_state.json` 載入失敗的 log 字串。
/// 修前 4 處 `serde_json::from_str(...).unwrap_or_default()` 跟
/// `read_to_string(...).ok().and_then(from_str).ok().unwrap_or_default()` 鏈 silent
/// 吞 error——auto_state.json 損壞（磁碟寫入半截 / 手動編輯壞 JSON / 編碼錯）會回
/// default，operator 看到 dedup state 漂移無從查起。對齊 R6 `discord_err_msg` /
/// R23 `config_persist_warn_msg` prefix 風格，log filter 可一條 query 抓出所有
/// 「持久化 markers 載入失敗」事件。
pub(crate) fn persisted_marker_warn_msg(err: &str) -> String {
    format!("[auto_rules] auto_state markers load failed: {err}")
}

/// 載入 `auto_state.json` 為 `PersistedSummaryMarkers`。三條路徑：
/// 1. 檔案不存在 → `default()`（first-run 預期，靜默）
/// 2. 讀取其他失敗（權限 / IO）→ `log::warn!` + `default()`
/// 3. JSON 解析失敗（磁碟寫入半截 / 手動編輯壞 JSON）→ `log::warn!` + `default()`，
///    帶 80 字元 preview 方便排查
///
/// 沿用 R6/R23 「caller 端 log 統一 prefix」pattern：把 fs / serde 兩條失敗路徑收斂到
/// 同一個 helper，避免 read-modify-write 跟初始 load 各寫各的 log 風格。
fn parse_persisted_markers_at(path: &Path) -> PersistedSummaryMarkers {
    let data = match std::fs::read_to_string(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return PersistedSummaryMarkers::default();
        }
        Err(e) => {
            log::warn!("{}", persisted_marker_warn_msg(&e.to_string()));
            return PersistedSummaryMarkers::default();
        }
    };
    match serde_json::from_str(&data) {
        Ok(p) => p,
        Err(e) => {
            let preview = prefix_chars(&data, 80);
            log::warn!(
                "{}",
                persisted_marker_warn_msg(&format!("invalid JSON ({e}); preview={preview:?}")),
            );
            PersistedSummaryMarkers::default()
        }
    }
}

// ─── Rule 3 hook_failure_burst 誤報過濾 ───
// 修前 5 條 hardcoded 子字串直接 inline 在 tick_inner，命中即 silent continue：
// 真實 hook 失敗若撞到這 5 個子字串會被一起吃掉，operator 沒 log 可查「為什麼某次
// PostToolUseFailure 沒被算進去」。抽成具名 const + helper，命中時 log::debug! 帶
// pattern name + provider，與 R6 Discord 14 sites / R8 hook_server 2 sites 同 surface pattern。
//
// 命名原則：name 描述「這是什麼誤報類型」，不是「為什麼觸發」（reason 在 commit log 留）。
//
// 順序重要：sub-match 是 `.find()` first-hit，specific pattern 必須排在一般 catch-all
// 之前。原 5 條 inline 用 `.contains() ||` 順序相同 → 「ripgrep」會先吃掉「取代 find/grep」
// 組合案例（latent 行為瑕疵）。本版先排 specific 再排 general，filter 行為不變（命中即丟）、
// 僅讓 log pattern name 更精準。
const HOOK_FAILURE_FALSE_POSITIVE_PATTERNS: &[(&str, &str)] = &[
    ("shell-enforce-preflight-hint", "請用"),
    ("ripgrep-replaces-find", "取代 find"),
    ("ripgrep-replaces-grep", "取代 grep"),
    ("ripgrep-enoent", "ripgrep"),
    ("rg-exe-enoent", "rg.exe"),
];

/// 若 `err` 內含已知誤報子字串，回 Some(pattern_name)，否則 None。
/// 名稱回傳值方便 log 端 grep 對應是哪條規則被觸發。
fn hook_failure_false_positive(err: &str) -> Option<&'static str> {
    HOOK_FAILURE_FALSE_POSITIVE_PATTERNS
        .iter()
        .find(|(_, needle)| err.contains(needle))
        .map(|(name, _)| *name)
}

fn dedup_gate(state: &mut AutoRuleState, key: &str, dedup_secs: i64) -> bool {
    let now = now_secs();
    if let Some(&last) = state.last_fired.get(key) {
        if now - last < dedup_secs {
            return false;
        }
    }
    state.last_fired.insert(key.to_string(), now);
    true
}

/// session_idle 規則專用：判斷「此 session 在當前 idle 週期是否已被通知過」。
///
/// 以 `session.last_event_time` epoch_secs 作為錨點：
/// - 同一個 idle 週期（session 沒有新事件），last_event_time 不變 → 失配已標記者 → 跳過
/// - session 再次 active（last_event_time 推進）→ 錨點變動 → 允許新一輪 fire
///
/// 為避免 HashMap 無限增長（session 被移除但紀錄沒清），map 超過 `max_entries` 時
/// 自動丟棄 64 筆最舊的（last_event_time 最小 = 最久沒被參照）。
fn should_notify_session_idle(
    state: &mut AutoRuleState,
    session_id: &str,
    last_event_ts: i64,
    max_entries: usize,
) -> bool {
    // Lazy GC: 防止 session 被外部移除後殘留 entry 撐大 map
    if state.last_session_idle_event_ts.len() > max_entries {
        let mut entries: Vec<(String, i64)> = state
            .last_session_idle_event_ts
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by_key(|(_, v)| *v); // 舊的（最小 ts）在前
        let to_drop: HashSet<String> = entries.into_iter().take(64).map(|(k, _)| k).collect();
        state
            .last_session_idle_event_ts
            .retain(|k, _| !to_drop.contains(k));
    }
    if let Some(&prev) = state.last_session_idle_event_ts.get(session_id) {
        if prev == last_event_ts {
            return false; // 同一個 idle 週期已通知過
        }
    }
    state
        .last_session_idle_event_ts
        .insert(session_id.to_string(), last_event_ts);
    // 持久化錨點——重啟後 `load_persisted_session_idle_markers` 會讀回，避免重啟
    // 重新進入的 idle 週期又被通知一次。best-effort：寫入失敗不擋 fire，log
    // 不 panic，當下 session 仍由 in-memory 擋重複。對齊 R5 summary marker pattern。
    if let Err(e) = persist_session_idle_markers(&state.last_session_idle_event_ts) {
        log::error!("[auto_rules] persist session_idle markers: {e}");
    }
    true
}

/// 讀 ~/.lobsterpulse/usage-local.json 挑出 < threshold% 的 runner。
/// 解析 text 內 `**XX%**` pattern（template 樣式）。找不到百分比就當 100（不觸發）。
fn scan_quota_low(threshold: u8) -> Vec<(String, u8, String)> {
    let Some(home) = dirs::home_dir() else {
        return vec![];
    };
    let path = home.join(".lobsterpulse").join("usage-local.json");
    let Ok(data) = std::fs::read_to_string(&path) else {
        return vec![];
    };
    let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(&data) else {
        return vec![];
    };
    let Some(runners) = v.get("runners").and_then(|r| r.as_array()) else {
        return vec![];
    };
    let mut out = Vec::new();
    for r in runners {
        let name = r
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string();
        let label = r
            .get("label")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let text = r.get("text").and_then(|x| x.as_str()).unwrap_or("");
        let ok = r.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        if !ok {
            continue;
        }
        if let Some(pct) = extract_min_percent(text) {
            if pct < threshold {
                out.push((name, pct, label));
            }
        }
    }
    out
}

/// 從 text 裡挑出所有「N%」或「N.M%」數字，回傳最小值（最緊迫的那條）。忽略 "N/A%"。
fn extract_min_percent(text: &str) -> Option<u8> {
    let mut min: Option<u8> = None;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i > 0 {
            let mut j = i;
            while j > 0 {
                let c = bytes[j - 1];
                if c.is_ascii_digit() || c == b'.' {
                    j -= 1;
                } else {
                    break;
                }
            }
            if j < i {
                if j > 0 && bytes[j - 1] == b'-' {
                    i += 1;
                    continue;
                }
                if let Ok(f) = std::str::from_utf8(&bytes[j..i])
                    .unwrap_or("0")
                    .parse::<f64>()
                {
                    if f.is_finite() && (0.0..=100.0).contains(&f) {
                        let n = f.round() as u8;
                        min = Some(min.map(|m| m.min(n)).unwrap_or(n));
                    }
                }
            }
        }
        i += 1;
    }
    min
}

/// 對外給 lib.rs 呼叫的名稱。新版支援 Discord + Windows Toast 雙通道，任一啟用即跑 rules。
pub fn tick_with_state(
    cfg: &AutoActionsConfig,
    mgr: &Mutex<SessionManager>,
    state: &SharedAutoState,
    openab_restart_command: &str,
    notify: NotifyChannels<'_>,
) {
    let discord_on =
        !notify.discord_token.trim().is_empty() && !notify.discord_channel.trim().is_empty();
    let toast_on = notify.toast_enabled && notify.app.is_some();
    // 任一通道可用就繼續；兩者都關就跳過
    if !discord_on && !toast_on {
        return;
    }
    // 主開關 — 所有 rule 停跑
    if !cfg.master_enabled {
        return;
    }
    tick_inner(cfg, mgr, state, openab_restart_command, notify);
}

/// 發 Windows toast（走 tauri-plugin-notification，CSP/style 無限制）
pub fn send_toast(app: &tauri::AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("toast send failed: {e}");
    }
}

fn tick_inner(
    cfg: &AutoActionsConfig,
    mgr: &Mutex<SessionManager>,
    state: &SharedAutoState,
    openab_restart_command: &str,
    notify: NotifyChannels<'_>,
) {
    let toast_active = notify.toast_enabled && notify.app.is_some();
    let discord_active =
        !notify.discord_token.trim().is_empty() && !notify.discord_channel.trim().is_empty();
    // ─── Rule 1: quota_low ───
    if cfg.quota_low_enabled {
        for (name, pct, label) in scan_quota_low(cfg.quota_low_threshold_pct) {
            let key = format!("quota_low:{name}");
            // quota_low 用 24h 窗口：首次跨越門檻後一整天不再重噴（降噪，避免每 5min 刷屏）
            let fire = {
                let mut s = state.lock().unwrap();
                dedup_gate(&mut s, &key, 86400)
            };
            if fire {
                let title = format!("{label} · 額度警告");
                let desc = format!(
                    "`{name}` 剩餘 **{pct}%** （門檻 {}%）",
                    cfg.quota_low_threshold_pct
                );
                if discord_active {
                    if let Err(e) = discord::send_embed(
                        notify.discord_token,
                        notify.discord_channel,
                        &title,
                        &desc,
                        0xFFA500,
                    ) {
                        log::warn!(
                            "{}",
                            discord_err_msg(&format!("quota_low name={name} pct={pct}"), &e)
                        );
                    }
                }
                if toast_active {
                    if let Some(a) = &notify.app {
                        send_toast(a, &title, &format!("{name} 剩 {pct}%"));
                    }
                }
            }
        }
    }

    // ─── Rule 2: session_idle ─── 發起確認 + polling
    if cfg.session_idle_enabled {
        // 掃描閒置超標的 session
        let idle_sessions: Vec<(String, String, i64)> = {
            let m = mgr.lock().unwrap();
            let now = chrono::Utc::now();
            m.sessions
                .values()
                .map(|s| {
                    let idle = (now - s.last_event_time).num_seconds();
                    (s.id.clone(), s.provider.clone(), idle)
                })
                .filter(|(_, _, idle)| *idle >= cfg.session_idle_trigger_secs)
                .collect()
        };
        for (sid, provider, idle_secs) in idle_sessions {
            let sid_short8 = prefix_chars(&sid, 8);
            let already_pending = {
                let s = state.lock().unwrap();
                s.pending_confirms.iter().any(|p| p.session_id == sid)
            };
            if already_pending {
                continue;
            }
            // 用 session.last_event_time 作為「idle 週期」錨點：
            // 純 toast 模式下若繼續用 5min dedup 窗，會在 30min 觸發後每 5min 重發
            // 直到 session 結束（永久 spam）。改用「同 last_event_time 只 fire 一次」後，
            // session 再次 active 之前不會再發；session 變 active 後允許下輪 idle 再 fire。
            let last_event_ts = {
                let m = mgr.lock().unwrap();
                m.sessions
                    .get(&sid)
                    .map(|s| s.last_event_time.timestamp())
                    .unwrap_or(0)
            };
            let fire = {
                let mut s = state.lock().unwrap();
                should_notify_session_idle(&mut s, &sid, last_event_ts, 256)
            };
            if !fire {
                continue;
            }

            // 從 session 撈 project / last_tool / last_prompt 30 字做美化
            let (project, last_tool, last_prompt) = {
                let m = mgr.lock().unwrap();
                m.sessions
                    .get(&sid)
                    .map(|s| {
                        let p = s.project_name();
                        let t = s.last_tool_name.clone().unwrap_or_else(|| "-".into());
                        let pr = s
                            .last_prompt
                            .clone()
                            .map(|x| {
                                let take30: String = x.chars().take(30).collect();
                                if x.chars().count() > 30 {
                                    format!("{take30}…")
                                } else {
                                    take30
                                }
                            })
                            .unwrap_or_else(|| "(無)".into());
                        (p, t, pr)
                    })
                    .unwrap_or_else(|| ("unknown".into(), "-".into(), "-".into()))
            };

            let content = format!(
                "**{provider}** 閒置 {} 分鐘 — 要 kill 嗎？\n\
                \u{2003}專案：`{project}`\n\
                \u{2003}最後工具：`{last_tool}`\n\
                \u{2003}最後 prompt：*{last_prompt}*\n\
                \u{2003}session：`{}`\n\n\
                ✅ kill　❌ 保留　（{} 分鐘無反應自動取消）",
                idle_secs / 60,
                sid_short8,
                cfg.confirm_timeout_secs / 60,
            );
            if toast_active {
                if let Some(a) = &notify.app {
                    send_toast(
                        a,
                        &format!("🟡 {provider} 閒置"),
                        &format!("{} 分鐘未動作（{project}）", idle_secs / 60),
                    );
                }
            }
            if discord_active {
                match discord::send_message(notify.discord_token, notify.discord_channel, &content)
                {
                    Ok(mid) => {
                        if let Err(e) = discord::add_reaction(
                            notify.discord_token,
                            notify.discord_channel,
                            &mid,
                            "✅",
                        ) {
                            log::warn!(
                                "{}",
                                discord_err_msg(
                                    &format!(
                                        "session_idle sid={} reaction=✅",
                                        prefix_chars(&sid, 8)
                                    ),
                                    &e
                                )
                            );
                        }
                        if let Err(e) = discord::add_reaction(
                            notify.discord_token,
                            notify.discord_channel,
                            &mid,
                            "❌",
                        ) {
                            log::warn!(
                                "{}",
                                discord_err_msg(
                                    &format!(
                                        "session_idle sid={} reaction=❌",
                                        prefix_chars(&sid, 8)
                                    ),
                                    &e
                                )
                            );
                        }
                        state.lock().unwrap().pending_confirms.push(PendingConfirm {
                            kind: "session_idle",
                            session_id: sid.clone(),
                            message_id: mid,
                            posted_at: Instant::now(),
                        });
                    }
                    Err(e) => {
                        // R25 surface：原 `if let Ok(mid) = ... { ... }` 在 Discord send 失敗時
                        // 整段（含 `pending_confirms.push`）靜默跳過——operator 看不到「user 點
                        // ❌/✅ 但 Discord 沒收到」+ state 沒推進會卡住後續 click 解析。
                        log::warn!(
                            "{}",
                            discord_err_msg(
                                &format!(
                                    "session_idle sid={} confirm send_message",
                                    prefix_chars(&sid, 8)
                                ),
                                &e
                            )
                        );
                    }
                }
            }
        }
    }

    // ─── Pending confirms 輪詢（獨立於 session_idle 開關，讓 !lp kill 也共用）───
    {
        let to_process: Vec<PendingConfirm> = state.lock().unwrap().pending_confirms.clone();
        let mut resolved_ids: Vec<String> = Vec::new();
        for p in &to_process {
            let age = p.posted_at.elapsed();
            let sid_short = prefix_chars(&p.session_id, 12);
            if age > Duration::from_secs(cfg.confirm_timeout_secs as u64) {
                if discord_active {
                    if let Err(e) = discord::send_message(
                        notify.discord_token,
                        notify.discord_channel,
                        &format!(
                            "⏲️ {} 分鐘無人回應，取消 `{}` 的 kill",
                            cfg.confirm_timeout_secs / 60,
                            sid_short
                        ),
                    ) {
                        log::warn!(
                            "{}",
                            discord_err_msg(&format!("session_idle timeout sid={sid_short}"), &e)
                        );
                    }
                }
                resolved_ids.push(p.message_id.clone());
                continue;
            }
            if !discord_active {
                continue;
            }

            let yes = discord::has_human_reactor(
                notify.discord_token,
                notify.discord_channel,
                &p.message_id,
                "✅",
            )
            .unwrap_or(false);
            let no = discord::has_human_reactor(
                notify.discord_token,
                notify.discord_channel,
                &p.message_id,
                "❌",
            )
            .unwrap_or(false);

            if yes {
                let mut m = mgr.lock().unwrap();
                m.sessions.remove(&p.session_id);
                if m.active_session_id.as_deref() == Some(&p.session_id) {
                    m.active_session_id = m.sessions.keys().next().cloned();
                }
                drop(m);
                if let Err(e) = discord::send_message(
                    notify.discord_token,
                    notify.discord_channel,
                    &format!("✅ 已 kill session `{}`（來源：{}）", sid_short, p.kind),
                ) {
                    log::warn!(
                        "{}",
                        discord_err_msg(
                            &format!("session_idle yes-kill sid={sid_short} kind={}", p.kind),
                            &e
                        )
                    );
                }
                resolved_ids.push(p.message_id.clone());
            } else if no {
                if let Err(e) = discord::send_message(
                    notify.discord_token,
                    notify.discord_channel,
                    &format!("❌ 取消 session `{}`（來源：{}）", sid_short, p.kind),
                ) {
                    log::warn!(
                        "{}",
                        discord_err_msg(
                            &format!("session_idle no-cancel sid={sid_short} kind={}", p.kind),
                            &e
                        )
                    );
                }
                resolved_ids.push(p.message_id.clone());
            }
        }
        if !resolved_ids.is_empty() {
            let resolved: std::collections::HashSet<String> = resolved_ids.into_iter().collect();
            let mut s = state.lock().unwrap();
            s.pending_confirms
                .retain(|p| !resolved.contains(&p.message_id));
        }
    }

    // ─── Rule 3: hook_failure_burst ───
    if cfg.hook_failure_burst_enabled {
        // 一次掃描：算每 provider 次數 + 挑最後一筆（帶 tool + error）
        let (bursts, last_fail): (ProviderBurstCounts, LastFailureByProvider) = {
            let m = mgr.lock().unwrap();
            let cutoff = chrono::Utc::now() - chrono::Duration::seconds(cfg.failure_window_secs);
            let mut counter: HashMap<String, u32> = HashMap::new();
            let mut last: HashMap<String, (String, Option<String>)> = HashMap::new();
            for e in m.recent_events.iter() {
                if e.event_name == "PostToolUseFailure" && e.timestamp >= cutoff {
                    // Filter 掉 shell-enforce-hook 的誤報（中文提示 / ripgrep 被擋）— 不算真失敗
                    let err_s = e.error.as_deref().unwrap_or("");
                    if let Some(pattern) = hook_failure_false_positive(err_s) {
                        // 命中已知誤報 pattern → 計數不算，但 log::debug 留下「為什麼被濾掉」的軌跡
                        // 與 R6/R8 同 surface pattern：silent 行為改為 debug-level 可觀察
                        log::debug!(
                            "[auto_rules] hook_failure_burst filtered: pattern={} provider={} err_preview={}",
                            pattern,
                            e.provider,
                            truncate_utf8_bytes(err_s, 80)
                        );
                        continue;
                    }
                    *counter.entry(e.provider.clone()).or_insert(0) += 1;
                    let tool = e.tool_name.clone().unwrap_or_else(|| "-".into());
                    last.insert(e.provider.clone(), (tool, e.error.clone()));
                }
            }
            let bursts: Vec<_> = counter
                .into_iter()
                .filter(|(_, c)| *c >= cfg.failure_count_threshold)
                .collect();
            (bursts, last)
        };
        for (provider, count) in bursts {
            let key = format!("burst:{provider}");
            let fire = {
                let mut s = state.lock().unwrap();
                dedup_gate(&mut s, &key, cfg.dedup_window_secs)
            };
            if !fire {
                continue;
            }

            let title = format!("{provider} · 失敗爆發");
            let sample = last_fail
                .get(&provider)
                .map(|(tool, err)| {
                    let err_preview = err
                        .as_deref()
                        .map(|s| s.chars().take(120).collect::<String>())
                        .unwrap_or_else(|| "(無詳細錯誤)".into());
                    format!("\n\n**最後樣本**\ntool: `{tool}`\nerror: ```{err_preview}```")
                })
                .unwrap_or_default();
            let desc = format!(
                "**{}** 分鐘內失敗 **{}** 次（≥ {} 觸發）{}",
                cfg.failure_window_secs / 60,
                count,
                cfg.failure_count_threshold,
                sample,
            );
            if discord_active {
                if let Err(e) = discord::send_embed(
                    notify.discord_token,
                    notify.discord_channel,
                    &title,
                    &desc,
                    0xFF0000,
                ) {
                    log::warn!(
                        "{}",
                        discord_err_msg(&format!("hook_failure_burst provider={provider}"), &e)
                    );
                }
            }
            if toast_active {
                if let Some(a) = &notify.app {
                    send_toast(a, &title, &desc);
                }
            }

            // 觸發 openab 重啟
            if !openab_restart_command.trim().is_empty() {
                if let Err(e) = std::process::Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-Command",
                        "Stop-Process -Name openab -Force -ErrorAction SilentlyContinue",
                    ])
                    .spawn()
                {
                    log::warn!("auto_rules: hook_failure_burst openab stop spawn failed: {e}");
                }
                if let Err(e) = std::process::Command::new("powershell.exe")
                    .args(["-NoProfile", "-Command", openab_restart_command])
                    .spawn()
                {
                    log::warn!(
                        "auto_rules: hook_failure_burst openab restart spawn failed (cmd={openab_restart_command}): {e}"
                    );
                }
                if let Err(e) = discord::send_message(
                    notify.discord_token,
                    notify.discord_channel,
                    "🔄 已觸發 OpenAB 重啟",
                ) {
                    log::warn!(
                        "{}",
                        discord_err_msg("hook_failure_burst restart-notice", &e)
                    );
                }
            }
        }
    }

    // ─── Rule 4: daily_summary ───
    // 本地時區當天 == summary_hour 時就推一次，每日 1 次（last_summary_date 擋重複）。
    // dedup marker 在「決定要 fire」時立即設，避免純 toast 模式（無 Discord send 成功路徑）
    // 下被 15s tick 反覆重發。
    if cfg.daily_summary_enabled {
        let now_local = chrono::Local::now();
        let today = now_local.format("%Y-%m-%d").to_string();
        let should_fire = {
            let s = state.lock().unwrap();
            now_local.hour() as u8 == cfg.daily_summary_hour && s.last_summary_date != today
        };
        if should_fire {
            // 先標 dedup（無論 send 成功與否，今天都只 fire 一次）
            let mut s = state.lock().unwrap();
            let _ = mark_summary_fired_if_new(&mut s, SummaryMarker::Daily, &today);
            drop(s);

            // 收集 provider_totals 作為「自 LP 啟動以來累計」數據
            let (rows, total_sessions, total_in, total_out, total_fail) = {
                let m = mgr.lock().unwrap();
                let mut rows: Vec<(String, u64, u64, u64, u64)> = m
                    .provider_totals
                    .iter()
                    .map(|(p, t)| {
                        (
                            p.clone(),
                            t.session_count,
                            t.tokens_input,
                            t.tokens_output,
                            t.failure_count,
                        )
                    })
                    .collect();
                rows.sort_by(|a, b| b.1.cmp(&a.1)); // 依 session 數排序
                let ts: u64 = rows.iter().map(|r| r.1).sum();
                let ti: u64 = rows.iter().map(|r| r.2).sum();
                let to: u64 = rows.iter().map(|r| r.3).sum();
                let tf: u64 = rows.iter().map(|r| r.4).sum();
                (rows, ts, ti, to, tf)
            };
            let mut desc = format!(
                "`{today}` 全體累計\n\n\
                Sessions **{total_sessions}** · In **{}** · Out **{}** · Fail **{total_fail}**\n\n\
                **Per-provider**",
                fmt_tokens(total_in),
                fmt_tokens(total_out)
            );
            for (p, sc, ti, to, tf) in rows.iter().take(10) {
                desc.push_str(&format!(
                    "\n• `{p}` — {sc} sessions · in {} / out {} · fail {tf}",
                    fmt_tokens(*ti),
                    fmt_tokens(*to)
                ));
            }
            let title = format!("每日摘要 · {today}");
            if toast_active {
                if let Some(a) = &notify.app {
                    send_toast(a, &title, "今日 quota 日報已送出");
                }
            }
            // Discord 為 best-effort；失敗不再 unblock dedup（避免失敗重試轟炸）
            if let Err(e) = discord::send_embed(
                notify.discord_token,
                notify.discord_channel,
                &title,
                &desc,
                0x4169E1,
            ) {
                log::warn!("{}", discord_err_msg("daily_summary", &e));
            }
            // 持久化 marker——避免重啟後整點 double-fire。失敗 best-effort log 不 panic
            let (persisted_date, persisted_week) = {
                let s = state.lock().unwrap();
                (s.last_summary_date.clone(), s.last_weekly_key.clone())
            };
            if let Err(e) = persist_summary_markers(&persisted_date, &persisted_week) {
                log::error!("[auto_rules] persist summary markers after daily: {e}");
            }
        }
    }

    // ─── Rule 5: weekly_summary — 週一 summary_hour 推 top 3 provider ───
    if cfg.weekly_summary_enabled {
        use chrono::Datelike;
        let now_local = chrono::Local::now();
        let iso = now_local.iso_week();
        let wkey = format!("{}-W{:02}", iso.year(), iso.week());
        // 只在週一（Mon=1）+ summary_hour 觸發
        let is_mon = now_local.weekday().num_days_from_monday() == 0;
        let should = {
            let s = state.lock().unwrap();
            is_mon && now_local.hour() as u8 == cfg.daily_summary_hour && s.last_weekly_key != wkey
        };
        if should {
            // 先標 dedup（無論 send 成功與否，本週都只 fire 一次）
            let mut s = state.lock().unwrap();
            let _ = mark_summary_fired_if_new(&mut s, SummaryMarker::Weekly, &wkey);
            drop(s);

            let top = {
                let m = mgr.lock().unwrap();
                let mut rows: Vec<(String, u64, u64, u64, u64)> = m
                    .provider_totals
                    .iter()
                    .map(|(p, t)| {
                        (
                            p.clone(),
                            t.session_count,
                            t.tokens_input,
                            t.tokens_output,
                            t.failure_count,
                        )
                    })
                    .collect();
                rows.sort_by(|a, b| b.1.cmp(&a.1));
                rows.into_iter().take(3).collect::<Vec<_>>()
            };
            let mut desc = format!("`{wkey}` 上週 top 3 provider（依 session 數）\n");
            if top.is_empty() {
                desc.push_str("\n（本週無資料）");
            } else {
                for (i, (p, sc, ti, to, tf)) in top.iter().enumerate() {
                    let rank = match i {
                        0 => "#1",
                        1 => "#2",
                        2 => "#3",
                        _ => "• ",
                    };
                    desc.push_str(&format!(
                        "\n{rank} **{p}** — {sc} sessions · in {} / out {} · fail {tf}",
                        fmt_tokens(*ti),
                        fmt_tokens(*to)
                    ));
                }
            }
            let title = format!("週摘要 · {wkey}");
            if toast_active {
                if let Some(a) = &notify.app {
                    send_toast(a, &title, "本週 quota 週報已送出");
                }
            }
            // Discord 為 best-effort；失敗不再 unblock dedup
            if let Err(e) = discord::send_embed(
                notify.discord_token,
                notify.discord_channel,
                &title,
                &desc,
                0x9370DB,
            ) {
                log::warn!("{}", discord_err_msg("weekly_summary", &e));
            }
            // 持久化 marker——避免重啟後整點 double-fire。失敗 best-effort log 不 panic
            let (persisted_date, persisted_week) = {
                let s = state.lock().unwrap();
                (s.last_summary_date.clone(), s.last_weekly_key.clone())
            };
            if let Err(e) = persist_summary_markers(&persisted_date, &persisted_week) {
                log::error!("[auto_rules] persist summary markers after weekly: {e}");
            }
        }
    }

    // ─── Rule 6: token_spike — 當日消耗 > N× 近 7 日均值 → 告警 ───
    if cfg.token_spike_enabled {
        // R30 silent-fail surfacing: 修前 `if let Ok(hist) = ...` 在 quota-history.csv
        // 損壞 / IO 錯 / 鎖 poison 時整條 token_spike 規則被 bypass 卻無 log, operator 看 alert
        // 沒觸發還以為是 token 沒爆, 其實是 history 讀不到. 改 match Err + 結構化 warn.
        let hist = match crate::quota_history::load_history() {
            Ok(h) => h,
            Err(e) => {
                log::warn!(
                    "[auto_rules] token_spike rule skipped: quota-history.csv load failed: {e}"
                );
                return;
            }
        };
        let now_s = now_secs();
        let today_start = now_s - (now_s % 86400);
        for (name, series) in hist.iter() {
            if series.len() < 2 {
                continue;
            }
            // 以 ts 排序 (load_history 保持插入序 ≈ 時間序)
            let mut sorted: Vec<&(u64, u8)> = series.iter().collect();
            sorted.sort_by_key(|(t, _)| *t);
            // 分組：今日 vs 近 7 日（不含今日）
            let today: Vec<&(u64, u8)> = sorted
                .iter()
                .filter(|(t, _)| *t as i64 >= today_start)
                .copied()
                .collect();
            if today.len() < 2 {
                continue;
            }
            let today_first = today.first().unwrap().1 as i32;
            let today_last = today.last().unwrap().1 as i32;
            let today_consumed = (today_first - today_last).max(0) as u32;
            if today_consumed < cfg.token_spike_min_consumption_pct as u32 {
                continue;
            }
            // 計算近 7 天每日消耗：用 day_bucket groupBy
            let mut daily: std::collections::BTreeMap<i64, Vec<u8>> =
                std::collections::BTreeMap::new();
            for (t, p) in sorted.iter() {
                let day = (*t as i64) / 86400;
                if day < today_start / 86400 && day >= (today_start / 86400) - 7 {
                    daily.entry(day).or_default().push(*p);
                }
            }
            if daily.is_empty() {
                continue;
            }
            let mut day_consumptions: Vec<u32> = daily
                .values()
                .filter(|v| v.len() >= 2)
                .map(|v| (v.first().unwrap().saturating_sub(*v.last().unwrap())) as u32)
                .collect();
            if day_consumptions.is_empty() {
                continue;
            }
            day_consumptions.sort();
            let avg: f64 =
                day_consumptions.iter().sum::<u32>() as f64 / day_consumptions.len() as f64;
            if avg < 1.0 {
                continue;
            } // 避免除 0 近值
            let ratio = today_consumed as f64 / avg;
            if ratio < cfg.token_spike_multiplier {
                continue;
            }
            // Fire
            let key = format!("token_spike:{name}");
            let fire = {
                let mut s = state.lock().unwrap();
                dedup_gate(&mut s, &key, 86400) // 一天一次
            };
            if !fire {
                continue;
            }
            let title = format!("{name} · 用量異常");
            let desc = format!(
                "今日消耗 **{}%**，是近 7 日平均 **{:.1}%** 的 **{:.1}×**（門檻 {}×）",
                today_consumed, avg, ratio, cfg.token_spike_multiplier
            );
            if discord_active {
                if let Err(e) = discord::send_embed(
                    notify.discord_token,
                    notify.discord_channel,
                    &title,
                    &desc,
                    0xFF6B6B,
                ) {
                    log::warn!(
                        "{}",
                        discord_err_msg(
                            &format!("token_spike name={name} pct={today_consumed}"),
                            &e
                        )
                    );
                }
            }
            if toast_active {
                if let Some(a) = &notify.app {
                    send_toast(
                        a,
                        &title,
                        &format!(
                            "{name} 今日 {}% · 平均 {:.0}% · {:.1}×",
                            today_consumed, avg, ratio
                        ),
                    );
                }
            }
        }
    }
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

/// Daily/weekly summary 共用 dedup：若 marker 與已標記值不同，標記並回傳 true（這次是新 fire）。
/// 修正前 dedup marker 在 send 成功後才設，純 toast 模式（無 Discord send 成功路徑）會被 15s tick
/// 反覆重發。改為「決定要 fire 立即標記」後，無論 send 是否成功都擋重複。
pub(crate) fn mark_summary_fired_if_new(
    state: &mut AutoRuleState,
    field: SummaryMarker,
    marker: &str,
) -> bool {
    let slot = match field {
        SummaryMarker::Daily => &mut state.last_summary_date,
        SummaryMarker::Weekly => &mut state.last_weekly_key,
    };
    if *slot == marker {
        return false;
    }
    *slot = marker.to_string();
    true
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SummaryMarker {
    Daily,
    Weekly,
}

// ─── summary marker 持久化 ───
//
// 修正前 `last_summary_date` / `last_weekly_key` 是純 in-memory `Arc<Mutex<…>>`：
// 每天 9:00 整點推完 daily summary 後若 app 重啟，state 回到 default ""，
// 下一個 15s tick 又符合「今天 != last_summary_date」→ 整點後短窗內 double-fire toast/embed。
// 持久化到 `~/.lobsterpulse/auto_state.json`，重啟讀回。
//
// 寫入失敗視為 best-effort：log 不 panic，dedup 仍靠 in-memory 擋當下 session 重複。

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PersistedSummaryMarkers {
    #[serde(default)]
    last_summary_date: String,
    #[serde(default)]
    last_weekly_key: String,
    /// session_idle 規則的 last_event_time 錨點：`(session_id, last_event_ts)` 已通知
    /// 過的紀錄。session 變 active 後錨點失配會自動放行下輪 idle；用「事件週期」做
    /// dedup 錨點是 R4 fix spam 的設計，持久化是避免重啟後舊 idle 週期又被通知一次。
    /// 結構沿用 R5 同檔 `auto_state.json` —— summary marker + session_idle marker 共存。
    #[serde(default)]
    last_session_idle_event_ts: HashMap<String, i64>,
}

fn summary_marker_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".lobsterpulse").join("auto_state.json"))
}

// 對外暴露給 test 模組：用傳入 dir 模擬 `~/.lobsterpulse/`，避免污染真 home
#[cfg(test)]
pub(crate) fn persist_summary_markers_at(
    dir: &Path,
    last_summary_date: &str,
    last_weekly_key: &str,
) -> Result<(), String> {
    persist_summary_markers_in(
        &dir.join("auto_state.json"),
        last_summary_date,
        last_weekly_key,
    )
}

/// 啟動時讀回上次 daily/weekly summary 觸發標記。檔案缺 / 壞 JSON → 回空字串。
pub fn load_persisted_summary_markers() -> (String, String) {
    let Some(path) = summary_marker_path() else {
        return (String::new(), String::new());
    };
    load_persisted_summary_markers_at_impl(&path)
}

fn load_persisted_summary_markers_at_impl(path: &Path) -> (String, String) {
    let parsed = parse_persisted_markers_at(path);
    (parsed.last_summary_date, parsed.last_weekly_key)
}

/// 讀本機 CLI quota snapshot（`~/.lobsterpulse/usage-local.json`），給
/// Discord `!lp quota` 指令顯示用。三條路徑分流（對齊 R28
/// `load_config_at` / R29 `parse_quota_history_row`）：
/// 1. `NotFound` → `Ok(None)`（first-run 預期,正常情況）
/// 2. 其他 IO 錯（權限拒絕 / 磁碟鎖住 / cross-device）→ `Err`，caller 端 log warn
/// 3. JSON 解析失敗（磁碟寫入半截 / 編碼錯）→ `Err`，caller 端 log warn
///
/// 修前 `!lp quota` 兩條 silent chain：
///   - `read_to_string(&path).unwrap_or_default()` 吞 IO 錯誤
///   - `from_str(&data).unwrap_or_default()` 吞壞 JSON
///     → 用戶在 `usage-local.json` 損壞（CLI crash 中斷寫入 / 磁碟滿）時點
///     `!lp quota` 看到「無 runner」訊息，operator 完全無 log 可查哪條 chain 失敗。
pub fn load_local_usage_snapshot() -> Result<Option<serde_json::Value>, String> {
    let Some(home) = dirs::home_dir() else {
        return Err("no home dir".into());
    };
    let path = home.join(".lobsterpulse").join("usage-local.json");
    load_local_usage_snapshot_at(&path)
}

/// Test 入口：抽 path 參數讓 unit test 可注入 tmpdir / 不存在路徑 / 壞 JSON，
/// 不必碰 process env。對齊 R28 `load_config_at` pattern。
pub(crate) fn load_local_usage_snapshot_at(
    path: &Path,
) -> Result<Option<serde_json::Value>, String> {
    let data = match std::fs::read_to_string(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    serde_json::from_str(&data)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// 測試輔助：從傳入 dir 讀 summary markers，避免污染真 `~/.lobsterpulse/`
#[cfg(test)]
pub(crate) fn load_persisted_summary_markers_at(dir: &Path) -> (String, String) {
    load_persisted_summary_markers_at_impl(&dir.join("auto_state.json"))
}

/// Daily/Weekly summary 真的 fire 出去之後，把當前 marker 寫回磁碟。
/// 重啟後 `load_persisted_summary_markers` 會讀回，避免整點重發。
pub fn persist_summary_markers(
    last_summary_date: &str,
    last_weekly_key: &str,
) -> Result<(), String> {
    persist_summary_markers_in(
        summary_marker_path()
            .ok_or_else(|| "no home dir".to_string())?
            .as_path(),
        last_summary_date,
        last_weekly_key,
    )
}

fn persist_summary_markers_in(
    path: &Path,
    last_summary_date: &str,
    last_weekly_key: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Read-modify-write 保留已存在的 session_idle markers——同檔共存
    let mut payload: PersistedSummaryMarkers = parse_persisted_markers_at(path);
    payload.last_summary_date = last_summary_date.to_string();
    payload.last_weekly_key = last_weekly_key.to_string();
    let data = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}

/// 啟動時讀回 session_idle 規則的 dedup 錨點（避免重啟後舊 idle 週期重發 toast）。
/// 檔案缺 / 壞 JSON → 回空 HashMap（等同 in-memory default，無破壞性）。
pub fn load_persisted_session_idle_markers() -> HashMap<String, i64> {
    let Some(path) = summary_marker_path() else {
        return HashMap::new();
    };
    load_persisted_session_idle_markers_at_impl(&path)
}

fn load_persisted_session_idle_markers_at_impl(path: &Path) -> HashMap<String, i64> {
    parse_persisted_markers_at(path).last_session_idle_event_ts
}

/// 測試輔助：從傳入 dir 讀 session_idle markers
#[cfg(test)]
pub(crate) fn load_persisted_session_idle_markers_at(dir: &Path) -> HashMap<String, i64> {
    load_persisted_session_idle_markers_at_impl(&dir.join("auto_state.json"))
}

/// session_idle 規則真的 fire 出去之後，把當前 (sid → last_event_ts) 錨點寫回磁碟。
/// 沿用同檔 `auto_state.json`，read-modify-write 保留 summary marker。寫入失敗視為
/// best-effort：log 不 panic，dedup 仍靠 in-memory 擋當下 session 重複。
pub fn persist_session_idle_markers(markers: &HashMap<String, i64>) -> Result<(), String> {
    persist_session_idle_markers_in(
        summary_marker_path()
            .ok_or_else(|| "no home dir".to_string())?
            .as_path(),
        markers,
    )
}

fn persist_session_idle_markers_in(
    path: &Path,
    markers: &HashMap<String, i64>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Read-modify-write 保留已存在的 summary marker——同檔共存
    let mut payload: PersistedSummaryMarkers = parse_persisted_markers_at(path);
    payload.last_session_idle_event_ts = markers.clone();
    let data = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}

/// Discord command polling — 看 #cctest 有沒有新 `!lp ...` 訊息，有就執行並回覆。
/// 呼叫時機：tick 結束後（每 15s 一次）。用 state.last_cmd_msg_id 去重。
pub fn poll_discord_commands(
    token: &str,
    channel: &str,
    mgr: &Mutex<SessionManager>,
    state: &SharedAutoState,
    config: &Mutex<crate::config::AppConfig>,
) {
    if token.trim().is_empty() || channel.trim().is_empty() {
        return;
    }
    let after = state.lock().unwrap().last_cmd_msg_id.clone();
    let after_opt = if after.is_empty() {
        None
    } else {
        Some(after.as_str())
    };
    let msgs = match discord::list_messages(token, channel, 10, after_opt) {
        Ok(m) => m,
        Err(e) => {
            // R6 修了 14 處 Discord transport silent fail，但本呼叫是唯一漏網之魚
            // (其他 21 處 Discord call 都已 `if let Err(e) => log::warn!` 對齊)。
            // 對齊 R6/R8 surface pattern：list_messages 失敗要 log ctx 讓 operator
            // 知道「!lp 指令 polling 整輪為何停了」而非全 tick silent 沒線索。
            log::warn!(
                "{}",
                discord_err_msg("poll_discord_commands list_messages", &e)
            );
            return;
        }
    };
    // 訊息預設時間由新到舊 — 我們反過來處理（舊的先），確保 last_cmd_msg_id 最後 = 最新
    let mut new_last_id: Option<String> = None;
    for m in msgs.iter().rev() {
        let Some(mid) = m.get("id").and_then(|x| x.as_str()) else {
            continue;
        };
        let content = m
            .get("content")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim();
        new_last_id = Some(mid.to_string()); // 無論是否處理，都推進 id，避免卡死
                                             // 只處理 `!lp` 開頭的訊息；bot 自己的回覆不會以此開頭（`**🦞`/`**📊` 等）
                                             // 不再按 is_bot filter 因為 dev 自測時也想用 bot token 發指令
        if !content.starts_with("!lp") {
            continue;
        }
        let reply = handle_command(content, mgr, config);
        if reply.is_empty() {
            continue;
        }

        // 特殊處理 kill 二次確認
        if let Some(sid) = reply.strip_prefix("__CONFIRM_KILL__:") {
            let content = format!(
                "🟡 請確認要 kill session `{}`？\n\n✅ = 執行  /  ❌ = 取消  /  10 分鐘無反應 = 自動取消",
                prefix_chars(sid, 12)
            );
            match discord::send_message(token, channel, &content) {
                Ok(mid) => {
                    if let Err(e) = discord::add_reaction(token, channel, &mid, "✅") {
                        log::warn!(
                            "{}",
                            discord_err_msg(
                                &format!(
                                    "discord_kill_cmd sid={} reaction=✅",
                                    prefix_chars(sid, 8)
                                ),
                                &e
                            )
                        );
                    }
                    if let Err(e) = discord::add_reaction(token, channel, &mid, "❌") {
                        log::warn!(
                            "{}",
                            discord_err_msg(
                                &format!(
                                    "discord_kill_cmd sid={} reaction=❌",
                                    prefix_chars(sid, 8)
                                ),
                                &e
                            )
                        );
                    }
                    state.lock().unwrap().pending_confirms.push(PendingConfirm {
                        kind: "discord_kill_cmd",
                        session_id: sid.to_string(),
                        message_id: mid,
                        posted_at: Instant::now(),
                    });
                }
                Err(e) => {
                    // R25 surface：原 `if let Ok(mid) = ... { ... }` 在 Discord send 失敗時
                    // 整段靜默跳過——user 輸入 `!lp kill` 後沒看到任何東西、operator 也不知道
                    // Discord 拒絕了，pending_confirms 也沒 push（後續 10 分鐘 timeout 邏輯不
                    // 會觸發，state 仍乾淨但 user 經驗是 bot 沒回應）。
                    log::warn!(
                        "{}",
                        discord_err_msg(
                            &format!(
                                "discord_kill_cmd sid={} confirm send_message",
                                prefix_chars(sid, 8)
                            ),
                            &e
                        )
                    );
                }
            }
        } else {
            if let Err(e) = discord::send_message(token, channel, &reply) {
                log::warn!("{}", discord_err_msg("discord_cmd reply", &e));
            }
        }
    }
    if let Some(id) = new_last_id {
        state.lock().unwrap().last_cmd_msg_id = id;
    }
}

/// 解析 `!lp <sub>` 指令並回傳 reply 文字（空字串 = 不回）。
fn handle_command(
    content: &str,
    mgr: &Mutex<SessionManager>,
    config: &Mutex<crate::config::AppConfig>,
) -> String {
    let rest = content.trim_start_matches("!lp").trim();
    let mut parts = rest.splitn(2, char::is_whitespace);
    let sub = parts.next().unwrap_or("").to_lowercase();
    let arg = parts.next().unwrap_or("").trim();

    match sub.as_str() {
        "help" | "" => [
            "**🦞 LobsterPulse**",
            "`!lp status`              sessions 總覽",
            "`!lp quota`               本機 runner 額度",
            "`!lp log [N]`             最近 N 筆事件（1–30）",
            "`!lp kill <sid8>`         kill 指定 session（需二次確認）",
            "`!lp summary`             立即推每日摘要",
            "`!lp pause` / `resume`    暫停／恢復自動化",
            "`!lp trend <runner>`      7 天 ASCII sparkline",
            "`!lp help`                本清單",
        ]
        .join("\n"),

        "status" => {
            let m = mgr.lock().unwrap();
            let sess_count = m.sessions.len();
            let active: usize = m.sessions.values().filter(|s| s.is_active()).count();
            let provider_counts: HashMap<String, usize> =
                m.sessions.values().fold(HashMap::new(), |mut acc, s| {
                    *acc.entry(s.provider.clone()).or_insert(0) += 1;
                    acc
                });
            let mut lines: Vec<String> = provider_counts
                .iter()
                .map(|(p, c)| format!("• `{p}`: {c}"))
                .collect();
            lines.sort();
            format!(
                "**Status**\nSessions **{sess_count}** · Active **{active}**\n\n{}",
                if lines.is_empty() {
                    "（無 session）".into()
                } else {
                    lines.join("\n")
                }
            )
        }

        "quota" => {
            // 對齊 R30 silent-fail surfacing：`usage-local.json` 讀壞 / parse 壞
            // 不再默默回「無 runner」,改 log warn + 區分「不存在」/「損壞」/
            // 「空 runners」三條路徑給 Discord 使用者。
            let v = match load_local_usage_snapshot() {
                Ok(Some(v)) => v,
                Ok(None) => {
                    return "（沒有 runner 結果，usage-local.json 不存在）".into();
                }
                Err(e) => {
                    log::warn!(
                        "[auto_rules] !lp quota: usage-local.json load failed: {e} \
                         — 回「無 runner」訊息給 Discord,但 log 可查根因"
                    );
                    return "（沒有 runner 結果，usage-local.json 損壞，詳見 log）".into();
                }
            };
            let runners = v
                .get("runners")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default();
            if runners.is_empty() {
                return "（沒有 runner 結果，usage-local.json 為空）".into();
            }
            let mut lines = vec!["**本機額度**".to_string()];
            for r in runners {
                let label = r.get("label").and_then(|x| x.as_str()).unwrap_or("?");
                let text = r
                    .get("text")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .replace("**", "");
                let compact: String = text.lines().collect::<Vec<_>>().join(" · ");
                lines.push(format!("**{label}**\n{compact}"));
            }
            lines.join("\n\n")
        }

        "kill" => {
            // 注意：這裡只做查找 + 回預覽，實際 kill 放到 tick 的 pending_confirms 流程
            // （呼叫方在 poll_discord_commands 裡會用 reply 特殊前綴讓 tick 掛 pending）
            if arg.len() < 3 {
                return "用法：`!lp kill <sid8>`（至少 3 碼）".into();
            }
            let m = mgr.lock().unwrap();
            let target: Option<String> = m.sessions.keys().find(|k| k.starts_with(arg)).cloned();
            match target {
                Some(sid) => {
                    // 用特殊前綴讓 caller 偵測到並加 pending_confirm
                    format!("__CONFIRM_KILL__:{sid}")
                }
                None => format!("❌ 找不到 session 以 `{arg}` 開頭"),
            }
        }

        "summary" => {
            // 手動觸發每日 summary
            let m = mgr.lock().unwrap();
            let total_sessions: u64 = m.provider_totals.values().map(|t| t.session_count).sum();
            let total_in: u64 = m.provider_totals.values().map(|t| t.tokens_input).sum();
            let total_out: u64 = m.provider_totals.values().map(|t| t.tokens_output).sum();
            let total_fail: u64 = m.provider_totals.values().map(|t| t.failure_count).sum();
            let mut rows: Vec<_> = m
                .provider_totals
                .iter()
                .map(|(p, t)| {
                    format!(
                        "• `{p}` — {} sessions · in {} / out {} · fail {}",
                        t.session_count,
                        fmt_tokens(t.tokens_input),
                        fmt_tokens(t.tokens_output),
                        t.failure_count
                    )
                })
                .collect();
            rows.sort();
            format!(
                "**手動摘要**\nSessions **{total_sessions}** · In **{}** · Out **{}** · Fail **{total_fail}**\n\n{}",
                fmt_tokens(total_in), fmt_tokens(total_out),
                if rows.is_empty() { "（無）".into() } else { rows.join("\n") }
            )
        }

        "log" => {
            let n: usize = arg.parse().unwrap_or(10).clamp(1, 30);
            let m = mgr.lock().unwrap();
            let total = m.recent_events.len();
            if total == 0 {
                return "（無事件紀錄）".into();
            }
            let take_n = n.min(total);
            // 取最後 n 筆（recent_events 越後越新）
            let tail: Vec<_> = m
                .recent_events
                .iter()
                .skip(total.saturating_sub(take_n))
                .collect();
            let mut lines = vec![format!("**最近 {take_n} 筆事件**")];
            lines.push("```".into());
            for e in tail.iter().rev() {
                let t = e
                    .timestamp
                    .with_timezone(&chrono::Local)
                    .format("%H:%M:%S")
                    .to_string();
                let prov = &e.provider;
                let ev = &e.event_name;
                let tool = e
                    .tool_name
                    .as_deref()
                    .map(|s| format!(" [{s}]"))
                    .unwrap_or_default();
                let err = e
                    .error
                    .as_deref()
                    .map(|s| {
                        let short: String = s.chars().take(40).collect();
                        format!(" ! {short}")
                    })
                    .unwrap_or_default();
                lines.push(format!("{t} {prov} · {ev}{tool}{err}"));
            }
            lines.push("```".into());
            // Discord 2000 char cap
            let joined = lines.join("\n");
            if joined.len() > 1950 {
                // 截掉尾段（char 邊界安全）
                format!(
                    "{}\n```\n…（超長已截斷，用 `!lp log 10`）",
                    truncate_utf8_bytes(&joined, 1900)
                )
            } else {
                joined
            }
        }

        "pause" => {
            let mut c = config.lock().unwrap();
            c.appearance.auto_actions.master_enabled = false;
            match crate::config::save_config(&c) {
                Ok(()) => "自動化 **暫停**（`!lp resume` 恢復）".into(),
                Err(e) => {
                    // R23 surface：使用者主動改 setting 失敗時（磁碟滿 / 權限 / path 鎖住），
                    // 原本 `let _ =` 沉默吞 → UI 顯示「暫停成功」、實際下次啟動 revert。
                    // 改 surfaced via log::warn + Discord 回應帶 ⚠️ 提示，讓 user 立即知道「未持久化」。
                    log::warn!("{}", config_persist_warn_msg("pause", &e));
                    format!(
                        "⚠️ 自動化 **暫停**（磁碟寫入失敗：{e}，重啟後會 revert，請查 `~/.lobsterpulse/config.json` 權限）"
                    )
                }
            }
        }

        "resume" => {
            let mut c = config.lock().unwrap();
            c.appearance.auto_actions.master_enabled = true;
            match crate::config::save_config(&c) {
                Ok(()) => "自動化 **恢復**".into(),
                Err(e) => {
                    log::warn!("{}", config_persist_warn_msg("resume", &e));
                    format!("⚠️ 自動化 **恢復**（磁碟寫入失敗：{e}，重啟後會 revert）")
                }
            }
        }

        "trend" => {
            if arg.is_empty() {
                return "用法：`!lp trend <runner>`（runner 名稱如 claude / codex / copilot / gemini）".into();
            }
            // R30 silent-fail surfacing: 修前 `unwrap_or_default()` 在 quota-history.csv
            // 損壞 / IO 錯 / 鎖 poison 時把 hist 變空 HashMap, 後續 `hist.get(arg)` miss
            // 會誤導 operator 回「找不到 runner `claude`」實際是 history 讀不到. 改 match Err
            // 三條分流 + 結構化 warn, Discord 端給明確「讀取失敗, 詳見 log」訊息.
            let hist = match crate::quota_history::load_history() {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("[auto_rules] !lp trend: quota-history.csv load failed: {e}");
                    return "quota-history.csv 讀取失敗, 詳見 log（prefix: `[auto_rules] !lp trend: quota-history.csv load failed`）".into();
                }
            };
            let Some(series) = hist.get(arg) else {
                return format!(
                    "找不到 runner `{arg}`（已有：{}）",
                    hist.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            };
            // 切最近 7 天
            let cutoff = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
                .saturating_sub(7 * 86400);
            let recent: Vec<(u64, u8)> = series
                .iter()
                .filter(|(ts, _)| *ts >= cutoff)
                .cloned()
                .collect();
            if recent.is_empty() {
                return format!("`{arg}` 7 天內無紀錄");
            }
            // ASCII sparkline：quantize pct → 8 level
            let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            let pcts: Vec<u8> = recent.iter().map(|(_, p)| *p).collect();
            // 取樣至 40 個 bucket（避免訊息太長）
            let bucket_n = 40.min(pcts.len());
            let step = (pcts.len() as f64) / (bucket_n as f64);
            let mut chart = String::new();
            for i in 0..bucket_n {
                let idx = (i as f64 * step) as usize;
                let p = pcts[idx.min(pcts.len() - 1)];
                let level = ((p as f64 / 100.0) * 7.0).round() as usize;
                chart.push(blocks[level.min(7)]);
            }
            let last = *pcts.last().unwrap();
            let min = *pcts.iter().min().unwrap();
            let max = *pcts.iter().max().unwrap();
            format!(
                "**{arg}** 7 天趨勢\n```\n{chart}\n```\n現在 **{last}%** · 低 {min}% · 高 {max}% · {} 筆",
                recent.len()
            )
        }

        _ => format!("未知指令 `{sub}`（試 `!lp help`）"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_chars_handles_unicode() {
        assert_eq!(prefix_chars("abc測試", 4), "abc測");
        assert_eq!(prefix_chars("🙂🙂🙂", 2), "🙂🙂");
    }

    /// R6 regression：14 處 `let _ = discord::xxx(...)` 改成 `if let Err(e) = ... { log::warn!(...) }`
    /// —— 集中 prefix `[auto_rules] discord <ctx> failed: <err>`，log filter 可一條 query 抓全部。
    #[test]
    fn discord_err_msg_unifies_prefix() {
        assert_eq!(
            discord_err_msg("quota_low name=claude pct=10", "curl exit 22: HTTP/1.1 401"),
            "[auto_rules] discord quota_low name=claude pct=10 failed: curl exit 22: HTTP/1.1 401"
        );
        // 中文 ctx 也應原樣保留（無 trim / lower-case）
        assert_eq!(
            discord_err_msg("session_idle 確認", "timeout"),
            "[auto_rules] discord session_idle 確認 failed: timeout"
        );
    }

    /// R23 regression：2 處 `let _ = crate::config::save_config(&c)` 改成 `match ... { Err => log::warn!(...) }`。
    /// `!lp pause` / `!lp resume` 失敗時（磁碟滿 / 權限 / path 鎖住）原本 silent 吞，使用者
    /// 主動改設定 UI 顯示「成功」但下次啟動 revert。集中 prefix `[auto_rules] !lp <action>:`
    /// 與 R6 `[auto_rules] discord <ctx>` 對齊，log filter 可一條 query 抓出所有 auto-config
    /// 持久化失敗。
    #[test]
    fn config_persist_warn_msg_unifies_prefix() {
        assert_eq!(
            config_persist_warn_msg("pause", "Permission denied (os error 5)"),
            "[auto_rules] !lp pause: save_config failed: Permission denied (os error 5) — setting will revert on next launch"
        );
        // 中文 / 特殊字元 error 也應原樣保留
        assert_eq!(
            config_persist_warn_msg("resume", "磁碟空間不足"),
            "[auto_rules] !lp resume: save_config failed: 磁碟空間不足 — setting will revert on next launch"
        );
    }

    /// R11 regression：`poll_discord_commands` 內的 `discord::list_messages` 是 R6 漏網的
    /// 最後一條 Discord transport silent fail（其他 21 處都已對齊 R6 pattern）。
    /// 鎖定 ctx 字串穩定，讓 log filter 可一條 query 抓出「!lp 指令 polling 整輪停了」的根因。
    #[test]
    fn poll_discord_commands_list_messages_error_uses_unified_prefix() {
        // ctx 必須是 `poll_discord_commands list_messages`（精確指出 call site）
        let msg = discord_err_msg(
            "poll_discord_commands list_messages",
            "curl exit 7: Failed to connect",
        );
        assert!(
            msg.contains("poll_discord_commands list_messages"),
            "ctx 必須指到 poll_discord_commands 的 list_messages 呼叫點：{msg}"
        );
        assert!(
            msg.starts_with("[auto_rules] discord ") && msg.contains(" failed: "),
            "必須走 R6 統一 prefix 讓 log filter 一條 query 抓全部 Discord 失敗：{msg}"
        );
    }

    /// R25 regression：session_idle confirm 與 kill confirm 兩條 `if let Ok(mid) = discord::send_message`
    /// → 改成 `match ... { Err(e) => log::warn!(discord_err_msg(...)) }`。原 pattern 在 Discord
    /// send 失敗時整段靜默跳過（含 `pending_confirms.push`），operator 看不到「user 已點 ❌/✅
    /// 但 Discord 沒收到」+ state 沒推進會卡住後續 click 解析。
    ///
    /// 鎖定 ctx 格式穩定（兩條都走 R6/R11 同 prefix `[auto_rules] discord ... failed: ...`，
    /// log filter 一條 query 抓全部）：
    ///   - `session_idle sid=<8char-prefix> confirm send_message`
    ///   - `discord_kill_cmd sid=<8char-prefix> confirm send_message`
    #[test]
    fn r25_confirm_send_message_errors_use_unified_prefix() {
        // session_idle confirm send_message ctx 必須含 `confirm send_message` 讓 operator 一眼看
        // 出「這是 send 確認訊息失敗」而非「add_reaction 失敗」（reaction 失敗還有 message id 可救）
        let sid = "abc12345deadbeef";
        let sid_short = prefix_chars(sid, 8);
        let msg = discord_err_msg(
            &format!("session_idle sid={sid_short} confirm send_message"),
            "curl exit 22: HTTP/1.1 401 Unauthorized",
        );
        assert!(
            msg.contains("confirm send_message"),
            "ctx 必須標 `confirm send_message`（區分 add_reaction fail 跟 send fail）：{msg}"
        );
        assert!(
            msg.contains(&format!("session_idle sid={sid_short}")),
            "ctx 必須帶 sid short prefix 對齊 reaction 既有 pattern：{msg}"
        );
        assert!(
            msg.starts_with("[auto_rules] discord ") && msg.contains(" failed: "),
            "必須走 R6 統一 prefix：{msg}"
        );

        // kill confirm send_message 同樣 contract
        let msg2 = discord_err_msg(
            &format!("discord_kill_cmd sid={sid_short} confirm send_message"),
            "curl exit 56: Recv failure",
        );
        assert!(
            msg2.contains("discord_kill_cmd") && msg2.contains("confirm send_message"),
            "ctx 必須標 `discord_kill_cmd ... confirm send_message`：{msg2}"
        );
        assert!(
            msg2.starts_with("[auto_rules] discord ") && msg2.contains(" failed: "),
            "必須走 R6 統一 prefix：{msg2}"
        );
    }

    /// R25 contract：`prefix_chars` 在 R25 兩處新 ctx 都用 `prefix_chars(sid, 8)` 對齊既有
    /// R6 reaction ctx pattern。鎖定 sid=8 截前綴的語意，避免後人改成 4 / 12 導致 log filter regex 失效。
    #[test]
    fn r25_confirm_ctx_uses_eight_char_sid_prefix() {
        // 16 字 sid → 8 字 short
        assert_eq!(prefix_chars("abcdef0123456789", 8), "abcdef01");
        // 8 字 sid → 整段
        assert_eq!(prefix_chars("abc12345", 8), "abc12345");
        // 4 字 sid → 整段（短於 8 不截）
        assert_eq!(prefix_chars("abc1", 8), "abc1");
    }

    #[test]
    fn truncate_utf8_bytes_is_boundary_safe() {
        let s = "abc測試🙂";
        assert_eq!(truncate_utf8_bytes(s, 2), "ab");
        assert_eq!(truncate_utf8_bytes(s, 5), "abc");
        assert_eq!(truncate_utf8_bytes(s, 6), "abc測");
        assert_eq!(truncate_utf8_bytes(s, 10), "abc測試");
    }

    #[test]
    fn extract_min_percent_ignores_invalid_values() {
        let t = "A: **49.7%** B: **N/A%** C: **101%** D: **-5%**";
        assert_eq!(extract_min_percent(t), Some(50));
    }

    /// R9 regression：5 條已知誤報子字串逐一命中要回對應 pattern name
    #[test]
    fn hook_false_positive_matches_each_known_pattern() {
        assert_eq!(
            hook_failure_false_positive("請用 rg 取代 grep"),
            Some("shell-enforce-preflight-hint")
        );
        assert_eq!(
            hook_failure_false_positive("ripgrep: command not found"),
            Some("ripgrep-enoent")
        );
        assert_eq!(
            hook_failure_false_positive(r"C:\rg.exe not found"),
            Some("rg-exe-enoent")
        );
        assert_eq!(
            hook_failure_false_positive("建議用 ripgrep 取代 find"),
            Some("ripgrep-replaces-find")
        );
        assert_eq!(
            hook_failure_false_positive("建議用 ripgrep 取代 grep"),
            Some("ripgrep-replaces-grep")
        );
    }

    /// R9 regression：合法 hook 錯誤（沒撞 5 條子字串）必須 None，否則會誤吞真失敗
    #[test]
    fn hook_false_positive_returns_none_for_legit_errors() {
        assert_eq!(hook_failure_false_positive(""), None);
        assert_eq!(hook_failure_false_positive("ENOENT: file not found"), None);
        assert_eq!(hook_failure_false_positive("permission denied"), None);
        // 裸 "rg" 不算 — 必須 "rg.exe" 或 "ripgrep" 才算誤報 pattern
        assert_eq!(hook_failure_false_positive("rg failed"), None);
        // "取代" 但沒 "find"/"grep" 也不算
        assert_eq!(hook_failure_false_positive("取代舊工具"), None);
    }

    /// R9 regression：const 順序固定，log 端能依賴 pattern name 而非 index
    #[test]
    fn hook_false_positive_pattern_names_are_stable() {
        let names: Vec<&str> = HOOK_FAILURE_FALSE_POSITIVE_PATTERNS
            .iter()
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(
            names,
            vec![
                "shell-enforce-preflight-hint",
                "ripgrep-replaces-find",
                "ripgrep-replaces-grep",
                "ripgrep-enoent",
                "rg-exe-enoent",
            ]
        );
    }

    /// Regression：純 toast 模式（無 Discord 成功路徑）下，dedup marker 必須在「決定要 fire」
    /// 時立即設，否則 15s tick 會反覆進入 send 區段。
    #[test]
    fn summary_dedup_marks_before_send_pure_toast_mode() {
        let mut state = AutoRuleState::default();
        // 第一次：未標記 → 新 fire、回傳 true、欄位被設
        assert!(mark_summary_fired_if_new(
            &mut state,
            SummaryMarker::Daily,
            "2026-06-01"
        ));
        assert_eq!(state.last_summary_date, "2026-06-01");

        // 第二次（同日、純 toast 模式 tick 第二次）：已是同 marker → 跳過、回傳 false
        // 修正前這條會走「send_toast 第二次」——bug
        assert!(!mark_summary_fired_if_new(
            &mut state,
            SummaryMarker::Daily,
            "2026-06-01"
        ));
        assert_eq!(state.last_summary_date, "2026-06-01");
    }

    #[test]
    fn weekly_dedup_marks_before_send_pure_toast_mode() {
        let mut state = AutoRuleState::default();
        assert!(mark_summary_fired_if_new(
            &mut state,
            SummaryMarker::Weekly,
            "2026-W22"
        ));
        assert_eq!(state.last_weekly_key, "2026-W22");
        // 第二次 tick（純 toast 模式）：應跳過
        assert!(!mark_summary_fired_if_new(
            &mut state,
            SummaryMarker::Weekly,
            "2026-W22"
        ));
    }

    /// Daily 與 Weekly marker 互相獨立——daily 標過不影響 weekly
    #[test]
    fn daily_and_weekly_markers_are_independent() {
        let mut state = AutoRuleState::default();
        mark_summary_fired_if_new(&mut state, SummaryMarker::Daily, "2026-06-01");
        assert!(mark_summary_fired_if_new(
            &mut state,
            SummaryMarker::Weekly,
            "2026-W22"
        ));
        // 隔天 daily 應該可以再 fire
        assert!(mark_summary_fired_if_new(
            &mut state,
            SummaryMarker::Daily,
            "2026-06-02"
        ));
    }

    /// Regression：純 toast 模式下，session_idle 同一個 idle 週期只 fire 一次。
    /// 修正前用 cfg.dedup_window_secs (5min)，會在 30min 觸發後每 5min 重發直到 session 結束。
    #[test]
    fn session_idle_dedup_uses_idle_period_anchor() {
        let mut state = AutoRuleState::default();
        // 第一次：未標記 → fire
        assert!(should_notify_session_idle(&mut state, "sess-A", 1000, 256));
        // 第二次：同 (sid, ts) → 跳過（同一個 idle 週期，純 toast 模式 15s tick 反覆跑就會到這）
        assert!(!should_notify_session_idle(&mut state, "sess-A", 1000, 256));
        assert!(!should_notify_session_idle(&mut state, "sess-A", 1000, 256));
        // 第三次：同 sid、不同 ts（session 變 active 後 last_event_time 推進）→ 允許再 fire
        assert!(should_notify_session_idle(&mut state, "sess-A", 2000, 256));
        // 第四次：又變 idle（ts 又不動）→ 同週期內不再 fire
        assert!(!should_notify_session_idle(&mut state, "sess-A", 2000, 256));
    }

    /// 不同 session 互不影響
    #[test]
    fn session_idle_dedup_per_session() {
        let mut state = AutoRuleState::default();
        assert!(should_notify_session_idle(&mut state, "sess-A", 1000, 256));
        assert!(should_notify_session_idle(&mut state, "sess-B", 1000, 256)); // 不同 sid 各自獨立
        assert!(!should_notify_session_idle(&mut state, "sess-A", 1000, 256));
        assert!(!should_notify_session_idle(&mut state, "sess-B", 1000, 256));
    }

    /// map 超過 max_entries 時 lazy GC：丟最舊 64 筆，避免 session 被外部移除後殘留撐大
    #[test]
    fn session_idle_dedup_lazy_gc() {
        let mut state = AutoRuleState::default();
        // 塞 300 筆不同 sid，ts 遞增
        for i in 0..300 {
            assert!(should_notify_session_idle(
                &mut state,
                &format!("s-{i}"),
                1000 + i as i64,
                256
            ));
        }
        // 觸發 GC 條件：> 256
        // 插入第 257 筆就會 GC，但這次 insert 之前 len 已經 300
        // 先驗證目前 map 大小
        assert!(state.last_session_idle_event_ts.len() <= 300);
        // 再插一筆，觸發 GC
        assert!(should_notify_session_idle(&mut state, "trigger", 9999, 256));
        // GC 後 map 應該 < 300
        assert!(state.last_session_idle_event_ts.len() < 300);
    }

    // ─── summary marker 持久化測試 ───
    // 用 std::env::temp_dir() 開專屬 subdir（避免污染真 ~/.lobsterpulse/），
    // 測試結束靠 struct Drop 回收。

    struct TmpDir(PathBuf);

    impl TmpDir {
        fn new(tag: &str) -> Self {
            use std::time::{SystemTime, UNIX_EPOCH};
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!("lp-auto-rules-test-{tag}-{nanos}"));
            std::fs::create_dir_all(&path).expect("create tmpdir");
            Self(path)
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 寫入後能原樣讀回——驗證 JSON round-trip + payload 對應欄位
    #[test]
    fn persist_then_load_round_trip() {
        let tmp = TmpDir::new("roundtrip");
        persist_summary_markers_at(&tmp.0, "2026-06-01", "2026-W22").expect("persist ok");
        let data = std::fs::read_to_string(tmp.0.join("auto_state.json")).expect("read file");
        // 直接驗 JSON 內容（不依賴 load_persisted_summary_markers，那條路讀真 home）
        let parsed: PersistedSummaryMarkers = serde_json::from_str(&data).expect("parse ok");
        assert_eq!(parsed.last_summary_date, "2026-06-01");
        assert_eq!(parsed.last_weekly_key, "2026-W22");
    }

    /// 父目錄不存在時 persist 會自己 mkdir——避免首次寫入因目錄缺失敗
    #[test]
    fn persist_creates_parent_dir() {
        let tmp = TmpDir::new("mkdir");
        // tmp 已存在；多挖一層 subdir 確認 mkdir 走到
        let nested = tmp.0.join("nested");
        assert!(!nested.exists());
        persist_summary_markers_at(&nested, "2026-06-01", "").expect("persist ok");
        assert!(nested.join("auto_state.json").exists());
    }

    /// 持久化後的 marker 餵回 AutoRuleState，隔天能再 fire、當天擋——重啟 dedup 行為正確
    #[test]
    fn persisted_markers_dedup_across_simulated_restart() {
        let tmp = TmpDir::new("restart");

        // 第一次啟動：daily 觸發、persist
        let mut state = AutoRuleState::default();
        assert!(mark_summary_fired_if_new(
            &mut state,
            SummaryMarker::Daily,
            "2026-06-01"
        ));
        persist_summary_markers_at(&tmp.0, &state.last_summary_date, &state.last_weekly_key)
            .expect("persist");

        // 模擬重啟：開新 state 餵回持久化值
        let data = std::fs::read_to_string(tmp.0.join("auto_state.json")).expect("read");
        let restored: PersistedSummaryMarkers = serde_json::from_str(&data).expect("parse");
        let mut state2 = AutoRuleState {
            last_summary_date: restored.last_summary_date,
            last_weekly_key: restored.last_weekly_key,
            ..AutoRuleState::default()
        };

        // 同日重啟：dedup 應擋下，不再 fire
        assert!(!mark_summary_fired_if_new(
            &mut state2,
            SummaryMarker::Daily,
            "2026-06-01"
        ));
        // 隔天：可再 fire
        assert!(mark_summary_fired_if_new(
            &mut state2,
            SummaryMarker::Daily,
            "2026-06-02"
        ));
    }

    /// 寫到不可寫目錄（這裡用「path 本身是檔案當 parent」模擬）應回 Err，不 panic
    #[test]
    fn persist_to_invalid_path_returns_err() {
        let tmp = TmpDir::new("invalid");
        // 把 parent 換成一個「是檔案不是目錄」的位置 → create_dir_all 會失敗
        let blocker = tmp.0.join("blocker");
        std::fs::write(&blocker, "not a dir").expect("write blocker");
        let invalid = blocker.join("auto_state.json"); // blocker 是檔案，不能在其下 mkdir
        let result = persist_summary_markers_in(&invalid, "x", "y");
        assert!(result.is_err());
    }

    // ─── session_idle marker 持久化 ───
    //
    // R16 補：沿用 R5 `auto_state.json` 同檔共存，read-modify-write 保留 summary marker。
    // 測試重點：(1) round-trip 不丟資料 (2) 檔案缺 → 空 HashMap（行為等同 in-memory default）

    /// round-trip 寫入 + 讀回不丟失；空 map / 多 entry 都要對
    #[test]
    fn session_idle_markers_round_trip_preserves_all_entries() {
        let tmp = TmpDir::new("sidle_rt");
        let mut markers = HashMap::new();
        markers.insert("sess-A".into(), 1700000000);
        markers.insert("sess-B".into(), 1700001234);
        markers.insert("sess-C".into(), 1_700_005_000);

        persist_session_idle_markers_in(&tmp.0.join("auto_state.json"), &markers)
            .expect("persist ok");

        let loaded = load_persisted_session_idle_markers_at(&tmp.0);
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded.get("sess-A"), Some(&1700000000));
        assert_eq!(loaded.get("sess-B"), Some(&1700001234));
        assert_eq!(loaded.get("sess-C"), Some(&1_700_005_000));
    }

    /// session_idle 寫入時，summary marker 不被覆蓋——驗 read-modify-write 行為
    #[test]
    fn persist_session_idle_preserves_summary_markers_in_same_file() {
        let tmp = TmpDir::new("sidle_preserve");
        // 先寫 summary marker
        persist_summary_markers_in(&tmp.0.join("auto_state.json"), "2026-06-01", "2026-W22")
            .expect("summary persist ok");
        // 再寫 session_idle marker
        let mut markers = HashMap::new();
        markers.insert("sess-X".into(), 1700000000);
        persist_session_idle_markers_in(&tmp.0.join("auto_state.json"), &markers)
            .expect("session_idle persist ok");

        // 從同一檔讀回，summary + session_idle 都要在
        let (date, week) = load_persisted_summary_markers_at(&tmp.0);
        assert_eq!(date, "2026-06-01");
        assert_eq!(week, "2026-W22");
        let sidle = load_persisted_session_idle_markers_at(&tmp.0);
        assert_eq!(sidle.get("sess-X"), Some(&1700000000));
    }

    // ─── R25 auto_state.json 載入 silent parse surfaced ───
    //
    // 修前 4 處：`load_persisted_summary_markers_at_impl` / `load_persisted_session_idle_markers_at_impl`
    // 跟兩個 `persist_*_in` 的 RMW 都用 `serde_json::from_str(...).unwrap_or_default()` 跟
    // `read_to_string(...).ok().and_then(from_str).ok().unwrap_or_default()` 鏈 silent 吞 error。
    // auto_state.json 損壞（磁碟寫入半截 / 手動編輯壞 JSON）會回 default，dedup state 漂移
    // operator 無從查起。測試重點：(1) missing 走 default 不 log (2) corrupt JSON 走 log::warn
    // 帶 preview (3) RMW 壞 JSON 寫回不丟已存在的同檔 marker (4) prefix 鎖住穩定供 log filter

    /// R25 regression：`persisted_marker_warn_msg` 對齊 R6 `discord_err_msg` / R23
    /// `config_persist_warn_msg` prefix 風格，log filter 一條 query 可抓出所有
    /// 「持久化 markers 載入失敗」事件。
    #[test]
    fn r25_persisted_marker_warn_msg_unifies_prefix() {
        assert_eq!(
            persisted_marker_warn_msg("invalid JSON (expected value at line 1 column 1); preview=\"\""),
            "[auto_rules] auto_state markers load failed: invalid JSON (expected value at line 1 column 1); preview=\"\""
        );
        // IO error 也應原樣保留
        assert_eq!(
            persisted_marker_warn_msg("Permission denied (os error 5)"),
            "[auto_rules] auto_state markers load failed: Permission denied (os error 5)"
        );
    }

    /// First-run 預期：檔案不存在時 parse helper 靜默回 default，不 log warn
    /// （避免啟動時每個 rule 都 spam 一次 log）
    #[test]
    fn r25_parse_missing_file_returns_default_no_log() {
        let tmp = TmpDir::new("parse_missing");
        let path = tmp.0.join("auto_state.json");
        assert!(!path.exists());
        let parsed = parse_persisted_markers_at(&path);
        assert_eq!(parsed.last_summary_date, "");
        assert_eq!(parsed.last_weekly_key, "");
        assert!(parsed.last_session_idle_event_ts.is_empty());
    }

    /// 壞 JSON 走 log::warn 帶 80 字元 preview + 回 default
    /// —— operator 排查時能看出檔案被破壞成什麼樣
    #[test]
    fn r25_parse_corrupt_json_logs_warn_and_returns_default() {
        let tmp = TmpDir::new("parse_corrupt");
        let path = tmp.0.join("auto_state.json");
        std::fs::write(&path, "this is not valid JSON {{ broken").expect("write corrupt");
        let parsed = parse_persisted_markers_at(&path);
        assert_eq!(parsed.last_summary_date, "");
        assert!(parsed.last_session_idle_event_ts.is_empty());
    }

    /// RMW 整合：壞 JSON 寫回時 helper log warn 但不丟掉「同檔其他 markers」——
    /// 等等，這條邏輯反過來：壞 JSON 進來 helper 回 default，然後 persist 用 default payload
    /// 寫回去，**會覆蓋掉原本 valid 的 summary marker**。這是現有 RMW 行為，跟 R16 同檔共存
    /// 設計保持一致（壞檔救不回，寧可重發也不要 silent 漂移）。本測試鎖定「壞 JSON 寫入
    /// 不 silent 漂移 + helper 走 warn」+ 「valid JSON 寫入保留 summary marker」兩個路徑。
    #[test]
    fn r25_persist_session_idle_over_corrupt_json_logs_warn_and_overwrites() {
        let tmp = TmpDir::new("rmw_corrupt");
        let path = tmp.0.join("auto_state.json");
        std::fs::write(&path, "garbage {{ not json").expect("write corrupt");

        // RMW 走 helper 看到壞 JSON → log warn + 用 default payload
        // 然後寫入 session_idle marker。下一輪 load 應能讀回（剛寫的 valid JSON）
        let mut markers = HashMap::new();
        markers.insert("sess-Y".into(), 1700000000);
        persist_session_idle_markers_in(&path, &markers).expect("persist ok");

        // 寫回後檔案是 valid JSON，loader 直接讀得到
        let loaded = load_persisted_session_idle_markers_at(&tmp.0);
        assert_eq!(loaded.get("sess-Y"), Some(&1700000000));
        // summary marker 沒出現在檔裡（壞 JSON 沒救回，這是 R16 設計的一致選擇）
        let (date, week) = load_persisted_summary_markers_at(&tmp.0);
        assert_eq!(date, "");
        assert_eq!(week, "");
    }

    // ─── R30 silent-fail surfacing: load_local_usage_snapshot_at ───
    //
    // 修前 `!lp quota` 路徑兩條 silent chain:
    //   - `read_to_string(&path).unwrap_or_default()` 吞 IO 錯誤
    //   - `from_str(&data).unwrap_or_default()` 吞壞 JSON
    // 結果: usage-local.json 損壞（CLI 中斷寫入 / 磁碟滿 / 手動編輯）時 Discord 使用者
    // 點 `!lp quota` 看到「無 runner」訊息,operator 完全無 log 可查。
    //
    // 修後 `load_local_usage_snapshot_at(path) -> Result<Option<Value>, String>`
    // 三條分流: NotFound 靜默回 Ok(None) / IO 錯 Err / parse 錯 Err,
    // caller 端 `match` 顯式分流 + log warn。

    #[test]
    fn load_local_usage_snapshot_at_missing_returns_ok_none() {
        // first-run 預期: 檔不存在不要 spam log, 直接 Ok(None) 讓 caller 走
        // 「無 runner 結果，usage-local.json 不存在」訊息。
        let tmp = TmpDir::new("quota-missing");
        let path = tmp.0.join("usage-local.json");
        assert!(!path.exists());
        let r = load_local_usage_snapshot_at(&path);
        assert!(
            matches!(r, Ok(None)),
            "first-run 應回 Ok(None), 實際: {r:?}"
        );
    }

    #[test]
    fn load_local_usage_snapshot_at_valid_returns_some() {
        // happy path: valid JSON → Ok(Some(Value)) 帶 runners array,
        // caller 端後續按原本 render 邏輯組訊息。
        let tmp = TmpDir::new("quota-valid");
        let path = tmp.0.join("usage-local.json");
        let payload = serde_json::json!({
            "runners": [
                {"label": "claude", "text": "剩 42%"},
                {"label": "codex", "text": "剩 7%"}
            ]
        });
        std::fs::write(&path, payload.to_string()).expect("write ok");
        let v = load_local_usage_snapshot_at(&path).expect("ok");
        let v = v.expect("some");
        let runners = v.get("runners").and_then(|r| r.as_array()).expect("arr");
        assert_eq!(runners.len(), 2);
        assert_eq!(
            runners[0].get("label").and_then(|x| x.as_str()),
            Some("claude")
        );
    }

    #[test]
    fn load_local_usage_snapshot_at_corrupt_json_returns_err() {
        // 對齊 R29 `parse_quota_history_row` contract: 壞 JSON 必須 Err,
        // 不能 unwrap_or_default() 變成空 Value 污染 render。修前 silent
        // chain 會把半截 JSON 當空 Value,呼叫端「無 runner」訊息完全看不出
        // 「是 usage-local.json 壞掉」,operator 一行 grep 都找不到。
        let tmp = TmpDir::new("quota-corrupt");
        let path = tmp.0.join("usage-local.json");
        // 半截 JSON (模擬磁碟寫入中斷 / OOM kill)
        let bad = br#"{"runners": [{"label": "claude","#;
        std::fs::write(&path, bad).expect("write bad");
        let r = load_local_usage_snapshot_at(&path);
        assert!(
            matches!(r, Err(_)),
            "壞 JSON 應回 Err 讓 caller log warn, 實際: {r:?}"
        );
    }

    #[test]
    fn load_local_usage_snapshot_at_io_error_returns_err() {
        // 對齊 R28 `load_config_at_io_error_warns_and_returns_default` pattern:
        // NotFound 以外 IO 錯（權限拒絕 / 磁碟鎖住 / NUL 路徑）必須 Err,
        // 不能默默當 NotFound 處理。
        //
        // 用 NUL 路徑是跨平台拒絕 IO 的最小依賴:
        // - Windows: 路徑含 NUL 直接拒絕
        // - Unix: 大多數 fs 拒絕
        // 跟 config.rs R28 同樣,只驗「不 panic + 回 Err」,不鎖特定 kind。
        let bad_path = std::path::Path::new("\x00not-a-real-path\x00");
        let r = load_local_usage_snapshot_at(bad_path);
        assert!(r.is_err(), "NUL 路徑應回 Err（IO 失敗），實際: {r:?}");
    }
}
