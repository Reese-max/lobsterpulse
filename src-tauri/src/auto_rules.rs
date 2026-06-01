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
use std::collections::HashMap;
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
                    let _ = discord::send_embed(
                        notify.discord_token,
                        notify.discord_channel,
                        &title,
                        &desc,
                        0xFFA500,
                    );
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
            let key = format!("session_idle:{provider}:{sid_short8}");
            let already_pending = {
                let s = state.lock().unwrap();
                s.pending_confirms.iter().any(|p| p.session_id == sid)
            };
            if already_pending {
                continue;
            }
            let fire = {
                let mut s = state.lock().unwrap();
                dedup_gate(&mut s, &key, cfg.dedup_window_secs)
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
                if let Ok(mid) =
                    discord::send_message(notify.discord_token, notify.discord_channel, &content)
                {
                    let _ = discord::add_reaction(
                        notify.discord_token,
                        notify.discord_channel,
                        &mid,
                        "✅",
                    );
                    let _ = discord::add_reaction(
                        notify.discord_token,
                        notify.discord_channel,
                        &mid,
                        "❌",
                    );
                    state.lock().unwrap().pending_confirms.push(PendingConfirm {
                        kind: "session_idle",
                        session_id: sid.clone(),
                        message_id: mid,
                        posted_at: Instant::now(),
                    });
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
                    let _ = discord::send_message(
                        notify.discord_token,
                        notify.discord_channel,
                        &format!(
                            "⏲️ {} 分鐘無人回應，取消 `{}` 的 kill",
                            cfg.confirm_timeout_secs / 60,
                            sid_short
                        ),
                    );
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
                let _ = discord::send_message(
                    notify.discord_token,
                    notify.discord_channel,
                    &format!("✅ 已 kill session `{}`（來源：{}）", sid_short, p.kind),
                );
                resolved_ids.push(p.message_id.clone());
            } else if no {
                let _ = discord::send_message(
                    notify.discord_token,
                    notify.discord_channel,
                    &format!("❌ 取消 session `{}`（來源：{}）", sid_short, p.kind),
                );
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
                    if err_s.contains("請用")          // /c/Users/*/.claude/hooks/enforce-shell-tools.sh 用的前置提示
                        || err_s.contains("ripgrep")   // rg 被 attack 時 ENOENT
                        || err_s.contains("rg.exe")
                        || err_s.contains("取代 find")
                        || err_s.contains("取代 grep")
                    {
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
                let _ = discord::send_embed(
                    notify.discord_token,
                    notify.discord_channel,
                    &title,
                    &desc,
                    0xFF0000,
                );
            }
            if toast_active {
                if let Some(a) = &notify.app {
                    send_toast(a, &title, &desc);
                }
            }

            // 觸發 openab 重啟
            if !openab_restart_command.trim().is_empty() {
                let _ = std::process::Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-Command",
                        "Stop-Process -Name openab -Force -ErrorAction SilentlyContinue",
                    ])
                    .spawn();
                let _ = std::process::Command::new("powershell.exe")
                    .args(["-NoProfile", "-Command", openab_restart_command])
                    .spawn();
                let _ = discord::send_message(
                    notify.discord_token,
                    notify.discord_channel,
                    "🔄 已觸發 OpenAB 重啟",
                );
            }
        }
    }

    // ─── Rule 4: daily_summary ───
    // 本地時區當天 == summary_hour 時就推一次，每日 1 次（last_summary_date 擋重複）。
    if cfg.daily_summary_enabled {
        let now_local = chrono::Local::now();
        let today = now_local.format("%Y-%m-%d").to_string();
        let should_fire = {
            let s = state.lock().unwrap();
            now_local.hour() as u8 == cfg.daily_summary_hour && s.last_summary_date != today
        };
        if should_fire {
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
            if discord::send_embed(
                notify.discord_token,
                notify.discord_channel,
                &title,
                &desc,
                0x4169E1,
            )
            .is_ok()
            {
                state.lock().unwrap().last_summary_date = today;
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
            if discord::send_embed(
                notify.discord_token,
                notify.discord_channel,
                &title,
                &desc,
                0x9370DB,
            )
            .is_ok()
            {
                state.lock().unwrap().last_weekly_key = wkey;
            }
        }
    }

    // ─── Rule 6: token_spike — 當日消耗 > N× 近 7 日均值 → 告警 ───
    if cfg.token_spike_enabled {
        if let Ok(hist) = crate::quota_history::load_history() {
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
                    let _ = discord::send_embed(
                        notify.discord_token,
                        notify.discord_channel,
                        &title,
                        &desc,
                        0xFF6B6B,
                    );
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
        Err(_) => return,
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
            if let Ok(mid) = discord::send_message(token, channel, &content) {
                let _ = discord::add_reaction(token, channel, &mid, "✅");
                let _ = discord::add_reaction(token, channel, &mid, "❌");
                state.lock().unwrap().pending_confirms.push(PendingConfirm {
                    kind: "discord_kill_cmd",
                    session_id: sid.to_string(),
                    message_id: mid,
                    posted_at: Instant::now(),
                });
            }
        } else {
            let _ = discord::send_message(token, channel, &reply);
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
            let Some(home) = dirs::home_dir() else {
                return "no home dir".into();
            };
            let path = home.join(".lobsterpulse").join("usage-local.json");
            let data = std::fs::read_to_string(&path).unwrap_or_default();
            let v: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            let runners = v
                .get("runners")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default();
            if runners.is_empty() {
                return "（沒有 runner 結果，usage-local.json 不存在或為空）".into();
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
            let _ = crate::config::save_config(&c);
            "自動化 **暫停**（`!lp resume` 恢復）".into()
        }

        "resume" => {
            let mut c = config.lock().unwrap();
            c.appearance.auto_actions.master_enabled = true;
            let _ = crate::config::save_config(&c);
            "自動化 **恢復**".into()
        }

        "trend" => {
            if arg.is_empty() {
                return "用法：`!lp trend <runner>`（runner 名稱如 claude / codex / copilot / gemini）".into();
            }
            let hist = crate::quota_history::load_history().unwrap_or_default();
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
}
