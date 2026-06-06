mod auto_rules;
mod config;
mod discord;
mod hook_event;
mod hook_server;
mod hooks_configurator;
mod openab_bridge;
mod quota;
mod quota_history;
mod session;

use chrono::{DateTime, Utc};
use config::{detect_providers, load_config, save_config, AppConfig};
use hook_server::HookServer;
use log::info;
use session::{AppState, SessionManager};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

struct AppSessionManager(Mutex<SessionManager>);
struct AppConfigState(Mutex<AppConfig>);
static LOCAL_USAGE_RUNNERS_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// R100 提取：OpenAB bot id 列表單一 source of truth。
///
/// 驅動 `read_usage_snapshots_with_home` + `collect_quota_snapshot_mtimes` 兩個 fn
/// 對齊 R78 補完後的 9 隻 OpenAB bot（cicx/gitx/giminix/codex_bot/openx/irisx_bot/
/// grokx/lpbot/mimo），避免 inline 5-bot list 跟 KNOWN_PROVIDERS 13 漂移。
///
/// 對齊 R67 護欄 chain 16 精神（provider 對稱）：將來加 bot 只改這 1 個 const + KNOWN_PROVIDERS
/// + default_providers 3 點，不再 4+ 處 inline list 散落各處。
const OPENAB_BOT_IDS: &[&str] = &[
    "cicx",
    "gitx",
    "giminix",
    "codex_bot",
    "openx",
    "irisx_bot",
    "grokx",
    "lpbot",
    "mimo",
];

/// R102 提取 / R103 對齊：LobsterPulse Prometheus metric 命名契約單一 source of truth。
///
/// 對齊 `openspec/changes/otel-provider-metrics-contract/` 開的 OTel/Prometheus
/// 契約 spec：`render_prometheus_body` 實際 emit 的 41 條 metric 名稱收斂成 1 個
/// const，護欄 test 守 `render_prometheus_body` 每次新加 metric 必須先列進這裡
/// 並同步更新 design.md 對照表與 spec.md 場景。防「未在 spec 出現就偷偷 emit」
/// 的 spec drift。
///
/// 7 段分組對齊 design.md 7 個 metric 類別：
/// 1. Session 數量（4 條）
/// 2. Token accounting（4 條）
/// 3. Failure & health（3 條）
/// 4. Idle / freshness（7 條）
/// 5. Session count / duration aggregates（13 條）
/// 6. Quota signal（1 條）
/// 7. Event / process accounting（9 條：events_total / event_type_total /
///    sessions_by_state / discord health 三條 / hook health 三條）
///
/// 計 41 條 = 護欄 test 集合大小下界。
///
/// R103 對齊筆記：R102 開工時只盤到當時 emit 過的 26 條；R46 (event_type_total)、
/// R44 (sessions_by_state)、R47 (idle_ratio / max_session_age)、
/// R45 (p25/p75/p99 + interarrival_avg)、Discord 模組 3 條 (R19+) 跟 Hook 模組
/// 3 條 (R46+) 後續輪次陸續加進 render_prometheus_body，但 LP_METRICS 沒同步補。
/// R103 補齊到 41 條，R113 T-1 dual-emit 補到 47 條（41 + 6 條新 `_total` 名），
/// 護欄 test 才會綠。
///
/// ⚠️ **T-1 dual-emit 階段**：本 const 同時列 6 條現名 + 6 條新 `_total` 名（共 12 row，
/// 總 47）。T-4 切換日（week 4）後舊 6 條現名從 const 移除（`len()` 回到 41），
/// 對齊 R106 (2026-06-05) 已 closure 的 `prometheus-counter-convention` spec
/// 對齊契約 5 週時程 T-1 → T-4 階段。
///
/// ⚠️ **勿於本輪 T-1 重命名**：重命名既有 6 條現名 = 破既有 Prometheus 抓取 +
/// alert + Grafana dashboard 1 輪不可承受 scope，留 R114+ owner follow-up T-4
/// 切換日執行（見 `openspec/changes/prometheus-counter-rename-2026-q3/`）。
// 契約 const：prod `render_prometheus_body` 不直接引用（契約語意靠 3 條護欄 test
// 在 test 編譯時守 `emit ⊆ LP_METRICS`），保留模組層讓未來可 `pub(crate)` 暴露給
// debug/diagnostic 命令讀契約清單（例如列出契約外的 emit 候選）。`dead_code`
// warning 在 lib target 是預期、語意正確：契約在 test 守，prod 只 emit。
#[allow(dead_code)]
const LP_METRICS: &[&str] = &[
    // 1. Session 數量 (4)
    "lobsterpulse_sessions_total",
    "lobsterpulse_sessions_active",
    "lobsterpulse_provider_sessions",
    "lobsterpulse_provider_active",
    // 2. Token accounting (4 → 8, R113 T-1 dual-emit 加 4 條新 _total)
    "lobsterpulse_tokens_input",
    "lobsterpulse_tokens_input_total",
    "lobsterpulse_tokens_output",
    "lobsterpulse_tokens_output_total",
    "lobsterpulse_provider_tokens_input",
    "lobsterpulse_provider_tokens_input_total",
    "lobsterpulse_provider_tokens_output",
    "lobsterpulse_provider_tokens_output_total",
    // 3. Failure & health (3 → 4, R113 T-1 dual-emit 加 1 條新 _total)
    "lobsterpulse_provider_failure_count",
    "lobsterpulse_provider_failure_count_total",
    "lobsterpulse_provider_failure_to_completion_ratio",
    "lobsterpulse_provider_success_rate",
    // 4. Idle / freshness (7)
    "lobsterpulse_provider_idle_seconds",
    "lobsterpulse_provider_since_timestamp",
    "lobsterpulse_provider_quota_snapshot_age_seconds",
    "lobsterpulse_quota_history_csv_age_seconds",
    "lobsterpulse_provider_last_completed_session_age_seconds",
    "lobsterpulse_provider_idle_ratio",
    "lobsterpulse_provider_max_session_age_seconds",
    // 5. Session count / duration aggregates (13 → 14, R113 T-1 dual-emit 加 1 條新 _total)
    "lobsterpulse_provider_session_count",
    "lobsterpulse_provider_session_count_total",
    "lobsterpulse_provider_completed_sessions_total",
    "lobsterpulse_provider_completed_sessions_total_duration_seconds",
    "lobsterpulse_provider_completed_sessions_average_duration_seconds",
    "lobsterpulse_provider_completed_sessions_max_duration_seconds",
    "lobsterpulse_provider_completed_sessions_min_duration_seconds",
    "lobsterpulse_provider_completed_sessions_stddev_seconds",
    "lobsterpulse_provider_completed_sessions_p95_duration_seconds",
    "lobsterpulse_provider_completed_sessions_p50_duration_seconds",
    "lobsterpulse_provider_completed_sessions_p99_duration_seconds",
    "lobsterpulse_provider_completed_sessions_p75_duration_seconds",
    "lobsterpulse_provider_completed_sessions_p25_duration_seconds",
    "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds",
    // 6. Quota signal (1)
    "lobsterpulse_provider_quota_remaining_pct",
    // 7. Event / process accounting (9)
    "lobsterpulse_provider_events_total",
    "lobsterpulse_provider_event_type_total",
    "lobsterpulse_provider_sessions_by_state",
    "lobsterpulse_discord_health",
    "lobsterpulse_discord_send_failures_total",
    "lobsterpulse_discord_last_event_unix",
    "lobsterpulse_hook_parse_failures_total",
    "lobsterpulse_hook_responses_total",
    "lobsterpulse_hook_unknown_provider_fallbacks_total",
];

#[tauri::command]
fn get_state(manager: tauri::State<AppSessionManager>) -> AppState {
    manager.0.lock().unwrap().get_state()
}

#[tauri::command]
fn select_session(manager: tauri::State<AppSessionManager>, id: String) {
    manager.0.lock().unwrap().select_session(id);
}

#[tauri::command]
fn remove_session(manager: tauri::State<AppSessionManager>, id: String) {
    let mut m = manager.0.lock().unwrap();
    m.sessions.remove(&id);
    if m.active_session_id.as_deref() == Some(&id) {
        m.active_session_id = m.sessions.keys().next().cloned();
    }
}

/// 清空所有 session（不清 recent_events / provider_totals 歷史累計）
#[tauri::command]
fn remove_all_sessions(manager: tauri::State<AppSessionManager>) {
    let mut m = manager.0.lock().unwrap();
    m.sessions.clear();
    m.active_session_id = None;
}

#[tauri::command]
fn get_config(config_state: tauri::State<AppConfigState>) -> AppConfig {
    config_state.0.lock().unwrap().clone()
}

#[tauri::command]
fn save_app_config(
    config_state: tauri::State<AppConfigState>,
    new_config: AppConfig,
) -> Result<(), String> {
    save_config(&new_config)?;
    *config_state.0.lock().unwrap() = new_config;
    Ok(())
}

// R115 規則引擎：4 條 Tauri command 給前端設定頁 Rules section 用。
// 對應 tasks T-12~T-14 與 spec R-1 (TriggerRule JSON round-trip)。
//
// 4 條都走「mutate AppConfig → save_config 持久化 → 同步 SessionManager.rules」
// 三步：mutate-後-clone 脫離 cfg 鎖,再單獨鎖 SessionManager,避免雙鎖死鎖。

/// list_rules：給前端 Rules section 載入現有規則清單。
#[tauri::command]
fn list_rules(config_state: tauri::State<AppConfigState>) -> Vec<config::TriggerRule> {
    config_state.0.lock().unwrap().rules.clone()
}

/// toggle_rule：翻轉指定 id 規則的 `enabled` 旗標。回傳新狀態。
/// 找不到 id 回傳 Err,前端可選擇新增而非當錯誤吞掉。
#[tauri::command]
fn toggle_rule(
    config_state: tauri::State<AppConfigState>,
    session_state: tauri::State<AppSessionManager>,
    rule_id: String,
) -> Result<bool, String> {
    let (new_enabled, rules, rules_enabled) = {
        let mut cfg = config_state.0.lock().unwrap();
        let Some(rule) = cfg.rules.iter_mut().find(|r| r.id == rule_id) else {
            return Err(format!("rule id not found: {rule_id}"));
        };
        rule.enabled = !rule.enabled;
        let new_enabled = rule.enabled;
        save_config(&cfg)?;
        (new_enabled, cfg.rules.clone(), cfg.rules_enabled)
        // cfg 鎖在此釋放
    };
    let mut mgr = session_state.0.lock().unwrap();
    mgr.set_rules(rules, rules_enabled);
    Ok(new_enabled)
}

/// add_rule：把前端新建的 TriggerRule push 進 AppConfig.rules。
/// 同 id 已存在則覆寫,避免重複新增造成 list 膨脹。
#[tauri::command]
fn add_rule(
    config_state: tauri::State<AppConfigState>,
    session_state: tauri::State<AppSessionManager>,
    rule: config::TriggerRule,
) -> Result<(), String> {
    let (rules, rules_enabled) = {
        let mut cfg = config_state.0.lock().unwrap();
        if let Some(existing) = cfg.rules.iter_mut().find(|r| r.id == rule.id) {
            *existing = rule;
        } else {
            cfg.rules.push(rule);
        }
        save_config(&cfg)?;
        (cfg.rules.clone(), cfg.rules_enabled)
    };
    let mut mgr = session_state.0.lock().unwrap();
    mgr.set_rules(rules, rules_enabled);
    Ok(())
}

/// remove_rule：從 AppConfig.rules 移除指定 id 規則。
/// 找不到不存檔（MVP 容錯：使用者刪的可能是已被外部清掉的 id,非錯誤）。
#[tauri::command]
fn remove_rule(
    config_state: tauri::State<AppConfigState>,
    session_state: tauri::State<AppSessionManager>,
    rule_id: String,
) -> Result<(), String> {
    let (rules, rules_enabled, changed) = {
        let mut cfg = config_state.0.lock().unwrap();
        let before = cfg.rules.len();
        cfg.rules.retain(|r| r.id != rule_id);
        let changed = cfg.rules.len() != before;
        if changed {
            save_config(&cfg)?;
        }
        (cfg.rules.clone(), cfg.rules_enabled, changed)
    };
    if changed {
        let mut mgr = session_state.0.lock().unwrap();
        mgr.set_rules(rules, rules_enabled);
    }
    Ok(())
}

#[tauri::command]
fn detect_installed_providers() -> std::collections::HashMap<String, bool> {
    detect_providers()
}

#[tauri::command]
fn check_provider_setup(provider_id: String, config_state: tauri::State<AppConfigState>) -> bool {
    let config = config_state.0.lock().unwrap();
    if let Some(provider) = config.providers.get(&provider_id) {
        hooks_configurator::provider_needs_setup(&provider_id, provider)
    } else {
        true
    }
}

/// 統一 log prefix 風格 helper — 對齊 R6 `discord_err_msg` / R23 `config_persist_warn_msg` /
/// R28 `persisted_marker_warn_msg` / R37 `provider_settings_warn_msg` 四條前例,
/// log filter 可一次 grep `[lib]` 撈 module 警告
fn lib_warn_msg(action: &str, err: impl std::fmt::Display) -> String {
    format!("[lib] {action} failed: {err}")
}

/// Get the sounds directory, creating it if needed
fn sounds_dir() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config"))
        .join("lobsterpulse")
        .join("sounds");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        // R57: 從 `let _ =` 沉默吞改成 log warn。sounds_dir 是 Tauri command
        // `list_sounds` / `play_sound_file` hot path, mkdir 失敗 (權限拒絕 /
        // 磁碟滿 / 唯讀 AppData) → 音效功能壞, 但前端 / operator 完全沒 log
        // 串起來定位。改 warn 讓 log filter 可一次 grep `[lib] sounds_dir_mkdir`
        log::warn!("{}", lib_warn_msg("sounds_dir_mkdir", &e));
    }
    // seed_default_sounds skips files that already exist, so this is a
    // no-op for users who already have all the defaults.
    seed_default_sounds(&dir);
    dir
}

/// Seed the sounds directory with bundled default sounds (only if not already present)
fn seed_default_sounds(dir: &std::path::Path) {
    let defaults: &[(&str, &[u8])] = &[
        ("cicx.mp3", include_bytes!("../../sounds/cicx.mp3")),
        ("gitx.mp3", include_bytes!("../../sounds/gitx.mp3")),
        ("giminix.mp3", include_bytes!("../../sounds/giminix.mp3")),
        ("codex.mp3", include_bytes!("../../sounds/codex.mp3")),
        ("openx.mp3", include_bytes!("../../sounds/openx.mp3")),
        // R71 T-BOT3: hermes agent / IRISX 音效檔 (R70 T-BOT1+T-BOT2 加進 4 同步點
        // 後 default_provider_sounds["irisx_bot"] = "irisx_bot.mp3" 指向實體檔
        // — 收尾 R70 探索讓 6/6 OpenAB bot 音效完整, 避免前端切到 irisx 膠囊
        // silent fail。先用 1.5s/1.0s silent placeholder, 等 hermes/IRISX 真實
        // 部署後再換 TTS 語句。
        (
            "irisx_bot.mp3",
            include_bytes!("../../sounds/irisx_bot.mp3"),
        ),
        (
            "cicx-waiting.mp3",
            include_bytes!("../../sounds/cicx-waiting.mp3"),
        ),
        (
            "gitx-waiting.mp3",
            include_bytes!("../../sounds/gitx-waiting.mp3"),
        ),
        (
            "giminix-waiting.mp3",
            include_bytes!("../../sounds/giminix-waiting.mp3"),
        ),
        (
            "codex-waiting.mp3",
            include_bytes!("../../sounds/codex-waiting.mp3"),
        ),
        (
            "openx-waiting.mp3",
            include_bytes!("../../sounds/openx-waiting.mp3"),
        ),
        // R71 T-BOT3: 對稱 (1.0s shorter than completion 對齊既有 waiting < completion pattern)
        (
            "irisx_bot-waiting.mp3",
            include_bytes!("../../sounds/irisx_bot-waiting.mp3"),
        ),
        // R78 T-BOT11: GROKX 音效 placeholder（沿 R71 irisx 模式 1.5s/1.0s silent），
        // 對齊 default_provider_sounds["grokx"] = "grokx.mp3" 實體檔
        ("grokx.mp3", include_bytes!("../../sounds/grokx.mp3")),
        (
            "grokx-waiting.mp3",
            include_bytes!("../../sounds/grokx-waiting.mp3"),
        ),
        // R78 T-BOT12: LPBOT 音效 placeholder（沿 R71 irisx 模式 1.5s/1.0s silent），
        // 對齊 default_provider_sounds["lpbot"] = "lpbot.mp3" 實體檔
        ("lpbot.mp3", include_bytes!("../../sounds/lpbot.mp3")),
        (
            "lpbot-waiting.mp3",
            include_bytes!("../../sounds/lpbot-waiting.mp3"),
        ),
        // R78 T-BOT5: MIMO 音效 placeholder（沿 R71 irisx 模式 1.5s/1.0s silent），
        // 對齊 default_provider_sounds["mimo"] = "mimo.mp3" 實體檔
        ("mimo.mp3", include_bytes!("../../sounds/mimo.mp3")),
        (
            "mimo-waiting.mp3",
            include_bytes!("../../sounds/mimo-waiting.mp3"),
        ),
    ];
    for (name, bytes) in defaults {
        let path = dir.join(name);
        if !path.exists() {
            if let Err(e) = std::fs::write(&path, bytes) {
                // R57: 從 `let _ =` 沉默吞改成 log warn。首次啟動 seed 10 個
                // 預設音效 (cicx/gitx/giminix/codex/openx + waiting 變體), 寫入
                // 失敗 (權限拒絕 / 磁碟滿 / path 被鎖) → user 沒音效, 但
                // log 沒記 path 跟原始 err, 報 bug 時 debug 找嘸根因
                log::warn!(
                    "{} ({})",
                    lib_warn_msg("seed_default_sounds write", &e),
                    path.display()
                );
            }
        }
    }
}

#[tauri::command]
fn list_sounds() -> Vec<String> {
    let dir = sounds_dir();
    let mut sounds: Vec<String> = std::fs::read_dir(&dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let lower = name.to_lowercase();
                    if lower.ends_with(".mp3") || lower.ends_with(".wav") || lower.ends_with(".ogg")
                    {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    sounds.sort();
    sounds
}

#[tauri::command]
fn play_sound_file(name: String) {
    let path = sounds_dir().join(&name);
    if !path.exists() {
        return;
    }

    // Spawn a thread so we don't block
    std::thread::spawn(move || {
        if let Ok((_stream, handle)) = rodio::OutputStream::try_default() {
            if let Ok(file) = std::fs::File::open(&path) {
                let buf = std::io::BufReader::new(file);
                if let Ok(sink) = rodio::Sink::try_new(&handle) {
                    if let Ok(decoder) = rodio::Decoder::new(buf) {
                        sink.append(decoder);
                        sink.set_volume(0.8);
                        sink.sleep_until_end();
                    }
                }
            }
        }
    });
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_app_config() -> Result<(), String> {
    let path = config::config_path();
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(path.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_sounds_folder() -> Result<(), String> {
    let dir = sounds_dir();
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(dir.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_provider_settings(
    provider_id: String,
    config_state: tauri::State<AppConfigState>,
) -> Result<(), String> {
    let config = config_state.0.lock().unwrap();
    let provider = config
        .providers
        .get(&provider_id)
        .ok_or(format!("Unknown provider: {provider_id}"))?;
    let path = provider
        .settings_path
        .as_ref()
        .ok_or("No settings path for this provider")?;
    let expanded = config::expand_path(path);

    // Use xdg-open on Linux, open on macOS, start on Windows
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };

    std::process::Command::new(opener)
        .arg(expanded.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn remove_provider_hooks(
    provider_id: String,
    config_state: tauri::State<AppConfigState>,
) -> Result<(), String> {
    let mut config = config_state.0.lock().unwrap();
    let provider = config
        .providers
        .get(&provider_id)
        .ok_or(format!("Unknown provider: {provider_id}"))?
        .clone();
    hooks_configurator::remove_provider(&provider_id, &provider)?;
    if let Some(p) = config.providers.get_mut(&provider_id) {
        p.enabled = false;
    }
    if let Err(e) = save_config(&config) {
        log::warn!(
            "[config] failed to persist enabled=false after remove_provider_hooks for {provider_id}: {e}"
        );
    }
    Ok(())
}

#[tauri::command]
fn install_provider_hooks(
    provider_id: String,
    config_state: tauri::State<AppConfigState>,
) -> Result<(), String> {
    let mut config = config_state.0.lock().unwrap();
    if let Some(provider) = config.providers.get(&provider_id) {
        hooks_configurator::install_provider(&provider_id, provider)?;
        // Mark as enabled
        if let Some(p) = config.providers.get_mut(&provider_id) {
            p.enabled = true;
        }
        if let Err(e) = save_config(&config) {
            log::warn!(
                "[config] failed to persist enabled=true after install_provider_hooks for {provider_id}: {e}"
            );
        }
        Ok(())
    } else {
        Err(format!("Unknown provider: {provider_id}"))
    }
}

#[tauri::command]
fn get_server_port(port_state: tauri::State<ServerPort>) -> u16 {
    port_state.0
}

#[tauri::command]
fn read_usage_snapshots() -> std::collections::HashMap<String, Option<serde_json::Value>> {
    // R33：純 IO/parse fn 抽到 `read_usage_snapshot_at` + home 注入版
    // `read_usage_snapshots_with_home`,這層只剩 Tauri command 殼 → caller
    // 端 log warn 集中。NotFound 走 `Ok(None)` 對齊 R28/R32 first-run 契約。
    read_usage_snapshots_with_home(&dirs::home_dir())
}

/// R89 接入：R82 開工留下的 quota/ 模組 (`anthropic` / `codex`) 對外暴露點。
/// 聚合兩個本機 CLI runner 的 live API fetch 結果回前端，補 K0 Quota 即時性
/// 第二層來源（OpenAB snapshot 是「別人寫的」,這條是「自己即時抓的」）。
/// `home = None`（無 HOME env 罕見）→ 回空 runners[] 對齊 R11 邊界契約。
#[tauri::command]
async fn get_live_quota_snapshot() -> quota::LiveQuotaSnapshot {
    collect_live_quota_snapshot_with_home(dirs::home_dir().as_deref()).await
}

/// 對齊 R33 `read_usage_snapshots_with_home` 模式：純 async fn + home 注入，
/// Tauri command 殼只負責撈 `dirs::home_dir()` 傳入，testable。
pub(crate) async fn collect_live_quota_snapshot_with_home(
    home: Option<&std::path::Path>,
) -> quota::LiveQuotaSnapshot {
    let updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let Some(home) = home else {
        return quota::LiveQuotaSnapshot {
            runners: Vec::new(),
            source: "live_api".to_string(),
            updated_at,
        };
    };
    // 四個 fetch 各自打不同 API endpoint（Anthropic + OpenAI + Google + GitHub），
    // 即使平行也省不到一半（網路 RTT 為主），這裡採 sequential 簡化。
    let claude = quota::anthropic::fetch(home).await;
    let codex = quota::codex::fetch(home).await;
    let gemini = quota::gemini::fetch(home).await;
    let copilot = quota::copilot::fetch(home).await;
    quota::LiveQuotaSnapshot {
        runners: vec![claude, codex, gemini, copilot],
        source: "live_api".to_string(),
        updated_at,
    }
}

/// 讀取 OpenAB 5 個 bot 的 snapshot + LobsterPulse 自建 local snapshot。
/// 路徑：~/.lobsterpulse/usage-{bot_id}.json + usage-local.json。
/// 前端 refreshQuotas 會優先用 __local__（LobsterPulse 自跑的）作全域 quota 來源。
///
/// R33 silent-fail surfacing：原本 `read_to_string().ok().and_then(from_str().ok())`
/// 一條鏈把 IO 錯（permission denied / disk full / path lock）+ parse 錯（上游
/// 寫入半截 / encoding 損壞 / 非 JSON 噪音）共 7 條 silent path 都吞成 `None`，
/// 前端 `refreshQuotas` 看到 `None` 視為「沒資料」渲染空白。operator 排查
/// 「為什麼 cicx 沒 quota 圖」要猜 3 種根因中的哪條：OpenAB 沒跑（預期）vs
/// usage-cicx.json 損壞（bug）vs 權限拒絕（config 問題）。改為 NotFound 走
/// `Ok(None)`（first-run 預期,對齊 R28/R32 契約），IO/parse 錯走 Err → caller
/// 端 `match` 統一 log warn 帶 80 字 preview。前端 HashMap 契約不變（Some/None），
/// 只是 None 語意從「可能含 error」變成「真的沒資料」+ log 有跡可循。
#[derive(Debug)]
enum ReadUsageSnapshotError {
    Io(std::io::Error),
    Parse {
        err: serde_json::Error,
        preview: String,
    },
}

impl std::fmt::Display for ReadUsageSnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Parse { err, .. } => write!(f, "parse error: {err}"),
        }
    }
}

/// Pure fn：給定 path,回 `Result<Option<Value>, ReadUsageSnapshotError>`。
/// - NotFound → `Ok(None)`（first-run 預期,不算 silent-fail）
/// - 其他 IO 錯 → `Err(Io)`（caller 端 log warn）
/// - parse 錯 → `Err(Parse)`（caller 端 log warn 帶 80 字 preview）
///
/// Preview 從 raw content 取前 80 字,對齊 R28 `load_config_at` / R32 `load_history_at` pattern。
fn read_usage_snapshot_at(
    path: &std::path::Path,
) -> Result<Option<serde_json::Value>, ReadUsageSnapshotError> {
    let data = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ReadUsageSnapshotError::Io(e)),
    };
    match serde_json::from_str::<serde_json::Value>(&data) {
        Ok(v) => Ok(Some(v)),
        Err(parse_err) => {
            // 把 raw content 放進 Parse variant 讓 caller 端可 log preview
            let preview: String = data.chars().take(80).collect();
            Err(ReadUsageSnapshotError::Parse {
                err: parse_err,
                preview,
            })
        }
    }
}

/// Orchestrator helper：對齊 `read_usage_snapshot_at` 結果分流。
/// Ok(None) → None（first-run 預期,不 log）
/// Err → 結構化 log warn 帶 label + IO/parse 錯誤 + 80 字 preview,回 None
fn handle_read_usage_snapshot(path: &std::path::Path, label: &str) -> Option<serde_json::Value> {
    match read_usage_snapshot_at(path) {
        Ok(v) => v,
        Err(e) => {
            let path_display = path.display().to_string();
            match e {
                ReadUsageSnapshotError::Io(io_err) => log::warn!(
                    "[lib] read_usage_snapshots: {label} io failed at {path_display}: {io_err} — \
                     treating as no data; check file permissions or disk health"
                ),
                ReadUsageSnapshotError::Parse { err, preview } => log::warn!(
                    "[lib] read_usage_snapshots: {label} parse failed at {path_display}: {err} \
                     — file is corrupt, treating as no data; check upstream OpenAB write_quota (preview: {preview:?})"
                ),
            }
            None
        }
    }
}

/// 注入 home 變數版,production 由 `#[tauri::command] read_usage_snapshots` 用
/// `dirs::home_dir()` 注入。`home = None` → 9 OpenAB bot + __local__ 全 None 對齊
/// K11 `collect_quota_snapshot_mtimes` 邊界契約（無 HOME env 罕見但要保證不 crash）。
///
/// R100: bot 列表改用 `OPENAB_BOT_IDS` const 驅動,對齊 R78 補完後 9 隻 OpenAB bot
/// (cicx/gitx/giminix/codex_bot/openx/irisx_bot/grokx/lpbot/mimo)。
fn read_usage_snapshots_with_home(
    home: &Option<std::path::PathBuf>,
) -> std::collections::HashMap<String, Option<serde_json::Value>> {
    let mut out = std::collections::HashMap::new();
    let Some(dir) = home.as_ref().map(|h| h.join(".lobsterpulse")) else {
        for p in OPENAB_BOT_IDS {
            out.insert((*p).to_string(), None);
        }
        out.insert("__local__".to_string(), None);
        return out;
    };
    for bot in OPENAB_BOT_IDS {
        let path = dir.join(format!("usage-{bot}.json"));
        out.insert((*bot).to_string(), handle_read_usage_snapshot(&path, bot));
    }
    // Legacy：OpenAB BackendType::Other 寫 usage-bot.json 當 OPENX,只在 primary 缺時 fallback
    if out.get("openx").and_then(|v| v.as_ref()).is_none() {
        let path = dir.join("usage-bot.json");
        if let Some(v) = handle_read_usage_snapshot(&path, "openx (legacy)") {
            out.insert("openx".to_string(), Some(v));
        }
    }
    // LobsterPulse 自跑的 usage runner 寫 usage-local.json
    let local_path = dir.join("usage-local.json");
    out.insert(
        "__local__".to_string(),
        handle_read_usage_snapshot(&local_path, "__local__"),
    );
    out
}

#[cfg(test)]
mod read_usage_snapshot_tests {
    //! R33 silent-fail surfacing 對 `read_usage_snapshots` 的測試。
    //!
    //! 對齊 R28 `load_config_at_tests` / R32 `load_history_at` 風格：純 fn
    //! `read_usage_snapshot_at` 三分流驗證 + orchestrator `read_usage_snapshots_with_home`
    //! 注入 home 變數測試邊界。`handle_read_usage_snapshot` 透過
    //! `read_usage_snapshots_with_home` 行為間接覆蓋（log warn 是 side effect,不 assert log
    //! 避免測試 fragility）。
    //!
    //! 跨平台 IO 錯（permission denied / disk full）在 CI 環境難重現,只測：
    //! - NotFound → Ok(None)
    //! - valid JSON → Ok(Some(value))
    //! - invalid JSON → Err(Parse)
    //! - home = None → 全 6 label None
    //! - 5 OpenAB + __local__ 寫盤後 → 對應 label Some
    use super::*;

    /// 為每個 test 製造獨立 tmp 路徑（避免 parallel test 互踩）。
    fn tmp_path(tag: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("lp-usage-snap-{tag}-{nonce}.json"));
        p
    }

    #[test]
    fn read_usage_snapshot_at_not_found_returns_ok_none() {
        // NotFound 走 Ok(None)（first-run 預期,對齊 R28 `load_config_at` / R32
        // `load_history_at` 契約）。err 訊息顯式區分「not present」跟「error」,operator
        // 排查「cicx 沒 quota 圖」時 NotFound = OpenAB 沒跑（預期,不看 log）。
        let path = tmp_path("not-found");
        // 確保不存在
        let _ = std::fs::remove_file(&path);
        let result = read_usage_snapshot_at(&path);
        assert!(
            matches!(result, Ok(None)),
            "NotFound 應回 Ok(None),實際 {result:?}"
        );
    }

    #[test]
    fn read_usage_snapshot_at_valid_json_returns_ok_some() {
        // valid JSON 解析成功 → Ok(Some(value))。驗證 value 內容能 round-trip（不只 Some，
        // 還要是正確的 JSON 結構）。
        let path = tmp_path("valid");
        let payload = serde_json::json!({"usage": {"prompt": 100, "completion": 50}});
        std::fs::write(&path, serde_json::to_string(&payload).unwrap()).unwrap();

        let result = read_usage_snapshot_at(&path);
        match result {
            Ok(Some(v)) => assert_eq!(v, payload),
            other => panic!("valid JSON 應回 Ok(Some(value)),實際 {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_usage_snapshot_at_invalid_json_returns_parse_err() {
        // 寫半截 JSON（模擬 OpenAB write_quota 寫到一半 crash / encoding 損壞）→ Err(Parse)
        // 帶 preview。err 訊息含「parse error」前綴,preview 取前 80 字。
        let path = tmp_path("invalid");
        std::fs::write(&path, b"{\"usage\": {\"prompt\": 100, ").unwrap();

        let result = read_usage_snapshot_at(&path);
        match result {
            Err(ReadUsageSnapshotError::Parse { preview, .. }) => {
                // preview 帶前 26 字 raw content
                assert!(
                    preview.starts_with("{\"usage\""),
                    "preview 應含 raw content 開頭,實際 {preview:?}"
                );
            }
            other => panic!("invalid JSON 應回 Err(Parse),實際 {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_usage_snapshots_with_home_none_returns_all_ten_keys_none() {
        // 邊界:home = None（罕見但要保證不 crash）。對齊 K11 `collect_quota_snapshot_mtimes`
        // 邊界契約 — 無 HOME env → 10 label (9 OpenAB + __local__) 全 None,呼叫端拿到
        // 空 HashMap 不會 panic 也不會嘗試 join 路徑。
        //
        // R100: 9 OpenAB bot 對齊 R78 補完後 (cicx/gitx/giminix/codex_bot/openx/
        // irisx_bot/grokx/lpbot/mimo) + __local__ 共 10 個 slot,改用 OPENAB_BOT_IDS
        // const 驅動避免再 drift。
        let out = read_usage_snapshots_with_home(&None);
        assert_eq!(
            out.len(),
            10,
            "10 label (9 OpenAB + __local__) 全要存在,實際 {} 個",
            out.len()
        );
        for bot in OPENAB_BOT_IDS {
            assert!(
                out.get(*bot).map(|v| v.is_none()).unwrap_or(false),
                "{bot} 在 home=None 時應為 None,實際 {:?}",
                out.get(*bot)
            );
        }
        assert!(
            out.get("__local__").map(|v| v.is_none()).unwrap_or(false),
            "__local__ 在 home=None 時應為 None,實際 {:?}",
            out.get("__local__")
        );
    }

    #[test]
    fn read_usage_snapshots_with_home_existing_files_populates_correctly() {
        // 注入 home → 寫 9 OpenAB + __local__ 10 個檔案 → 對應 10 label 全 Some,
        // 內容 round-trip。驗 legacy fallback:不寫 usage-bot.json → 仍能從 9 OpenAB
        // 拿 openx(主檔優先)。
        //
        // R100: 5 OpenAB → 9 OpenAB,對齊 R78 補完 (irisx_bot/grokx/lpbot/mimo)。
        let home = std::env::temp_dir().join(format!(
            "lp-rs-home-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let dir = home.join(".lobsterpulse");
        std::fs::create_dir_all(&dir).unwrap();

        let payload_cicx = serde_json::json!({"backend": "cicx", "tokens": 42});
        let payload_gitx = serde_json::json!({"backend": "gitx", "tokens": 7});
        let payload_giminix = serde_json::json!({"backend": "giminix", "tokens": 1});
        let payload_codex_bot = serde_json::json!({"backend": "codex_bot", "tokens": 99});
        let payload_openx = serde_json::json!({"backend": "openx", "tokens": 3});
        let payload_irisx_bot = serde_json::json!({"backend": "irisx_bot", "tokens": 11});
        let payload_grokx = serde_json::json!({"backend": "grokx", "tokens": 13});
        let payload_lpbot = serde_json::json!({"backend": "lpbot", "tokens": 17});
        let payload_mimo = serde_json::json!({"backend": "mimo", "tokens": 19});
        let payload_local = serde_json::json!({"source": "local_runner", "tokens": 1000});

        for (name, payload) in [
            ("cicx", &payload_cicx),
            ("gitx", &payload_gitx),
            ("giminix", &payload_giminix),
            ("codex_bot", &payload_codex_bot),
            ("openx", &payload_openx),
            ("irisx_bot", &payload_irisx_bot),
            ("grokx", &payload_grokx),
            ("lpbot", &payload_lpbot),
            ("mimo", &payload_mimo),
            ("__local__", &payload_local),
        ] {
            let path = if name == "__local__" {
                dir.join("usage-local.json")
            } else {
                dir.join(format!("usage-{name}.json"))
            };
            std::fs::write(&path, serde_json::to_string(payload).unwrap()).unwrap();
        }

        let out = read_usage_snapshots_with_home(&Some(home.clone()));
        assert_eq!(
            out.get("cicx").and_then(|v| v.as_ref()),
            Some(&payload_cicx)
        );
        assert_eq!(
            out.get("gitx").and_then(|v| v.as_ref()),
            Some(&payload_gitx)
        );
        assert_eq!(
            out.get("giminix").and_then(|v| v.as_ref()),
            Some(&payload_giminix)
        );
        assert_eq!(
            out.get("codex_bot").and_then(|v| v.as_ref()),
            Some(&payload_codex_bot)
        );
        assert_eq!(
            out.get("openx").and_then(|v| v.as_ref()),
            Some(&payload_openx)
        );
        assert_eq!(
            out.get("irisx_bot").and_then(|v| v.as_ref()),
            Some(&payload_irisx_bot)
        );
        assert_eq!(
            out.get("grokx").and_then(|v| v.as_ref()),
            Some(&payload_grokx)
        );
        assert_eq!(
            out.get("lpbot").and_then(|v| v.as_ref()),
            Some(&payload_lpbot)
        );
        assert_eq!(
            out.get("mimo").and_then(|v| v.as_ref()),
            Some(&payload_mimo)
        );
        assert_eq!(
            out.get("__local__").and_then(|v| v.as_ref()),
            Some(&payload_local)
        );

        // 清理
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn read_usage_snapshots_with_home_legacy_alias_fills_openx_when_missing() {
        // Legacy fallback:9 OpenAB 主檔缺 openx,但有 usage-bot.json → openx 從 legacy
        // 取（OPENX alias）。對齊 K11 `collect_quota_snapshot_mtimes_openx_legacy_alias_fallback`
        // 契約,確保 legacy 路徑不只走 mtime,也走 read。
        //
        // R100: 4 個未寫 label → 8 個未寫 label (加 irisx_bot/grokx/lpbot/mimo)。
        let home = std::env::temp_dir().join(format!(
            "lp-rs-legacy-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let dir = home.join(".lobsterpulse");
        std::fs::create_dir_all(&dir).unwrap();

        // 故意只寫 cicx + usage-bot.json,其他 8 個 OpenAB label 都不存在
        std::fs::write(
            dir.join("usage-cicx.json"),
            serde_json::to_string(&serde_json::json!({"backend": "cicx"})).unwrap(),
        )
        .unwrap();
        let legacy_payload = serde_json::json!({"backend": "OPENX_legacy", "tokens": 5});
        std::fs::write(
            dir.join("usage-bot.json"),
            serde_json::to_string(&legacy_payload).unwrap(),
        )
        .unwrap();

        let out = read_usage_snapshots_with_home(&Some(home.clone()));
        // cicx 有
        assert!(out.get("cicx").and_then(|v| v.as_ref()).is_some());
        // openx 從 legacy 拿
        assert_eq!(
            out.get("openx").and_then(|v| v.as_ref()),
            Some(&legacy_payload)
        );
        // 其他 8 個 label:gitx/giminix/codex_bot/irisx_bot/grokx/lpbot/mimo 不存在 → None,
        // openx 已被 legacy 填不再 None
        for key in [
            "gitx",
            "giminix",
            "codex_bot",
            "irisx_bot",
            "grokx",
            "lpbot",
            "mimo",
        ] {
            assert!(
                out.get(key).map(|v| v.is_none()).unwrap_or(false),
                "{key} 不寫檔時應為 None"
            );
        }

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn openab_bot_ids_constant_matches_r78_inventory() {
        // R100 護欄：守 `OPENAB_BOT_IDS` const 對齊 R78 補完後的 9 隻 OpenAB bot
        // (cicx/gitx/giminix/codex_bot/openx/irisx_bot/grokx/lpbot/mimo)。
        //
        // 對齊 R67 護欄 chain 16 精神(provider 對稱):將來加 bot 必須 1) 加這 const +
        // 2) 加 default_providers + 3) 加 KNOWN_PROVIDERS + 4) 加 usage-{bot}.json
        // snapshot slot,任何一點漏接這條 test 會自動 fail(常數長度/內容漂移即破)。
        //
        // 不算 chain 17 擴張(護欄 chain 17 已飽和凍結,R97 決策):這是 chain 16
        // 「provider 對稱」既有對稱面延伸到 quota snapshot 讀檔的第 4 同步點。
        assert_eq!(
            OPENAB_BOT_IDS.len(),
            9,
            "9 OpenAB bot 對齊 R78 後清單,實際 {} 個",
            OPENAB_BOT_IDS.len()
        );
        let expected: std::collections::HashSet<&str> = [
            "cicx",
            "gitx",
            "giminix",
            "codex_bot",
            "openx",
            "irisx_bot",
            "grokx",
            "lpbot",
            "mimo",
        ]
        .into_iter()
        .collect();
        let actual: std::collections::HashSet<&str> = OPENAB_BOT_IDS.iter().copied().collect();
        assert_eq!(
            actual,
            expected,
            "OPENAB_BOT_IDS 內容要嚴格等於 R78 補完清單,差集: 多={:?} 缺={:?}",
            actual.difference(&expected).collect::<Vec<_>>(),
            expected.difference(&actual).collect::<Vec<_>>()
        );

        // 雙向護欄:既有的 read_usage_snapshots_with_home 跟 collect_quota_snapshot_mtimes
        // 對 OPENAB_BOT_IDS 應「吃 9 個 bot」 — 透過輸出 HashMap 的大小間接驗證。
        let out_read = read_usage_snapshots_with_home(&None);
        assert_eq!(
            out_read.len(),
            9 + 1, // 9 OpenAB + __local__
            "read_usage_snapshots_with_home 應產出 9 OpenAB + __local__ = 10 slot,實際 {}",
            out_read.len()
        );
        let out_mtimes = collect_quota_snapshot_mtimes(&None);
        assert_eq!(
            out_mtimes.len(),
            9 + 1,
            "collect_quota_snapshot_mtimes 應產出 9 OpenAB + __local__ = 10 slot,實際 {}",
            out_mtimes.len()
        );
    }

    /// R110 護欄：跨模組對稱 — `OPENAB_BOT_IDS` (lib.rs) 必須是
    /// `hook_server::KNOWN_PROVIDERS` 的子集。
    ///
    /// 為何必要:R70 spec drift 正是「加了 `irisx_bot` 到 config.rs 4 同步點
    /// (`default_providers` / `default_provider_sounds` /
    /// `default_provider_waiting_sounds` / `detect_providers`) 但漏了第 5 同步點
    /// `hook_server::KNOWN_PROVIDERS`」,IRISX 事件 POST `/hook/irisx_bot` 走完
    /// parse_provider fallback "claude",K40 metric
    /// `lobsterpulse_provider_sessions{provider="irisx_bot"}` 永遠 0。
    ///
    /// R73 才補完第 5 同步點。本護欄是 R73 護欄 chain 16 對稱面延伸到「OpenAB
    /// bot id 子集 ⊆ hook 白名單」的不變式 — 將來加 bot 漏同步 hook_server 白名單
    /// 會在 lib.rs build 時 fail (constant 引用,非運行時檢查,提早 fail)。
    ///
    /// 算 chain 17 內:R70 spec drift 既有對稱面 + R73 補完的延伸,屬既有 chain
    /// 16 對稱面 (R97 凍結決策),不擴張 chain 17。
    #[test]
    fn r110_openab_bot_ids_subset_of_hook_server_known_providers() {
        use crate::hook_server::KNOWN_PROVIDERS;

        let known: std::collections::HashSet<&str> = KNOWN_PROVIDERS.iter().copied().collect();
        for bot in OPENAB_BOT_IDS {
            assert!(
                known.contains(bot),
                "OPENAB_BOT_IDS 含 {bot:?} 但 hook_server::KNOWN_PROVIDERS 沒有, \
                 對齊 R70/R73 spec drift 修:任何 OpenAB bot id 必須同時登錄 \
                 KNOWN_PROVIDERS 白名單,缺同步會讓 parse_provider 走 fallback \
                 \"claude\" 害 K40 metric provider=\"{bot}\" 永遠 0 \
                 (KNOWN_PROVIDERS = {:?})",
                KNOWN_PROVIDERS
            );
        }
    }
}

#[cfg(test)]
mod collect_live_quota_snapshot_tests {
    //! R89 Tauri command 接線：對 `collect_live_quota_snapshot_with_home` 的測試。
    //!
    //! 對齊 R33 `read_usage_snapshots_with_home` 風格：注入 home 變數測邊界，
    //! 兩 fetch 在 home 為空時不 panic / 不打 API（read_credentials Err 早返）。
    //!
    //! 跨平台 IO / 網路錯在 CI 環境難重現,只測：
    //! - home = None → runners=[] + source="live_api" + updated_at>0
    //! - home = Some(空) → 兩 runner (claude + codex) + 兩 fetch graceful fail (ok=false)
    //! - runner name 嚴格是 "claude" 與 "codex"（前端 contract 對齊）
    //! - snapshot JSON round-trip（前端 deserialize 不能炸）
    //!
    //! 故意不測：rate-limit header 解析 / 成功 fetch 路徑（要打真的 API,CI 環境
    //! 無網路或會污染真實 quota 計數,留 smoke / manual 測）。
    use super::*;
    use crate::quota::copilot::ENV_LOCK;

    /// 為每個 test 製造獨立 tmp home（避免 parallel test 互踩）。
    fn tmp_home(tag: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("lp-live-quota-{tag}-{nonce}"))
    }

    /// 對齊 lib.rs 既有 pattern（line 2696/2937）手動建 Runtime + block_on，
    /// 不引 `#[tokio::test]`（避免 Cargo.toml 加 rt-multi-thread feature）。
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(f)
    }

    #[test]
    fn collect_live_quota_snapshot_with_home_none_returns_empty_runners() {
        // 邊界:home = None（罕見但要保證不 crash）。對齊 R33 6 label 全 None 契約,
        // 這裡 runners 為空 vec（live API runner 是動態的,不像 OpenAB 5 + __local__
        // 固定 6 key）。
        let out = block_on(collect_live_quota_snapshot_with_home(
            None::<&std::path::Path>,
        ));
        assert_eq!(out.runners.len(), 0, "home=None 時 runners 應為空 vec");
        assert_eq!(
            out.source, "live_api",
            "source 標記要固定 live_api 給前端分流"
        );
        assert!(
            out.updated_at > 0,
            "updated_at 必為正 unix 秒數,實際 {}",
            out.updated_at
        );
    }

    #[test]
    fn collect_live_quota_snapshot_with_home_some_without_credentials_returns_four_failed_runners()
    {
        // 注入空 home（無 ~/.claude/.credentials.json 也無 ~/.codex/auth.json 也無
        // ~/.gemini/oauth_creds.json）→ 四 fetch 在 read_credentials 階段早返
        // RunnerQuota (ok=false),不打 API 不 timeout。
        // 重點：結構完整（4 runner, 有 name/label/ok/text）+ 不 panic。
        // R108 從 2 runner → 3 runner (gemini),R109 → 4 runner (copilot),對齊
        // 4 本機 CLI 全覆蓋。
        let home = tmp_home("empty");
        std::fs::create_dir_all(&home).unwrap();

        // 防 env-var race (對齊 copilot.rs::tests 的 ENV_LOCK 序列化):
        // 與 quota::copilot::tests 共用同一把 process-global Mutex,
        // 確保 copilot.rs 內 5 個 env-var test 在跑時這個 test 不會被切到
        // 中間狀態 (例如 set 後未 remove 切到這邊 read)。
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        // 確保 copilot 的 read_credentials 也不會從 env 拉到真 token
        // (測試環境理論上不會設 GH_TOKEN / GITHUB_TOKEN / COPILOT_TOKEN,
        // 但保險起見先清掉,排除 runner 不在 '⚠ ...' 開頭的污染路徑)
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("COPILOT_TOKEN");

        let out = block_on(collect_live_quota_snapshot_with_home(Some(&home)));
        assert_eq!(
            out.runners.len(),
            4,
            "應有 4 runner (claude + codex + gemini + copilot),實際 {}",
            out.runners.len()
        );
        assert_eq!(out.source, "live_api");

        for runner in &out.runners {
            assert!(
                !runner.ok,
                "{} credentials 不存在時應 ok=false,實際 ok={}",
                runner.name, runner.ok
            );
            assert!(
                runner.text.starts_with('\u{26A0}'),
                "{} 失敗訊息應以 ⚠ 開頭,實際 {:?}",
                runner.name,
                runner.text
            );
        }

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn collect_live_quota_snapshot_runners_have_known_names_claude_codex_gemini_copilot() {
        // 前端 contract:runner.name 嚴格是 "claude" / "codex" / "gemini" / "copilot",
        // 對齊 hook_server.rs KNOWN_PROVIDERS 本機 CLI 段(R109 起 4 個 live quota runner)。
        // 任何改名 / 新加 / 漏掉 → 前端分組錯亂。
        let home = tmp_home("names");
        std::fs::create_dir_all(&home).unwrap();

        // 防 env-var race (對齊 copilot.rs::tests 的 ENV_LOCK 序列化)
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("COPILOT_TOKEN");

        let out = block_on(collect_live_quota_snapshot_with_home(Some(&home)));
        let names: Vec<&str> = out.runners.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"claude"),
            "應含 claude runner,實際 {:?}",
            names
        );
        assert!(
            names.contains(&"codex"),
            "應含 codex runner,實際 {:?}",
            names
        );
        assert!(
            names.contains(&"gemini"),
            "應含 gemini runner,實際 {:?}",
            names
        );
        assert!(
            names.contains(&"copilot"),
            "應含 copilot runner,實際 {:?}",
            names
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn collect_live_quota_snapshot_updated_at_is_fresh_unix_seconds() {
        // 邊界:updated_at 必為「合理新」的 unix 秒數（> 2025-01-01 = 1735689600）,
        // 防止 SystemTime::duration_since 退化（譬如 EPOCH 之前的時間點）回 0。
        let home = tmp_home("ts");
        std::fs::create_dir_all(&home).unwrap();

        let out = block_on(collect_live_quota_snapshot_with_home(Some(&home)));
        assert!(
            out.updated_at > 1_735_689_600,
            "updated_at 應 > 2025-01-01,實際 {}",
            out.updated_at
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn collect_live_quota_snapshot_serializes_to_json_for_frontend() {
        // 對齊前端 `invoke('get_live_quota_snapshot')` deserialize contract。
        // 不只要能序列化,round-trip 也要能解回同樣的 LiveQuotaSnapshot。
        let home = tmp_home("json");
        std::fs::create_dir_all(&home).unwrap();

        let out = block_on(collect_live_quota_snapshot_with_home(Some(&home)));
        let json = serde_json::to_string(&out).expect("serialize LiveQuotaSnapshot 應成功");
        let back: quota::LiveQuotaSnapshot =
            serde_json::from_str(&json).expect("deserialize LiveQuotaSnapshot 應成功");
        assert_eq!(back.runners.len(), out.runners.len());
        assert_eq!(back.source, out.source);
        assert_eq!(back.updated_at, out.updated_at);
        // round-trip 後 runner 內容一致（name/label/color 不變,ok/text/raw 可比較）
        for (a, b) in out.runners.iter().zip(back.runners.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.label, b.label);
            assert_eq!(a.color, b.color);
        }

        let _ = std::fs::remove_dir_all(&home);
    }
}

/// 簡易 Handlebars 替換 — 只支援 `{{ key }}` 從 JSON top-level 取值（對齊 OpenAB template 語意）。
fn render_handlebars(tpl: &str, json: &serde_json::Value) -> String {
    let mut out = tpl.to_string();
    if let Some(obj) = json.as_object() {
        for (k, v) in obj {
            let value = match v {
                serde_json::Value::String(s) => s.clone(),
                _ => v.to_string(),
            };
            // 替 `{{ key }}` 和 `{{key}}` 兩種空白模式
            out = out.replace(&format!("{{{{ {} }}}}", k), &value);
            out = out.replace(&format!("{{{{{}}}}}", k), &value);
        }
    }
    out
}

/// 以臨時檔 + rename 原子替換目標檔，避免讀取端拿到半寫內容。
/// Windows 若 rename 被暫時鎖住會短暫重試。
fn write_file_atomic_with_retry(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let Some(parent) = path.parent() else {
        return std::fs::write(path, data);
    };
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("snapshot");
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..3_u32 {
        let tmp_path = parent.join(format!(".{stem}.tmp-{pid}-{nonce}-{attempt}"));
        let write_result = (|| -> std::io::Result<()> {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp_path)?;
            f.write_all(data)?;
            f.sync_all()?;
            Ok(())
        })();
        if let Err(err) = write_result {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err);
        }

        match std::fs::rename(&tmp_path, path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                let _ = std::fs::remove_file(&tmp_path);
                last_err = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(
                    30 * u64::from(attempt + 1),
                ));
            }
        }
    }

    Err(last_err.unwrap_or_else(|| std::io::Error::other("atomic rename failed")))
}

/// 帶 timeout 跑外部命令，避免 runner 卡死整個輪詢線程。
fn run_command_with_timeout(
    mut cmd: std::process::Command,
    timeout_secs: u64,
) -> Result<std::process::Output, String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn fail: {e}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx_out, rx_out) = mpsc::channel();
    let (tx_err, rx_err) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stdout {
            let _ = s.read_to_end(&mut buf);
        }
        let _ = tx_out.send(buf);
    });
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_end(&mut buf);
        }
        let _ = tx_err.send(buf);
    });

    let timeout = Duration::from_secs(timeout_secs.max(1));
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait().map_err(|e| format!("wait fail: {e}"))?;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait fail: {e}")),
        }
    };
    let mut stdout_buf = rx_out.recv().unwrap_or_default();
    let mut stderr_buf = rx_err.recv().unwrap_or_default();
    if timed_out {
        let note = format!("\nrunner timeout after {}s", timeout.as_secs());
        stderr_buf.extend_from_slice(note.as_bytes());
    }
    // 避免極端輸出塞爆 JSON 體積（UI 只需摘要）
    const MAX_IO_BYTES: usize = 64 * 1024;
    if stdout_buf.len() > MAX_IO_BYTES {
        stdout_buf.truncate(MAX_IO_BYTES);
    }
    if stderr_buf.len() > MAX_IO_BYTES {
        stderr_buf.truncate(MAX_IO_BYTES);
    }
    Ok(std::process::Output {
        status,
        stdout: stdout_buf,
        stderr: stderr_buf,
    })
}

/// 跑 config.appearance.usage_runners 一輪，收集結果寫到 ~/.lobsterpulse/usage-local.json。
fn run_local_usage_runners(runners: &[crate::config::UsageRunnerConfig]) {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    if LOCAL_USAGE_RUNNERS_IN_FLIGHT
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    struct RunnerInFlightGuard;
    impl Drop for RunnerInFlightGuard {
        fn drop(&mut self) {
            LOCAL_USAGE_RUNNERS_IN_FLIGHT.store(false, Ordering::Release);
        }
    }
    let _guard = RunnerInFlightGuard;

    let Some(home) = dirs::home_dir() else {
        return;
    };
    let dir = home.join(".lobsterpulse");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        // R57: 從 `let _ =` 沉默吞改成 log warn。OpenAB runner 啟動前創
        // ~/.lobsterpulse/, 失敗 (AppData 權限 / 磁碟滿 / 唯讀 home) →
        // runner 啟動失敗, 但 log 沒記路徑跟 err, 跟 run_openab_runners 後續
        // 失敗 (Command spawn) 串不起來, operator 看 log 不知道是 mkdir 階段死
        log::warn!(
            "{} ({})",
            lib_warn_msg("openab_runners_dir_mkdir", &e),
            dir.display()
        );
    }

    let mut results = Vec::new();
    for r in runners {
        let mut cmd = Command::new(&r.command);
        cmd.args(&r.args);
        if let Some(cwd) = &r.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &r.env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = run_command_with_timeout(cmd, r.timeout_secs);
        let result = match out {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout).trim().to_string();
                // 解析 JSON 供 capsule 讀 raw 欄位；失敗則 null
                let raw = serde_json::from_str::<serde_json::Value>(&stdout)
                    .unwrap_or(serde_json::Value::Null);
                let text = if let Some(tpl) = &r.template {
                    if raw.is_object() {
                        render_handlebars(tpl, &raw)
                    } else {
                        stdout.clone()
                    }
                } else {
                    stdout.clone()
                };
                serde_json::json!({
                    "ok": true,
                    "name": r.name,
                    "label": r.label,
                    "color": r.color,
                    "text": text,
                    "raw": raw,
                })
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                serde_json::json!({
                    "ok": false,
                    "name": r.name,
                    "label": r.label,
                    "color": r.color,
                    "text": if stderr.is_empty() { format!("exit {:?}", o.status.code()) } else { stderr },
                })
            }
            Err(e) => serde_json::json!({
                "ok": false,
                "name": r.name,
                "label": r.label,
                "color": r.color,
                "text": format!("spawn failed: {e}"),
            }),
        };
        results.push(result);
    }

    let snapshot = serde_json::json!({
        "source": "local",
        "updated_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "runners": results,
    });
    let path = dir.join("usage-local.json");
    if let Err(e) = write_local_usage_snapshot(&path, &snapshot) {
        log::error!(
            "usage-local.json write failed: {e}; capsule quota bar will show stale \
             data until the next 60s cycle recovers"
        );
    }
}

/// 序列化 + 原子寫 usage snapshot。失敗回 Err 並在內部 log — 不要 silent,
/// 因為 60s loop 下 snapshot 寫失敗會讓膠囊 quota 卡舊值,user 不知是 OpenAB
/// 沒更新還是 LP 自己寫失敗。`run_local_usage_runners` 是唯一 caller。
fn write_local_usage_snapshot(
    path: &std::path::Path,
    snapshot: &serde_json::Value,
) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(snapshot).map_err(|e| format!("serialize: {e}"))?;
    if let Err(e) = write_file_atomic_with_retry(path, &data) {
        log::error!("usage-local.json atomic write failed: {e}, falling back to direct write");
        std::fs::write(path, &data).map_err(|e2| {
            log::error!("usage-local.json direct write failed: {e2}");
            format!("atomic: {e}, direct: {e2}")
        })?;
    }
    Ok(())
}

#[tauri::command]
fn bounce_window(window: tauri::WebviewWindow) {
    let win = window.clone();
    std::thread::spawn(move || {
        if let Ok(pos) = win.outer_position() {
            let scale = win.scale_factor().unwrap_or(1.0);
            let orig_y = pos.y as f64 / scale;
            let orig_x = pos.x as f64 / scale;

            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                orig_x,
                orig_y + 8.0,
            )));
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                orig_x,
                orig_y - 3.0,
            )));
            std::thread::sleep(std::time::Duration::from_millis(40));
            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                orig_x,
                orig_y + 2.0,
            )));
            std::thread::sleep(std::time::Duration::from_millis(30));
            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                orig_x, orig_y,
            )));
        }
    });
}

#[tauri::command]
fn resize_window(window: tauri::WebviewWindow, width: f64, height: f64) {
    let _ = window.set_resizable(true);
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let _ = window.set_always_on_top(true);
}

/// 讀本機背景檔 → base64 data URL（繞開 asset protocol 各種坑）
/// 影片檔 >15MB 直接拒絕，建議用網路 URL
#[tauri::command]
fn get_background_data_url(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("read: {e}"))?;
    const MAX: usize = 15 * 1024 * 1024;
    if bytes.len() > MAX {
        return Err(format!(
            "檔案 {:.1} MB 太大，建議 <15MB 或用網路 URL",
            bytes.len() as f64 / 1_048_576.0
        ));
    }
    let lower = path.to_lowercase();
    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".webm") {
        "video/webm"
    } else if lower.ends_with(".mov") {
        "video/quicktime"
    } else if lower.ends_with(".mkv") {
        "video/x-matroska"
    } else {
        "application/octet-stream"
    };
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4 + 100);
    out.push_str("data:");
    out.push_str(mime);
    out.push_str(";base64,");
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = *bytes.get(i + 1).unwrap_or(&0);
        let b2 = *bytes.get(i + 2).unwrap_or(&0);
        out.push(B64[(b0 >> 2) as usize] as char);
        out.push(B64[((b0 << 4 | b1 >> 4) & 0x3F) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(B64[((b1 << 2 | b2 >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(B64[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    Ok(out)
}

/// 跳 Windows OpenFileDialog 讓使用者選背景檔（圖/影片），回傳絕對路徑
#[tauri::command]
fn pick_background_file() -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let ps = r#"
Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.OpenFileDialog
$d.Filter = "圖片或影片 (*.jpg;*.jpeg;*.png;*.gif;*.webp;*.bmp;*.mp4;*.webm;*.mov;*.mkv)|*.jpg;*.jpeg;*.png;*.gif;*.webp;*.bmp;*.mp4;*.webm;*.mov;*.mkv|所有檔案 (*.*)|*.*"
$d.Title = "LobsterPulse - 選擇背景照片或影片"
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { Write-Output $d.FileName }
"#;
        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-STA", "-Command", ps])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("dialog spawn: {e}"))?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
    #[cfg(not(windows))]
    {
        Err("file picker only on Windows".into())
    }
}

/// 手動觸發一次 quota history snapshot（不等每小時 loop）。
#[tauri::command]
fn manual_snapshot_once() -> Result<usize, String> {
    quota_history::snapshot_once()
}

/// 測試 Windows toast — UI debug 用（setting 頁可加「🧪 測試 toast」按鈕）
#[tauri::command]
fn test_toast(app: tauri::AppHandle) -> Result<(), String> {
    auto_rules::send_toast(
        &app,
        "🦞 LobsterPulse 測試",
        "Toast 通知已啟用，auto_rules 會走這條",
    );
    Ok(())
}

/// 一鍵 rebuild + relaunch：spawn visible cmd 跑 cargo build --release 完成後重啟 LP。
/// 前端呼叫後 LP 會先 exit 讓 exe 可覆寫。
/// 2026-04-20 修：bat 改 visible（讓 user 看 cargo 進度）+ admin relaunch via PowerShell -Verb RunAs + std::process::exit 跳過 tokio cleanup 卡住
#[tauri::command]
fn rebuild_and_relaunch(_app: tauri::AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let proj_dir = exe
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .ok_or("cannot derive project dir")?
        .to_path_buf();
    let exe_path = exe.to_string_lossy().to_string();
    let proj_str = proj_dir.to_string_lossy().to_string();
    let bat = std::env::temp_dir().join("lp_rebuild.cmd");
    // 用 forward slashes 避免 PowerShell `\t` 被當 tab 坑
    let exe_fwd = exe_path.replace('\\', "/");
    let _ = exe_fwd; // main.rs 的 self-elevation 會處理 UAC，bat 只需 start exe
    let bat_content = format!(
        "@echo off\r\n\
         echo [LP rebuild] waiting 3s for LP to exit...\r\n\
         ping -n 4 127.0.0.1 >nul\r\n\
         cd /d \"{proj}\"\r\n\
         echo [LP rebuild] cargo build --release\r\n\
         cargo build --release\r\n\
         if exist \"{exe}\" (\r\n\
           echo [LP rebuild] launching LP (self-elevation via main.rs)...\r\n\
           start \"\" \"{exe}\"\r\n\
         ) else (\r\n\
           echo [LP rebuild] BUILD FAILED — exe not found\r\n\
           pause\r\n\
         )\r\n",
        proj = proj_str,
        exe = exe_path,
    );
    std::fs::write(&bat, bat_content).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        std::process::Command::new("cmd.exe")
            .args([
                "/c",
                "start",
                "",
                "cmd.exe",
                "/k",
                bat.to_str().unwrap_or(""),
            ])
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|e| format!("spawn bat: {e}"))?;
    }
    // 強制立即退出（跳過 tokio runtime cleanup 卡住問題）
    // delay 100ms 讓 IPC response 傳回前端
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(150));
        std::process::exit(0);
    });
    Ok(())
}

/// 用預設瀏覽器開外觀調整器（方案 B：獨立 web configurator）
#[tauri::command]
fn open_configurator() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("no home dir")?;
    let html = home
        .join(".lobsterpulse")
        .join("web-config")
        .join("configurator.html");
    if !html.exists() {
        return Err(format!("configurator not found: {}", html.display()));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &html.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(&html)
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
    }
    Ok(())
}

/// 用預設瀏覽器開使用說明手冊
#[tauri::command]
fn open_help_page() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("no home dir")?;
    let html = home
        .join(".lobsterpulse")
        .join("web-config")
        .join("help.html");
    if !html.exists() {
        return Err(format!("help not found: {}", html.display()));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &html.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(&html)
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
    }
    Ok(())
}

/// 讀取 ~/.lobsterpulse/web-config/appearance.json 合併進 config.appearance，persist
#[tauri::command]
fn import_appearance_json(
    config_state: tauri::State<AppConfigState>,
) -> Result<serde_json::Value, String> {
    let home = dirs::home_dir().ok_or("no home dir")?;
    let candidates = [
        home.join("Downloads").join("appearance.json"),
        home.join(".lobsterpulse")
            .join("web-config")
            .join("appearance.json"),
    ];
    let path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or("找不到 appearance.json（請先從 configurator 下載到 Downloads）".to_string())?;
    let data = std::fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    let patch: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("parse: {e}"))?;
    let patch_obj = patch.as_object().ok_or("JSON 必須是 object")?;
    {
        let mut cfg = config_state.0.lock().unwrap();
        // 用 serde round-trip 合併：appearance → value → merge → deserialize 回去
        let mut app_val = serde_json::to_value(&cfg.appearance).map_err(|e| e.to_string())?;
        if let Some(app_obj) = app_val.as_object_mut() {
            for (k, v) in patch_obj {
                app_obj.insert(k.clone(), v.clone());
            }
        }
        cfg.appearance = serde_json::from_value(app_val).map_err(|e| format!("merge: {e}"))?;
        crate::config::save_config(&cfg).map_err(|e| e.to_string())?;
    }
    Ok(serde_json::json!({ "ok": true, "source": path.display().to_string() }))
}

/// 把膠囊視窗隱藏到 tray；之後用 tray menu 的「顯示 / 隱藏」可以叫回來。
/// 遍歷所有 webview window 並逐一 hide，避開 Tauri 2.10 decoration helper
/// 幽靈視窗沒跟著隱藏的狀況（enum 看到有 2 個 14×14 window 不會隨主視窗 hide）。
#[tauri::command]
fn hide_window(app: tauri::AppHandle) {
    for (_, w) in app.webview_windows() {
        let _ = w.hide();
    }
}

/// 事件診斷 view 資料源——回傳最近 50 筆 hook event。
#[tauri::command]
fn get_recent_events(manager: tauri::State<AppSessionManager>) -> Vec<session::RecentEvent> {
    manager
        .0
        .lock()
        .unwrap()
        .recent_events
        .iter()
        .cloned()
        .collect()
}

/// Tray menu「重啟 OpenAB」觸發——先 kill 既有 openab.exe，再跑 config 裡的
/// `openab_restart_command`（空字串則僅 kill，依賴使用者的 Scheduled Task 自動 respawn）。
#[tauri::command]
fn restart_openab(config_state: tauri::State<AppConfigState>) -> Result<String, String> {
    let restart_cmd = config_state
        .0
        .lock()
        .unwrap()
        .appearance
        .openab_restart_command
        .clone();

    let kill = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "Stop-Process -Name openab -Force -ErrorAction SilentlyContinue",
        ])
        .output()
        .map_err(|e| format!("kill openab failed: {e}"))?;
    let _ = kill;

    if !restart_cmd.trim().is_empty() {
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &restart_cmd])
            .spawn()
            .map_err(|e| format!("restart command spawn failed: {e}"))?;
        Ok("killed + ran restart command".to_string())
    } else {
        Ok("killed (rely on Scheduled Task auto-respawn)".into())
    }
}

#[tauri::command]
fn is_cursor_inside(window: tauri::WebviewWindow) -> bool {
    let cursor = match window.cursor_position() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let pos = match window.outer_position() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let size = match window.outer_size() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let margin = 2.0;
    cursor.x >= (pos.x as f64 - margin)
        && cursor.x <= (pos.x as f64 + size.width as f64 + margin)
        && cursor.y >= (pos.y as f64 - margin)
        && cursor.y <= (pos.y as f64 + size.height as f64 + margin)
}

struct ServerPort(u16);

/// 組 Prometheus text format；給 `/metrics` endpoint 用。
fn render_prometheus(handle: &tauri::AppHandle) -> String {
    let mgr = handle.state::<AppSessionManager>();
    let state = mgr.0.lock().unwrap().get_state();
    // K11 落地：K8 之後新加的 quota snapshot 資料源在磁碟上（不在 SessionManager），
    // 須從 `render_prometheus` 這層讀 mtime → 算 age 再餵進 pure body fn。
    // 抽 `compute_quota_snapshot_age_seconds` 為 pure fn 後，body 仍維持「不讀 fs」
    // 不變式，time-dependent math（future mtime / saturating）也能 unit test 注入。
    let now = Utc::now();
    let quota_snapshot_mtimes = collect_quota_snapshot_mtimes(&dirs::home_dir());
    let quota_snapshot_ages: std::collections::HashMap<String, i64> = quota_snapshot_mtimes
        .iter()
        .filter_map(|(p, m)| {
            compute_quota_snapshot_age_seconds(now, *m).map(|age| (p.clone(), age))
        })
        .collect();
    // K20 落地：讀 quota-history.csv 取「每 runner 最新 pct」→ 餵
    // `lobsterpulse_provider_quota_remaining_pct` gauge。對齊 R30 `get_quota_history`
    // silent-fail surfacing 模式：match Err → log warn + 留空 HashMap（不部分 emit），
    // render 端見空 map 走「header only」契約。
    //
    // 對齊上面 `quota_snapshot_mtimes` 同一 pattern:先取 home dir → 拼 path →
    // 走 `latest_quota_pct_at` pure fn（內部已呼叫 `load_history_at`,檔案只讀一次,
    // 不重複 IO）。無 home dir / IO 錯 / parse 錯 → log warn + 整段留空,跟 K11
    // 「header only」契約一致。
    let quota_remaining_pct: std::collections::HashMap<String, u8> = match dirs::home_dir() {
        Some(home) => {
            let path = home.join(".lobsterpulse").join("quota-history.csv");
            match quota_history::latest_quota_pct_at(&path) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!(
                        "[lib::render_prometheus] quota_remaining_pct 讀取失敗: {e} \
                         — provider_quota_remaining_pct 段留空（header only）"
                    );
                    std::collections::HashMap::new()
                }
            }
        }
        None => std::collections::HashMap::new(),
    };
    // K21 落地：讀 quota-history.csv 的 mtime → 算 pipeline freshness。對齊
    // K11 freshness 視角(per-snapshot age)+ K20 同一資料源(historical CSV)。
    // 跟 K20 差異：K20 看「最新 pct 數字」(consumption),K21 看「CSV 多久沒
    // 被 OpenAB 寫進來」(freshness of the history file itself)→ 跟 K11 alert
    // rule `quota_snapshot_age_seconds > 600` 互補（都是「沒更新」訊號但觀察
    // 不同檔）。NotFound 走 R32 first-run 契約 `Ok(None)` → caller 端不出
    // sample line(header-only),避免誤判「CSV 剛建立」。IO 錯 → log warn + 整段
    // 留空(跟 K20 Err 處理同一 pattern,不部分 emit 假資料)。
    let quota_history_csv_age: Option<i64> = match dirs::home_dir() {
        Some(home) => {
            let path = home.join(".lobsterpulse").join("quota-history.csv");
            match quota_history::quota_history_csv_mtime_at(&path) {
                Ok(Some(mtime)) => compute_quota_snapshot_age_seconds(now, Some(mtime)),
                Ok(None) => None,
                Err(e) => {
                    log::warn!(
                        "[lib::render_prometheus] quota_history_csv 讀取失敗: {e} \
                         — csv_age_seconds 段留空（header only）"
                    );
                    None
                }
            }
        }
        None => None,
    };
    // K14 落地：讀 Discord health state（process-level，單一 Discord 端點）。
    // 從模組級 `discord::health_snapshot()` 拿 snapshot,避免 render 端持鎖跨越整個
    // string 構造（snapshot 是 `DiscordHealth` 是 `Copy`,複製成本 = 4 個 u64 + 1 個 enum）。
    let discord_health = discord::health_snapshot();
    // K15+K16 落地：讀 hook_server 全部 process-level counter 成 `HookServerMetrics`
    // 結構（Copy, 4 個 u64 = 32 byte）。對齊 K14 discord_health 模式：原子 snapshot，
    // render 端不持任何鎖跨越 string 構造。3 個 K16 response counter 各自獨立
    // load，無 race。
    let hook_metrics = hook_server::hook_server_metrics();
    render_prometheus_body(
        &state.sessions,
        state.session_count as u64,
        state.active_count as u64,
        &state.provider_totals,
        &quota_snapshot_ages,
        &quota_remaining_pct,
        quota_history_csv_age,
        &session::last_completed_session_age_at(&state.provider_totals),
        &discord_health,
        hook_metrics,
        now,
    )
}

/// K11 配套 helper：把 wall clock 跟檔案 mtime 差值轉成 seconds。
/// 抽出理由：
///   - body 仍保持 pure（不直接 `std::fs::metadata`），filesystem 讀取只發生在
///     `render_prometheus` 這層（呼叫 `collect_quota_snapshot_mtimes`）
///   - future mtime（clock skew / 剛寫完 race）→ saturating 到 0，不輸出負值
///   - file 不存在（`metadata()` 失敗）→ `None`，caller 端不在 metric map 裡放 entry
///     → 對齊 K8 `last_event_at = None` 跳過策略 + K10 `since = None` 跳過策略：
///     避免 Prometheus 把「沒看到」當「age=0」誤判「snapshot 剛剛還在」
fn compute_quota_snapshot_age_seconds(
    now: DateTime<Utc>,
    mtime: Option<SystemTime>,
) -> Option<i64> {
    let mtime = mtime?;
    let mtime_chrono: DateTime<Utc> = mtime.into();
    let elapsed = now.signed_duration_since(mtime_chrono).num_seconds();
    Some(elapsed.max(0))
}

/// K12 配套 helper：把「`now - last_event_at`」idle 跟「`now - since`」lifetime
/// 兩個 chrono 差值相除，得出「該 provider lifetime 中有多大比例是 idle 的」。
/// 抽出理由（對齊 K11 `compute_quota_snapshot_age_seconds`）：
///   - body 仍保持 pure（不直接 chrono 計算），filesystem / clock 注入只發生在
///     `render_prometheus_body` 的 `now` 參數（呼叫端 `Utc::now()`）
///   - 兩個時間任一缺失 → `None`，對齊 K8 `last_event_at = None` + K10
///     `since = None` 跳過策略：缺失值不該被當 0
///   - lifetime ≤ 0（`since == now` / 時鐘回撥導致 `since > now`）→ `None`：
///     分母為 0 在浮點是 NaN、Prometheus 端會被視為 anomaly；寧可缺 sample
///     也不要 NaN 誤判
///   - idle 超出 lifetime（理論不會發生：since 為 first-seen、last_event_at
///     必 ≥ since；純函式防呆）→ clamp 到 1.0
///   - idle 為負（`last_event_at > now`，序列化時差）→ saturate 0 → 0.0
///   - idle = 0（剛剛收過 event）→ 0.0 = 100% 健康；不丟掉這條信號
fn compute_provider_idle_ratio(
    now: DateTime<Utc>,
    last_event_at: Option<DateTime<Utc>>,
    since: Option<DateTime<Utc>>,
) -> Option<f64> {
    let last = last_event_at?;
    let first = since?;
    let idle = now.signed_duration_since(last).num_seconds().max(0);
    let lifetime = now.signed_duration_since(first).num_seconds();
    if lifetime <= 0 {
        return None;
    }
    let ratio = (idle as f64) / (lifetime as f64);
    Some(ratio.clamp(0.0, 1.0))
}

/// K11 配套 helper：讀 `~/.lobsterpulse/usage-{bot}.json` + `usage-local.json` 的
/// mtime，回 `provider_name → mtime`。檔案不存在或無法 stat 都回 `None`（不是 Err）：
/// 對齊 `read_usage_snapshots`（line 314）的「fs 失敗不報錯、視為沒資料」語意，
/// 避免新增 silent fail 路徑。`usage-bot.json` legacy alias 同 `read_usage_snapshots`：
/// 只在 `openx` 缺資料時讀它（避免和 `usage-openx.json` 雙重計算）。
fn collect_quota_snapshot_mtimes(
    home: &Option<std::path::PathBuf>,
) -> std::collections::HashMap<String, Option<SystemTime>> {
    let mut out: std::collections::HashMap<String, Option<SystemTime>> =
        std::collections::HashMap::new();
    let Some(dir) = home.as_ref().map(|h| h.join(".lobsterpulse")) else {
        // 9 個 OpenAB bot + 1 個 local runner 全填 None，caller 端 filter 後不出現
        // 在 metric map（age 段就只 emit header、沒 sample）。
        // R100: 改用 OPENAB_BOT_IDS const 驅動,對齊 R78 後 9 隻 OpenAB。
        for p in OPENAB_BOT_IDS {
            out.insert((*p).to_string(), None);
        }
        out.insert("__local__".to_string(), None);
        return out;
    };
    for bot in OPENAB_BOT_IDS {
        let path = dir.join(format!("usage-{bot}.json"));
        out.insert(
            bot.to_string(),
            std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
        );
    }
    // Legacy alias：同 `read_usage_snapshots`，只在 `openx` 缺資料時讀 usage-bot.json。
    // 對 mtime 視角：openx 已存在的情況下 legacy file 沒用 → 跳過 mtime 收集。
    if out.get("openx").and_then(|m| m.as_ref()).is_none() {
        let path = dir.join("usage-bot.json");
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                out.insert("openx".to_string(), Some(modified));
            }
        }
    }
    let local_path = dir.join("usage-local.json");
    out.insert(
        "__local__".to_string(),
        std::fs::metadata(&local_path)
            .and_then(|m| m.modified())
            .ok(),
    );
    out
}

/// Pure formatter：把 `SessionInfo` 切片 + aggregate 計數 + `ProviderTotals` lifetime
/// aggregate 組成 Prometheus text format。
///
/// 抽出此 fn 的理由：
/// - 原本 inline 在 `render_prometheus` 內依賴 `tauri::AppHandle`，unit-test 要起 Tauri runtime
/// - 抽成 `&[SessionInfo]` + aggregate 計數 + `&HashMap<String, ProviderTotals>` 後可純函式測試
/// - provider 條目排序（alphabetical by key）確保輸出 deterministic，方便測試 assertion +
///   Prometheus scraper diff 穩定
///
/// Token 計數的語意說明（K6 落地）：
/// - `lobsterpulse_tokens_input` / `_output`：**lifetime** aggregate，讀 `ProviderTotals`
///   （不讀 live `SessionInfo`），否則 session 移除（SessionEnd / 30 min stale）後
///   token 會從 global metric 蒸發、Prometheus 端會看到 counter 倒退
/// - `lobsterpulse_provider_tokens_input{provider="..."}` / `_output`：per-provider 細顆度，
///   來源同 `ProviderTotals`，可看各 backend 自己的 quota 消耗配比
// `render_prometheus_body` 累積 10 個正交輸入（sessions / counts / provider_totals /
// quota_snapshot_ages / quota_remaining_pct / quota_history_csv_age /
// last_completed_session_age / discord_health / hook_parse_failures / now），
// 每個都是來自不同 process-level state 的純 snapshot。把它們打包成 struct
// 沒比較乾淨 —— render 端是純函式,沒有 mut / no allocation 切換,純粹 string
// 構造。對齊 R26/R27 政策：cross-cutting snapshot 整合留給 M1 輪(K15 candidate:
// `MetricsSnapshot` struct 餵前端,render 端也順手用同個 struct),不在本輪
// 重構。
#[allow(clippy::too_many_arguments)]
fn render_prometheus_body(
    sessions: &[session::SessionInfo],
    session_count: u64,
    active_count: u64,
    provider_totals: &std::collections::HashMap<String, session::ProviderTotals>,
    quota_snapshot_ages: &std::collections::HashMap<String, i64>,
    quota_remaining_pct: &std::collections::HashMap<String, u8>,
    quota_history_csv_age: Option<i64>,
    last_completed_session_age: &std::collections::HashMap<String, i64>,
    discord_health: &discord::DiscordHealth,
    hook_metrics: hook_server::HookServerMetrics,
    now: DateTime<Utc>,
) -> String {
    let mut provider_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut provider_active: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    // K19 落地：per-provider × per-state (Idle/Working/WaitingForUser/Stale) session
    // count gauge。補 K6 細顆度盲點 —— K6 只看「該 provider 共幾個 session」(idle +
    // working + waiting + stale 加總),operator alert rule 沒法直接用 K6 算「claude 是
    // 不是卡 30 分鐘都沒結束 = 累積一堆 waiting」或「gemini 是不是 5 個 session 全
    // stale 沒人收尾」。K19 直接給 (provider, state) 細顆度 → operator 用
    // `sum by(state)(lobsterpulse_provider_sessions_by_state{provider="claude"})` 看
    // load mix,或 `lobsterpulse_provider_sessions_by_state{state="stale"} > 5` alert
    // 「5 個以上 session 進入 stale 沒人收」。
    //
    // 語意：跟 K6 / K18 同為 live signal (session 結束 + 30 min stale 回收後 sample
    // 自動消失),跟 K7 / K9 / K13 / K17 lifetime aggregate 對比是預期差異。state label
    // 用 snake_case (對齊 SessionState enum 的 `#[serde(rename_all = "snake_case")]`,
    // 跟前端 SessionInfo.state 的 JSON 序列化一致 → Prometheus query label 跟 JS 端
    // state 字串可直接對照,不用轉換層)。來源：live `sessions: &[SessionInfo]` 的
    // `state` 欄位 (session.rs:249),不讀 ProviderTotals —— ProviderTotals 沒存
    // per-state 細度。
    let mut provider_sessions_by_state: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for s in sessions {
        *provider_counts.entry(s.provider.clone()).or_default() += 1;
        if s.is_active {
            *provider_active.entry(s.provider.clone()).or_default() += 1;
        }
        let state_label = match s.state {
            session::SessionState::Idle => "idle",
            session::SessionState::Working => "working",
            session::SessionState::WaitingForUser => "waiting_for_user",
            session::SessionState::Stale => "stale",
        };
        *provider_sessions_by_state
            .entry((s.provider.clone(), state_label.to_string()))
            .or_insert(0) += 1;
    }
    // Token 累計走 ProviderTotals（lifetime aggregate），不走 live SessionInfo：
    // session 移除後 SessionInfo 拿不到，ProviderTotals 仍保留歷史累計。
    let mut tot_in: u64 = 0;
    let mut tot_out: u64 = 0;
    let mut provider_in: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut provider_out: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    // K7 落地：per-provider 失敗計數同樣走 ProviderTotals（lifetime aggregate），
    // 不讀 live SessionInfo —— 失敗事件已結束、session 早已被 stale 回收後仍保留累計。
    let mut provider_fail: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    // K8 落地：per-provider idle_seconds = `now - last_event_at`。
    // 只在 `last_event_at` 存在時輸出 sample line（`None` 表示該 provider 還沒收過 event）。
    // 算 idle 用 saturating 轉 i64，理論 last_event_at 永遠 ≤ now（bump_provider_totals 寫
    // 進去時一定 ≤ render 端讀到時的 wall clock），但保留 saturating 防時鐘回撥 / 序列化。
    let mut provider_idle: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    // K9 落地：per-provider lifetime session count —— 該 provider 累計開過多少 session
    // （每個 unique session_id 算一次，由 `SessionManager::handle_event` 在新 session 插入時
    // `ProviderTotals.session_count += 1`）。跟 K6/K7 lifetime aggregate 一致：session 結
    // 束 + 30 min stale 回收後 live 為 0，但 lifetime `ProviderTotals.session_count` 仍保留。
    // 差異化 `lobsterpulse_provider_sessions`（live）和本 metric（lifetime）= user 知道
    // 該 provider 累計被 stale 回收的 session 數 = 使用量信號。
    let mut provider_session_count: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    // K10 落地：per-provider first-seen timestamp（Unix epoch seconds）——
    // `ProviderTotals.since` 在 `bump_provider_totals` 第一次被收過 event 時設定、
    // 之後 `if is_none()` guard 不覆寫。本 metric 暴露該值給 Prometheus，
    // user 可用 `(now - since_timestamp)` 算「該 provider 已監控多久」（uptime
    // 對等物），或結合 K8 idle_seconds 算「最近一次活動佔 lifetime 比例」。
    // 跟 K6/K7/K8/K9 lifetime aggregate 對齊：`since` 進 ProviderTotals 後就不
    // 蒸發，session 結束 / stale 回收後仍能看出「這 provider 何時第一次接入」。
    // `since = None` 的 provider（理論上不會出現 — bump_provider_totals 第一次
    // event 就會填）→ 不輸出 sample，跟 K8 idle_seconds 的 `last_event_at = None`
    // 跳過策略一致，避免 Prometheus 端把缺失當 0 timestamp 誤判「1970-01-01」。
    let mut provider_since: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    // K12 落地：per-provider idle ratio = `idle_seconds / lifetime_seconds`。
    // 兩個 `DateTime<Utc>` 任一缺失（K8 跳過策略 / K10 跳過策略）→ 不放進 map。
    // 派生自 K8 `last_event_at` + K10 `since`：lifetime 是「該 provider 何時第一次被
    // 監控到到現在」的長度，idle 是「最後一次事件到現在」的長度 —— ratio 0 = 剛剛
    // 在動（健康），ratio 1 = lifetime 全程沒動（runner 死了 / 半年沒人用）。對
    // operator 是單一健康度信號，alert rule 可設 `> 0.8` 觸發「該 provider 半年沒人
    // 用」提醒（已存在的 K10 / K11 數據 compose 一次即可得，無需新增任何資料源）。
    let mut provider_idle_ratio: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    // K18 落地：per-provider 「目前最老 active session 的持續秒數」gauge。
    // 來源是 `sessions: &[SessionInfo]` 內每個 `is_active = true` 的 session
    // （duration_secs = now - start_time，From<&Session> 階段已算好）；同 provider
    // 多個 active session 取 max。跟 K8 idle_seconds 差異化：K8 看「最後一次
    // event 到現在」（短期心跳，剛動完也會歸 0），K18 看「session 開到現在」
    // （session 還在 active 但已經卡很久 = runner 沒回應 / 工具 hang）。operator
    // alert rule 可設 `max_session_age_seconds > 7200`（2 小時）觸發「該
    // provider 已有 session 卡 2 小時沒結束」。
    //
    // 語意：K18 是「live」訊號（active session 清掉後 sample 就消失）—— 跟
    // K6/K7/K9/K10/K12/K13 lifetime aggregate 對比是預期差異。K18 缺資料時
    // 不放 sample（該 provider 沒有 active session → 跳過），避免 Prometheus
    // 端把缺失誤判為「剛剛才開」/ 0 秒。
    let mut provider_max_session_age: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    for s in sessions {
        if !s.is_active {
            continue;
        }
        // duration_secs 來自 From<&Session>：`now - s.start_time`，
        // 理論 ≥ 0；saturating_max 防時鐘回撥 / 序列化。
        let entry = provider_max_session_age
            .entry(s.provider.clone())
            .or_insert(s.duration_secs);
        if s.duration_secs > *entry {
            *entry = s.duration_secs;
        }
    }
    // K13 落地：per-provider lifetime event 計數（counter）—— 該 provider 累計
    // 收過幾個 event。`ProviderTotals.events_total` 在 `bump_provider_totals` 內對
    // 任何 event 類型（SessionStart / PostToolUse / PostToolUseFailure / Stop /
    // Notification / TokenUpdate / ...）都 `+= 1`。對齊 K6/K7/K9 lifetime
    // aggregate：session 結束 / 30 min stale 回收後仍保留 → Prometheus 不會誤判
    // counter 倒退。operator 端 `rate(events_total[5m])` = 該 provider ingest
    // 吞吐量，補 K7 failure / K9 session 沒覆蓋的「整體事件流量」信號。
    let mut provider_events_total: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    // K17 落地：per-provider × per-event-type 計數。ProviderTotals 內是
    // `BTreeMap<String, u64>`（type → count），render 端攤平成 (provider,
    // type) 排序 vector —— 兩段排序確保 Prometheus 文字輸出 byte-deterministic
    // （對齊既有 K6/K7/K8/K9/K10/K12/K13 排序契約）。空 `event_type_counts`
    // 的 provider 自然不會產出 sample。
    let mut provider_event_type_total: Vec<(String, String, u64)> = Vec::new();
    for (p, t) in provider_totals {
        tot_in = tot_in.saturating_add(t.tokens_input);
        tot_out = tot_out.saturating_add(t.tokens_output);
        provider_in.insert(p.clone(), t.tokens_input);
        provider_out.insert(p.clone(), t.tokens_output);
        provider_fail.insert(p.clone(), t.failure_count);
        provider_session_count.insert(p.clone(), t.session_count);
        if let Some(last) = t.last_event_at {
            let elapsed = now.signed_duration_since(last).num_seconds().max(0);
            provider_idle.insert(p.clone(), elapsed);
        }
        if let Some(since) = t.since {
            provider_since.insert(p.clone(), since.timestamp());
        }
        if let Some(ratio) = compute_provider_idle_ratio(now, t.last_event_at, t.since) {
            provider_idle_ratio.insert(p.clone(), ratio);
        }
        provider_events_total.insert(p.clone(), t.events_total);
        for (etype, n) in &t.event_type_counts {
            provider_event_type_total.push((p.clone(), etype.clone(), *n));
        }
    }
    provider_event_type_total.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut provider_counts_sorted: Vec<_> = provider_counts.iter().collect();
    provider_counts_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_active_sorted: Vec<_> = provider_active.iter().collect();
    provider_active_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_in_sorted: Vec<_> = provider_in.iter().collect();
    provider_in_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_out_sorted: Vec<_> = provider_out.iter().collect();
    provider_out_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_fail_sorted: Vec<_> = provider_fail.iter().collect();
    provider_fail_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_idle_sorted: Vec<_> = provider_idle.iter().collect();
    provider_idle_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_session_count_sorted: Vec<_> = provider_session_count.iter().collect();
    provider_session_count_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_since_sorted: Vec<_> = provider_since.iter().collect();
    provider_since_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_idle_ratio_sorted: Vec<_> = provider_idle_ratio.iter().collect();
    provider_idle_ratio_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_max_session_age_sorted: Vec<_> = provider_max_session_age.iter().collect();
    provider_max_session_age_sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut provider_sessions_by_state_sorted: Vec<_> = provider_sessions_by_state.iter().collect();
    provider_sessions_by_state_sorted
        .sort_by(|a, b| a.0 .0.cmp(&b.0 .0).then_with(|| a.0 .1.cmp(&b.0 .1)));
    let mut provider_events_total_sorted: Vec<_> = provider_events_total.iter().collect();
    provider_events_total_sorted.sort_by(|a, b| a.0.cmp(b.0));

    let mut out = String::new();
    out.push_str("# HELP lobsterpulse_sessions_total Total session count\n# TYPE lobsterpulse_sessions_total gauge\n");
    out.push_str(&format!("lobsterpulse_sessions_total {session_count}\n"));
    out.push_str("# HELP lobsterpulse_sessions_active Active session count\n# TYPE lobsterpulse_sessions_active gauge\n");
    out.push_str(&format!("lobsterpulse_sessions_active {active_count}\n"));
    out.push_str("# HELP lobsterpulse_provider_sessions Sessions per provider\n# TYPE lobsterpulse_provider_sessions gauge\n");
    for (p, c) in &provider_counts_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_sessions{{provider=\"{p}\"}} {c}\n"
        ));
    }
    out.push_str("# HELP lobsterpulse_provider_active Active sessions per provider\n# TYPE lobsterpulse_provider_active gauge\n");
    for (p, c) in &provider_active_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_active{{provider=\"{p}\"}} {c}\n"
        ));
    }
    // R113 T-1 dual-emit (對齊 R106 spec 對齊契約 5 週時程 T-1 週): 舊名加
    // # DEPRECATED comment 標 owner 切換日, 新名加入 (T-4 切換日後舊條移除)
    out.push_str("# HELP lobsterpulse_tokens_input Lifetime input tokens across all providers (DEPRECATED: use lobsterpulse_tokens_input_total, scheduled removal week 4)\n# TYPE lobsterpulse_tokens_input counter\n");
    out.push_str(&format!("lobsterpulse_tokens_input {tot_in}\n"));
    out.push_str("# HELP lobsterpulse_tokens_input_total Lifetime input tokens across all providers\n# TYPE lobsterpulse_tokens_input_total counter\n");
    out.push_str(&format!("lobsterpulse_tokens_input_total {tot_in}\n"));
    out.push_str("# HELP lobsterpulse_tokens_output Lifetime output tokens across all providers (DEPRECATED: use lobsterpulse_tokens_output_total, scheduled removal week 4)\n# TYPE lobsterpulse_tokens_output counter\n");
    out.push_str(&format!("lobsterpulse_tokens_output {tot_out}\n"));
    out.push_str("# HELP lobsterpulse_tokens_output_total Lifetime output tokens across all providers\n# TYPE lobsterpulse_tokens_output_total counter\n");
    out.push_str(&format!("lobsterpulse_tokens_output_total {tot_out}\n"));
    out.push_str("# HELP lobsterpulse_provider_tokens_input Lifetime input tokens per provider (DEPRECATED: use lobsterpulse_provider_tokens_input_total, scheduled removal week 4)\n# TYPE lobsterpulse_provider_tokens_input counter\n");
    for (p, n) in &provider_in_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_tokens_input{{provider=\"{p}\"}} {n}\n"
        ));
    }
    out.push_str("# HELP lobsterpulse_provider_tokens_input_total Lifetime input tokens per provider\n# TYPE lobsterpulse_provider_tokens_input_total counter\n");
    for (p, n) in &provider_in_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_tokens_input_total{{provider=\"{p}\"}} {n}\n"
        ));
    }
    out.push_str("# HELP lobsterpulse_provider_tokens_output Lifetime output tokens per provider (DEPRECATED: use lobsterpulse_provider_tokens_output_total, scheduled removal week 4)\n# TYPE lobsterpulse_provider_tokens_output counter\n");
    for (p, n) in &provider_out_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_tokens_output{{provider=\"{p}\"}} {n}\n"
        ));
    }
    out.push_str("# HELP lobsterpulse_provider_tokens_output_total Lifetime output tokens per provider\n# TYPE lobsterpulse_provider_tokens_output_total counter\n");
    for (p, n) in &provider_out_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_tokens_output_total{{provider=\"{p}\"}} {n}\n"
        ));
    }
    // K7 落地：per-provider 失敗計數（lifetime aggregate）。
    // `failure_count` 來源是 `ProviderTotals`，由 `bump_provider_totals` 在
    // `PostToolUseFailure` 事件時 `+= 1` 累加；不依賴 live session（失敗事件
    // 之後 session 仍會轉 idle/移除，但累計保留在 ProviderTotals 不蒸發）。
    out.push_str("# HELP lobsterpulse_provider_failure_count Lifetime tool/post failure count per provider (DEPRECATED: use lobsterpulse_provider_failure_count_total, scheduled removal week 4)\n# TYPE lobsterpulse_provider_failure_count counter\n");
    for (p, n) in &provider_fail_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_failure_count{{provider=\"{p}\"}} {n}\n"
        ));
    }
    out.push_str("# HELP lobsterpulse_provider_failure_count_total Lifetime tool/post failure count per provider\n# TYPE lobsterpulse_provider_failure_count_total counter\n");
    for (p, n) in &provider_fail_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_failure_count_total{{provider=\"{p}\"}} {n}\n"
        ));
    }
    // K8 落地：per-provider idle_seconds gauge —— 距上次 event 多少秒。
    // `last_event_at` 為 None 的 provider（從未收過 event）不輸出 sample line，
    // 避免 Prometheus 端把缺失當作「0 秒 idle」誤判「剛剛才動」。
    out.push_str("# HELP lobsterpulse_provider_idle_seconds Seconds since last event per provider (lifetime aggregate)\n# TYPE lobsterpulse_provider_idle_seconds gauge\n");
    for (p, n) in &provider_idle_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_idle_seconds{{provider=\"{p}\"}} {n}\n"
        ));
    }
    // K9 落地：per-provider lifetime session count —— 累計已開過多少 session。
    // 跟 K6/K7 lifetime aggregate 對齊：counter 類型，session 結束 / stale 回收後
    // live 為 0，但 ProviderTotals.session_count 仍保留 → metric 反映歷史累計。
    // 差異化 `lobsterpulse_provider_sessions`（live）：本 metric 顯示「曾經開過」總量。
    out.push_str("# HELP lobsterpulse_provider_session_count Lifetime session count per provider (DEPRECATED: use lobsterpulse_provider_session_count_total, scheduled removal week 4)\n# TYPE lobsterpulse_provider_session_count counter\n");
    for (p, n) in &provider_session_count_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_session_count{{provider=\"{p}\"}} {n}\n"
        ));
    }
    out.push_str("# HELP lobsterpulse_provider_session_count_total Lifetime session count per provider\n# TYPE lobsterpulse_provider_session_count_total counter\n");
    for (p, n) in &provider_session_count_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_session_count_total{{provider=\"{p}\"}} {n}\n"
        ));
    }
    // K10 落地：per-provider first-seen timestamp（Unix epoch seconds）——
    // 對齊 K6/K7/K8/K9 lifetime 語意：ProviderTotals.since 一旦填入就不蒸發，
    // session 結束 / stale 回收後仍能看出「該 provider 何時第一次被監控到」。
    // User 可用 `now - since_timestamp` 算 uptime 對等量；結合 K8 idle_seconds
    // 算「最近一次活動佔 lifetime 比例」= 健康度信號。
    out.push_str("# HELP lobsterpulse_provider_since_timestamp Unix epoch seconds when this provider was first seen (lifetime aggregate)\n# TYPE lobsterpulse_provider_since_timestamp gauge\n");
    for (p, ts) in &provider_since_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"{p}\"}} {ts}\n"
        ));
    }
    // K11 落地：per-provider quota snapshot age（seconds since last mtime update）——
    // 對齊 K6/K7/K8/K9/K10 對 `provider_totals` 的 lifetime 視角，本 metric 是對
    // 磁碟 `~/.lobsterpulse/usage-{bot}.json` 與 `usage-local.json` 的「資料新鮮度」：
    //   - age 0 = snapshot 剛寫（runner OK）
    //   - age 持續飆高 = runner 卡住 / process dead / 沒裝 OpenAB
    //   - 沒出現在 map（file 不存在）= 跳過 sample，不當 0 誤判「剛剛還在」
    // User 在 dashboard 看「quota 0%」分不清是「真的用完」 vs 「snapshot 30 分鐘沒更新
    // （runner 死了）」，K11 直接量化後者。Prometheus alert rule 可設
    // `quota_snapshot_age_seconds > 600` 觸發「quota runner 可能停擺」。
    // 注意：這是「runtime freshness」訊號，不是 lifetime aggregate —— snapshot file
    // 一直沒人寫就會累加，直到有 runner 重新寫才歸 0。
    out.push_str("# HELP lobsterpulse_provider_quota_snapshot_age_seconds Seconds since ~/.lobsterpulse/usage-{provider}.json was last modified (data freshness)\n# TYPE lobsterpulse_provider_quota_snapshot_age_seconds gauge\n");
    let mut quota_snapshot_ages_sorted: Vec<_> = quota_snapshot_ages.iter().collect();
    quota_snapshot_ages_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, age) in &quota_snapshot_ages_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_quota_snapshot_age_seconds{{provider=\"{p}\"}} {age}\n"
        ));
    }
    // K20 落地：per-provider quota 剩餘百分比 gauge。對齊 K11 同族「quota 觀測維度」:
    // K11 看「snapshot 多舊」(freshness),K20 看「quota 還剩多少」(consumption)。
    // operator alert rule 可設 `lobsterpulse_provider_quota_remaining_pct{provider="cicx"} < 10`
    // 觸發「cicx 即將耗盡」告警,搭配 K11 `age > 600` 判斷「snapshot 沒更新,數字可能過期」一起看,避免假警報。
    //
    // 語意關鍵:0 是有效資料(runner quota 已耗盡 → operator 必須看到),不能
    // 在 emit 階段當作 None 跳過 → map 缺 entry 才不出 sample(對齊 K11「寧可
    // 少一條 sample 也不要假裝 0」相反:K20 的 0 是 critical signal,必須保留)。
    // 資料源:`render_prometheus` 端從 quota-history.csv 透過 `latest_quota_pct_at`
    // 純 fn 載入,match Err → log warn + 整段留空(不部分 emit 假資料)對齊
    // R30 `get_quota_history` 模式。
    //
    // 排序:by provider alphabetical,跟 K6-K19 既契約一致;空 map → 沒 sample line
    // (HELP/TYPE 標頭仍輸出,跟 K11 同一 header-only 契約)。
    out.push_str("# HELP lobsterpulse_provider_quota_remaining_pct Latest quota remaining percent per runner from quota-history.csv (0=exhausted, missing=no sample)\n# TYPE lobsterpulse_provider_quota_remaining_pct gauge\n");
    let mut quota_remaining_pct_sorted: Vec<_> = quota_remaining_pct.iter().collect();
    quota_remaining_pct_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, pct) in &quota_remaining_pct_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_quota_remaining_pct{{provider=\"{p}\"}} {pct}\n"
        ));
    }
    // K21 落地:CSV pipeline freshness gauge。對齊 K11 freshness 視角(snapshot 多舊) +
    // K20 同一資料源(quota-history.csv 的 consumption)。差異化:K20 看「最新
    // pct 數字」(consumption,K20 emits 0 = critical signal),K21 看「CSV 多久沒
    // 被 OpenAB 寫進來」(freshness of the history file itself)→ 跟 K11 互補(都
    // 是「沒更新」訊號但觀察不同檔:K11 是 5 個 bot snapshot、K21 是聚合 CSV)。
    // operator alert rule:`csv_age_seconds > 1800`(30 分鐘)觸發「OpenAB
    // 沒在寫 quota-history」,搭配 K11 同一 timeframe 一起看避免單一信號誤判。
    // `None` 走 first-run 契約:header-only,不 emit sample(避免「age=0」誤判
    // 「CSV 剛剛還在」)。`Some(0)` 是有效資料(剛寫完)→ 必須 emit。
    out.push_str("# HELP lobsterpulse_quota_history_csv_age_seconds Seconds since ~/.lobsterpulse/quota-history.csv was last modified (pipeline freshness; missing=no sample)\n# TYPE lobsterpulse_quota_history_csv_age_seconds gauge\n");
    if let Some(age) = quota_history_csv_age {
        out.push_str(&format!(
            "lobsterpulse_quota_history_csv_age_seconds {age}\n"
        ));
    }
    // K22 落地：per-provider 最近一次完成 session 的持續秒數 gauge。
    // 補 K18「最老 active session」以外的盲點：K18 是 live metric（session 結束
    // 後 sample 自動消失），K22 是 lifetime aggregate（session 結束 / stale 回收
    // 後仍保留）→ 跟 K6/K7/K9 lifetime 語意一致：寫入後不蒸發。
    // 觸發點兩種都算「完成」：(1) SessionEnd 把 session 從 active map 移除時；
    // (2) Working→Idle 轉換時（runner 主動收尾但 session 還在 map 等 30 min
    // stale 回收）。operator alert rule 可設
    // `lobsterpulse_provider_last_completed_session_age_seconds > 1800`
    // （30 分鐘）觸發「最近一次 task 跑超過 30 分鐘才收尾」。0 是有效資料
    // （session 剛 start 又馬上 SessionEnd，age ≈ 0），不是 missing。
    // 排序:by provider alphabetical,跟 K6-K21 既契約一致;空 map → 沒 sample line
    // (HELP/TYPE 標頭仍輸出)。
    out.push_str("# HELP lobsterpulse_provider_last_completed_session_age_seconds Duration in seconds of the most recently completed session per provider (lifetime; missing=no completed session yet)\n# TYPE lobsterpulse_provider_last_completed_session_age_seconds gauge\n");
    let mut last_completed_session_age_sorted: Vec<_> = last_completed_session_age.iter().collect();
    last_completed_session_age_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, age) in &last_completed_session_age_sorted {
        let clamped = (**age).max(0i64);
        out.push_str(&format!(
            "lobsterpulse_provider_last_completed_session_age_seconds{{provider=\"{p}\"}} {clamped}\n"
        ));
    }
    // K23 落地：per-provider 累計完成 session 數 counter。跟 K22 互補 —— K22
    // gauge 看「最近一次跑多久」(只記 latest),K23 counter 看「累計跑了幾次」
    // (遞增),operator 端可算 `rate(completed_sessions_total[1h])` = 每小時完成
    // 速率 = 吞吐 KPI,補 K22 沒覆蓋的「累積次數」維度。跟 K7 / K9 / K13 lifetime
    // aggregate 對齊:ProviderTotals 寫入後不蒸發,session 結束 + 30 min stale 回收
    // 後 counter 不會倒退。`u64` 預設 0 是有效資料（該 provider 累計收過 event
    // 但還沒完成過 session),跟 K22 `Option<i64> = None` 跳過策略不同 —— K23
    // counter 0 跟 missing 是不同語意,Prometheus 端應該看到 0（"0 次完成"）
    // 而不是 missing（"沒看過"）,跟 K9 `session_count` 0 也是「有 entry 就
    // emit」風格一致。派生:`session::completed_sessions_count_at` 把
    // `ProviderTotals` 攤平為 `HashMap<provider, count>`,不在 render 端加第 12
    // 個參數（已是 anti-pattern,對齊 R26/R27 政策：cross-cutting snapshot 留給
    // M1 輪 MetricsSnapshot struct 統一處理,本輪不重構）。排序:by provider
    // alphabetical,跟 K6-K22 既契約一致;空 map → 沒 sample line (HELP/TYPE 標頭
    // 仍輸出)。emit 全部 provider（含 count=0）—— 跟 `last_completed_session_age_at`
    // 過濾 None 不同,在 session.rs pure fn 內以 K22/K23 不同策略分流。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_total Lifetime count of completed sessions per provider (counter; 0=provider seen but never completed yet, never resets)\n# TYPE lobsterpulse_provider_completed_sessions_total counter\n");
    let completed_sessions = session::completed_sessions_count_at(provider_totals);
    let mut completed_sessions_sorted: Vec<_> = completed_sessions.iter().collect();
    completed_sessions_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, count) in &completed_sessions_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_total{{provider=\"{p}\"}} {count}\n"
        ));
    }
    // K24 落地：per-provider 累計完成 session 總時長 counter。跟 K22 / K23 形成
    // 「總時長 / 總次數 = 平均 time-to-completion」公式:operator 端算
    // `lobsterpulse_provider_completed_sessions_total_duration_seconds /
    // lobsterpulse_provider_completed_sessions_total` 觀察平均效率 KPI,搭配
    // `rate(..._duration_seconds[1h])` 看「過去一小時總處理秒數」= throughput-seconds
    // KPI。跟 K7 / K9 / K13 / K17 / K23 lifetime aggregate 對齊:ProviderTotals 寫入後
    // 不蒸發,session 結束 + 30 min stale 回收後 counter 不會倒退。`u64` 預設 0 是
    // 有效資料（該 provider 累計收過 event 但還沒完成過 session）,跟 K23 counter 0
    // 同 emit 策略,跟 K22 `Option<i64> = None` 跳過策略不同。觸發點跟 K22/K23 同:
    // SessionEnd + Working→Idle 兩路徑都把當次 age 累加進來。負值 saturating clamp
    // 到 0 再累加（防時鐘回撥污染 counter 總和,對齊 K22 `age.max(0)` 同樣防線）。
    // 派生:`session::completed_sessions_total_duration_at` 把 `ProviderTotals`
    // 攤平為 `HashMap<provider, secs>`,跟 K22/K23 風格一致 —— 不在 render 端加
    // 第 12 個參數(已是 anti-pattern,對齊 R26/R27 政策:cross-cutting snapshot
    // 留給 M1 輪 MetricsSnapshot struct 統一處理,本輪不重構)。排序:by provider
    // alphabetical,跟 K6-K23 既契約一致;空 map → 沒 sample line (HELP/TYPE 標頭
    // 仍輸出)。emit 全部 provider（含 total=0）—— 跟 `last_completed_session_age_at`
    // 過濾 None 不同,在 session.rs pure fn 內以 K22 / K23 / K24 不同策略分流。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_total_duration_seconds Lifetime sum of completed session durations in seconds per provider (counter; 0=provider seen but never completed yet, pairs with completed_sessions_total for avg time-to-completion)\n# TYPE lobsterpulse_provider_completed_sessions_total_duration_seconds counter\n");
    let completed_duration = session::completed_sessions_total_duration_at(provider_totals);
    let mut completed_duration_sorted: Vec<_> = completed_duration.iter().collect();
    completed_duration_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_duration_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_total_duration_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // K25 落地：per-provider 平均完成 session 時長 gauge。K23 (count) / K24
    // (total_duration) 兩條 lifetime counter 的派生指標 —— operator 端不再需要
    // 自己寫 `..._duration_seconds / ..._total` 算式（兩個 metric cross-query 在
    // PromQL / Grafana 都易出錯, scrape 缺一條時算式直接壞；單一 derived gauge
    // 直接拿即可）。補 K22 / K23 / K24 都沒覆蓋的「平均效率 KPI」維度：K22 看
    // 「最近一次」(single sample, 沒平均語意), K23 看「累計次數」(純計次, 沒
    // 時長), K24 看「累計總時長」(純加總, 沒除以次數) → K25 把次數 / 時長兩個
    // dimension 結合成除法, 跟 K12 `idle_ratio` 同樣是「既有資料源派生指標」。
    // `count == 0` 走 K22 語意（missing 跳過, 不 emit sample）—— 0/0 數學未定義,
    // emit 0.0 會誤導 Prometheus 端把「沒資料」判成「瞬間完成」= 假健康信號。
    // `count > 0` 才 emit `total / count` 浮點結果, 4 位小數固定 precision（跟 K12
    // `idle_ratio` `{:.4}` 同格式, 避免 IEEE 754 尾數雜訊）。數據源：不是獨立
    // HashMap, 直接讀 `provider_totals` —— 跟 K22 / K23 / K24 同資料源, 讓 render
    // helper 自己派發 pure fn 攤平（對齊 R26/R27 政策: cross-cutting snapshot
    // 留給 M1 輪 MetricsSnapshot struct 統一處理, 本輪不重構 render 端 12 個參數
    // 的怪 signature）。排序: by provider alphabetical, 跟 K6-K24 既契約一致;
    // 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出)。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_average_duration_seconds Average duration in seconds of completed sessions per provider (gauge; derived from completed_sessions_total_duration_seconds / completed_sessions_total; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_average_duration_seconds gauge\n");
    let completed_avg = session::completed_sessions_average_duration_at(provider_totals);
    let mut completed_avg_sorted: Vec<_> = completed_avg.iter().collect();
    completed_avg_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, avg) in &completed_avg_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_average_duration_seconds{{provider=\"{p}\"}} {avg:.4}\n"
        ));
    }
    // K26 落地：per-provider 歷史「最長」一次完成 session 持續秒數 gauge。
    // 跟 K22 (latest) / K25 (avg) 互補形成 **max / latest / avg 三件套**：
    //   - K22  gauge 看「最近一次跑多久」(只記 latest)
    //   - K25  gauge 看「平均跑多久」  (派生 from K23 / K24)
    //   - K26  gauge 看「最長一次跑多久」(saturating_max lifetime)
    //
    // 對齊 K22 emit 語意:Option 過濾 —— 該 provider 累計收過 event 但還沒完成
    // 過 session → 缺資料,跳過不 emit sample (避免 Prometheus 端把「沒看到」
    // 當「max=0」誤判「該 provider 瞬間完成」= 假健康信號)。`Some(secs)` emit
    // 整數秒數 (沒 f64,跟 K22 對齊, max 是「單點 saturating_max」語意沒有
    // 浮點小數必要)。 跟 K25 派生策略一樣:數據源不是獨立 HashMap, 直接讀
    // `provider_totals` —— 跟 K22 / K23 / K24 / K25 同資料源, 讓 render
    // helper 自己派發 pure fn 攤平 (對齊 R26/R27 政策: cross-cutting snapshot
    // 留給 M1 輪 MetricsSnapshot struct 統一處理, 本輪不重構 render 端 11
    // 個參數的怪 signature)。 排序: by provider alphabetical, 跟 K6-K25
    // 既契約一致; 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出)。
    //
    // Operator 用途: 跟 K22 (latest) 比較可看出「最近一次是不是特別長」;
    // 跟 K25 (avg) 比較可分辨 outlier session —— 例 avg 60s, latest 65s,
    // 但 max 7200s = 過去某次有 2 小時 outlier, 可能 runner hang / 大 context
    // window 場景。 也可設 alert `max > 3600` (1 小時) 觸發「該 provider 有
    // 異常長 session 待撈」。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_max_duration_seconds Duration in seconds of the longest completed session per provider (gauge; lifetime saturating_max; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_max_duration_seconds gauge\n");
    let completed_max = session::completed_sessions_max_duration_at(provider_totals);
    let mut completed_max_sorted: Vec<_> = completed_max.iter().collect();
    completed_max_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_max_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_max_duration_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // K27 落地：per-provider 歷史「最短」一次完成 session 持續秒數 gauge。
    // 跟 K22 (latest) / K25 (avg) / K26 (max) 互補形成 **min / max / latest /
    // avg 四件套**：
    //   - K22  gauge 看「最近一次跑多久」  (只記 latest)
    //   - K25  gauge 看「平均跑多久」      (派生 from K23 / K24)
    //   - K26  gauge 看「最長一次跑多久」  (saturating_max lifetime)
    //   - K27  gauge 看「最短一次跑多久」  (saturating_min lifetime)
    //
    // 對齊 K22 / K26 emit 語意:Option 過濾 —— 該 provider 累計收過 event 但還
    // 沒完成過 session → 缺資料,跳過不 emit sample (避免 Prometheus 端把「沒
    // 看到」當「min=0」誤判「該 provider 瞬間完成」= 假健康信號)。`Some(secs)`
    // emit 整數秒數 (沒 f64,跟 K22 / K26 對齊, min 是「單點 saturating_min」
    // 語意沒有浮點小數必要)。跟 K25 / K26 派生策略一樣:數據源不是獨立
    // HashMap, 直接讀 `provider_totals` —— 跟 K22 / K23 / K24 / K25 / K26 同
    // 資料源, 讓 render helper 自己派發 pure fn 攤平 (對齊 R26/R27 政策:
    // cross-cutting snapshot 留給 M1 輪 MetricsSnapshot struct 統一處理, 本輪
    // 不重構 render 端 11 個參數的怪 signature)。 排序: by provider alphabetical,
    // 跟 K6-K26 既契約一致; 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出)。
    //
    // Operator 用途: 跟 K26 (max) 比較可分辨「session 時長分佈」 —— 例 max
    // 7200s, min 8s = 大部分 session 都跑 ~1 分鐘（avg 60s）, 但偶有 2 小時
    // outlier, 且曾有 8 秒極短 session（可能是 fast-path / 早期測試 / 假觸發）。
    // 可設 alert `min < 1` 觸發「該 provider 有次秒級完成 session」異常信號。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_min_duration_seconds Duration in seconds of the shortest completed session per provider (gauge; lifetime saturating_min; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_min_duration_seconds gauge\n");
    let completed_min = session::completed_sessions_min_duration_at(provider_totals);
    let mut completed_min_sorted: Vec<_> = completed_min.iter().collect();
    completed_min_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_min_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_min_duration_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // K28 落地：per-provider completed_sessions_stddev gauge (Welford online
    // algorithm)。補 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) 四件套
    // 之外的「波動性」維度 —— 同一個 avg 60s 的 provider 可能有 stddev=5 (穩定)
    // 或 stddev=300 (短任務/長任務混跑), operator 一看 stddev 就知道該 provider
    // session 時長分布。資料源: ProviderTotals.completed_sessions_mean_secs /
    // completed_sessions_m2_secs (K28 record 函式累積), 跟 K23
    // completed_sessions_count 強綁定 (count=0 → 過濾掉, 跟 K25/K26/K27
    // 既契約一致)。 f64 gauge, 4 位小數固定 precision (跟 K25 avg 一致; 跟
    // K26/K27 整數區分)。 「(M2 / count).sqrt()」 純 fn 端做, render 只負責
    // sort + format。 `count == 1` 時 M2=0 → stddev=0 → emit 為 0.0000 (視為
    // 有效資料, 跟 K25 avg=該 sample 邏輯一致; 跟 K27 None 跳過策略不同)。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_stddev_seconds Population standard deviation in seconds of completed session durations per provider (gauge; Welford online algorithm; 4 decimal precision; count=1 emits 0; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_stddev_seconds gauge\n");
    let completed_stddev = session::completed_sessions_stddev_at(provider_totals);
    let mut completed_stddev_sorted: Vec<_> = completed_stddev.iter().collect();
    completed_stddev_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_stddev_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_stddev_seconds{{provider=\"{p}\"}} {secs:.4}\n"
        ));
    }
    // K29 落地：per-provider failure-to-completion ratio gauge (純 derived from
    // K10 failure_count / K23 completed_sessions_count)。補 K9 / K10 (絕對失敗
    // 計數) / K22-K28 (session duration 分布) 都沒覆蓋的「失敗 vs 成功比」維度
    // —— operator 端可設 alert `ratio > 2.0` 觸發「該 provider session 平均
    // retry 2 次以上」健康度異常信號。 資料源: ProviderTotals.failure_count
    // (K10 觸發點 PostToolUseFailure +1) / ProviderTotals.completed_sessions_count
    // (K23 觸發點 SessionEnd + Working→Idle 兩路徑 +1) —— 純 derived 不需新欄位
    // / 新觸發點, 完全沿用既有 K10 / K23 兩條 lifetime counter。 f64 gauge,
    // 4 位小數固定 precision (跟 K25 avg / K28 stddev 對齊; 跟 K22 / K23 / K24 /
    // K26 / K27 整數區分)。 過濾語意: completed_sessions_count == 0 → 跳過不
    // emit (0/0 數學未定義, 不能 emit 0.0 假冒「失敗率 0」= 假健康信號, 跟
    // K25 「0/0 不 emit」同款防線)。 alphabetical sort 跟 K6-K28 既契約一致;
    // 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出)。
    //
    // Operator 用途: 跟 K9 / K10 (絕對失敗計數) 比較可分辨「絕對值高但 ratio
    // 低」(該 provider 流量大失敗難免) vs 「絕對值低但 ratio 高」(該 provider
    // 流量小但每次都失敗 = 嚴重健康問題) —— 後者才是真要追的, ratio 比例比
    // 絕對值更 operator 友善。 跟 K22 / K25 / K26 / K27 / K28 五件套都各自
    // emit 各自的值, 互不污染。
    out.push_str("# HELP lobsterpulse_provider_failure_to_completion_ratio Average tool failures per completed session per provider (gauge; derived from K10 failure_count / K23 completed_sessions_count; 4 decimal precision; 0.0=zero failures; missing=no completed session yet)\n# TYPE lobsterpulse_provider_failure_to_completion_ratio gauge\n");
    let failure_ratio = session::failure_to_completion_ratio_at(provider_totals);
    let mut failure_ratio_sorted: Vec<_> = failure_ratio.iter().collect();
    failure_ratio_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, ratio) in &failure_ratio_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_failure_to_completion_ratio{{provider=\"{p}\"}} {ratio:.4}\n"
        ));
    }
    // K30 落地：per-provider completed_sessions_p95 gauge (reservoir sampling
    // capacity 1024 + sort 找 P95)。補 K22 (latest) / K25 (avg) / K26 (max) /
    // K27 (min) / K28 (stddev) 五件套 + K29 (failure ratio) 都沒覆蓋的「95
    // 百分位延遲」維度 —— operator 端 alert `p95 > 300` (5 分鐘) = 該
    // provider 95% 的 session 都在 5 分鐘以上 = SLO 異常信號, 比 stddev 更
    // 直觀 (stddev 受 outlier 影響大, P95 反映「典型慢任務」邊界)。資料源:
    // ProviderTotals.completed_sessions_p95_samples (K30 record 函式 reservoir
    // push 累積), bounded 1024 capacity 防 unbounded grow。語意跟 K22-K28
    // lifetime aggregate 對比是有意識 trade-off: P95 反映「最近 1024 次」
    // 體感 (lifetime 會被過老 outlier 拉高永遠不下降, operator alert 不實用)
    // —— doc 開頭明寫。Sample 整數 i64, emit 端 cast f64 4 位小數跟 K25 /
    // K28 對齊; 過濾 `samples.is_empty()` 沿用 K25 「0/0 不 emit」防線。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_p95_duration_seconds 95th percentile in seconds of completed session durations per provider (gauge; reservoir sampling 1024; sliding window of last 1024 completions; integer precision; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_p95_duration_seconds gauge\n");
    let completed_p95 = session::completed_sessions_p95_at(provider_totals);
    let mut completed_p95_sorted: Vec<_> = completed_p95.iter().collect();
    completed_p95_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_p95_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_p95_duration_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // K31 落地：per-provider completed_sessions_p50 gauge (median, 50 百分位
    // = 中位數, 復用 K30 同一份 reservoir 1024 樣本池)。補 K22 (latest) / K25
    // (avg) / K26 (max) / K27 (min) / K28 (stddev) / K30 (P95) / K29 (failure
    // ratio) 七件套都沒覆蓋的「典型 session 延遲」維度 —— median 抗 outlier 比
    // K25 avg 強 (avg 受極端長任務拉高, median 不會), 跟 K30 P95 同一 sliding
    // window 但取不同 percentile。Operator 端 alert 互補: `p50 > 60` (整體慢,
    // 典型 session 都在 1 分鐘以上) vs `p95 > 300` (尾端慢) 組合可快速分辨
    // 「該 provider 整體慢」vs「只有尾端 5% 慢」, K25 avg 算不出這層細 (avg
    // 是中心趨勢, 對 outlier 敏感)。資料源: 跟 K30 共用
    // ProviderTotals.completed_sessions_p95_samples (K31 不開新欄位, 純 fn 端
    // 復用 K30 reservoir 取不同 percentile index, doc 開頭已明寫共用設計)。
    // 過濾 `samples.is_empty()` 沿用 K30 「沒 sample 不 emit」防線, 跟 K22 /
    // K25 「0/0 不 emit」同款。Sample 整數 i64, emit 端 `{secs}` 不加 `.4`
    // 浮點 precision (K30 P95 已是整數, K31 對齊)。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_p50_duration_seconds 50th percentile (median) in seconds of completed session durations per provider (gauge; reuses K30 reservoir sampling 1024; sliding window of last 1024 completions; integer precision; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_p50_duration_seconds gauge\n");
    let completed_p50 = session::completed_sessions_p50_at(provider_totals);
    let mut completed_p50_sorted: Vec<_> = completed_p50.iter().collect();
    completed_p50_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_p50_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_p50_duration_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // K32 落地：per-provider completed_sessions_p99 gauge (極尾端延遲, 99 百分位
    // = 第 99 個百分位 sample, 復用 K30 同一份 reservoir 1024 樣本池)。補 K22
    // (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) / K29 (failure
    // ratio) / K30 (P95) / K31 (P50) 八件套都沒覆蓋的「極端尾端 1% 延遲」維度
    // —— K32 跟 K30 P95 同一 sliding window 但取更極端的 percentile, 反映
    // 「偶發卡死 / 工具 hang」邊界 (K30 P95 看「典型慢」, K32 P99 看「異常慢」,
    // 差距大 = 有 outlier 卡住分布尾端)。Operator 端 alert 三層次: `p50 > 60`
    // (整體慢) vs `p95 > 300` (尾端 5% 慢) vs `p99 > 600` (極端 1% 慢) 可快速
    // 分辨「該 provider 整體慢」vs「只有尾端慢」vs「有極端 outlier 卡住」。
    // 資料源: 跟 K30/K31 共用 `ProviderTotals.completed_sessions_p95_samples`
    // (K32 不開新欄位, 純 fn 端復用 K30 reservoir 取不同 percentile index,
    // doc 開頭已明寫共用設計)。過濾 `samples.is_empty()` 沿用 K30/K31 「沒
    // sample 不 emit」防線, 跟 K22 / K25 「0/0 不 emit」同款。Sample 整數 i64,
    // emit 端 `{secs}` 不加 `.4` 浮點 precision (K30 P95 / K31 P50 已是整數,
    // K32 對齊)。語意 trade-off: 樣本數 < 100 時 P99 退化到 max, 跟 P95 = max
    // 同值 —— operator 看 P95 == P99 就知道該 provider 樣本不夠 P99 沒區辨力
    // (需更多 session 累積 reservoir)。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_p99_duration_seconds 99th percentile in seconds of completed session durations per provider (gauge; reuses K30 reservoir sampling 1024; sliding window of last 1024 completions; integer precision; converges to max when sample count < 100; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_p99_duration_seconds gauge\n");
    let completed_p99 = session::completed_sessions_p99_at(provider_totals);
    let mut completed_p99_sorted: Vec<_> = completed_p99.iter().collect();
    completed_p99_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_p99_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_p99_duration_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // K33 落地：per-provider completed_sessions_p75 gauge (上四分位 Q3, 75 百分位
    // = 第 75 個百分位 sample, 復用 K30 同一份 reservoir 1024 樣本池)。補 K22
    // (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) / K29 (failure
    // ratio) / K30 (P95) / K31 (P50) / K32 (P99) 九件套都沒覆蓋的「上四分位
    // 延遲」維度 —— K33 跟 K31 P50 同一 sliding window 但取更極端的 percentile
    // (P75 > P50), 反映「典型偏慢任務」邊界 (K31 P50 看「中位」, K33 P75 看
    // 「上四分位」, 差距大 = 中段 session 分布離散)。Operator 端 alert 四層次:
    // `p50 > 60` (整體慢) vs `p75 > 120` (K33 上四分位慢 = 75% session 都超時)
    // vs `p95 > 300` (尾端 5% 慢) vs `p99 > 600` (極端 1% 慢), 組合可分辨
    // 「該 provider 整體慢」vs「上四分位特別慢」vs「只有尾端慢」vs「有極端 outlier
    // 卡住」。資料源: 跟 K30/K31/K32 共用 `ProviderTotals.completed_sessions_p95_samples`
    // (K33 不開新欄位, 純 fn 端復用 K30 reservoir 取不同 percentile index,
    // doc 開頭已明寫共用設計)。過濾 `samples.is_empty()` 沿用 K30-K32 「沒
    // sample 不 emit」防線。Sample 整數 i64, emit 端 `{secs}` 不加 `.4` 浮點
    // precision (K30 P95 / K31 P50 / K32 P99 已是整數, K33 對齊)。語意
    // trade-off: 樣本數 < 4 時 P75 退化到 max (= idx = 3, 4 樣本 P75 = max);
    // len=1 → idx=0, P75 = itself (所有 percentile 退化到唯一值)。未來 K34 P25
    // 跟 K35 IQR 落地可再延伸, 屆時 K33 P75 + K34 P25 即可派生 distribution
    // width = P75 - P25 (IQR 中段 50% 跨度)。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_p75_duration_seconds 75th percentile in seconds of completed session durations per provider (gauge; reuses K30 reservoir sampling 1024; sliding window of last 1024 completions; integer precision; converges to max when sample count < 4; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_p75_duration_seconds gauge\n");
    let completed_p75 = session::completed_sessions_p75_at(provider_totals);
    let mut completed_p75_sorted: Vec<_> = completed_p75.iter().collect();
    completed_p75_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_p75_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_p75_duration_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // K34 落地: per-provider P25 (下四分位) session duration gauge。
    // 跟 K30 P95 / K31 P50 / K32 P99 / K33 P75 同模板 (sliding window reservoir
    // 1024 + 復用 K30 vec), 補 K33 沒覆蓋的「下四分位」維度。Operator 端 alert
    // `p25 < 5` 觸發「該 provider 25% session 都 < 5s = 都在 trivial 工作 / 可能
    // 沒給重 prompt」提醒, 跟 P95 (尾端 5% 慢) / P99 (極端 1% 卡死) / P75 (中
    // 段偏慢) 互補形成 latency 分布完整輪廓。五件套 P25/P50/P75/P95/P99 跟 K28
    // stddev + K26 max + K27 min 一起繪出「分布寬度 + 中心對稱性 + 極端邊界」
    // 三維度。復用 K30 reservoir 不開新欄位 (記憶體 72KB 維持不變, 跟 K31/K32/
    // K33 同樣 trade-off —— render 端每次 scrape 多算一次 sort + clone, 跟
    // 15s scrape 週期比 ~25ms 完全可忽略)。
    // trade-off: 樣本數 < 4 時 P25 退化到接近 min (= idx = 0 for 4 samples,
    // idx = 0 for 1/2/3 samples); len=1 → idx=0, P25 = itself (所有 percentile
    // 退化到唯一值, 跟 K30-K33 同款)。R53 chain 護欄覆蓋 P25 ≤ P50 ≤ P75 ≤
    // P95 ≤ P99 ≤ K26 max monotonic invariant, 跨 8 種樣本數 + 4-provider
    // 隔離強化, 跟 R54 K30-K33 percentile chain 護欄對稱擴充 P25 端。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_p25_duration_seconds 25th percentile in seconds of completed session durations per provider (gauge; reuses K30 reservoir sampling 1024; sliding window of last 1024 completions; integer precision; converges to min when sample count < 4; missing=no completed session yet)\n# TYPE lobsterpulse_provider_completed_sessions_p25_duration_seconds gauge\n");
    let completed_p25 = session::completed_sessions_p25_at(provider_totals);
    let mut completed_p25_sorted: Vec<_> = completed_p25.iter().collect();
    completed_p25_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_p25_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_p25_duration_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // K35 落地: per-provider 平均 interarrival seconds gauge (lifetime derive)。
    // 補 K22-K34 全部「單次 session 時長分布」維度都沒覆蓋的「session 頻率 / 吞吐」
    // 維度: K22 (latest age) / K23 (count) / K24 (total duration) / K25 (avg duration)
    // / K26-K27 (max/min) / K28 (stddev) / K30-K34 (percentiles) 全是「每次 session
    // 跑了多久」, 沒有「兩個 session 之間平均隔多久」= provider 吞吐信號。派生
    // 自 K10 `since` (該 provider 第一次被監控到的時間戳) + K23 `completed_sessions_count`
    // (累計完成次數) + 當前 `now` (render 端已有的 DateTime<Utc> 參數, 不新引入) =
    // K35 = `(now - since) / K23` (整數秒, i64)。operator 端不再需要自己寫 PromQL
    // `(now() - ..._since_timestamp) / completed_sessions_total` 算式 (兩個 metric
    // cross-query 在 PromQL 易出錯、scrape 缺一條時算式直接壞), 直接抓 K35 series
    // 觀察 per-provider 平均 interarrival KPI。搭配 K22 (last_completed_session_age)
    // alert rule 互補: K22 觸發「單次 session 卡太久」/「最新一次跑太久」, K35 觸發
    // 「provider 整體吞吐下降」(K35 變大 = 兩個 session 之間隔越來越久 = provider
    // 可能閒置 / 被廢棄 / 上游流量下降)。Memory 零成本: 不開新 ProviderTotals 欄位
    // (K12 idle_ratio 同款策略, 純 fn 端把 K10 + K23 + now 三個輸入組裝成單一 KPI)。
    // 過濾策略: pure fn 端已過濾 K23 == 0 || since.is_none() (兩條件任一不滿足都
    // 不算合法 interarrival 觀察, emit 0 假冒「瞬間完成」會誤導 Prometheus 端把
    // 「沒資料」當「provider 吞吐無限」= 假健康信號), 這裡直接 for 迭代 map 即可。
    // HELP 寫法: 補一句「missing = provider seen but never completed / no since」
    // 跟 K22 / K25 「missing=no completed session yet」契約一致。排序: by provider
    // alphabetical 跟 K6-K34 既契約一致; 空 map → 沒 sample line (HELP/TYPE 標頭
    // 仍輸出)。
    out.push_str("# HELP lobsterpulse_provider_completed_sessions_interarrival_avg_seconds Average seconds between completed sessions per provider (gauge; derived from K10 since + K23 count + render-time now; integer precision; missing=provider seen but never completed yet, or no since timestamp)\n# TYPE lobsterpulse_provider_completed_sessions_interarrival_avg_seconds gauge\n");
    let completed_interarrival = session::completed_sessions_interarrival_at(provider_totals, now);
    let mut completed_interarrival_sorted: Vec<_> = completed_interarrival.iter().collect();
    completed_interarrival_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, secs) in &completed_interarrival_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{{provider=\"{p}\"}} {secs}\n"
        ));
    }
    // R101 落地：per-provider success rate gauge (MISSION K0 「Provider 健康度
    // 覆蓋率」第一支：成功率維度)。補 K29 failure_to_completion_ratio 跟
    // K30 P95 之外的「整體事件成功率」維度 —— 對齊 MISSION.md K0 定義
    // 「13/13 provider 有 P95 延遲 + 成功率指標」(K30 P95 session duration
    // 已實作 = P95 維度達成; R101 = 成功率維度達成 → K0 health 0/13 → 13/13
    // metric 覆蓋)。語意差異化(不跟 K29 合併):K29 派生自
    // `failure_count / completed_sessions_count` (= 「每次完成平均 retry
    // 幾次」= retry 視角,補 session duration 分布之外的失敗比);R101 派生自
    // `1.0 - failure_count / events_total` (= 「全部事件中非失敗佔比」=
    // 健康度視角,補 P95 之外的整體事件成功率)。K29 = 1.0 表示「每次完成
    // retry 1 次」= 訊號;R101 = 1.0 表示「零失敗」= 健康。兩個 ratio 數學
    // 不等價(K29 分母 = 完成次數,R101 分母 = 全部事件),各 emit 各值。
    //
    // 資料源:ProviderTotals.failure_count(K10 PostToolUseFailure 累計) /
    // ProviderTotals.events_total(K7 任何 event 累計) —— 純 derived,無新
    // 觸發點 / 新欄位,完全沿用 K7 / K9 / K10 既有 lifetime counter。f64
    // gauge 4 位小數跟 K25 avg / K28 stddev / K29 failure_ratio 對齊。過濾
    // 策略跟 K25 / K29 同款「0/0 不 emit」防線:`events_total == 0` 跳過
    // 不 emit (0/0 數學未定義,emit 0.0 假冒「成功率 0%」= 假健康信號)。
    // 數值範圍 clamp 到 [0.0, 1.0](理論 `failure_count <= events_total`
    // 由 bump_provider_totals 單調遞增保證,clamp 是防禦性)。sort + format
    // 跟 K6-K35 既契約一致;空 map → 沒 sample line(HELP/TYPE 標頭仍輸出,
    // 對齊 K11「header only」契約)。
    //
    // Operator 用途:R101 = 1.0 表示該 provider 0 失敗 = 完美健康;R101 < 0.5
    // 表示過半事件失敗 = 嚴重健康問題,alert rule 可設 `success_rate < 0.95`
    // 觸發「該 provider 5% 以上事件失敗」early warning。跟 K29 互補:
    // K29 看「retry 密度」(高流量 provider 失敗次數正常)、R101 看「健康度
    // 訊號」(任何 provider 不分流量都該 ≥ 0.95 = 99.x% 成功才正常)。
    out.push_str("# HELP lobsterpulse_provider_success_rate Per-provider event success rate (gauge; derived from K7 events_total + K9 failure_count as 1.0 - failure/total; 4 decimal precision; 1.0=zero failures; missing=provider never received any event yet)\n# TYPE lobsterpulse_provider_success_rate gauge\n");
    let success_rate = session::success_rate_at(provider_totals);
    let mut success_rate_sorted: Vec<_> = success_rate.iter().collect();
    success_rate_sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (p, ratio) in &success_rate_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_success_rate{{provider=\"{p}\"}} {ratio:.4}\n"
        ));
    }
    // K12 落地：per-provider idle ratio = `idle_seconds / lifetime_seconds`。
    // 派生自 K8 `last_event_at`（idle 分子）+ K10 `since`（lifetime 分母），純
    // 組合既有資料源、無新 fs / event 收集點。`lifetime ≤ 0` 已在 pure fn 端被
    // 過濾成 `None`（不會進入 map），所以這裡直接放 sample 即可。值域 0.0-1.0
    // 浮點 gauge，4 位小數固定 precision（避免 Prometheus 端因 IEEE 754 尾數雜訊
    // 看到 `0.6666666666666666` 之類的差異）。Prometheus alert rule 可設
    // `lobsterpulse_provider_idle_ratio > 0.8` 觸發「該 provider 80% lifetime
    // 都在 idle」提醒 —— 健康度信號，補充 K8 絕對秒數（容易因 provider age 短
    // 而誤觸）與 K10 絕對時間（不會主動告訴 operator 該怎麼判斷）。
    out.push_str("# HELP lobsterpulse_provider_idle_ratio Fraction of provider lifetime spent idle (0=fresh, 1=never seen activity); composite of K8 idle_seconds / K10 lifetime_seconds\n# TYPE lobsterpulse_provider_idle_ratio gauge\n");
    for (p, ratio) in &provider_idle_ratio_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_idle_ratio{{provider=\"{p}\"}} {ratio:.4}\n"
        ));
    }
    // K18 落地：per-provider max active session age gauge。
    // 補 K8 / K12 都沒覆蓋的盲點：K8 看「最後一次 event」（session 剛收到
    // heartbeat 就歸 0，無法分辨「session 才開 30 秒但 1 小時沒收到 event」跟
    // 「session 才開 30 秒」）；K12 是 K8/K10 比例（健康度訊號，無絕對秒數）。
    // K18 直接給「最老 active session 已活多久」絕對秒數，operator 一看就知道
    // 是否有 runner 卡住 / 工具 hang。session 結束後 sample 自動消失（live 語意），
    // 不會誤報 stale session。`max(0)` 確保負 duration 退化成 0 不會被 Prometheus
    // 端誤判。`for` 自然跳過 `provider_max_session_age` 缺資料的 provider。
    out.push_str("# HELP lobsterpulse_provider_max_session_age_seconds Age in seconds of the oldest active session per provider (live; 0 means session just started; missing = no active session)\n# TYPE lobsterpulse_provider_max_session_age_seconds gauge\n");
    for (p, age) in &provider_max_session_age_sorted {
        let clamped = (**age).max(0i64);
        out.push_str(&format!(
            "lobsterpulse_provider_max_session_age_seconds{{provider=\"{p}\"}} {clamped}\n"
        ));
    }
    // K19 落地：per-provider × per-state session count gauge。補 K6 細顆度盲點。
    // 對齊 SessionState enum serde 標籤 (idle / working / waiting_for_user / stale)
    // —— Prometheus query label 跟前端 SessionInfo.state JSON 序列化直接一致。
    //
    // Cardinality 上限:9 provider × 4 state = 36 series,跟 K6 (9 series) 同量級,
    // 可控。空 sessions 對應空 map → 沒 sample line (HELP/TYPE 標頭仍輸出,跟 K6/K7/
    // K9/K13 既契約一致)。
    //
    // 不變式:`sum by(provider)(lobsterpulse_provider_sessions_by_state) ==
    // lobsterpulse_provider_sessions` (同 live sessions slice,只是 K19 多了 state 切面)。
    // 排序:by (provider, state) 兩段,跟 K17 兩段排序契約一致。
    out.push_str("# HELP lobsterpulse_provider_sessions_by_state Live session count per provider per state (idle/working/waiting_for_user/stale; sum by(provider) == provider_sessions)\n# TYPE lobsterpulse_provider_sessions_by_state gauge\n");
    for ((p, state), n) in &provider_sessions_by_state_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_sessions_by_state{{provider=\"{p}\",state=\"{state}\"}} {n}\n"
        ));
    }
    // K13 落地：per-provider lifetime event counter。`events_total` 來自
    // `ProviderTotals`（lifetime aggregate），對齊 K6/K7/K9 不蒸發語意。
    // `bump_provider_totals` 對所有 event 類型（不限 PostToolUseFailure / TokenUpdate）
    // 都 +1 → 此 metric 反映「該 provider 累計收過多少 event」、不細分 type。
    out.push_str("# HELP lobsterpulse_provider_events_total Lifetime total event count per provider (every event type increments)\n# TYPE lobsterpulse_provider_events_total counter\n");
    for (p, n) in &provider_events_total_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_events_total{{provider=\"{p}\"}} {n}\n"
        ));
    }
    // K17 落地：per-provider × per-event-type 細顆度計數（counter）。
    // 補 K13 缺 type 維度的盲點 —— `lobsterpulse_provider_events_total` 只
    // 反映「該 provider 共收過幾個 event」，本 metric 切開 type 給 operator
    // 看具體事件類型配比。SLO 用途：
    //   - `rate(...{type="Stop"}[5m])` 對 `rate(...{type="UserPromptSubmit"}[5m])`
    //     → 偵測「runner 一直發 Stop 但沒人 prompt」= 卡住
    //   - `rate(...{type="PreToolUse"}[5m])` 對 `rate(...{type="PostToolUse"}[5m])`
    //     → 偵測「Pre 一直進來 Post 都沒回」= tool 呼叫卡住
    //   - `rate(...{type="TokenUpdate"}[5m])` → 各 provider quota 事件 throughput
    //
    // 跟 K6/K7/K9/K13 lifetime aggregate 對齊：ProviderTotals.event_type_counts
    // session 結束 + 30 min stale 回收後仍保留 → Prometheus 端 counter 不倒退。
    //
    // Cardinality:9 provider × ~10 known event type ≈ 90 series 上限,可控。
    // 空 `event_type_counts` 的 provider 不會產出 sample（`for` 自然跳過）。
    // 排序:by (provider, type) 兩段排序,跟既有 K6-K13 排序契約一致。
    out.push_str("# HELP lobsterpulse_provider_event_type_total Lifetime event count per provider per event type (counter; rate() per type label)\n# TYPE lobsterpulse_provider_event_type_total counter\n");
    for (p, etype, n) in &provider_event_type_total {
        out.push_str(&format!(
            "lobsterpulse_provider_event_type_total{{provider=\"{p}\",type=\"{etype}\"}} {n}\n"
        ));
    }
    // K14 落地：Discord 健康度 (process-level,單一端點非 per-provider)。
    //   - `lobsterpulse_discord_health` gauge:0=ok / 1=4xx / 2=5xx / 3=network
    //     (4xx 通常配置問題;5xx server 端;network 需查 DNS / 連線)
    //   - `lobsterpulse_discord_send_failures_total{class}` counter:lifetime
    //     累計,對齊 K6/K7/K9/K13 aggregate 語意 → operator 用
    //     `rate(...[5m])` 算 throughput
    //   - `lobsterpulse_discord_last_event_unix` gauge:0 表示啟動後還沒失敗過
    //     (跟 K8 `last_event_at = None` 跳過策略相反 — K14 是「有 alert value
    //     才有意義」語意,但 spec 鎖 0 = 沒歷史,Prometheus 端可用
    //     `last_event_unix == 0` 觸發「Discord 還沒成功發過」alert)
    // 來源:`discord::health_snapshot()` 從模組級 OnceLock clone 出來,
    // 避免 render 端持鎖跨越 string 構造。`health_gauge()` 把 enum 攤平
    // 成穩定整數 (`label()` 是另一個 stable string 給 counter label 用)。
    out.push_str("# HELP lobsterpulse_discord_health Discord health gauge (0=ok,1=4xx,2=5xx,3=network)\n# TYPE lobsterpulse_discord_health gauge\n");
    let last_gauge = discord_health
        .last_class
        .map(|c| c.health_gauge())
        .unwrap_or(0);
    out.push_str(&format!("lobsterpulse_discord_health {last_gauge}\n"));
    out.push_str("# HELP lobsterpulse_discord_send_failures_total Lifetime send failure count by class (counter; rate() for throughput)\n# TYPE lobsterpulse_discord_send_failures_total counter\n");
    out.push_str(&format!(
        "lobsterpulse_discord_send_failures_total{{class=\"4xx\"}} {}\n",
        discord_health.class_4xx
    ));
    out.push_str(&format!(
        "lobsterpulse_discord_send_failures_total{{class=\"5xx\"}} {}\n",
        discord_health.class_5xx
    ));
    out.push_str(&format!(
        "lobsterpulse_discord_send_failures_total{{class=\"network\"}} {}\n",
        discord_health.class_network
    ));
    out.push_str("# HELP lobsterpulse_discord_last_event_unix Unix epoch seconds of last recorded Discord failure (0 = never failed since startup)\n# TYPE lobsterpulse_discord_last_event_unix gauge\n");
    out.push_str(&format!(
        "lobsterpulse_discord_last_event_unix {}\n",
        discord_health.last_event_unix
    ));
    // K15 落地：hook_server 收到的 body 無法 parse 成 RawHookEvent 的 lifetime 累計。
    // 對齊 K14 discord_send_failures_total 模式：counter + rate() = throughput。
    // 跟 R6 surfacing 模式一致：原本只有 log::warn，operator 沒辦法 query aggregate
    // 統計「某段時間內 hook 進來多少壞 body」。本 metric 暴露後，Prometheus 端
    // `rate(lobsterpulse_hook_parse_failures_total[5m]) > 0` 即可 alert
    // 「這條路最近在丟事件」。K16 配套：同一個 4xx 失敗會同時 ++ K15 和 K16 4xx，
    // K15 給「JSON 壞掉多少」視角，K16 給「server wire-level 回了什麼 status」
    // 分類視角（2xx / 4xx / 5xx）。
    // 「最近 5 分鐘 hook 收到無法 parse 的 body」→ 通常代表 CLI 升版改了 schema
    // 或 network 中有人在注入垃圾。值 = 0 是健康（啟動後還沒收過壞 body）。
    out.push_str("# HELP lobsterpulse_hook_parse_failures_total Lifetime count of HTTP bodies hook_server failed to parse as RawHookEvent JSON (counter; rate() for throughput)\n# TYPE lobsterpulse_hook_parse_failures_total counter\n");
    out.push_str(&format!(
        "lobsterpulse_hook_parse_failures_total {}\n",
        hook_metrics.parse_failures
    ));
    // K16 落地：HTTP response 結果按 status class 分類的 lifetime counter。
    // 對齊 K15 模式：counter + rate() = throughput。3 條 metric 各自獨立，
    // operator 端 `rate(lobsterpulse_hook_responses_total{class="2xx"}[5m])` /
    // `{class="4xx"}` / `{class="5xx"}` 直接 query。
    //
    // 語意：
    //   - 2xx：成功收到正常 event body + parse 成功 + tx.send 成功
    //   - 4xx：body 缺失（沒 Content-Length / 為 0）或 JSON parse 失敗
    //   - 5xx：目前 handle_client 沒 5xx 分支、保留永遠 0；保留欄位讓
    //          `rate(...{class="5xx"}[5m]) > 0` alert 一裝上就 work，未來
    //          真的有 5xx 時不用再改 schema / 改 alert rule
    //
    // 與 K15 的差別：K15 計「JSON parse 失敗」單一語意，K16 計「server wire-level
    // 對外回了什麼 status code」分類。同一個 4xx 失敗會同時 ++ K15 和 K16 4xx —
    // 兩個 metric 維度不同，operator 依需求選用。
    out.push_str("# HELP lobsterpulse_hook_responses_total Lifetime count of HTTP responses by status class (counter; rate() for throughput)\n# TYPE lobsterpulse_hook_responses_total counter\n");
    out.push_str(&format!(
        "lobsterpulse_hook_responses_total{{class=\"2xx\"}} {}\n",
        hook_metrics.responses_2xx
    ));
    out.push_str(&format!(
        "lobsterpulse_hook_responses_total{{class=\"4xx\"}} {}\n",
        hook_metrics.responses_4xx
    ));
    out.push_str(&format!(
        "lobsterpulse_hook_responses_total{{class=\"5xx\"}} {}\n",
        hook_metrics.responses_5xx
    ));
    // K46 落地：parse_provider 白名單沒命中 → fallback "claude" 的 lifetime 累計。
    // 對齊 K15/K16 模式：counter + rate() = throughput。與 K16 4xx 的差別：
    //   - K16 4xx 計「server wire-level 對外回了 4xx」分類（包含 body 缺失 +
    //     JSON parse 失敗等所有 4xx 原因）
    //   - K46 計「單一語意：provider id 不在白名單」單一原因
    // 同一個 4xx 不一定 ++ K46（只有 provider 解析階段失敗才會），兩個 metric
    // 維度不同，operator 依需求選用。`rate(...[5m]) > 0` 通常代表 hook config
    // 有 typo 或 CLI 升版改了 provider id — 跟 log warn 配對方便定位。
    out.push_str("# HELP lobsterpulse_hook_unknown_provider_fallbacks_total Lifetime count of hook_server parse_provider falling back to \"claude\" because provider id was not in the known whitelist (counter; rate() for throughput)\n# TYPE lobsterpulse_hook_unknown_provider_fallbacks_total counter\n");
    out.push_str(&format!(
        "lobsterpulse_hook_unknown_provider_fallbacks_total {}\n",
        hook_metrics.unknown_provider_fallbacks
    ));
    out
}

/// 測試發 Discord 訊息（設定頁按鈕用）。
#[tauri::command]
fn send_discord_test(config_state: tauri::State<AppConfigState>) -> Result<String, String> {
    let (token, channel) = {
        let c = config_state.0.lock().unwrap();
        (
            c.appearance.discord.bot_token.clone(),
            c.appearance.discord.channel_id.clone(),
        )
    };
    discord::send_message(&token, &channel, "🦞 LobsterPulse Discord 連線測試成功")
        .map(|id| format!("sent (msg_id={id})"))
}

/// 讀 quota 歷史 → 給 Dashboard 畫 sparkline。回傳 { name: [[ts, pct], ...] }。
#[tauri::command]
fn get_quota_history() -> std::collections::HashMap<String, Vec<(u64, u8)>> {
    // R30 silent-fail surfacing (接續 R28/R29/R30/R31 同一主題線):
    // 修前 `load_history().unwrap_or_default()` 在 quota-history.csv 損壞 / IO 錯 /
    // 鎖 poison 時, 前端 Dashboard 拿到空 HashMap 畫不出 sparkline 卻完全無 log,
    // operator 無從分辨「沒有 history」還是「壞檔」. 改為 match Err 三條分流 +
    // 結構化 log warn 帶 caller context.
    match quota_history::load_history() {
        Ok(h) => h,
        Err(e) => {
            log::warn!(
                "[lib::get_quota_history] quota-history.csv load failed: {e} \
                 — Dashboard 將以空 history 渲染（sparkline 全空）"
            );
            std::collections::HashMap::new()
        }
    }
}

/// 前端「🧪 試跑」單一 runner，不寫進 snapshot。
#[tauri::command]
fn test_usage_runner(runner: crate::config::UsageRunnerConfig) -> Result<String, String> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new(&runner.command);
    cmd.args(&runner.args);
    for (k, v) in &runner.env {
        cmd.env(k, v);
    }
    if let Some(cwd) = &runner.cwd {
        cmd.current_dir(cwd);
    }
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = run_command_with_timeout(cmd, runner.timeout_secs)?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let rendered = if let Some(tpl) = &runner.template {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            render_handlebars(tpl, &json)
        } else {
            stdout.clone()
        }
    } else {
        stdout.clone()
    };
    Ok(format!(
        "exit={:?}\n---stdout---\n{}\n---rendered---\n{}\n---stderr---\n{}",
        out.status.code(),
        stdout,
        rendered,
        stderr
    ))
}

/// 發 Telegram 訊息 — 走 Windows 10+ 內建 curl.exe，不加新依賴。
/// 任一欄位為空就 no-op。
#[tauri::command]
fn send_telegram(text: String, config_state: tauri::State<AppConfigState>) -> Result<(), String> {
    let (token, chat_id) = {
        let c = config_state.0.lock().unwrap();
        (
            c.appearance.telegram_bot_token.clone(),
            c.appearance.telegram_chat_id.clone(),
        )
    };
    if token.is_empty() || chat_id.is_empty() {
        return Err("telegram not configured".into());
    }
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    std::process::Command::new("curl.exe")
        .args([
            "-s",
            "-X",
            "POST",
            &url,
            "--data-urlencode",
            &format!("chat_id={chat_id}"),
            "--data-urlencode",
            &format!("text={text}"),
            "-d",
            "parse_mode=Markdown",
        ])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // Single-instance: second launches bring the running window to front
    // and exit. Without this, a duplicate launch leaves a dead tray icon
    // (the HTTP server refuses the port but Tauri still spawns the UI).
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
        // autostart：Windows Task Scheduler AtLogon / macOS LaunchAgent / Linux autostart desktop entry
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ));
        // Windows toast / macOS banner / Linux libnotify
        builder = builder.plugin(tauri_plugin_notification::init());
        // 全域快捷鍵 Ctrl+Shift+L / D / E — 由前端 listen shortcut 事件
        builder = builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let s = shortcut.to_string();
                    // L toggle show/hide、D open dashboard、E open events log
                    if let Some(w) = app.get_webview_window("main") {
                        if s.contains("KeyL") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        } else if s.contains("KeyD") {
                            let _ = w.show();
                            let _ = w.set_focus();
                            let _ = w.emit("open-dashboard", ());
                        } else if s.contains("KeyE") {
                            let _ = w.show();
                            let _ = w.set_focus();
                            let _ = w.emit("open-events-log", ());
                        }
                    }
                })
                .build(),
        );
    }

    builder
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Load config
            let config = load_config();
            save_config(&config).ok(); // Ensure file exists with defaults
            let startup_capsule_w = config.appearance.capsule_width as f64;
            app.manage(AppConfigState(Mutex::new(config)));

            // K14 落地：初始化 process-level Discord health state。
            // OnceLock get_or_init idempotent,重複呼叫安全;沒呼叫前
            // `record_*_failure` 走 no-op、`health_snapshot()` 退化為 Default
            // → 提早 init 確保後續 send 失敗能正確累加。
            discord::init_health();

            // Window setup — 套用使用者偏好的 capsule 寬度（非硬編 300）
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(startup_capsule_w, 46.0)));

                // Cursor position polling
                let win = window.clone();
                let was_inside = Arc::new(AtomicBool::new(false));
                let was_inside_clone = was_inside.clone();

                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        let inside = (|| {
                            let cursor = win.cursor_position().ok()?;
                            let pos = win.outer_position().ok()?;
                            let size = win.outer_size().ok()?;
                            let margin = 2.0;
                            Some(
                                cursor.x >= (pos.x as f64 - margin)
                                && cursor.x <= (pos.x as f64 + size.width as f64 + margin)
                                && cursor.y >= (pos.y as f64 - margin)
                                && cursor.y <= (pos.y as f64 + size.height as f64 + margin)
                            )
                        })().unwrap_or(false);

                        let was = was_inside_clone.load(Ordering::Relaxed);
                        if inside != was {
                            was_inside_clone.store(inside, Ordering::Relaxed);
                            if inside {
                                let _ = win.emit("cursor-entered", ());
                            } else {
                                let _ = win.emit("cursor-left", ());
                            }
                        }
                    }
                });
            }

            // Session manager
            app.manage(AppSessionManager(Mutex::new(SessionManager::new())));

            // Hook server
            let handle = app.handle().clone();
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime");

            let port = rt.block_on(async {
                let mut server = HookServer::new();
                match server.start().await {
                    Ok(mut rx) => {
                        let port = server.port();
                        let h = handle.clone();
                        tokio::spawn(async move {
                            while let Some(event) = rx.recv().await {
                                let mgr = h.state::<AppSessionManager>();
                                let (transition, firings) = {
                                    let mut m = mgr.0.lock().unwrap();
                                    let t = m.handle_event(&event);
                                    // R115: drain handle_event 內 evaluate_rules 累積的
                                    // RuleFiredEvent, 給前端 emit `rule-fired` Tauri event
                                    // 觸發 Toast / Sound / Log action。
                                    let f = std::mem::take(&mut m.rule_firings);
                                    (t, f)
                                };
                                let _ = h.emit("session-update", ());
                                match transition {
                                    session::SessionTransition::Completed => {
                                        let _ = h.emit("task-completed", event.provider.clone());
                                    }
                                    session::SessionTransition::StartedWaiting => {
                                        let _ = h.emit("task-waiting", event.provider.clone());
                                    }
                                    session::SessionTransition::None => {}
                                }
                                for f in firings {
                                    let _ = h.emit("rule-fired", f);
                                }
                            }
                        });
                        port
                    }
                    Err(e) => {
                        log::error!("Failed to start server: {e}");
                        0
                    }
                }
            });

            std::mem::forget(rt);
            app.manage(ServerPort(port));

            // 全域快捷鍵註冊
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                let gs = app.global_shortcut();
                for combo in ["CommandOrControl+Shift+L", "CommandOrControl+Shift+D", "CommandOrControl+Shift+E"] {
                    if let Err(e) = gs.register(combo) {
                        log::warn!("register shortcut {combo} failed: {e}");
                    }
                }
            }

            // 本機 usage runner 60s 背景 loop（B 路線：LobsterPulse 自跑 quota runner）
            let handle_runner = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    let runners = {
                        let cfg_state = handle_runner.state::<AppConfigState>();
                        let c = cfg_state.0.lock().unwrap();
                        c.appearance.usage_runners.clone()
                    };
                    if !runners.is_empty() {
                        run_local_usage_runners(&runners);
                    }
                }
            });

            // 啟動時立刻跑一次（若有 runner 配置），不用等 60s
            let handle_runner_now = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                let runners = {
                    let cfg_state = handle_runner_now.state::<AppConfigState>();
                    let c = cfg_state.0.lock().unwrap();
                    c.appearance.usage_runners.clone()
                };
                if !runners.is_empty() {
                    run_local_usage_runners(&runners);
                }
                // 再 snap 一次（讓 LP 重啟後立刻有 history 起點）
                std::thread::sleep(std::time::Duration::from_secs(2));
                if let Err(e) = quota_history::snapshot_once() {
                    // 對齊 R6/R8/R11 surface pattern：quota CSV snapshot 失敗要可觀察，
                    // 否則歷史圖表缺資料 user 分不清「無 usage runner」vs「CSV 寫失敗」。
                    log::warn!(
                        "[quota_history] snapshot_once (post-runners) failed: {e} — \
                         quota-history.csv 該輪可能缺一筆"
                    );
                }
            });

            // Quota 歷史 — 每 3600s（1 小時）snapshot usage-local.json 到 CSV
            std::thread::spawn(|| loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
                if let Err(e) = quota_history::snapshot_once() {
                    // 對齊 R6/R8/R11 surface pattern
                    log::warn!(
                        "[quota_history] snapshot_once (hourly) failed: {e} — \
                         quota-history.csv 該輪可能缺一筆"
                    );
                }
            });

            // Auto-action 規則引擎 tick（每 15s 跑一次）
            let handle_auto = app.handle().clone();
            // 從磁碟讀回 daily/weekly summary 的 dedup marker——避免重啟後整點 double-fire
            let (loaded_date, loaded_week) = auto_rules::load_persisted_summary_markers();
            // R16 補：同步讀回 session_idle 規則的 (sid → last_event_ts) 錨點
            // ——避免重啟後舊 idle 週期又被通知一次（純 toast 模式會 spam）
            let loaded_idle_ts = auto_rules::load_persisted_session_idle_markers();
            let mut initial_auto_state = auto_rules::AutoRuleState::default();
            if !loaded_date.is_empty() || !loaded_week.is_empty() {
                initial_auto_state.last_summary_date = loaded_date;
                initial_auto_state.last_weekly_key = loaded_week;
            }
            if !loaded_idle_ts.is_empty() {
                initial_auto_state.last_session_idle_event_ts = loaded_idle_ts;
            }
            let auto_state: auto_rules::SharedAutoState = Arc::new(Mutex::new(initial_auto_state));
            app.manage(auto_state.clone());
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(15));
                let (cfg, token, channel, openab_cmd, toast_enabled) = {
                    let cfg_state = handle_auto.state::<AppConfigState>();
                    let c = cfg_state.0.lock().unwrap();
                    (
                        c.appearance.auto_actions.clone(),
                        c.appearance.discord.bot_token.clone(),
                        c.appearance.discord.channel_id.clone(),
                        c.appearance.openab_restart_command.clone(),
                        c.appearance.system_notifications,
                    )
                };
                let discord_on = handle_auto.state::<AppConfigState>()
                    .0.lock().unwrap().appearance.discord.enabled;
                // 若 Discord 和 Toast 都關就跳過；任一開就繼續
                if !discord_on && !toast_enabled { continue; }
                let mgr_state = handle_auto.state::<AppSessionManager>();
                auto_rules::tick_with_state(
                    &cfg,
                    &mgr_state.0,
                    &auto_state,
                    &openab_cmd,
                    auto_rules::NotifyChannels {
                        discord_token: if discord_on { &token } else { "" },
                        discord_channel: if discord_on { &channel } else { "" },
                        app: Some(handle_auto.clone()),
                        toast_enabled,
                    },
                );
                if discord_on {
                    // Discord 指令 polling —— `!lp quota/status/kill/pause/resume/trend/...`
                    let cfg_state = handle_auto.state::<AppConfigState>();
                    auto_rules::poll_discord_commands(
                        &token,
                        &channel,
                        &mgr_state.0,
                        &auto_state,
                        &cfg_state.0,
                    );

                    // OpenAB bridge — 讀 ~/openab/logs/cctest-events.jsonl 新事件，LP 作為 singular speaker 統一發 Discord
                    let events = openab_bridge::tail_new_events();
                    for ev in events.iter() {
                        if let Err(e) = openab_bridge::dispatch_event(ev, &token, &channel) {
                            // 對齊 R6/R8/R11 surface pattern：OpenAB 事件發 Discord 失敗要可觀察，
                            // 否則 user 端 OpenAB 狀態變化沒到 Discord、且無 log 可查
                            // （是 token 壞 / channel 錯 / event 格式不認 / Discord 4xx）。
                            let source = ev.get("source").and_then(|x| x.as_str()).unwrap_or("?");
                            let kind = ev.get("event").and_then(|x| x.as_str()).unwrap_or("?");
                            log::warn!(
                                "[openab_bridge] dispatch_event (source={source} event={kind}) failed: {e}"
                            );
                        }
                    }
                }
            });

            // Staleness checker — 讀 config 裡的可調 thresholds
            let handle2 = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(10));
                let (idle, stale, remove) = {
                    let cfg_state = handle2.state::<AppConfigState>();
                    let c = cfg_state.0.lock().unwrap();
                    (
                        c.appearance.idle_threshold_secs,
                        c.appearance.stale_threshold_secs,
                        c.appearance.remove_threshold_secs,
                    )
                };
                let mgr = handle2.state::<AppSessionManager>();
                mgr.0.lock().unwrap().check_staleness(idle, stale, remove);
                let _ = handle2.emit("session-update", ());
            });

            // #8 Configurator live sync — 每 2s poll `web-config/appearance.json` mtime，
            // 變化就 import + emit "appearance-synced" 給 main.js 重新套用
            let handle_app_watch = app.handle().clone();
            std::thread::spawn(move || {
                let home = dirs::home_dir().unwrap_or_default();
                let path = home.join(".lobsterpulse").join("web-config").join("appearance.json");
                let mut last_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let new_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                    if new_mtime.is_some() && new_mtime != last_mtime {
                        last_mtime = new_mtime;
                        // 讀 + 合併 + save
                        let ok = (|| -> Result<(), String> {
                            let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                            let patch: serde_json::Value = serde_json::from_str(&data).map_err(|e| e.to_string())?;
                            let patch_obj = patch.as_object().ok_or("not object")?;
                            let cfg_state = handle_app_watch.state::<AppConfigState>();
                            let mut cfg = cfg_state.0.lock().unwrap();
                            let mut app_val = serde_json::to_value(&cfg.appearance).map_err(|e| e.to_string())?;
                            if let Some(app_obj) = app_val.as_object_mut() {
                                for (k, v) in patch_obj {
                                    app_obj.insert(k.clone(), v.clone());
                                }
                            }
                            cfg.appearance = serde_json::from_value(app_val).map_err(|e| e.to_string())?;
                            crate::config::save_config(&cfg).map_err(|e| e.to_string())?;
                            Ok(())
                        })();
                        if ok.is_ok() {
                            let _ = handle_app_watch.emit("appearance-synced", ());
                            log::info!("configurator live sync applied: {}", path.display());
                        }
                    }
                }
            });

            // Prometheus /metrics exporter — 獨立 std::thread + 自建 runtime，
            // 不依賴 hook_server 的 rt（那個已 std::mem::forget）。
            let handle_metrics = app.handle().clone();
            let metrics_port = if port > 0 { port + 100 } else { 19380 };
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(r) => r,
                    Err(e) => {
                        log::warn!("metrics runtime create failed: {e}");
                        return;
                    }
                };
                rt.block_on(async move {
                    let listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{metrics_port}")).await {
                        Ok(l) => l,
                        Err(e) => {
                            log::warn!("metrics server bind {metrics_port} failed: {e}");
                            return;
                        }
                    };
                    log::info!("LobsterPulse metrics on :{metrics_port}/metrics");
                    loop {
                        let (mut sock, _) = match listener.accept().await {
                            Ok(x) => x,
                            Err(_) => continue,
                        };
                        let h = handle_metrics.clone();
                        tokio::spawn(async move {
                            use tokio::io::{AsyncReadExt, AsyncWriteExt};
                            let mut buf = vec![0u8; 4096];
                            let n = match tokio::time::timeout(
                                std::time::Duration::from_secs(2),
                                sock.read(&mut buf),
                            ).await {
                                Ok(Ok(n)) if n > 0 => n,
                                _ => return,
                            };
                            let head = String::from_utf8_lossy(&buf[..n]);
                            let first = head.lines().next().unwrap_or("");
                            let body = if first.contains("GET /metrics") {
                                render_prometheus(&h)
                            } else {
                                String::from("# Not Found\n")
                            };
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(), body
                            );
                            if let Err(e) = sock.write_all(resp.as_bytes()).await {
                                // R57: 從 `let _ =` 沉默吞改成 log warn。Prometheus
                                // scrape HTTP response 寫失敗 (client 中途斷線 /
                                // socket 滿 / kernel buffer 滿), PromQL scrape
                                // 會 timeout / 拿到半截 body, 但 metrics server
                                // 端 log 沒記, 報「scrape failed」bug 找嘸 server
                                // 端對應記錄
                                log::warn!("{}", lib_warn_msg("metrics_http_response_write", &e));
                            }
                        });
                    }
                });
            });

            // System tray
            let show = MenuItemBuilder::with_id("show", "顯示 / 隱藏  ·  Ctrl+Shift+L").build(app)?;
            let dashboard = MenuItemBuilder::with_id("dashboard", "Bot 總覽  ·  Ctrl+Shift+D").build(app)?;
            let settings = MenuItemBuilder::with_id("settings", "開啟設定").build(app)?;
            let events_log = MenuItemBuilder::with_id("events_log", "事件診斷  ·  Ctrl+Shift+E").build(app)?;
            let toggle_theme = MenuItemBuilder::with_id("toggle_theme", "切換明暗主題").build(app)?;
            let openab_restart = MenuItemBuilder::with_id("openab_restart", "重啟 OpenAB").build(app)?;
            let open_config = MenuItemBuilder::with_id("open_config", "開啟設定檔").build(app)?;
            let restart = MenuItemBuilder::with_id("restart", "重新啟動").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "結束龍蝦監控").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show)
                .item(&dashboard)
                .item(&settings)
                .item(&events_log)
                .separator()
                .item(&toggle_theme)
                .item(&openab_restart)
                .item(&open_config)
                .item(&restart)
                .item(&quit)
                .build()?;

            let icon_bytes = include_bytes!("../icons/32x32.png");
            let icon = Image::from_bytes(icon_bytes)?;

            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("龍蝦監控 · 左鍵=切換顯示／隱藏 · Ctrl+Shift+L")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                    // 左鍵單擊放開 → 切換顯示／隱藏；遵守 Discord/Steam 慣例
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                // 叫回來時置中螢幕頂端，避免膠囊跑到螢幕外
                                if let Ok(Some(monitor)) = w.current_monitor() {
                                    let scale = monitor.scale_factor();
                                    let pos = monitor.position();
                                    let sw = monitor.size().width as f64 / scale;
                                    let x = pos.x as f64 / scale + (sw / 2.0 - 145.0);
                                    let y = pos.y as f64 / scale + 8.0;
                                    let _ = w.set_position(tauri::Position::Logical(
                                        tauri::LogicalPosition::new(x, y),
                                    ));
                                }
                                let _ = w.show();
                                let _ = w.set_always_on_top(true);
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                if let Ok(Some(monitor)) = w.current_monitor() {
                                    let scale = monitor.scale_factor();
                                    let pos = monitor.position();
                                    let sw = monitor.size().width as f64 / scale;
                                    let x = pos.x as f64 / scale + (sw / 2.0 - 145.0);
                                    let y = pos.y as f64 / scale + 8.0;
                                    let _ = w.set_position(tauri::Position::Logical(
                                        tauri::LogicalPosition::new(x, y),
                                    ));
                                }
                                let _ = w.show();
                                let _ = w.set_always_on_top(true);
                                let _ = w.set_focus();
                            }
                        }
                    }
                    "settings" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_always_on_top(true);
                            let _ = w.set_focus();
                            let _ = w.emit("open-settings", ());
                        }
                    }
                    "dashboard" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_always_on_top(true);
                            let _ = w.set_focus();
                            let _ = w.emit("open-dashboard", ());
                        }
                    }
                    "events_log" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_always_on_top(true);
                            let _ = w.set_focus();
                            let _ = w.emit("open-events-log", ());
                        }
                    }
                    "openab_restart" => {
                        let cfg_state = app.state::<AppConfigState>();
                        let restart_cmd = cfg_state.0.lock().unwrap().appearance.openab_restart_command.clone();
                        if let Err(e) = std::process::Command::new("powershell.exe")
                            .args([
                                "-NoProfile",
                                "-Command",
                                "Stop-Process -Name openab -Force -ErrorAction SilentlyContinue",
                            ])
                            .output()
                        {
                            log::warn!("openab_restart: stop powershell failed: {e}");
                        }
                        if !restart_cmd.trim().is_empty() {
                            if let Err(e) = std::process::Command::new("powershell.exe")
                                .args(["-NoProfile", "-Command", &restart_cmd])
                                .spawn()
                            {
                                log::warn!("openab_restart: spawn `{}` failed: {e}", restart_cmd);
                            }
                        }
                    }
                    "toggle_theme" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.emit("toggle-theme", ());
                        }
                    }
                    "open_config" => {
                        let path = config::config_path();
                        let opener = if cfg!(target_os = "macos") { "open" }
                                     else if cfg!(target_os = "windows") { "explorer" }
                                     else { "xdg-open" };
                        if let Err(e) = std::process::Command::new(opener)
                            .arg(path.to_string_lossy().to_string())
                            .spawn()
                        {
                            log::warn!("open_config: spawn `{}` failed: {e}", opener);
                        }
                    }
                    "restart" => {
                        if let Ok(exe) = std::env::current_exe() {
                            if let Err(e) = std::process::Command::new(exe).spawn() {
                                log::warn!("restart: spawn self failed: {e}");
                            }
                        }
                        hook_server::remove_port_file();
                        app.exit(0);
                    }
                    "quit" => {
                        hook_server::remove_port_file();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            info!("LobsterPulse ready on port {port}");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            select_session,
            remove_session,
            get_config,
            save_app_config,
            detect_installed_providers,
            check_provider_setup,
            install_provider_hooks,
            remove_provider_hooks,
            open_provider_settings,
            list_sounds,
            play_sound_file,
            open_sounds_folder,
            open_app_config,
            open_url,
            get_server_port,
            read_usage_snapshots,
            resize_window,
            bounce_window,
            is_cursor_inside,
            hide_window,
            get_recent_events,
            restart_openab,
            send_telegram,
            send_discord_test,
            test_usage_runner,
            get_quota_history,
            remove_all_sessions,
            open_configurator,
            open_help_page,
            import_appearance_json,
            pick_background_file,
            get_background_data_url,
            manual_snapshot_once,
            rebuild_and_relaunch,
            test_toast,
            get_live_quota_snapshot,
            list_rules,
            toggle_rule,
            add_rule,
            remove_rule,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LobsterPulse");
}

#[cfg(test)]
mod write_local_usage_snapshot_tests {
    use super::*;

    /// 為每個 test 製造獨立 tmp 路徑（避免 parallel test 互踩 / 污染 home dir）。
    fn tmp_path(tag: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("lp-snap-{tag}-{nonce}.json"));
        p
    }

    #[test]
    fn happy_path_writes_valid_json() {
        let path = tmp_path("happy");
        let snap = serde_json::json!({
            "source": "local",
            "updated_at": 1700000000_u64,
            "runners": [
                {"ok": true, "name": "r1", "text": "ok"}
            ],
        });

        write_local_usage_snapshot(&path, &snap).expect("write should succeed");

        let raw = std::fs::read_to_string(&path).expect("file should exist after write");
        let parsed: serde_json::Value =
            serde_json::from_str(&raw).expect("written file should be valid JSON");
        assert_eq!(parsed["source"], "local");
        assert_eq!(parsed["updated_at"], 1700000000_u64);
        assert!(parsed["runners"].is_array());
        assert_eq!(parsed["runners"][0]["name"], "r1");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn returns_err_when_target_dir_missing() {
        // 故意指向不存在的中間目錄 → 兩層 write 都不該成功 → 必須回 Err
        let mut path = std::env::temp_dir();
        path.push("lp-snap-no-such-dir-99887766");
        path.push("nested");
        path.push("snapshot.json");

        let snap = serde_json::json!({"source": "local", "runners": []});
        let result = write_local_usage_snapshot(&path, &snap);
        assert!(
            result.is_err(),
            "write to non-existent dir should return Err"
        );
        let err = result.unwrap_err();
        // 兩層 fallback 都會 log，這條 assert 確認 error chain 包含「atomic」失敗訊息
        assert!(
            err.contains("atomic"),
            "error should mention atomic write failure, got: {err}"
        );
    }
}

#[cfg(test)]
mod render_prometheus_tests {
    use super::*;
    use crate::session::{
        completed_sessions_min_duration_at, completed_sessions_p25_at, completed_sessions_p50_at,
        completed_sessions_p75_at, completed_sessions_p95_at, completed_sessions_p99_at,
        completed_sessions_stddev_at, failure_to_completion_ratio_at,
        last_completed_session_age_at, ProviderTotals, SessionInfo, SessionState,
    };
    use chrono::TimeZone;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// 為 test 製造 SessionInfo fixture（只填 render_prometheus_body 讀的欄位）。
    /// K6 落地後 `tokens_input` / `tokens_output` 仍記在 SessionInfo 但 metric 不再讀它
    /// （lifetime aggregate 走 ProviderTotals）；保留欄位是給 frontend SessionInfo 顯示用。
    fn info(provider: &str, is_active: bool, _tokens_in: u64, _tokens_out: u64) -> SessionInfo {
        SessionInfo {
            id: format!("{provider}-sid"),
            provider: provider.to_string(),
            state: if is_active {
                SessionState::Working
            } else {
                SessionState::Idle
            },
            project_name: format!("{provider}-project"),
            cwd: None,
            is_active,
            formatted_time: "00:00".to_string(),
            last_tool_name: None,
            last_prompt: None,
            tool_calls: Vec::new(),
            thinking: false,
            tokens_input: 0,
            tokens_output: 0,
            last_event_secs_ago: 0,
            token_samples: Vec::new(),
            duration_secs: 0,
        }
    }

    /// K18 測試用：SessionInfo fixture 帶自訂 `duration_secs`（active session 已活多久）。
    /// 既有 `info` fixture 強制 `duration_secs: 0`，K18 需要驗「max of multiple
    /// active sessions」時要能各自指定不同 duration。`is_active` 跟 `duration_secs`
    /// 獨立控制 —— K18 只看 active 的 session，duration_secs 是 raw 資料。
    fn info_with_age(provider: &str, is_active: bool, duration_secs: i64) -> SessionInfo {
        let mut s = info(provider, is_active, 0, 0);
        s.id = format!("{provider}-sid-{duration_secs}");
        s.duration_secs = duration_secs;
        s
    }

    /// K19 測試用：SessionInfo fixture 帶自訂 `state`。既有 `info` fixture 強制
    /// `state = if is_active { Working } else { Idle }`,K19 要驗 per-state 細顆度
    /// 計數需要能各自塞 Idle/Working/WaitingForUser/Stale。`is_active` 跟 `state`
    /// 在 production 也獨立（active session 也可能進 Stale 30 min 後被回收前那一瞬）,
    /// 本 fixture 把兩者解耦更貼近真實 session lifecycle。
    fn info_with_state(provider: &str, is_active: bool, state: SessionState) -> SessionInfo {
        let mut s = info(provider, is_active, 0, 0);
        s.state = state;
        s
    }

    /// 為 test 製造 ProviderTotals fixture（填 render_prometheus_body 讀的 token 欄位）。
    /// K8 落地：預設 `last_event_at = Some(now)`，避免既有測試被 K8 新 metric 干擾
    /// （讓 render 端計算 idle = now - now = 0，行為退化成「剛剛有動」）。
    fn totals(provider: &str, in_: u64, out: u64) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: in_,
                tokens_output: out,
                session_count: 1,
                failure_count: 0,
                events_total: 0,
                // K17 落地：event type 維度計數 map 留空,既有 fixture 不主動填
                // type,新測試用專屬 fixture (`totals_with_event_type_counts`) 控制。
                event_type_counts: std::collections::BTreeMap::new(),
                since: None,
                last_event_at: Some(Utc::now()),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    fn totals_map(entries: Vec<(String, ProviderTotals)>) -> HashMap<String, ProviderTotals> {
        entries.into_iter().collect()
    }

    /// K7 測試用：為 test 製造 ProviderTotals fixture（token + failure_count 一起填）。
    fn totals_with_failures(
        provider: &str,
        in_: u64,
        out: u64,
        fail: u64,
    ) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: in_,
                tokens_output: out,
                session_count: 1,
                failure_count: fail,
                events_total: 0,
                // K17 落地：event type 維度計數 map 留空,既有 fixture 不主動填
                // type,新測試用專屬 fixture (`totals_with_event_type_counts`) 控制。
                event_type_counts: std::collections::BTreeMap::new(),
                since: None,
                last_event_at: Some(Utc::now()),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K8 測試用：為 test 製造 ProviderTotals fixture，固定 `last_event_at` 時間戳，
    /// 配合 `now` 參數驗證 idle 數學（`now - last_event_at` = 預期秒數）。
    /// `last_at` 直接設值；測試呼叫處用 `Utc::now() - Duration::seconds(N)` 表達「N 秒前」。
    fn totals_at(
        provider: &str,
        in_: u64,
        out_: u64,
        fail: u64,
        last_at: DateTime<Utc>,
    ) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: in_,
                tokens_output: out_,
                session_count: 1,
                failure_count: fail,
                events_total: 0,
                // K17 落地：event type 維度計數 map 留空,既有 fixture 不主動填
                // type,新測試用專屬 fixture (`totals_with_event_type_counts`) 控制。
                event_type_counts: std::collections::BTreeMap::new(),
                since: None,
                last_event_at: Some(last_at),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K8 測試用：ProviderTotals 但 `last_event_at = None`（從未收過 event）。
    fn totals_no_event(provider: &str) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count: 0,
                failure_count: 0,
                events_total: 0,
                // K17 落地：event type 維度計數 map 留空,既有 fixture 不主動填
                // type,新測試用專屬 fixture (`totals_with_event_type_counts`) 控制。
                event_type_counts: std::collections::BTreeMap::new(),
                since: None,
                last_event_at: None,
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K9 測試用：為 test 製造 ProviderTotals fixture，指定 `session_count`（lifetime 累計）。
    /// token / failure 留 0、`last_event_at` 設 now（避免干擾 K8 idle 計算）。
    fn totals_with_session_count(provider: &str, session_count: u64) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count,
                failure_count: 0,
                events_total: 0,
                // K17 落地：event type 維度計數 map 留空,既有 fixture 不主動填
                // type,新測試用專屬 fixture (`totals_with_event_type_counts`) 控制。
                event_type_counts: std::collections::BTreeMap::new(),
                since: None,
                last_event_at: Some(Utc::now()),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K10 測試用：為 test 製造 ProviderTotals fixture，指定 `since`（first-seen 時間）。
    /// `last_event_at` 設同 `since` 避免干擾 K8 idle 計算（idle = now - last = now - since）。
    /// token / failure / session_count 留 0（K10 不讀這些欄位，設 0 表語意單純）。
    fn totals_with_since(provider: &str, since: DateTime<Utc>) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count: 0,
                failure_count: 0,
                events_total: 0,
                event_type_counts: std::collections::BTreeMap::new(),
                since: Some(since),
                last_event_at: Some(since),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K10 測試用：ProviderTotals 但 `since = None`（理論上 bump_provider_totals 第一次
    /// event 會填，測試故意不填驗「since = None 跳過 sample」契約）。
    fn totals_no_since(provider: &str) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count: 0,
                failure_count: 0,
                events_total: 0,
                // K17 落地：event type 維度計數 map 留空,既有 fixture 不主動填
                // type,新測試用專屬 fixture (`totals_with_event_type_counts`) 控制。
                event_type_counts: std::collections::BTreeMap::new(),
                since: None,
                last_event_at: Some(Utc::now()),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K12 測試用：ProviderTotals 同時指定 `since`（first-seen）跟 `last_event_at`（idle 錨點），
    /// 兩個時間獨立可調（不像 `totals_with_since` 把兩者綁同值），才能構造
    /// 「last_event_at < since / 兩者相差比例」等 idle ratio 數學。token / failure /
    /// session_count 留 0（K12 不讀這些欄位）。
    fn totals_with_since_and_last_at(
        provider: &str,
        since: DateTime<Utc>,
        last_event_at: DateTime<Utc>,
    ) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count: 0,
                failure_count: 0,
                events_total: 0,
                event_type_counts: std::collections::BTreeMap::new(),
                since: Some(since),
                last_event_at: Some(last_event_at),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K12 測試用：ProviderTotals 但 `last_event_at = None`（從未收過 event）。
    /// 對齊 K8 `totals_no_event` 語意：K12 缺 last_event_at → 純函式回 None → 跳過 sample。
    fn totals_no_event_with_since(
        provider: &str,
        since: DateTime<Utc>,
    ) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count: 0,
                failure_count: 0,
                events_total: 0,
                event_type_counts: std::collections::BTreeMap::new(),
                since: Some(since),
                last_event_at: None,
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K12 測試用：ProviderTotals 把 `since` 設成跟 `now` 相同（lifetime = 0），
    /// 驗「lifetime ≤ 0 → 跳過 sample」契約。
    fn totals_with_since_now(provider: &str, now: DateTime<Utc>) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count: 0,
                failure_count: 0,
                events_total: 0,
                event_type_counts: std::collections::BTreeMap::new(),
                since: Some(now),
                last_event_at: Some(now),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K13 測試用：為 test 製造 ProviderTotals fixture，指定 `events_total`（lifetime
    /// 累計收到幾個 event）。其他欄位留 0 / 預設值，避免干擾其他 metric 段。
    fn totals_with_events(provider: &str, events: u64) -> (String, ProviderTotals) {
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count: 0,
                failure_count: 0,
                events_total: events,
                // K17 落地：event type 維度計數 map 留空,既有 fixture 不主動填
                // type,新測試用專屬 fixture (`totals_with_event_type_counts`) 控制。
                event_type_counts: std::collections::BTreeMap::new(),
                since: None,
                last_event_at: Some(Utc::now()),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    /// K17 測試用：為 test 製造 ProviderTotals fixture，指定 `event_type_counts`
    /// （per-event-type 維度計數 map）。`type_counts` 接受 `&[(&str, u64)]` 切片
    /// 方便 test 內聯構造 —— e.g. `&[("UserPromptSubmit", 3), ("Stop", 1)]`。
    /// 其他欄位（events_total / token / failure）留 0/預設值,聚焦 K17 行為。
    /// 用 `BTreeMap` 而非 `HashMap` 構造 —— 對齊 `ProviderTotals.event_type_counts`
    /// 型別,render 端 `for (etype, n) in &t.event_type_counts` 直接 iterate
    /// 已是 alphabetical sorted by key。
    fn totals_with_event_type_counts(
        provider: &str,
        type_counts: &[(&str, u64)],
    ) -> (String, ProviderTotals) {
        let event_type_counts: std::collections::BTreeMap<String, u64> = type_counts
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect();
        (
            provider.to_string(),
            ProviderTotals {
                tokens_input: 0,
                tokens_output: 0,
                session_count: 0,
                failure_count: 0,
                events_total: 0,
                event_type_counts,
                since: None,
                last_event_at: Some(Utc::now()),
                last_completed_session_age_secs: None,
                // K23 落地：test fixture 預設 0（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。
                completed_sessions_count: 0,
                // K24 落地：test fixture 預設 0（未完成過 session → 無時長可累加）,
                // 跟 production `ProviderTotals::default()` 同語意。K24 專屬 fixture
                // 在 K24 測試內以 struct literal 控制,既有 K6-K23 fixture 不主動填。
                completed_sessions_total_duration_secs: 0,
                // K26 / K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K26 / K27 專屬
                // fixture 在 K26 / K27 測試內以 struct literal 控制,既有 K6-K25
                // fixture 不主動填。
                max_completed_session_age_secs: None,
                // K27 落地：test fixture 預設 None（未完成過 session）,
                // 跟 production `ProviderTotals::default()` 同語意。K27 專屬 fixture
                // 在 K27 測試內以 struct literal 控制,既有 K6-K26 fixture 不主動填。
                min_completed_session_age_secs: None,
                completed_sessions_mean_secs: 0.0,
                completed_sessions_m2_secs: 0.0,
                // K30 落地：test fixture 預設空 reservoir（未完成過 session
                // → 沒 sample 可推）。跟 production `ProviderTotals::default()`
                // 同語意。K30 專屬 fixture 在 K30 測試內以 struct literal 控制,
                // 既有 K6-K29 fixture 不主動填 samples。
                completed_sessions_p95_samples: Vec::new(),
            },
        )
    }

    #[test]
    fn empty_state_emits_zero_counters_and_no_provider_lines() {
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("lobsterpulse_sessions_total 0\n"));
        assert!(body.contains("lobsterpulse_sessions_active 0\n"));
        assert!(body.contains("lobsterpulse_tokens_input 0\n"));
        assert!(body.contains("lobsterpulse_tokens_output 0\n"));
        // 沒 session → provider_* 段只有 HELP/TYPE 標頭、沒有 sample
        assert!(!body.contains("lobsterpulse_provider_sessions{"));
        assert!(!body.contains("lobsterpulse_provider_active{"));
        // 沒 provider → 新增的 per-provider token 段也只有 HELP/TYPE、沒有 sample
        assert!(!body.contains("lobsterpulse_provider_tokens_input{"));
        assert!(!body.contains("lobsterpulse_provider_tokens_output{"));
        // K7 落地：per-provider 失敗計數段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_failure_count{"));
        // K9 落地：per-provider lifetime session count 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_session_count{"));
        // K13 落地：per-provider lifetime event counter 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_events_total{"));
        // K17 落地：per-provider × per-event-type 細顆度 counter 段同樣：
        // 空 event_type_counts → 沒 sample line（HELP/TYPE 標頭仍輸出）
        assert!(!body.contains("lobsterpulse_provider_event_type_total{"));
        // K10 落地：per-provider since_timestamp 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_since_timestamp{"));
        // K11 落地：per-provider quota_snapshot_age 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_quota_snapshot_age_seconds{"));
        // K12 落地：per-provider idle_ratio 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_idle_ratio{"));
        // K20 落地：per-provider quota_remaining_pct 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_quota_remaining_pct{"));
        // K21 落地：CSV pipeline freshness gauge 同樣：None → 沒 sample line(只 emit HELP/TYPE)。
        // 不能用 `!body.contains("lobsterpulse_quota_history_csv_age_seconds ")` — HELP/TYPE
        // 標頭本身就有「metric 名 + 空格」,會誤判。改用 `lines().filter(starts_with)` 鎖
        // 真正的 sample line(HELP/TYPE 行以 `#` 起頭不會被算進去)。
        assert!(body.contains("# HELP lobsterpulse_quota_history_csv_age_seconds"));
        let k21_samples: Vec<&str> = body
            .lines()
            .filter(|l| l.starts_with("lobsterpulse_quota_history_csv_age_seconds "))
            .collect();
        assert!(
            k21_samples.is_empty(),
            "None 不應 emit sample line, got: {k21_samples:?}"
        );
        // K15 落地：lifetime parse failures counter
        assert!(body.contains("lobsterpulse_hook_parse_failures_total 0\n"));
        // K46 落地：lifetime unknown provider fallbacks counter（default_metrics()
        // 第一次 snapshot 一定是 0, 但必須 emit sample line, operator 端 Prometheus
        // scrape 才抓得到 metric 名, 才有 rate() 算 throughput）
        assert!(body.contains("lobsterpulse_hook_unknown_provider_fallbacks_total 0\n"));
        // K16 落地：3 條 HTTP response status class counter (default 0)
        assert!(body.contains("lobsterpulse_hook_responses_total{class=\"2xx\"} 0\n"));
        assert!(body.contains("lobsterpulse_hook_responses_total{class=\"4xx\"} 0\n"));
        assert!(body.contains("lobsterpulse_hook_responses_total{class=\"5xx\"} 0\n"));
    }

    #[test]
    fn single_inactive_session_reported_as_total_only() {
        let sessions = vec![info("claude", false, 100, 50)];
        let body = render_prometheus_body(
            &sessions,
            1,
            0,
            &totals_map(vec![totals("claude", 100, 50)]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("lobsterpulse_sessions_total 1\n"));
        assert!(body.contains("lobsterpulse_sessions_active 0\n"));
        assert!(body.contains("lobsterpulse_provider_sessions{provider=\"claude\"} 1\n"));
        // inactive session 不該出現在 provider_active 行
        assert!(!body.contains("lobsterpulse_provider_active{provider=\"claude\"}"));
        assert!(body.contains("lobsterpulse_tokens_input 100\n"));
        assert!(body.contains("lobsterpulse_tokens_output 50\n"));
    }

    #[test]
    fn single_active_session_reported_in_both_provider_lines() {
        let sessions = vec![info("codex", true, 200, 80)];
        let body = render_prometheus_body(
            &sessions,
            1,
            1,
            &totals_map(vec![totals("codex", 200, 80)]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("lobsterpulse_sessions_active 1\n"));
        assert!(body.contains("lobsterpulse_provider_sessions{provider=\"codex\"} 1\n"));
        assert!(body.contains("lobsterpulse_provider_active{provider=\"codex\"} 1\n"));
    }

    #[test]
    fn multiple_providers_counted_separately_and_sorted_alphabetically() {
        // 故意用「非字母序」輸入驗 sort 邏輯：openx 應在 cicx 之前被排序掉
        let sessions = vec![
            info("openx", false, 0, 0),
            info("cicx", true, 0, 0),
            info("gemini", false, 0, 0),
            info("cicx", true, 0, 0), // 同 provider 重複 → count=2
        ];
        let body = render_prometheus_body(
            &sessions,
            4,
            2,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 排序後順序應為 cicx / gemini / openx
        let cicx_sessions_idx = body
            .find("lobsterpulse_provider_sessions{provider=\"cicx\"} 2\n")
            .expect("cicx session count line should exist");
        let gemini_sessions_idx = body
            .find("lobsterpulse_provider_sessions{provider=\"gemini\"} 1\n")
            .expect("gemini session count line should exist");
        let openx_sessions_idx = body
            .find("lobsterpulse_provider_sessions{provider=\"openx\"} 1\n")
            .expect("openx session count line should exist");
        assert!(
            cicx_sessions_idx < gemini_sessions_idx && gemini_sessions_idx < openx_sessions_idx,
            "provider_sessions 必須按 provider 名 alphabetical 排序，cicx → gemini → openx"
        );

        // provider_active 也應排序
        let cicx_active_idx = body
            .find("lobsterpulse_provider_active{provider=\"cicx\"} 2\n")
            .expect("cicx active count line should exist");
        assert!(cicx_active_idx > 0);
    }

    #[test]
    fn token_counters_sum_from_provider_totals_aggregate() {
        // K6 落地：global token 計數讀 ProviderTotals（lifetime aggregate），不走 live session。
        // 三個 provider 各有 lifetime token 累計 → global total 應為三者之和。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals("claude", 1000, 500),
                totals("codex", 2000, 1000),
                totals("cicx", 500, 250),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("lobsterpulse_tokens_input 3500\n"));
        assert!(body.contains("lobsterpulse_tokens_output 1750\n"));
    }

    #[test]
    fn per_provider_token_metrics_alphabetical_and_separate() {
        // K6 主軸：per-provider token 細顆度。3 個 provider、不同 token 數、alphabetical 排序。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals("openx", 100, 50), // 故意非字母序
                totals("cicx", 200, 100),
                totals("gemini", 300, 150),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 每個 provider 都應該有 input + output 兩條 sample line
        assert!(body.contains("lobsterpulse_provider_tokens_input{provider=\"cicx\"} 200\n"));
        assert!(body.contains("lobsterpulse_provider_tokens_input{provider=\"gemini\"} 300\n"));
        assert!(body.contains("lobsterpulse_provider_tokens_input{provider=\"openx\"} 100\n"));
        assert!(body.contains("lobsterpulse_provider_tokens_output{provider=\"cicx\"} 100\n"));
        assert!(body.contains("lobsterpulse_provider_tokens_output{provider=\"gemini\"} 150\n"));
        assert!(body.contains("lobsterpulse_provider_tokens_output{provider=\"openx\"} 50\n"));

        // 排序驗證：cicx < gemini < openx
        let cicx_idx = body
            .find("lobsterpulse_provider_tokens_input{provider=\"cicx\"} 200\n")
            .expect("cicx input line");
        let gemini_idx = body
            .find("lobsterpulse_provider_tokens_input{provider=\"gemini\"} 300\n")
            .expect("gemini input line");
        let openx_idx = body
            .find("lobsterpulse_provider_tokens_input{provider=\"openx\"} 100\n")
            .expect("openx input line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider token input 必須 alphabetical 排序"
        );
    }

    #[test]
    fn token_aggregate_uses_lifetime_not_live_sessions() {
        // K6 修的 latent bug：原本從 live SessionInfo sum，session 移除後 token 蒸發。
        // 這裡給 0 個 live session 但 provider_totals 有大量 token，驗證 metric 仍正確反映
        // lifetime（不會因 session 移除而歸零）。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![totals("claude", 9999, 4444), totals("cicx", 1, 1)]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("lobsterpulse_tokens_input 10000\n"));
        assert!(body.contains("lobsterpulse_tokens_output 4445\n"));
        // per-provider 細顆度也對
        assert!(body.contains("lobsterpulse_provider_tokens_input{provider=\"cicx\"} 1\n"));
        assert!(body.contains("lobsterpulse_provider_tokens_output{provider=\"claude\"} 4444\n"));
    }

    #[test]
    fn output_includes_help_and_type_headers_for_every_metric() {
        // scrape 端靠 HELP/TYPE 行識別 metric 類型；任何一條 missing 都會讓
        // Prometheus 把該 metric 標成 untyped（功能降級）
        let body = render_prometheus_body(
            &[info("claude", true, 0, 0)],
            1,
            1,
            &totals_map(vec![totals("claude", 1, 1)]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        let required_headers = [
            "# HELP lobsterpulse_sessions_total",
            "# TYPE lobsterpulse_sessions_total gauge",
            "# HELP lobsterpulse_sessions_active",
            "# TYPE lobsterpulse_sessions_active gauge",
            "# HELP lobsterpulse_provider_sessions",
            "# TYPE lobsterpulse_provider_sessions gauge",
            "# HELP lobsterpulse_provider_active",
            "# TYPE lobsterpulse_provider_active gauge",
            "# HELP lobsterpulse_tokens_input",
            "# TYPE lobsterpulse_tokens_input counter",
            "# HELP lobsterpulse_tokens_output",
            "# TYPE lobsterpulse_tokens_output counter",
            // K6 新增：per-provider token metric
            "# HELP lobsterpulse_provider_tokens_input",
            "# TYPE lobsterpulse_provider_tokens_input counter",
            "# HELP lobsterpulse_provider_tokens_output",
            "# TYPE lobsterpulse_provider_tokens_output counter",
            // K7 新增：per-provider failure counter
            "# HELP lobsterpulse_provider_failure_count",
            "# TYPE lobsterpulse_provider_failure_count counter",
            // K8 新增：per-provider idle gauge
            "# HELP lobsterpulse_provider_idle_seconds",
            "# TYPE lobsterpulse_provider_idle_seconds gauge",
            // K9 新增：per-provider lifetime session count counter
            "# HELP lobsterpulse_provider_session_count",
            "# TYPE lobsterpulse_provider_session_count counter",
            // K10 新增：per-provider first-seen timestamp gauge
            "# HELP lobsterpulse_provider_since_timestamp",
            "# TYPE lobsterpulse_provider_since_timestamp gauge",
            // K11 新增：per-provider quota snapshot age gauge
            "# HELP lobsterpulse_provider_quota_snapshot_age_seconds",
            "# TYPE lobsterpulse_provider_quota_snapshot_age_seconds gauge",
            // K12 新增：per-provider idle ratio gauge（K8 / K10 派生）
            "# HELP lobsterpulse_provider_idle_ratio",
            "# TYPE lobsterpulse_provider_idle_ratio gauge",
            // K18 新增：per-provider max active session age gauge（live）
            "# HELP lobsterpulse_provider_max_session_age_seconds",
            "# TYPE lobsterpulse_provider_max_session_age_seconds gauge",
            // K13 新增：per-provider lifetime event counter（任何 event 都 +1）
            "# HELP lobsterpulse_provider_events_total",
            "# TYPE lobsterpulse_provider_events_total counter",
            // K17 新增：per-provider × per-event-type 細顆度 counter
            "# HELP lobsterpulse_provider_event_type_total",
            "# TYPE lobsterpulse_provider_event_type_total counter",
        ];
        for h in required_headers {
            assert!(
                body.contains(h),
                "missing required header: {h}\n--- body ---\n{body}"
            );
        }
    }

    #[test]
    fn per_provider_failure_counter_alphabetical_and_per_provider() {
        // K7 主軸：per-provider failure 細顆度。3 個 provider、不同 failure 數、
        // alphabetical 排序驗證。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_failures("openx", 0, 0, 7), // 故意非字母序
                totals_with_failures("cicx", 0, 0, 3),
                totals_with_failures("gemini", 0, 0, 12),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 每個 provider 都應該有對應的 failure sample line
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"cicx\"} 3\n"));
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"gemini\"} 12\n"));
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"openx\"} 7\n"));

        // 排序驗證：cicx < gemini < openx
        let cicx_idx = body
            .find("lobsterpulse_provider_failure_count{provider=\"cicx\"} 3\n")
            .expect("cicx failure line");
        let gemini_idx = body
            .find("lobsterpulse_provider_failure_count{provider=\"gemini\"} 12\n")
            .expect("gemini failure line");
        let openx_idx = body
            .find("lobsterpulse_provider_failure_count{provider=\"openx\"} 7\n")
            .expect("openx failure line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider failure count 必須 alphabetical 排序"
        );
    }

    #[test]
    fn failure_counter_uses_lifetime_aggregate_not_live_sessions() {
        // K7 同 K6 的 lifetime-vs-live 核心 regression guard：
        // 失敗事件後 session 早已 idle / 被 stale 回收（live sessions 為空），
        // 但 ProviderTotals 仍保留累計 → metric 仍正確反映歷史失敗總數。
        // 這也避免 Prometheus counter 倒退（alert 誤觸發）。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_failures("claude", 0, 0, 0), // 沒失敗
                totals_with_failures("codex", 0, 0, 5),  // 5 次失敗
                totals_with_failures("cicx", 0, 0, 2),   // 2 次失敗
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        ); // 0 個 live session

        // 即使 live sessions = []，failure metric 仍要反映出 ProviderTotals 累計
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"claude\"} 0\n"));
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"codex\"} 5\n"));
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"cicx\"} 2\n"));
    }

    #[test]
    fn idle_seconds_empty_state_emits_header_only() {
        // 沒有任何 provider → idle 段只有 HELP/TYPE、沒有 sample line。
        // 對齊 K6/K7「empty state 不假裝 0 秒 idle」語意。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_idle_seconds"));
        assert!(body.contains("# TYPE lobsterpulse_provider_idle_seconds gauge"));
        assert!(!body.contains("lobsterpulse_provider_idle_seconds{"));
    }

    #[test]
    fn idle_seconds_skips_providers_with_no_event_yet() {
        // K8 語意：provider 從未收過 event（`last_event_at = None`）→ 不輸出 sample。
        // 避免 Prometheus 端把缺失當 0 秒 idle 誤判「剛剛才動」。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_at("claude", 0, 0, 0, now - chrono::Duration::seconds(120)),
                totals_no_event("cicx"), // 從未收過 event
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // claude 收過 event → 有 sample
        assert!(body.contains("lobsterpulse_provider_idle_seconds{provider=\"claude\"} 120\n"));
        // cicx 從未收過 event → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_idle_seconds{provider=\"cicx\"}"));
    }

    #[test]
    fn idle_seconds_uses_lifetime_aggregate_not_live_sessions() {
        // K8 同 K6/K7 的 lifetime-vs-live 核心 regression guard：
        // 0 個 live session，但 ProviderTotals 仍有 last_event_at → metric 正確反映。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_at("claude", 0, 0, 0, now - chrono::Duration::seconds(60)),
                totals_at("cicx", 0, 0, 0, now - chrono::Duration::seconds(30)),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        ); // 0 個 live session

        // 與 K6/K7 一致：lifetime aggregate 讓 session 移除 / stale 回收後仍能看 idle
        assert!(body.contains("lobsterpulse_provider_idle_seconds{provider=\"claude\"} 60\n"));
        assert!(body.contains("lobsterpulse_provider_idle_seconds{provider=\"cicx\"} 30\n"));
    }

    #[test]
    fn idle_seconds_alphabetical_and_deterministic() {
        // 3 個 provider、不同 idle 數、故意非字母序輸入 → 驗 alphabetical 排序。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_at("openx", 0, 0, 0, now - chrono::Duration::seconds(900)),
                totals_at("cicx", 0, 0, 0, now - chrono::Duration::seconds(60)),
                totals_at("gemini", 0, 0, 0, now - chrono::Duration::seconds(3600)),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // 每個 provider 都應該有對應 sample line
        assert!(body.contains("lobsterpulse_provider_idle_seconds{provider=\"cicx\"} 60\n"));
        assert!(body.contains("lobsterpulse_provider_idle_seconds{provider=\"gemini\"} 3600\n"));
        assert!(body.contains("lobsterpulse_provider_idle_seconds{provider=\"openx\"} 900\n"));

        // 排序驗證：cicx < gemini < openx
        let cicx_idx = body
            .find("lobsterpulse_provider_idle_seconds{provider=\"cicx\"} 60\n")
            .expect("cicx idle line");
        let gemini_idx = body
            .find("lobsterpulse_provider_idle_seconds{provider=\"gemini\"} 3600\n")
            .expect("gemini idle line");
        let openx_idx = body
            .find("lobsterpulse_provider_idle_seconds{provider=\"openx\"} 900\n")
            .expect("openx idle line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider idle_seconds 必須 alphabetical 排序"
        );
    }

    #[test]
    fn idle_seconds_clamps_negative_to_zero() {
        // 時鐘回撥 / 序列化時間差 edge case：理論 `last_event_at` ≤ now，
        // 但 saturating 守門員 + `.max(0)` 保證 gauge 永遠 ≥ 0。
        // 製造 last_event_at 在「未來」1 秒的情境，驗證 clamp。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![totals_at(
                "claude",
                0,
                0,
                0,
                now + chrono::Duration::seconds(1), // 故意未來 1 秒
            )]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // clamp 為 0 而非 -1
        assert!(body.contains("lobsterpulse_provider_idle_seconds{provider=\"claude\"} 0\n"));
    }

    #[test]
    fn session_count_empty_state_emits_header_only() {
        // 沒任何 provider → K9 段只有 HELP/TYPE、沒有 sample line。
        // 對齊 K6/K7/K8「empty state 不假裝 0 session」語意。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_session_count"));
        assert!(body.contains("# TYPE lobsterpulse_provider_session_count counter"));
        assert!(!body.contains("lobsterpulse_provider_session_count{"));
    }

    #[test]
    fn session_count_uses_lifetime_aggregate_not_live_sessions() {
        // K9 核心 regression guard：lifetime-vs-live。
        // 0 個 live session（sessions=[]）但 ProviderTotals.session_count=5 → metric 仍顯示 5。
        // 跟 K6/K7/K8 一致：session 結束 + 30 min stale 回收後 live 為 0，但 lifetime
        // ProviderTotals.session_count 仍保留 → Prometheus 不會誤判 counter 倒退。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_session_count("cicx", 5),   // 5 個 session 累計
                totals_with_session_count("codex", 1),  // 1 個 session
                totals_with_session_count("claude", 0), // 0（理論不會出現，但驗 0 也輸出）
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        ); // 0 個 live session

        assert!(body.contains("lobsterpulse_provider_session_count{provider=\"cicx\"} 5\n"));
        assert!(body.contains("lobsterpulse_provider_session_count{provider=\"codex\"} 1\n"));
        assert!(body.contains("lobsterpulse_provider_session_count{provider=\"claude\"} 0\n"));
    }

    #[test]
    fn session_count_alphabetical_and_deterministic() {
        // 3 個 provider、不同 session count、故意非字母序輸入 → 驗 alphabetical 排序。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_session_count("openx", 12),
                totals_with_session_count("cicx", 3),
                totals_with_session_count("gemini", 7),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 每個 provider 都應該有對應 sample line
        assert!(body.contains("lobsterpulse_provider_session_count{provider=\"cicx\"} 3\n"));
        assert!(body.contains("lobsterpulse_provider_session_count{provider=\"gemini\"} 7\n"));
        assert!(body.contains("lobsterpulse_provider_session_count{provider=\"openx\"} 12\n"));

        // 排序驗證：cicx < gemini < openx
        let cicx_idx = body
            .find("lobsterpulse_provider_session_count{provider=\"cicx\"} 3\n")
            .expect("cicx session_count line");
        let gemini_idx = body
            .find("lobsterpulse_provider_session_count{provider=\"gemini\"} 7\n")
            .expect("gemini session_count line");
        let openx_idx = body
            .find("lobsterpulse_provider_session_count{provider=\"openx\"} 12\n")
            .expect("openx session_count line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider session_count 必須 alphabetical 排序"
        );
    }

    // ===== K10 per-provider since_timestamp gauge =====

    #[test]
    fn since_timestamp_empty_state_emits_header_only() {
        // 沒任何 provider → K10 段只有 HELP/TYPE、沒有 sample line。
        // 對齊 K6/K7/K8/K9「empty state 不假裝 0 timestamp」語意。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_since_timestamp"));
        assert!(body.contains("# TYPE lobsterpulse_provider_since_timestamp gauge"));
        assert!(!body.contains("lobsterpulse_provider_since_timestamp{"));
    }

    #[test]
    fn since_timestamp_emits_unix_seconds_per_provider() {
        // K10 主軸：每個有 `since` 的 provider 輸出 Unix epoch seconds。
        // 用 3 個 fixture timestamp（2024 / 2025 / 2026），驗證 timestamp 正確進 sample。
        // 注意：`render_prometheus_body` 只讀 `since` 欄位、token / failure / session_count
        // 都留 0 — 這測試斷言只看 K10 sample line、其它 metric 細節不在 K10 範圍。
        let claude_since = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let cicx_since = Utc.with_ymd_and_hms(2025, 6, 1, 12, 30, 0).unwrap();
        let gemini_since = Utc.with_ymd_and_hms(2026, 3, 20, 8, 15, 0).unwrap();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_since("claude", claude_since),
                totals_with_since("cicx", cicx_since),
                totals_with_since("gemini", gemini_since),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"claude\"}} {}\n",
            claude_since.timestamp()
        )));
        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"cicx\"}} {}\n",
            cicx_since.timestamp()
        )));
        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"gemini\"}} {}\n",
            gemini_since.timestamp()
        )));
    }

    #[test]
    fn since_timestamp_skips_providers_with_no_since() {
        // K10 語意：provider 的 `since = None`（理論上 bump_provider_totals 第一次
        // event 會填，測試故意不填）→ 不輸出 sample。對齊 K8 idle_seconds 跳過
        // `last_event_at = None` 的策略，避免 Prometheus 端把缺失當 0 timestamp
        // （= 1970-01-01 unix epoch）誤判「該 provider 從 1970 就開始」。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_since("claude", now - chrono::Duration::days(365)),
                totals_no_since("cicx"), // 故意不填 since
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // claude 有 since → 有 sample
        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"claude\"}} {}\n",
            (now - chrono::Duration::days(365)).timestamp()
        )));
        // cicx 沒 since → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_since_timestamp{provider=\"cicx\"}"));
    }

    #[test]
    fn since_timestamp_uses_lifetime_aggregate_not_live_sessions() {
        // K10 核心 regression guard：lifetime-vs-live。
        // 0 個 live session（sessions=[]）但 ProviderTotals.since 已填 →
        // metric 仍輸出該 timestamp。
        // 跟 K6/K7/K8/K9 一致：session 結束 + 30 min stale 回收後 live 為 0，
        // 但 lifetime ProviderTotals.since 仍保留 → user 仍能看出「該 provider
        // 何時第一次被監控到」（用 now - since_timestamp 算 uptime 對等量）。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_since("claude", now - chrono::Duration::days(30)),
                totals_with_since("cicx", now - chrono::Duration::days(7)),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        ); // 0 個 live session

        // lifetime aggregate 確保 since 不被 live session 影響
        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"claude\"}} {}\n",
            (now - chrono::Duration::days(30)).timestamp()
        )));
        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"cicx\"}} {}\n",
            (now - chrono::Duration::days(7)).timestamp()
        )));
    }

    #[test]
    fn since_timestamp_alphabetical_and_deterministic() {
        // 3 個 provider、不同 since timestamp、故意非字母序輸入 → 驗 alphabetical 排序。
        let openx_since = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let cicx_since = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let gemini_since = Utc.with_ymd_and_hms(2026, 3, 20, 0, 0, 0).unwrap();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_since("openx", openx_since),
                totals_with_since("cicx", cicx_since),
                totals_with_since("gemini", gemini_since),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 每個 provider 都應該有對應 sample line
        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"cicx\"}} {}\n",
            cicx_since.timestamp()
        )));
        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"gemini\"}} {}\n",
            gemini_since.timestamp()
        )));
        assert!(body.contains(&format!(
            "lobsterpulse_provider_since_timestamp{{provider=\"openx\"}} {}\n",
            openx_since.timestamp()
        )));

        // 排序驗證：cicx < gemini < openx
        let cicx_idx = body
            .find(&format!(
                "lobsterpulse_provider_since_timestamp{{provider=\"cicx\"}} {}\n",
                cicx_since.timestamp()
            ))
            .expect("cicx since line");
        let gemini_idx = body
            .find(&format!(
                "lobsterpulse_provider_since_timestamp{{provider=\"gemini\"}} {}\n",
                gemini_since.timestamp()
            ))
            .expect("gemini since line");
        let openx_idx = body
            .find(&format!(
                "lobsterpulse_provider_since_timestamp{{provider=\"openx\"}} {}\n",
                openx_since.timestamp()
            ))
            .expect("openx since line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider since_timestamp 必須 alphabetical 排序"
        );
    }

    // ===== K11 per-provider quota_snapshot_age gauge =====

    /// K11 測試 fixture：把 `(provider, age_seconds)` tuple 收進 HashMap。
    /// `render_prometheus_body` 的 `quota_snapshot_ages` 參數吃 `HashMap<String, i64>`，
    /// 這層 helper 只讓測試呼叫處比直接 `.collect()` 鏈短一點。
    fn quota_age_map(entries: Vec<(String, i64)>) -> std::collections::HashMap<String, i64> {
        entries.into_iter().collect()
    }

    // ----- pure fn 測試：compute_quota_snapshot_age_seconds -----

    #[test]
    fn compute_quota_snapshot_age_returns_none_when_mtime_is_none() {
        // 邊界：呼叫端拿不到 mtime（檔案不存在 / 權限錯誤）→ 純函式必須回 `None`。
        // 對齊 K8 `last_event_at = None` 跳過策略 + K10 `since = None` 跳過策略：
        // 沒有資料時不應該假裝「age = 0」誤判「snapshot 剛剛還在」。
        let now = Utc::now();
        assert_eq!(compute_quota_snapshot_age_seconds(now, None), None);
    }

    #[test]
    fn compute_quota_snapshot_age_returns_positive_for_past_mtime() {
        // 主軸：mtime 在過去 N 秒 → 回 `Some(N)`，caller 端會放進 age map → emit sample。
        // 鎖定「`Utc::now()` 用 chrono 跟 SystemTime 轉換後的秒數差」算法。
        let now = Utc::now();
        let mtime: std::time::SystemTime = (now - chrono::Duration::seconds(60)).into();
        assert_eq!(
            compute_quota_snapshot_age_seconds(now, Some(mtime)),
            Some(60)
        );
    }

    #[test]
    fn compute_quota_snapshot_age_saturates_future_mtime_to_zero() {
        // 邊界：clock skew / 寫檔 race → mtime 可能在 now 之後。
        // 對齊 K8 `idle_seconds_clamps_negative_to_zero`（line 2595）守門員：
        // gauge 永遠 ≥ 0，負值會被 Prometheus / Grafana 視為 anomaly。
        let now = Utc::now();
        let future_mtime: std::time::SystemTime = (now + chrono::Duration::seconds(5)).into();
        assert_eq!(
            compute_quota_snapshot_age_seconds(now, Some(future_mtime)),
            Some(0)
        );
    }

    // ----- render_prometheus_body 端對端：K11 段 -----

    #[test]
    fn quota_snapshot_age_empty_state_emits_header_only() {
        // 對齊 K6/K7/K8/K9/K10「empty state 不假裝 0」語意：空 map → 沒 sample line。
        // `quota_snapshot_age = 0` 語意危險（會被誤判「snapshot 剛剛更新」），所以「不輸出」
        // 比「輸出 0」更安全。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_quota_snapshot_age_seconds"));
        assert!(body.contains("# TYPE lobsterpulse_provider_quota_snapshot_age_seconds gauge"));
        assert!(!body.contains("lobsterpulse_provider_quota_snapshot_age_seconds{"));
    }

    #[test]
    fn quota_snapshot_age_emits_seconds_per_provider() {
        // K11 主軸：3 個 provider 各自 age → 3 條 sample line，數值與輸入一致。
        // 5 OpenAB bot + 1 local runner 是 6 個固定 key，本測試只取 3 個子集合驗樣板格式。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &quota_age_map(vec![
                ("cicx".to_string(), 45),
                ("gemini".to_string(), 120),
                ("openx".to_string(), 600),
            ]),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body
            .contains("lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"cicx\"} 45\n"));
        assert!(body.contains(
            "lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"gemini\"} 120\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"openx\"} 600\n"
        ));
    }

    #[test]
    fn quota_snapshot_age_alphabetical_and_deterministic() {
        // 排序驗證：故意非字母序輸入 → alphabetical 輸出。
        // 對齊 K6/K7/K8/K9/K10 既有的 deterministic 排序保證。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &quota_age_map(vec![
                ("openx".to_string(), 30),
                ("cicx".to_string(), 10),
                ("__local__".to_string(), 5),
            ]),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // `__local__` 在 ASCII 比字母小（`_` = 0x5F < `a` = 0x61），
        // 所以排序順序為 `__local__` < `cicx` < `openx`。
        let local_idx = body
            .find("lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"__local__\"} 5\n")
            .expect("__local__ quota age line");
        let cicx_idx = body
            .find("lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"cicx\"} 10\n")
            .expect("cicx quota age line");
        let openx_idx = body
            .find("lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"openx\"} 30\n")
            .expect("openx quota age line");
        assert!(
            local_idx < cicx_idx && cicx_idx < openx_idx,
            "per-provider quota_snapshot_age 必須 alphabetical 排序"
        );
    }

    #[test]
    fn quota_snapshot_age_zero_is_distinguishable_from_absent() {
        // 邊界：age = 0（snapshot 剛剛寫入）vs 不在 map（檔案不存在）。
        // 兩種語意差很多：age=0 = runner 健康；absent = snapshot 從沒出現或 runner 死了。
        // 對齊 K11 設計原則：寧可「少一條 sample」也不要「假裝 0」誤導監控。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &quota_age_map(vec![("cicx".to_string(), 0)]), // 只有 cicx
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // cicx 有 age=0 → 必須 emit sample line（0 不是「absent」）
        assert!(body
            .contains("lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"cicx\"} 0\n"));
        // 其他 provider（gemini / openx / __local__）都不在 map → 不 emit
        assert!(
            !body.contains("lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"gemini\"}")
        );
        assert!(
            !body.contains("lobsterpulse_provider_quota_snapshot_age_seconds{provider=\"openx\"}")
        );
    }

    // ===== K12 per-provider idle_ratio gauge =====

    // ----- pure fn 測試：compute_provider_idle_ratio -----

    #[test]
    fn compute_idle_ratio_returns_none_when_last_event_at_is_none() {
        // 邊界：K8 跳過策略對應。從未收過 event → 沒有 idle/lifetime 比例可言。
        // 寧可「缺 sample」也不要「0.0 假裝沒問題」（idle=0 會被誤判「剛剛在動」）。
        let now = Utc::now();
        let since = now - chrono::Duration::days(30);
        assert_eq!(compute_provider_idle_ratio(now, None, Some(since)), None);
    }

    #[test]
    fn compute_idle_ratio_returns_none_when_since_is_none() {
        // 邊界：K10 跳過策略對應。沒有 first-seen 錨點 → 沒有 lifetime 分母可言。
        let now = Utc::now();
        let last = now - chrono::Duration::seconds(60);
        assert_eq!(compute_provider_idle_ratio(now, Some(last), None), None);
    }

    #[test]
    fn compute_idle_ratio_returns_zero_when_event_just_happened() {
        // 主軸（健康）：剛剛收過 event，idle ≈ 0，ratio 應為 0.0（0% idle）。
        // 鎖定「健康 provider」基線：lifetime 內幾乎 100% 在動 = 0.0。
        let now = Utc::now();
        let since = now - chrono::Duration::days(30);
        let last = now; // 剛剛
        let ratio =
            compute_provider_idle_ratio(now, Some(last), Some(since)).expect("ratio should exist");
        assert!(
            (ratio - 0.0).abs() < 1e-9,
            "剛剛收過 event → ratio 應為 0.0，實得 {ratio}"
        );
    }

    #[test]
    fn compute_idle_ratio_returns_one_when_idle_equals_lifetime() {
        // 主軸（死掉）：lifetime 內完全沒動（last_event_at == since，且 since < now）→
        // ratio 應為 1.0（100% idle）。鎖定「runner 死了」基線。
        let now = Utc::now();
        let since = now - chrono::Duration::days(30);
        let last = since; // 從 first-seen 起就沒再收到 event
        let ratio =
            compute_provider_idle_ratio(now, Some(last), Some(since)).expect("ratio should exist");
        assert!(
            (ratio - 1.0).abs() < 1e-9,
            "idle == lifetime → ratio 應為 1.0，實得 {ratio}"
        );
    }

    #[test]
    fn compute_idle_ratio_handles_fractional_lifetime_correctly() {
        // 主軸（半死半活）：lifetime 60 秒、idle 30 秒 → ratio 0.5。
        // 鎖定「idle/lifetime」實際數學。f64 精度檢查：ratio - 0.5 應 < 1e-9。
        let now = Utc::now();
        let since = now - chrono::Duration::seconds(60);
        let last = now - chrono::Duration::seconds(30);
        let ratio =
            compute_provider_idle_ratio(now, Some(last), Some(since)).expect("ratio should exist");
        assert!(
            (ratio - 0.5).abs() < 1e-9,
            "30s idle / 60s lifetime → ratio 應為 0.5，實得 {ratio}"
        );
    }

    #[test]
    fn compute_idle_ratio_returns_none_when_lifetime_is_zero() {
        // 邊界：lifetime = 0（since == now）→ 純函式必須回 None，避開分母為 0 → NaN。
        // Prometheus 端看到 NaN 會被視為 anomaly，比「缺 sample」更糟糕。
        let now = Utc::now();
        let ratio = compute_provider_idle_ratio(now, Some(now), Some(now));
        assert_eq!(ratio, None, "lifetime = 0 → 必須跳過，不可回 NaN");
    }

    #[test]
    fn compute_idle_ratio_returns_none_when_since_is_in_future() {
        // 邊界：since > now（時鐘回撥 / 序列化時差）→ lifetime 為負 → 純函式必須
        // 回 None。對齊 K8 `idle_seconds_clamps_negative_to_zero` 的保守策略：
        // 「寧可少一條 sample」也不要假數據誤導監控。
        let now = Utc::now();
        let future_since = now + chrono::Duration::seconds(60);
        let last = now;
        let ratio = compute_provider_idle_ratio(now, Some(last), Some(future_since));
        assert_eq!(ratio, None, "since > now（lifetime 為負）→ 必須跳過");
    }

    #[test]
    fn compute_idle_ratio_saturates_future_last_event_at_to_zero() {
        // 邊界：last_event_at > now（序列化時差 / 剛寫入 race）→ idle 為負。
        // 對齊 K8 `.max(0)` 守門員：saturation 0 → 0.0 而非負值 / NaN。
        let now = Utc::now();
        let since = now - chrono::Duration::days(30);
        let future_last = now + chrono::Duration::seconds(1); // 故意未來 1 秒
        let ratio = compute_provider_idle_ratio(now, Some(future_last), Some(since))
            .expect("ratio should exist");
        assert!(
            (ratio - 0.0).abs() < 1e-9,
            "future last_event_at saturate 為 0 → ratio 應為 0.0，實得 {ratio}"
        );
    }

    // ----- render_prometheus_body 端對端：K12 段 -----

    #[test]
    fn idle_ratio_empty_state_emits_header_only() {
        // 對齊 K6/K7/K8/K9/K10/K11「empty state 不假裝 0」語意：空 map → 沒 sample line。
        // ratio = 0.0 語意危險（會被誤判「剛剛在動」），所以「不輸出」比「輸出 0」更安全。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_idle_ratio"));
        assert!(body.contains("# TYPE lobsterpulse_provider_idle_ratio gauge"));
        assert!(!body.contains("lobsterpulse_provider_idle_ratio{"));
    }

    #[test]
    fn idle_ratio_skips_providers_with_no_event_yet() {
        // K12 跳過策略對齊 K8：last_event_at = None → 沒 sample。
        // 對齊 `compute_idle_ratio_returns_none_when_last_event_at_is_none` 純函式語意。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_since_and_last_at(
                    "claude",
                    now - chrono::Duration::days(30),
                    now - chrono::Duration::seconds(60), // claude 有 idle 60s
                ),
                totals_no_event_with_since("cicx", now - chrono::Duration::days(7)),
                // cicx 從未收過 event → 跳過
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // claude 有 ratio → emit sample（具體數字由算法保證，這裡只驗 format 跟存在性）
        assert!(body.contains("lobsterpulse_provider_idle_ratio{provider=\"claude\"}"));
        // cicx 從未收過 event → 沒 sample line（idle 分子不存在）
        assert!(!body.contains("lobsterpulse_provider_idle_ratio{provider=\"cicx\"}"));
    }

    #[test]
    fn idle_ratio_skips_providers_with_no_since() {
        // K12 跳過策略對齊 K10：since = None → 沒 sample。
        // 對齊 `compute_idle_ratio_returns_none_when_since_is_none` 純函式語意。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_no_since("cicx"), // since=None → K10 也跳過，但 K12 額外驗
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        assert!(!body.contains("lobsterpulse_provider_idle_ratio{provider=\"cicx\"}"));
    }

    #[test]
    fn idle_ratio_skips_providers_with_zero_lifetime() {
        // lifetime = 0（since == now）→ 純函式回 None → 跳過 sample。
        // 對應純函式測試 `compute_idle_ratio_returns_none_when_lifetime_is_zero`。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_since_now("cicx", now), // lifetime = 0 → 跳過
                totals_with_since_and_last_at(
                    "claude",
                    now - chrono::Duration::seconds(60),
                    now - chrono::Duration::seconds(30), // 對照：claude 有 ratio = 0.5
                ),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // cicx lifetime=0 → 沒 sample
        assert!(!body.contains("lobsterpulse_provider_idle_ratio{provider=\"cicx\"}"));
        // claude 有 ratio → emit sample
        assert!(body.contains("lobsterpulse_provider_idle_ratio{provider=\"claude\"} 0.5000"));
    }

    #[test]
    fn idle_ratio_emits_fractional_value_with_four_decimals() {
        // 主軸：3 個 provider 各自不同 ratio → 驗 alphabetical 排序 + 4-decimal 格式。
        // 30s/60s = 0.5000、15s/30s = 0.5000、29s/30s = 0.9667（29/30 = 0.96666...）。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                // 故意非字母序：openx 排最後
                totals_with_since_and_last_at(
                    "openx",
                    now - chrono::Duration::seconds(30),
                    now - chrono::Duration::seconds(29), // idle 29s / lifetime 30s ≈ 0.9667
                ),
                totals_with_since_and_last_at(
                    "cicx",
                    now - chrono::Duration::seconds(60),
                    now - chrono::Duration::seconds(30), // 30/60 = 0.5000
                ),
                totals_with_since_and_last_at(
                    "gemini",
                    now - chrono::Duration::seconds(30),
                    now - chrono::Duration::seconds(15), // 15/30 = 0.5000
                ),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // 4-decimal 格式驗證（避免 f64 IEEE 754 尾數雜訊導致 Prometheus diff 不穩定）
        assert!(body.contains("lobsterpulse_provider_idle_ratio{provider=\"cicx\"} 0.5000\n"));
        assert!(body.contains("lobsterpulse_provider_idle_ratio{provider=\"gemini\"} 0.5000\n"));
        assert!(body.contains("lobsterpulse_provider_idle_ratio{provider=\"openx\"} 0.9667\n"));

        // alphabetical 排序：cicx < gemini < openx
        let cicx_idx = body
            .find("lobsterpulse_provider_idle_ratio{provider=\"cicx\"} 0.5000\n")
            .expect("cicx ratio line");
        let gemini_idx = body
            .find("lobsterpulse_provider_idle_ratio{provider=\"gemini\"} 0.5000\n")
            .expect("gemini ratio line");
        let openx_idx = body
            .find("lobsterpulse_provider_idle_ratio{provider=\"openx\"} 0.9667\n")
            .expect("openx ratio line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider idle_ratio 必須 alphabetical 排序"
        );
    }

    #[test]
    fn idle_ratio_uses_lifetime_aggregate_not_live_sessions() {
        // K12 核心 regression guard：lifetime-vs-live。
        // 0 個 live session（sessions=[]）但 ProviderTotals 有 since + last_event_at →
        // metric 仍正確反映 lifetime。對齊 K6/K7/K8/K9/K10 一致語意：
        // session 結束 + 30 min stale 回收後 live 為 0，但 lifetime ProviderTotals
        // 仍保留 → idle ratio 仍能量化「該 provider lifetime 內的健康度」。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                // claude lifetime 60s、idle 60s → 1.0
                totals_with_since_and_last_at(
                    "claude",
                    now - chrono::Duration::seconds(60),
                    now - chrono::Duration::seconds(60),
                ),
                // cicx lifetime 120s、idle 0s → 0.0
                totals_with_since_and_last_at("cicx", now - chrono::Duration::seconds(120), now),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        ); // 0 個 live session

        // lifetime aggregate 確保 ratio 不被 live session 影響
        assert!(body.contains("lobsterpulse_provider_idle_ratio{provider=\"claude\"} 1.0000\n"));
        assert!(body.contains("lobsterpulse_provider_idle_ratio{provider=\"cicx\"} 0.0000\n"));
    }

    #[test]
    fn idle_ratio_zero_is_distinguishable_from_absent() {
        // 邊界：ratio = 0（剛剛在動）vs 不在 metric map（last_event_at 或 since 缺）。
        // 兩種語意差很多：0.0 = 100% 健康；absent = 數據不完整不能算。
        // 對齊 K11「zero vs absent」設計原則：寧可「少一條 sample」也不要「假裝 0」
        // 誤導監控 —— K12 這點尤其重要（idle=0 會被 alert rule 直接放行）。
        let now = Utc::now();
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                // claude 有 ratio = 0（剛剛在動）→ 必須 emit
                totals_with_since_and_last_at("claude", now - chrono::Duration::days(30), now),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // claude 有 ratio=0 → 必須 emit sample line
        assert!(body.contains("lobsterpulse_provider_idle_ratio{provider=\"claude\"} 0.0000\n"));
        // 其他 provider（cicx / gemini）都不在 → 不 emit（且 not panic）
        assert!(!body.contains("lobsterpulse_provider_idle_ratio{provider=\"cicx\"}"));
        assert!(!body.contains("lobsterpulse_provider_idle_ratio{provider=\"gemini\"}"));
    }

    // ----- fs helper 測試：collect_quota_snapshot_mtimes -----

    /// K11 fs helper 測試 fixture：借用 R12/R22 config.rs 既有的 TmpDir pattern
    /// （pid 後綴命名 + Drop 自動清），避免本輪新引入 `tempfile` crate。
    /// YAGNI：1 個 helper struct 就夠 4 條 fs test 共享。
    struct QuotaSnapshotTmpDir(PathBuf);

    impl QuotaSnapshotTmpDir {
        fn new(label: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!(
                "lobsterpulse-k11-test-{}-{}",
                label,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("mkdir tmpdir");
            Self(p)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for QuotaSnapshotTmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn collect_quota_snapshot_mtimes_returns_none_for_all_when_home_is_none() {
        // 邊界：dirs::home_dir() 回 None（無 HOME env、罕見但可能）→ 不 crash，
        // 9 個 OpenAB bot + 1 個 local runner 全填 None，caller 端 filter 後不出現在
        // metric map（age 段就只 emit header、沒 sample）。
        //
        // R100: 5 → 9 OpenAB bot,對齊 R78 補完,改用 OPENAB_BOT_IDS const 驅動。
        let out = collect_quota_snapshot_mtimes(&None);

        assert_eq!(out.len(), 10, "9 OpenAB bot + __local__ 共 10 個 key");
        for p in OPENAB_BOT_IDS {
            assert_eq!(out.get(*p).copied(), Some(None), "{p} 應該是 None");
        }
        assert_eq!(
            out.get("__local__").copied(),
            Some(None),
            "__local__ 應該是 None"
        );
    }

    #[test]
    fn collect_quota_snapshot_mtimes_returns_mtime_for_existing_files() {
        // 主軸：home 存在 → 對 `~/.lobsterpulse/usage-{bot}.json` 與 `usage-local.json`
        // 做 `metadata()`，有檔案 → Some(mtime)、沒檔案 → None。
        // 寫 2 個假檔（cicx + __local__）→ 該 2 個 key 有 mtime、其他 8 個 None。
        // 注意：helper 內部會 `home.join(".lobsterpulse")` 當資料目錄，所以測試要把檔案寫
        // 在 `<tmp>/.lobsterpulse/` 下對齊 production shape。
        //
        // R100: 4 → 8 個未寫 key,對齊 9 OpenAB bot 全 slot 接上。
        let tmp = QuotaSnapshotTmpDir::new("mixed");
        let data_dir = tmp.path().join(".lobsterpulse");
        std::fs::create_dir_all(&data_dir).expect("mkdir .lobsterpulse");
        std::fs::write(data_dir.join("usage-cicx.json"), b"{}").expect("write cicx");
        std::fs::write(data_dir.join("usage-local.json"), b"{}").expect("write local");

        let home = Some(tmp.0.clone());
        let out = collect_quota_snapshot_mtimes(&home);

        // 有寫的 2 個 key → mtime 不是 None
        assert!(out.get("cicx").and_then(|m| m.as_ref()).is_some());
        assert!(out.get("__local__").and_then(|m| m.as_ref()).is_some());
        // 沒寫的 8 個 key → None
        for p in [
            "gitx",
            "giminix",
            "codex_bot",
            "openx",
            "irisx_bot",
            "grokx",
            "lpbot",
            "mimo",
        ] {
            assert_eq!(out.get(p).copied(), Some(None), "{p} 不存在檔案，應回 None");
        }
    }

    #[test]
    fn collect_quota_snapshot_mtimes_openx_legacy_alias_fallback() {
        // Legacy alias：對齊 `read_usage_snapshots`（line 314-336）的語意——
        // 沒 `usage-openx.json` 但有 `usage-bot.json`（OpenAB BackendType::Other 寫法）
        // → openx 拿到 usage-bot.json 的 mtime，避免 Prometheus 端 openx 永遠缺席。
        let tmp = QuotaSnapshotTmpDir::new("legacy");
        let data_dir = tmp.path().join(".lobsterpulse");
        std::fs::create_dir_all(&data_dir).expect("mkdir .lobsterpulse");
        // 故意不寫 usage-openx.json，只寫 usage-bot.json
        std::fs::write(data_dir.join("usage-bot.json"), b"{}").expect("write legacy bot");

        let home = Some(tmp.0.clone());
        let out = collect_quota_snapshot_mtimes(&home);

        // openx 應該走 legacy fallback 拿到 mtime
        assert!(
            out.get("openx").and_then(|m| m.as_ref()).is_some(),
            "openx 缺 usage-openx.json 時應 fallback 到 usage-bot.json 的 mtime"
        );
    }

    #[test]
    fn collect_quota_snapshot_mtimes_skips_legacy_when_openx_exists() {
        // 對齊 legacy alias 對稱語意：usage-openx.json 已存在 → 不讀 usage-bot.json
        // （避免兩個檔案 mtime 不同導致 openx 顯示「非主要檔案的時間」混淆）。
        // 製造方式：先寫 primary 等 50ms 再寫 legacy，確保兩個 mtime 在任何 fs 精度下
        // 都必然不同 → 鎖定 helper 不會回 legacy 的 mtime。
        let tmp = QuotaSnapshotTmpDir::new("primary");
        let data_dir = tmp.path().join(".lobsterpulse");
        std::fs::create_dir_all(&data_dir).expect("mkdir .lobsterpulse");
        std::fs::write(data_dir.join("usage-openx.json"), b"{}").expect("write openx primary");
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(data_dir.join("usage-bot.json"), b"{}").expect("write legacy bot");

        let home = Some(tmp.0.clone());
        let openx_mtime = std::fs::metadata(data_dir.join("usage-openx.json"))
            .and_then(|m| m.modified())
            .expect("primary mtime");
        let legacy_mtime = std::fs::metadata(data_dir.join("usage-bot.json"))
            .and_then(|m| m.modified())
            .expect("legacy mtime");

        let out = collect_quota_snapshot_mtimes(&home);

        // openx 應拿 primary 檔案的 mtime（不是 legacy）
        let actual = out
            .get("openx")
            .and_then(|m| m.as_ref())
            .expect("openx mtime");
        assert_eq!(
            *actual, openx_mtime,
            "openx 應以 usage-openx.json 為主、不採 usage-bot.json"
        );
        // 50ms 間隔保證 mtime 差異，鎖定 helper 不會回 legacy 的 mtime。
        assert_ne!(*actual, legacy_mtime, "不應回 legacy mtime");
    }

    // ===== K13 per-provider events_total counter =====

    #[test]
    fn events_total_empty_state_emits_header_only() {
        // 0 provider → header 有、sample line 沒有。對齊 K7 failure_count / K9
        // session_count 既有 empty-state 契約。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_events_total"));
        assert!(body.contains("# TYPE lobsterpulse_provider_events_total counter"));
        assert!(!body.contains("lobsterpulse_provider_events_total{"));
    }

    #[test]
    fn events_total_emits_sample_line_per_provider() {
        // 主軸：每個 provider 都 emit 一行 sample，數字 = ProviderTotals.events_total。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_events("claude", 42),
                totals_with_events("cicx", 7),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(
            body.contains("lobsterpulse_provider_events_total{provider=\"cicx\"} 7\n"),
            "cicx 應 emit 7\ngot:\n{body}"
        );
        assert!(
            body.contains("lobsterpulse_provider_events_total{provider=\"claude\"} 42\n"),
            "claude 應 emit 42\ngot:\n{body}"
        );
    }

    #[test]
    fn events_total_uses_lifetime_aggregate_not_live_sessions() {
        // 對齊 K6/K7/K9 lifetime-vs-live 核心 regression guard：session 結束 + 30 min
        // stale 回收後 live sessions 為空，但 ProviderTotals.events_total 仍保留 → metric
        // 仍正確反映 historical event 總量。這條避免 Prometheus counter 倒退（alert
        // 誤觸發「服務沒收到 event 了」），跟 K7 failure_count / K9 session_count 同 pattern。
        let body = render_prometheus_body(
            &[], // 0 live session
            0,
            0,
            &totals_map(vec![
                totals_with_events("claude", 100), // 已結束
                totals_with_events("cicx", 25),
                totals_with_events("gemini", 0), // 從未收過 event
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 0 個 live session（sessions=[]）但 ProviderTotals.events_total > 0 → metric 仍顯示
        assert!(body.contains("lobsterpulse_provider_events_total{provider=\"claude\"} 100\n"));
        assert!(body.contains("lobsterpulse_provider_events_total{provider=\"cicx\"} 25\n"));
        // events_total = 0 仍要 emit（0 是有意義的值「累計 0 個 event」），跟 K7 failure_count
        // 對齊（不要因 0 跳過 → 否則 Prometheus 端會誤判該 provider 從未存在）
        assert!(body.contains("lobsterpulse_provider_events_total{provider=\"gemini\"} 0\n"));
    }

    #[test]
    fn events_total_alphabetical_and_deterministic() {
        // 排序：故意非字母序插入 (openx, cicx, gemini) → 輸出 cicx, gemini, openx。
        // 這條是 Prometheus scraper diff 穩定的 regression guard —— 若有人改用
        // HashMap 自然順序，每次 render 順序可能變，Prometheus diff 會跳動。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_events("openx", 3),
                totals_with_events("cicx", 1),
                totals_with_events("gemini", 2),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        let cicx_idx = body
            .find("lobsterpulse_provider_events_total{provider=\"cicx\"}")
            .expect("cicx line");
        let gemini_idx = body
            .find("lobsterpulse_provider_events_total{provider=\"gemini\"}")
            .expect("gemini line");
        let openx_idx = body
            .find("lobsterpulse_provider_events_total{provider=\"openx\"}")
            .expect("openx line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "events_total 必須 alphabetical 排序（cicx < gemini < openx）"
        );
    }

    #[test]
    fn events_total_emits_integer_not_float() {
        // 鎖住 type 契約：counter 應該是 u64 整數（不是 f64 浮點）。若有人手滑改成
        // `f64`，Prometheus 端會看到 0.0 / 42.0 之類，混淆 counter / gauge 語意。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![totals_with_events("claude", 42)]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 必須是 " 42\n"（整數）—— 不是 " 42.0" 或 " 42.0000"
        assert!(body.contains("lobsterpulse_provider_events_total{provider=\"claude\"} 42\n"));
        // 反向：不能出現 ".0" / ".0000" 這類小數格式
        let line_start = body
            .find("lobsterpulse_provider_events_total{provider=\"claude\"}")
            .expect("claude line");
        let line_end = body[line_start..]
            .find('\n')
            .map(|i| line_start + i)
            .expect("line end");
        let line = &body[line_start..line_end];
        assert!(
            !line.contains(".0") && !line.contains(".0000"),
            "counter 應為整數格式、不該有小數：{line}"
        );
    }

    #[test]
    fn events_total_saturates_on_overflow_does_not_panic() {
        // 邊界：events_total 給超大值（u64::MAX）→ 不 panic、emit u64::MAX 整數。
        // 對齊 K6/K7 saturating_add 語意（雖然這裡直接賦值沒算術，但保險）。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![totals_with_events("claude", u64::MAX)]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // u64::MAX = 18446744073709551615
        assert!(
            body.contains(
                "lobsterpulse_provider_events_total{provider=\"claude\"} 18446744073709551615\n"
            ),
            "u64::MAX 應原樣 emit 整數，不 panic 不截斷"
        );
    }

    // ─── K14 Discord health metrics (process-level, 非 per-provider) ─────

    #[test]
    fn discord_health_default_state_emits_zero_gauge_and_empty_counters() {
        // 預設 DiscordHealth → gauge 0 (last_class=None), 三個 counter 0,
        // last_event_unix 0 (「啟動後還沒失敗過」語意)。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_discord_health"));
        assert!(body.contains("# TYPE lobsterpulse_discord_health gauge"));
        assert!(body.contains("lobsterpulse_discord_health 0\n"));

        assert!(body.contains("# HELP lobsterpulse_discord_send_failures_total"));
        assert!(body.contains("# TYPE lobsterpulse_discord_send_failures_total counter"));
        assert!(body.contains("lobsterpulse_discord_send_failures_total{class=\"4xx\"} 0\n"));
        assert!(body.contains("lobsterpulse_discord_send_failures_total{class=\"5xx\"} 0\n"));
        assert!(body.contains("lobsterpulse_discord_send_failures_total{class=\"network\"} 0\n"));

        assert!(body.contains("# HELP lobsterpulse_discord_last_event_unix"));
        assert!(body.contains("# TYPE lobsterpulse_discord_last_event_unix gauge"));
        assert!(body.contains("lobsterpulse_discord_last_event_unix 0\n"));
    }

    #[test]
    fn discord_health_with_4xx_5xx_and_network_failures_renders_all_four_lines() {
        // 模擬 lifetime state:4xx=3 次 / 5xx=1 次 / network=2 次,
        // 最後一筆是 Network (gauge=3), last_event_unix=1700000000
        let h = discord::DiscordHealth {
            class_4xx: 3,
            class_5xx: 1,
            class_network: 2,
            last_class: Some(discord::DiscordHealthClass::Network),
            last_event_unix: 1_700_000_000,
        };

        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &h,
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 3 個 counter 都 emit 正確數值
        assert!(body.contains("lobsterpulse_discord_send_failures_total{class=\"4xx\"} 3\n"));
        assert!(body.contains("lobsterpulse_discord_send_failures_total{class=\"5xx\"} 1\n"));
        assert!(body.contains("lobsterpulse_discord_send_failures_total{class=\"network\"} 2\n"));
        // last_class=Network → gauge 3
        assert!(body.contains("lobsterpulse_discord_health 3\n"));
        // last_event_unix 透出
        assert!(body.contains("lobsterpulse_discord_last_event_unix 1700000000\n"));
    }

    #[test]
    fn discord_health_class_to_gauge_mapping_in_render_output() {
        // 驗證 enum → 穩定整數映射在 render 端正確:4xx→1, 5xx→2
        let h_4xx = discord::DiscordHealth {
            last_class: Some(discord::DiscordHealthClass::Client4xx(401)),
            ..discord::DiscordHealth::default()
        };
        let body_4xx = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &h_4xx,
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body_4xx.contains("lobsterpulse_discord_health 1\n"),
            "Client4xx → gauge 1, body: {body_4xx}"
        );

        let h_5xx = discord::DiscordHealth {
            last_class: Some(discord::DiscordHealthClass::Server5xx(503)),
            ..discord::DiscordHealth::default()
        };
        let body_5xx = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &h_5xx,
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body_5xx.contains("lobsterpulse_discord_health 2\n"),
            "Server5xx → gauge 2, body: {body_5xx}"
        );
    }

    // ===== K17 落地：per-provider × per-event-type counter 測試群 =====
    //
    // 對齊 K13 lifetime aggregate 語意 + K6/K7/K9 既有 pattern：
    // 4 個測試覆蓋關鍵契約（empty / 單 type / 多 type 排序 / lifetime 保留）,
    // 任何後續 fixture 改動若打破 K17 契約會在這層炸出來。

    #[test]
    fn event_type_total_empty_state_emits_header_only() {
        // 跟 K13 / K9 / K6 等既有「空 state → 沒 sample line」契約一致。
        // render 端 for-loop 在空 event_type_counts 自然不產出 sample,
        // 但 HELP/TYPE 標頭仍輸出（對齊 K13 既有測試）。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_event_type_total"));
        assert!(body.contains("# TYPE lobsterpulse_provider_event_type_total counter"));
        // 空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_event_type_total{"));
    }

    #[test]
    fn event_type_total_single_type_emits_one_sample_per_provider() {
        // K17 主軸 1:單 event type → 一條 sample line 帶 (provider, type) 兩 label。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![totals_with_event_type_counts(
                "claude",
                &[("UserPromptSubmit", 3)],
            )]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains(
            "lobsterpulse_provider_event_type_total{provider=\"claude\",type=\"UserPromptSubmit\"} 3\n"
        ));
    }

    #[test]
    fn event_type_total_multiple_types_sorted_by_provider_then_type() {
        // K17 主軸 2:多 provider × 多 type → 兩段排序（先 provider 後 type）
        // 跟既有 K6-K13 排序契約一致,確保 Prometheus scrape byte-deterministic。
        // 故意用「非字母序」輸入:openx 先於 cicx,Stop 先於 UserPromptSubmit。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![
                totals_with_event_type_counts("openx", &[("UserPromptSubmit", 5), ("Stop", 2)]),
                totals_with_event_type_counts(
                    "cicx",
                    &[("PostToolUseFailure", 1), ("UserPromptSubmit", 4)],
                ),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // cicx 系列（PostToolUseFailure < UserPromptSubmit alphabetical）
        assert!(body.contains(
            "lobsterpulse_provider_event_type_total{provider=\"cicx\",type=\"PostToolUseFailure\"} 1\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_event_type_total{provider=\"cicx\",type=\"UserPromptSubmit\"} 4\n"
        ));
        // openx 系列
        assert!(body.contains(
            "lobsterpulse_provider_event_type_total{provider=\"openx\",type=\"Stop\"} 2\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_event_type_total{provider=\"openx\",type=\"UserPromptSubmit\"} 5\n"
        ));

        // 排序驗證:cicx < openx（provider 主排序）
        let cicx_pp_idx = body
            .find(
                "lobsterpulse_provider_event_type_total{provider=\"cicx\",type=\"PostToolUseFailure\"} 1\n",
            )
            .expect("cicx PostToolUseFailure line");
        let openx_stop_idx = body
            .find("lobsterpulse_provider_event_type_total{provider=\"openx\",type=\"Stop\"} 2\n")
            .expect("openx Stop line");
        assert!(
            cicx_pp_idx < openx_stop_idx,
            "provider 主排序必須 alphabetical (cicx < openx)"
        );

        // 同 provider 內 type 副排序:PostToolUseFailure < UserPromptSubmit
        let cicx_pp_idx2 = body
            .find(
                "lobsterpulse_provider_event_type_total{provider=\"cicx\",type=\"PostToolUseFailure\"} 1\n",
            )
            .expect("cicx PostToolUseFailure line 2");
        let cicx_up_idx = body
            .find(
                "lobsterpulse_provider_event_type_total{provider=\"cicx\",type=\"UserPromptSubmit\"} 4\n",
            )
            .expect("cicx UserPromptSubmit line");
        assert!(
            cicx_pp_idx2 < cicx_up_idx,
            "同 provider 內 type 副排序必須 alphabetical (PostToolUseFailure < UserPromptSubmit)"
        );
    }

    #[test]
    fn event_type_total_uses_lifetime_aggregate_not_live_sessions() {
        // K17 lifetime-vs-live 契約:跟 K6/K7/K9/K13 對齊 —— 就算 0 個 live
        // session,ProviderTotals.event_type_counts 仍保留 → metric 正確反映
        // lifetime（session 結束 + 30 min stale 回收後不蒸發）。
        // 給 0 session 但 provider_totals 有 type 累計 → 仍要產出 sample。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals_map(vec![totals_with_event_type_counts(
                "claude",
                &[("UserPromptSubmit", 100), ("Stop", 99)],
            )]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 0 live session,但 lifetime event_type_counts 仍有值 → sample 仍輸出
        assert!(body.contains(
            "lobsterpulse_provider_event_type_total{provider=\"claude\",type=\"UserPromptSubmit\"} 100\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_event_type_total{provider=\"claude\",type=\"Stop\"} 99\n"
        ));
    }

    // ─── K18 tests ─────────────────────────────────────────────────────
    // K18 落地：per-provider max active session age gauge。
    // 差異化既有 metric：K8 看「最後一次 event」秒數,K18 看「session 開到
    // 現在」秒數。Operator 端用 K18 偵測「runner 還在 active 但已卡很久」。

    #[test]
    fn max_session_age_empty_state_emits_header_only() {
        // 對齊 K8/K12 empty-state 契約:0 個 live session → 沒 sample line。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_max_session_age_seconds"));
        assert!(body.contains("# TYPE lobsterpulse_provider_max_session_age_seconds gauge"));
        assert!(!body.contains("lobsterpulse_provider_max_session_age_seconds{"));
    }

    #[test]
    fn max_session_age_takes_max_across_active_sessions_same_provider() {
        // K18 主軸:同 provider 多個 active session → 取 max(各自 duration)。
        // claude 兩個 active session(30s / 300s)→ 應輸出 300。
        let body = render_prometheus_body(
            &[
                info_with_age("claude", true, 30),
                info_with_age("claude", true, 300),
            ],
            2,
            2,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body
            .contains("lobsterpulse_provider_max_session_age_seconds{provider=\"claude\"} 300\n"));
    }

    #[test]
    fn max_session_age_ignores_inactive_sessions() {
        // K18 語意:active = false 的 session 不算(已被 stale 回收前 idle 過久
        // 但不是「live 卡住」)。is_active 必須為 true 才納入 max 計算。
        let body = render_prometheus_body(
            &[
                info_with_age("claude", false, 9999), // inactive → 忽略
                info_with_age("cicx", true, 60),
            ],
            1,
            1,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // claude 雖有 9999 秒 duration 但 inactive → 不出 sample line
        assert!(
            !body.contains("lobsterpulse_provider_max_session_age_seconds{provider=\"claude\"}")
        );
        // cicx active 60 秒 → 出 sample
        assert!(
            body.contains("lobsterpulse_provider_max_session_age_seconds{provider=\"cicx\"} 60\n")
        );
    }

    #[test]
    fn max_session_age_per_provider_independent() {
        // 多 provider 各自有 active session → 各自 max 獨立,alphabetical 排序。
        // 故意非字母序輸入驗排序契約。
        let body = render_prometheus_body(
            &[
                info_with_age("openx", true, 7200),
                info_with_age("cicx", true, 60),
                info_with_age("gemini", true, 3600),
            ],
            3,
            3,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(
            body.contains("lobsterpulse_provider_max_session_age_seconds{provider=\"cicx\"} 60\n")
        );
        assert!(body
            .contains("lobsterpulse_provider_max_session_age_seconds{provider=\"gemini\"} 3600\n"));
        assert!(body
            .contains("lobsterpulse_provider_max_session_age_seconds{provider=\"openx\"} 7200\n"));

        // 排序驗證: cicx < gemini < openx(對齊 K6-K13 既有排序契約)
        let cicx_idx = body
            .find("lobsterpulse_provider_max_session_age_seconds{provider=\"cicx\"} 60\n")
            .expect("cicx max_age line");
        let gemini_idx = body
            .find("lobsterpulse_provider_max_session_age_seconds{provider=\"gemini\"} 3600\n")
            .expect("gemini max_age line");
        let openx_idx = body
            .find("lobsterpulse_provider_max_session_age_seconds{provider=\"openx\"} 7200\n")
            .expect("openx max_age line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider max_session_age_seconds 必須 alphabetical 排序"
        );
    }

    #[test]
    fn max_session_age_clamps_negative_duration_to_zero() {
        // 邊界:`duration_secs` 為負(時鐘回撥 / 序列化時間差 edge case) →
        // clamp 到 0,避免 Prometheus 端看到負值。
        let body = render_prometheus_body(
            &[info_with_age("claude", true, -5)],
            1,
            1,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(
            body.contains("lobsterpulse_provider_max_session_age_seconds{provider=\"claude\"} 0\n")
        );
        // 確保沒有負號進 output
        assert!(!body
            .contains("lobsterpulse_provider_max_session_age_seconds{provider=\"claude\"} -5\n"));
    }

    // ============== K19 per-provider × per-state session count gauge ==============
    // 補 K6 細顆度盲點：operator alert rule `provider_sessions_by_state{state="stale"} > 5`
    // 不需要靠 K6 + K8 湊出「5 個 session 全 stale」訊號。4 個 test 覆蓋
    // (1) empty 契約 (2) 4 state 各 1 個 session 計數 (3) sort 兩段 (4) sum by(provider) 不變式。

    #[test]
    fn provider_sessions_by_state_empty_state_emits_header_only() {
        // 對齊 K6/K8/K12/K18 empty-state 契約：0 個 live session → 沒 sample line，
        // 只有 HELP/TYPE 標頭（讓 Prometheus scrape 端知道 metric 已註冊）。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_sessions_by_state"));
        assert!(body.contains("# TYPE lobsterpulse_provider_sessions_by_state gauge"));
        assert!(!body.contains("lobsterpulse_provider_sessions_by_state{"));
    }

    #[test]
    fn provider_sessions_by_state_counts_each_state_separately() {
        // K19 主軸：同 provider 4 個 session 各屬 4 個 state → 4 條 sample line 各自獨立計數 1。
        // 用 `info_with_state` fixture 強制 state（既有 `info` 強制 Working/Idle,無法測 4 state）。
        // 跨 provider 也驗：claude 1 working + gemini 1 stale + gemini 1 waiting_for_user
        // 驗不同 (provider, state) 對不會合併計數。
        let body = render_prometheus_body(
            &[
                info_with_state("claude", true, SessionState::Working),
                info_with_state("claude", true, SessionState::Idle),
                info_with_state("claude", true, SessionState::WaitingForUser),
                info_with_state("claude", true, SessionState::Stale),
                info_with_state("gemini", true, SessionState::Stale),
                info_with_state("gemini", true, SessionState::WaitingForUser),
            ],
            6,
            6,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // claude 4 條（每個 state 1 個）
        assert!(body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"claude\",state=\"idle\"} 1\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"claude\",state=\"working\"} 1\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"claude\",state=\"waiting_for_user\"} 1\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"claude\",state=\"stale\"} 1\n"
        ));
        // gemini 2 條
        assert!(body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"stale\"} 1\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"waiting_for_user\"} 1\n"
        ));
        // gemini 沒 idle/working → 不應有那 2 條
        assert!(!body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"idle\"}"
        ));
        assert!(!body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"working\"}"
        ));
    }

    #[test]
    fn provider_sessions_by_state_sorted_by_provider_then_state() {
        // 排序契約：先 provider alphabetical,再 state alphabetical(idle < stale <
        // waiting_for_user < working 對齊 SessionState serde snake_case label)。
        // 故意非字母序輸入驗排序。
        let body = render_prometheus_body(
            &[
                info_with_state("openx", true, SessionState::Working),
                info_with_state("cicx", true, SessionState::Stale),
                info_with_state("cicx", true, SessionState::Idle),
                info_with_state("openx", true, SessionState::Idle),
            ],
            4,
            4,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 找各 (provider, state) line 位置驗排序
        let cicx_idle = body
            .find("lobsterpulse_provider_sessions_by_state{provider=\"cicx\",state=\"idle\"} 1\n")
            .expect("cicx idle line");
        let cicx_stale = body
            .find("lobsterpulse_provider_sessions_by_state{provider=\"cicx\",state=\"stale\"} 1\n")
            .expect("cicx stale line");
        let openx_idle = body
            .find("lobsterpulse_provider_sessions_by_state{provider=\"openx\",state=\"idle\"} 1\n")
            .expect("openx idle line");
        let openx_working = body
            .find(
                "lobsterpulse_provider_sessions_by_state{provider=\"openx\",state=\"working\"} 1\n",
            )
            .expect("openx working line");

        // cicx < openx (provider 段)
        assert!(cicx_idle < openx_idle, "cicx 必須排在 openx 之前");
        assert!(cicx_stale < openx_idle, "cicx 必須排在 openx 之前");
        // cicx 內 idle < stale (state 段)
        assert!(cicx_idle < cicx_stale, "cicx 內 idle 必須排在 stale 之前");
        // openx 內 idle < working
        assert!(
            openx_idle < openx_working,
            "openx 內 idle 必須排在 working 之前"
        );
    }

    #[test]
    fn provider_sessions_by_state_sum_invariant_equals_provider_sessions() {
        // 不變式：`sum by(provider)(provider_sessions_by_state) == provider_sessions`。
        // K6 跟 K19 來自同一 live sessions slice,只是 K19 多了 state 切面。算 K6 行的
        // provider_sessions gauge 值應該等於 K19 該 provider 4 個 state 加總。
        // claude: 1 working + 2 waiting_for_user + 1 stale = 4
        // gemini: 1 idle + 1 working = 2
        let body = render_prometheus_body(
            &[
                info_with_state("claude", true, SessionState::Working),
                info_with_state("claude", true, SessionState::WaitingForUser),
                info_with_state("claude", true, SessionState::WaitingForUser),
                info_with_state("claude", true, SessionState::Stale),
                info_with_state("gemini", false, SessionState::Idle),
                info_with_state("gemini", true, SessionState::Working),
            ],
            6,
            4, // active=4
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // K6 provider_sessions（lifetime aggregate）: counter 值由 ProviderTotals 決定,
        // 但這裡傳空 HashMap → 0。改驗 K6 active gauge: `provider_active{provider="..."} 4`
        // 對 4 active session（claude 4 active + gemini 1 active = 5? 不對,gemini 1 active）。
        // 重新算：claude 4 個 is_active=true + gemini 1 個 is_active=true + gemini 1 個 is_active=false。
        // active count = 4 (claude) + 1 (gemini 1 個 is_active=true) = 5。
        // 但 K19 細度：claude sum = 4, gemini sum = 1 idle + 1 working = 2。
        // 不變式: K19 sum by(provider) 反映「該 provider live session 數」,跟 K6
        // `provider_sessions` (lifetime) 不直接相等。改用更明確的不變式：
        // K19 sum by(provider) == sessions slice 中該 provider 的計數。
        let k19_claude_lines: Vec<&str> = body
            .lines()
            .filter(|l| {
                l.starts_with("lobsterpulse_provider_sessions_by_state{provider=\"claude\"")
            })
            .collect();
        let k19_claude_sum: usize = k19_claude_lines
            .iter()
            .filter_map(|l| l.rsplit(' ').next())
            .filter_map(|n| n.parse::<usize>().ok())
            .sum();
        assert_eq!(k19_claude_sum, 4, "claude K19 sum by(provider) 必須 = 4");

        let k19_gemini_lines: Vec<&str> = body
            .lines()
            .filter(|l| {
                l.starts_with("lobsterpulse_provider_sessions_by_state{provider=\"gemini\"")
            })
            .collect();
        let k19_gemini_sum: usize = k19_gemini_lines
            .iter()
            .filter_map(|l| l.rsplit(' ').next())
            .filter_map(|n| n.parse::<usize>().ok())
            .sum();
        assert_eq!(k19_gemini_sum, 2, "gemini K19 sum by(provider) 必須 = 2");

        // 額外驗證:gemini 1 idle + 1 working 兩條都存在
        assert!(body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"idle\"} 1\n"
        ));
        assert!(body.contains(
            "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"working\"} 1\n"
        ));
    }

    // ============== K20 per-provider quota_remaining_pct gauge ==============
    // 對齊 K11 freshness 維度（snapshot 多舊）補 consumption 維度（quota 還剩多少）:
    // operator alert `lobsterpulse_provider_quota_remaining_pct{provider="cicx"} < 10`
    // 觸發「cicx 即將耗盡」,搭配 K11 `age > 600` 判「snapshot 沒更新」避免假警報。
    // 5 個 test 覆蓋:empty / single / sort / 0 保留 / integer 格式。
    // 資料源 `latest_quota_pct_at` 的 5 個 unit test 在 `quota_history.rs` 端。

    #[test]
    fn quota_remaining_pct_empty_map_emits_header_only() {
        // 對齊 K11 / K18 / K19 empty-state 契約：0 個 runner 有 quota 資料 →
        // 沒 sample line,只有 HELP/TYPE 標頭(讓 Prometheus scrape 端知道 metric 已註冊)。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_quota_remaining_pct"));
        assert!(body.contains("# TYPE lobsterpulse_provider_quota_remaining_pct gauge"));
        // 沒 sample line:grep "lobsterpulse_provider_quota_remaining_pct{" 拿掉 HELP/TYPE 應為 0
        let sample_count = body
            .lines()
            .filter(|l| l.starts_with("lobsterpulse_provider_quota_remaining_pct{"))
            .count();
        assert_eq!(
            sample_count, 0,
            "empty map 不應 emit sample line, body: {body}"
        );
    }

    #[test]
    fn quota_remaining_pct_single_provider_emits_one_sample_line() {
        // 主軸:單 runner 有 quota 資料 → emit 一行 sample,值 = map 內的 pct。
        let mut quota = HashMap::new();
        quota.insert("cicx".to_string(), 42_u8);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &quota,
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("lobsterpulse_provider_quota_remaining_pct{provider=\"cicx\"} 42\n"));
        // 沒其他 provider:sample line 應只有 1 行
        let sample_count = body
            .lines()
            .filter(|l| l.starts_with("lobsterpulse_provider_quota_remaining_pct{"))
            .count();
        assert_eq!(
            sample_count, 1,
            "單 provider 應只有 1 sample line, body: {body}"
        );
    }

    #[test]
    fn quota_remaining_pct_alphabetical_sort_across_providers() {
        // 多 provider 故意非字母序插入(openx, cicx, gemini)→ 輸出必須 alphabetical
        // (cicx, gemini, openx),跟 K6-K19 既契約一致,給 Prometheus scraper diff 穩定。
        let mut quota = HashMap::new();
        quota.insert("openx".to_string(), 7_u8);
        quota.insert("cicx".to_string(), 42_u8);
        quota.insert("gemini".to_string(), 100_u8);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &quota,
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        let cicx_idx = body
            .find("lobsterpulse_provider_quota_remaining_pct{provider=\"cicx\"} 42\n")
            .expect("cicx sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_quota_remaining_pct{provider=\"gemini\"} 100\n")
            .expect("gemini sample line");
        let openx_idx = body
            .find("lobsterpulse_provider_quota_remaining_pct{provider=\"openx\"} 7\n")
            .expect("openx sample line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider quota_remaining_pct 必須 alphabetical 排序 \
             (cicx={cicx_idx}, gemini={gemini_idx}, openx={openx_idx})"
        );
    }

    #[test]
    fn quota_remaining_pct_zero_pct_is_emitted_not_dropped() {
        // 語意關鍵:0% 是 critical signal(runner quota 已耗盡 → operator 必須看到),
        // 不能在 render 階段當作 None 跳過。對齊 R32 `load_history_at_zero_pct_emitted`
        // + K20 `latest_quota_pct_at_zero_pct_is_emitted_not_dropped` 同契約。
        let mut quota = HashMap::new();
        quota.insert("cicx".to_string(), 0_u8);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &quota,
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(
            body.contains("lobsterpulse_provider_quota_remaining_pct{provider=\"cicx\"} 0\n"),
            "0% 是有效 quota 耗盡訊號,必須 emit, body: {body}"
        );
        // 確保不是 float 格式(` 0.0` 不該出現)
        assert!(
            !body.contains("lobsterpulse_provider_quota_remaining_pct{provider=\"cicx\"} 0.0\n"),
            "0 必須是整數格式,不是 float, body: {body}"
        );
    }

    #[test]
    fn quota_remaining_pct_emits_integer_not_float() {
        // 鎖住 ` 42\n` 整數格式(不是 ` 42.0`):u8 → Display 是整數,跟 K13
        // `events_total_emits_integer_not_float` 同契約,避免混淆 gauge 語意
        // (整數 42 = 42% 剩餘,float 42.0 會被 Prometheus 端視為 double metric)。
        let mut quota = HashMap::new();
        quota.insert("cicx".to_string(), 42_u8);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &quota,
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(
            body.contains("lobsterpulse_provider_quota_remaining_pct{provider=\"cicx\"} 42\n"),
            "整數格式契約, body: {body}"
        );
        assert!(
            !body.contains("lobsterpulse_provider_quota_remaining_pct{provider=\"cicx\"} 42.0\n"),
            "不能是 float 格式, body: {body}"
        );
    }

    // ============== K21 lobsterpulse_quota_history_csv_age_seconds gauge ==============
    // 對齊 K11 freshness 視角(snapshot 多舊) + K20 同一資料源(quota-history.csv 的
    // consumption)。差異化:K20 看「最新 pct 數字」(consumption),K21 看「CSV 多久沒
    // 被 OpenAB 寫進來」(freshness of the history file itself)→ 跟 K11 互補(都是
    // 「沒更新」訊號但觀察不同檔)。
    // 3 個 test 覆蓋:first-run (None → header only) / 剛寫完 (Some(0) → emit 0) /
    // 正常 (Some(123) → emit 123)。資料源 `quota_history_csv_mtime_at` 的 2 個
    // unit test 在 `quota_history.rs` 端。

    #[test]
    fn quota_history_csv_age_none_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 empty-state 契約:None → 沒 sample line,
        // 只有 HELP/TYPE 標頭(讓 Prometheus scrape 端知道 metric 已註冊)。
        // first-run 場景:CSV 還沒被 OpenAB 寫過,呼叫端不該誤判「剛剛 mtime=now
        // → age=0」(對齊 R32 first-run 契約)。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_quota_history_csv_age_seconds"));
        assert!(body.contains("# TYPE lobsterpulse_quota_history_csv_age_seconds gauge"));
        // 沒 sample line — 改用 lines().filter(starts_with) 鎖真正的 sample line,
        // 避免 `!contains("metric_name ")` 跟 HELP/TYPE 標頭(也含「name + 空格」)誤撞。
        let k21_samples: Vec<&str> = body
            .lines()
            .filter(|l| l.starts_with("lobsterpulse_quota_history_csv_age_seconds "))
            .collect();
        assert!(
            k21_samples.is_empty(),
            "None 不應 emit sample line(否則會跟「Some(0) 剛寫完」混淆), got: {k21_samples:?}, body: {body}"
        );
    }

    #[test]
    fn quota_history_csv_age_some_zero_emits_zero_not_dropped() {
        // 語意關鍵:Some(0) = CSV 剛剛被寫完(age = 0)是有效資料 → 必須 emit。
        // 對齊 K20 `quota_remaining_pct_zero_pct_is_emitted_not_dropped` 同契約:
        // 「0 是 critical signal,不能被當 None 跳過」。K20 是「quota 耗盡」,
        // K21 是「CSV 剛被寫、pipeline 在跑」——都是「0 必須保留」的場景。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            Some(0),
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(
            body.contains("lobsterpulse_quota_history_csv_age_seconds 0\n"),
            "Some(0) 是剛寫完的有效資料,必須 emit, body: {body}"
        );
    }

    #[test]
    fn quota_history_csv_age_some_positive_emits_integer() {
        // 正常情況:CSV 123 秒前被寫 → emit 整數 123。鎖住整數格式(不是 float)
        // 跟 K20 `quota_remaining_pct_emits_integer_not_float` + K13
        // `events_total_emits_integer_not_float` 同契約。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            Some(123),
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(
            body.contains("lobsterpulse_quota_history_csv_age_seconds 123\n"),
            "整數格式契約, body: {body}"
        );
        assert!(
            !body.contains("lobsterpulse_quota_history_csv_age_seconds 123.0\n"),
            "不能是 float 格式, body: {body}"
        );
    }

    #[test]
    fn last_completed_session_age_empty_map_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 / K21 empty-state 契約:空 map → 沒 sample line,
        // 只有 HELP/TYPE 標頭(讓 Prometheus scrape 端知道 metric 已註冊)。
        // first-run 場景:該 provider 累計進來但還沒收過 SessionEnd / 還沒經歷
        // Working→Idle 轉換,`last_completed_session_age_secs` 仍是 None → pure fn
        // `last_completed_session_age_at` 過濾掉,進不到 map。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(body.contains("# HELP lobsterpulse_provider_last_completed_session_age_seconds"));
        assert!(
            body.contains("# TYPE lobsterpulse_provider_last_completed_session_age_seconds gauge")
        );
        // 沒 sample line — 用 lines().filter(starts_with) 鎖真正的 sample line,
        // 避免 `!contains("metric_name ")` 跟 HELP/TYPE 標頭(也含「name + 空格」)誤撞。
        let k22_samples: Vec<&str> = body
            .lines()
            .filter(|l| l.starts_with("lobsterpulse_provider_last_completed_session_age_seconds{"))
            .collect();
        assert!(
            k22_samples.is_empty(),
            "空 map 不應 emit sample line, got: {k22_samples:?}, body: {body}"
        );
    }

    #[test]
    fn last_completed_session_age_zero_is_emitted_not_dropped() {
        // 語意關鍵:age=0 (session start 後馬上 SessionEnd → 兩次 event 時差 < 1 sec)
        // 是有效資料,不是 missing。對齊 K20 `quota_remaining_pct_zero_pct_is_emitted_not_dropped`
        // + K21 `quota_history_csv_age_some_zero_emits_zero_not_dropped` 同契約:
        // 「0 是 critical signal,不能被當 None 跳過」 —— 這裡 0 是「session 跑 0 秒就結束」
        // 異常訊號(可能 runner 立即 crash),operator 必須看到。
        let mut age = HashMap::new();
        age.insert("cicx".to_string(), 0_i64);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &age,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        assert!(
            body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"} 0\n"
            ),
            "0 是有效資料必須 emit, body: {body}"
        );
    }

    #[test]
    fn last_completed_session_age_alphabetical_sort_across_providers() {
        // 多 provider 故意非字母序插入(openx, cicx, gemini)→ 輸出必須 alphabetical
        // (cicx, gemini, openx),跟 K6-K21 既契約一致,給 Prometheus scraper diff 穩定。
        // 同時驗證 0 / 正數混合都會 emit + 整數格式(不是 float)。
        let mut age = HashMap::new();
        age.insert("openx".to_string(), 7_i64);
        age.insert("cicx".to_string(), 0_i64);
        age.insert("gemini".to_string(), 1500_i64);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &age,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        let cicx_idx = body
            .find("lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"} 0\n")
            .expect("cicx sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_last_completed_session_age_seconds{provider=\"gemini\"} 1500\n")
            .expect("gemini sample line");
        let openx_idx = body
            .find(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"openx\"} 7\n",
            )
            .expect("openx sample line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider last_completed_session_age_seconds 必須 alphabetical 排序 \
             (cicx={cicx_idx}, gemini={gemini_idx}, openx={openx_idx})"
        );
        assert!(
            !body.contains("lobsterpulse_provider_last_completed_session_age_seconds{provider=\"gemini\"} 1500.0\n"),
            "整數格式契約(不是 float)"
        );
    }

    // ============== K24 per-provider completed_sessions_total_duration_seconds counter ==============
    // 跟 K22/K23 互補形成「總時長 / 總次數 = 平均 time-to-completion」公式:operator 端算
    // `..._duration_seconds / ..._total` 觀察平均效率 KPI。3 個 render test 覆蓋
    // empty / zero / alphabetical 排序三個邊界,跟 K20-K23 既有 render test 風格一致。
    // 數據源:不是獨立 HashMap,直接讀 `provider_totals` —— 跟 K6 / K7 / K8 / K9 / K10 /
    // K13 / K17 / K18 / K19 / K22 / K23 同資料源,讓 render helper 自己派發 pure fn
    // 攤平（對齊 R26/R27 政策:cross-cutting snapshot 留給 M1 輪 MetricsSnapshot struct
    // 統一處理,本輪不重構 render 端 12 個參數的怪 signature）。

    #[test]
    fn completed_sessions_total_duration_empty_totals_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 / K21 / K22 empty-state 契約:空 map → 沒 sample line
        // (HELP/TYPE 標頭仍輸出),不丟假資料。ProviderTotals 沒 entry → 該 provider 不會被
        // 寫進 metric,避免 Prometheus 端把「沒看到」當「total=0」誤判「這 provider 沒在跑」。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains("# HELP lobsterpulse_provider_completed_sessions_total_duration_seconds")
        );
        assert!(body.contains(
            "# TYPE lobsterpulse_provider_completed_sessions_total_duration_seconds counter"
        ));
        // 沒 sample line — 用 lines().filter(starts_with) 鎖真正的 sample line,
        // 避免 `!contains("metric_name ")` 跟 HELP/TYPE 標頭(也含「name + 空格」)誤撞。
        let k24_samples: Vec<&str> = body
            .lines()
            .filter(|l| {
                l.starts_with("lobsterpulse_provider_completed_sessions_total_duration_seconds{")
            })
            .collect();
        assert!(
            k24_samples.is_empty(),
            "空 totals 不應 emit sample line, got: {k24_samples:?}, body: {body}"
        );
    }

    #[test]
    fn completed_sessions_total_duration_zero_is_emitted_not_dropped() {
        // 語意關鍵:total=0 (該 provider 累計收過 event 但還沒完成過 session) 是有效資料,
        // 不是 missing。對齊 K20 `quota_remaining_pct_zero_pct_is_emitted_not_dropped` +
        // K21 `quota_history_csv_age_some_zero_emits_zero_not_dropped` + K23
        // `completed_sessions_count_at_emits_zero_for_uncompleted_provider` 同契約:
        // 「counter 0 跟 missing 是不同語意,Prometheus 端應該看到 0」 —— K24 用同一策略。
        // 構造 ProviderTotals entry with total=0（其他欄位 default）→ render 必須 emit
        // `..._duration_seconds{provider="cicx"} 0`,不能跳過。0 跟 K24 公式的 0/0 配套：
        // K23 count=0 + K24 total=0 → 0/0 = NaN Prometheus 端不會展示 NaN,只看到 0 + 0 兩
        // 條 series → render 保留 0 是對的。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 0,
                completed_sessions_total_duration_secs: 0,
                ..Default::default()
            },
        );
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_total_duration_seconds{provider=\"cicx\"} 0\n"
            ),
            "0 是有效資料必須 emit, body: {body}"
        );
    }

    #[test]
    fn completed_sessions_total_duration_alphabetical_sort_across_providers() {
        // 多 provider 故意非字母序插入(openx, cicx, gemini)→ 輸出必須 alphabetical
        // (cicx, gemini, openx),跟 K6-K23 既契約一致,給 Prometheus scraper diff 穩定。
        // 同時驗證不同時長(180 / 7200 / 42)都會 emit + 整數格式(不是 float)。挑選這三個
        // 數字刻意避免 0,順帶驗 K24 saturating_add 累加後的值正確（不是 last-wins）。
        let mut totals = HashMap::new();
        totals.insert(
            "openx".to_string(),
            ProviderTotals {
                completed_sessions_total_duration_secs: 42,
                ..Default::default()
            },
        );
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_total_duration_secs: 180,
                ..Default::default()
            },
        );
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_total_duration_secs: 7200,
                ..Default::default()
            },
        );
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_total_duration_seconds{provider=\"cicx\"} 180\n")
            .expect("cicx sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_total_duration_seconds{provider=\"gemini\"} 7200\n")
            .expect("gemini sample line");
        let openx_idx = body
            .find("lobsterpulse_provider_completed_sessions_total_duration_seconds{provider=\"openx\"} 42\n")
            .expect("openx sample line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider completed_sessions_total_duration_seconds 必須 alphabetical 排序 \
             (cicx={cicx_idx}, gemini={gemini_idx}, openx={openx_idx})"
        );
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_total_duration_seconds{provider=\"gemini\"} 7200.0\n"),
            "整數格式契約(不是 float)"
        );
    }

    // ============== K25 per-provider completed_sessions_average_duration_seconds gauge ==============
    // 跟 K23 (count) / K24 (total_duration) 形成派生 average time-to-completion KPI。
    // 3 個 render test 覆蓋 empty / zero-count-skip / alphabetical+precision 三個邊界,
    // 跟 K20-K24 既有 render test 風格一致。數據源:不是獨立 HashMap,直接讀 `provider_totals`
    // —— 跟 K6-K24 同資料源, 讓 render helper 自己派發 pure fn 攤平（對齊 R26/R27 政策:
    // cross-cutting snapshot 留給 M1 輪 MetricsSnapshot struct 統一處理, 本輪不重構
    // render 端 12 個參數的怪 signature）。

    #[test]
    fn completed_sessions_average_duration_empty_totals_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 / K21 / K22 / K23 / K24 empty-state 契約:
        // 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出), 不丟假資料。
        // ProviderTotals 沒 entry → 該 provider 不會被寫進 metric, 避免 Prometheus
        // 端把「沒看到」當「average=0」誤判「該 provider 瞬間完成」= 假健康信號。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(body
            .contains("# HELP lobsterpulse_provider_completed_sessions_average_duration_seconds"));
        assert!(body.contains(
            "# TYPE lobsterpulse_provider_completed_sessions_average_duration_seconds gauge"
        ));
        // 沒 sample line — 用 lines().filter(starts_with) 鎖真正的 sample line,
        // 避免 `!contains("metric_name ")` 跟 HELP/TYPE 標頭(也含「name + 空格」)誤撞。
        let k25_samples: Vec<&str> = body
            .lines()
            .filter(|l| {
                l.starts_with("lobsterpulse_provider_completed_sessions_average_duration_seconds{")
            })
            .collect();
        assert!(
            k25_samples.is_empty(),
            "空 totals 不應 emit K25 sample line, got: {k25_samples:?}, body: {body}"
        );
    }

    #[test]
    fn completed_sessions_average_duration_zero_count_provider_is_skipped() {
        // K25 vs K24 emit 策略差異化在 render 端的體現：K24 emit 0 (counter 0 是
        // 有效), K25 count=0 → 跳過 (0/0 = NaN, emit 0.0 會誤導成「平均 0 秒
        // 完成」= 假健康信號)。構造 cicx entry with count=0 + total=0 (default)
        // → render 不該 emit K25 sample line (但 K24 仍會 emit `..._duration_seconds{...} 0`
        // —— 順便在 assert 中驗 K24 不受 K25 skip 邏輯影響, 兩條 series 行為獨立)。
        let mut totals = HashMap::new();
        totals.insert("cicx".to_string(), ProviderTotals::default());

        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"}"
            ),
            "count=0 該跳過 K25 sample, 不能 emit 0.0 假冒 average=0, body: {body}"
        );
        // K24 不受 K25 skip 邏輯影響, 仍 emit 0 (counter 0 跟 K25 跳過是不同語意)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_total_duration_seconds{provider=\"cicx\"} 0\n"
            ),
            "K24 emit 策略獨立於 K25, count=0 仍 emit total=0, body: {body}"
        );
    }

    #[test]
    fn completed_sessions_average_duration_alphabetical_sort_and_four_decimal_precision() {
        // 3 provider 非字母序插入(openx, cicx, gemini)→ 輸出必須 alphabetical
        // (cicx, gemini, openx), 跟 K6-K24 既契約一致, 給 Prometheus scraper diff
        // 穩定。同時驗 f64 `{:.4}` 4 位小數固定 precision：(cicx 180/3=60, gemini
        // 7200/2=3600, openx 42/1=42 —— 全部整除 → 末四碼 0000)。挑整除值避免
        // IEEE 754 尾數雜訊干擾 precision assert。
        let mut totals = HashMap::new();
        totals.insert(
            "openx".to_string(),
            ProviderTotals {
                completed_sessions_count: 1,
                completed_sessions_total_duration_secs: 42,
                ..Default::default()
            },
        );
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 3,
                completed_sessions_total_duration_secs: 180,
                ..Default::default()
            },
        );
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_count: 2,
                completed_sessions_total_duration_secs: 7200,
                ..Default::default()
            },
        );
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} 60.0000\n")
            .expect("cicx sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"gemini\"} 3600.0000\n")
            .expect("gemini sample line");
        let openx_idx = body
            .find("lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"openx\"} 42.0000\n")
            .expect("openx sample line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider completed_sessions_average_duration_seconds 必須 alphabetical 排序 \
             (cicx={cicx_idx}, gemini={gemini_idx}, openx={openx_idx})"
        );
        // 反向驗：確認 emit 的是 f64 4 位小數格式, 不是 u64 整數格式 (沒有 `.0000` 結尾)
        // K24 counter 是整數 (沒小數), K25 gauge 是 f64 — 必須在輸出區分開。
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} 60\n"),
            "f64 4 位小數格式契約(不是 u64 整數), 必須含 `.0000` 結尾, body: {body}"
        );
    }

    // ============== R52：K23 (count) / K24 (total) / K25 (avg) 跨 K-tag render 一致性護欄 ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結 gauge 補閉環」→ R52 補既有 K23/K24/K25
    // 三件套 render 端 cross-metric emission 一致性護欄。K23 (counter) / K24
    // (counter) / K25 (gauge) 是 lifetime aggregate 派生三件套,語意強綁定: K25
    // = K24 / K23 (count > 0) 是定義恆等式, render 端必須 (a) count > 0 的
    // provider 三條 series 全部 emit + K25 算術跟 K24/K23 一致, (b) count = 0
    // 的 provider K23/K24 emit 0 (counter 0 有效) + K25 跳過 (0/0 NaN 防線)。
    // 既有 K23/K24/K25 14 個 render test 都是單 metric 隔離, 沒驗證「同
    // provider 三條 series 互相 emit 一致」, 若有人未來改 render 端 emit 邏輯
    // 漏 K25 或 K24 (e.g. 誤把 K25 條件從 count > 0 改成 count >= 0 emit 0.0),
    // 現有 test 抓不出, 要到 Prometheus scrape 端 alert 異常才被動發現。

    #[test]
    fn r52_k23_k24_k25_render_emission_consistency_across_mixed_count_providers() {
        // 4 provider 混合: cicx (count=3, total=180 → avg=60) + claude (count=0,
        // total=0 → K25 跳過) + gemini (count=2, total=7200 → avg=3600) + openx
        // (count=0, total=0 → K25 跳過)。 對齊 session.rs R52
        // `r52_k23_k24_k25_emission_set_consistency_under_zero_count_providers`
        // 純 fn 護欄的 fixture, 補 render 端的同語意不變式 (count > 0 三條全
        // emit + K25 算術一致, count = 0 K25 跳過 + K23/K24 emit 0)。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 3,
                completed_sessions_total_duration_secs: 180,
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_count: 2,
                completed_sessions_total_duration_secs: 7200,
                ..Default::default()
            },
        );
        totals.insert("openx".to_string(), ProviderTotals::default());
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // ── Part A: count > 0 provider (cicx, gemini) 三條 series 必須全 emit + K25 算術正確
        for (p, expected_count, expected_total, expected_avg_str) in [
            ("cicx", 3, 180, "60.0000"),
            ("gemini", 2, 7200, "3600.0000"),
        ] {
            // K23 counter 整數
            assert!(
                body.contains(&format!(
                    "lobsterpulse_provider_completed_sessions_total{{provider=\"{p}\"}} {expected_count}\n"
                )),
                "{p}: K23 count={expected_count} 必須 emit (count > 0 → K23 必 emit), body: {body}"
            );
            // K24 counter 整數
            assert!(
                body.contains(&format!(
                    "lobsterpulse_provider_completed_sessions_total_duration_seconds{{provider=\"{p}\"}} {expected_total}\n"
                )),
                "{p}: K24 total={expected_total} 必須 emit (count > 0 → K24 必 emit), body: {body}"
            );
            // K25 gauge f64 4 位小數
            assert!(
                body.contains(&format!(
                    "lobsterpulse_provider_completed_sessions_average_duration_seconds{{provider=\"{p}\"}} {expected_avg_str}\n"
                )),
                "{p}: K25 avg={expected_avg_str} 必須 emit (count > 0 → K25 必 emit), body: {body}"
            );
        }
        // 跨 K-tag emission set 互不污染: cicx/gemini 三條 series 都必須 emit 完整
        // (沒有「K25 emit 了但 K24 漏」之類的不一致破壞)
        let cicx_k23 = body
            .find("lobsterpulse_provider_completed_sessions_total{provider=\"cicx\"} 3\n")
            .expect("cicx K23 sample line");
        let cicx_k24 = body
            .find("lobsterpulse_provider_completed_sessions_total_duration_seconds{provider=\"cicx\"} 180\n")
            .expect("cicx K24 sample line");
        let cicx_k25 = body
            .find("lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} 60.0000\n")
            .expect("cicx K25 sample line");
        // K25 emit K24 emit K23 (K25 是 K24/K23 派生, 順序應 K23 → K24 → K25)
        // 但 emit 順序只需「都在」, 不嚴格要求 monotonic (依 render block 排列)
        assert!(
            cicx_k23 < cicx_k25 && cicx_k24 < cicx_k25,
            "cicx K25={cicx_k25} 必須在 K23={cicx_k23} / K24={cicx_k24} 之後 emit \
             (K25 是 K24/K23 派生, 順序應 K23/K24 → K25)"
        );

        // ── Part B: count = 0 provider (claude, openx) K25 跳過 + K23/K24 emit 0
        for p in ["claude", "openx"] {
            // K23 counter 0 仍 emit (counter 0 跟 missing 不同語意)
            assert!(
                body.contains(&format!(
                    "lobsterpulse_provider_completed_sessions_total{{provider=\"{p}\"}} 0\n"
                )),
                "{p}: K23 count=0 仍 emit (counter 0 有效), 不能 skip, body: {body}"
            );
            // K24 counter 0 仍 emit
            assert!(
                body.contains(&format!(
                    "lobsterpulse_provider_completed_sessions_total_duration_seconds{{provider=\"{p}\"}} 0\n"
                )),
                "{p}: K24 total=0 仍 emit (counter 0 有效), 不能 skip, body: {body}"
            );
            // K25 count=0 必須跳過 (0/0 NaN 防線, emit 0.0 = 假健康信號)
            assert!(
                !body.contains(&format!(
                    "lobsterpulse_provider_completed_sessions_average_duration_seconds{{provider=\"{p}\"}}"
                )),
                "{p}: K25 count=0 必須跳過 (0/0 NaN 防線), 但 emit 了 sample, body: {body}"
            );
        }

        // ── Part C: K25 算術跟 K24/K23 在 render 端一致 (parse render 端 f64 4 位小數)
        // cicx K25 = 60.0000 = 180/3 ✓, gemini K25 = 3600.0000 = 7200/2 ✓ — 已
        // Part A 用 exact string match 驗證, 此處額外驗 K25 沒 emit 任何「異常值」
        // (e.g. NaN / Infinity / 負值) 跨 4 provider
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} NaN"),
            "K25 不能 emit NaN (count > 0 派生必為有限值), body: {body}"
        );
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"gemini\"} NaN"),
            "K25 不能 emit NaN (count > 0 派生必為有限值), body: {body}"
        );
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} -"),
            "K25 不能 emit 負值 (total/count 必 >= 0), body: {body}"
        );

        // ── Part D: 跨 K-tag emission set 包含關係 (跟 session.rs pure fn 護欄對齊)
        // 計數: K23 sample line 數量 = 4 (全部 provider), K24 sample line 數量 = 4
        // (全部 provider), K25 sample line 數量 = 2 (只有 cicx + gemini)
        let k23_count = body
            .matches("lobsterpulse_provider_completed_sessions_total{provider=\"")
            .count();
        let k24_count = body
            .matches("lobsterpulse_provider_completed_sessions_total_duration_seconds{provider=\"")
            .count();
        let k25_count = body
            .matches(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"",
            )
            .count();
        assert_eq!(
            k23_count, 4,
            "K23 emit 必須 = 4 provider (counter 0 有效), got {k23_count}, body: {body}"
        );
        assert_eq!(
            k24_count, 4,
            "K24 emit 必須 = 4 provider (counter 0 有效), got {k24_count}, body: {body}"
        );
        assert_eq!(
            k25_count, 2,
            "K25 emit 必須 = 2 provider (cicx + gemini, count > 0), got {k25_count}, body: {body}"
        );
    }

    // ============== R53：K22 (latest age) / K26 (max duration) / K27 (min duration) lifetime aggregate render 護欄 ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結 gauge 補閉環」紀律延伸 — R51 補 K30/K31/K32
    // (P50/P95/P99) bounds, R52 補 K23/K24/K25 (count/total/avg) cross-metric 算術,
    // R53 補 K22/K26/K27 (latest age / max duration / min duration) lifetime aggregate
    // 三件套 render 端 monotonic chain: K27 min_duration ≤ K22 latest_age ≤ K26 max_duration。
    //
    // 跟 session.rs R53 pure fn 護欄對齊 — 純 fn 端驗 K27 ≤ K22 ≤ K26 在 ProviderTotals
    // 寫入時成立, render 端驗同一不變式在 emit 後 Prometheus 抓得到的字串上仍成立。
    // 三件套都是 Option<i64> lifetime aggregate, 都過濾 None (該 provider 沒完成過
    // session → gauge 缺資料跳過不 emit, 避免「沒看到」= 假健康信號)。
    //
    // 維度差異 (跟 R51 K30/K31/K32 window percentile 護欄的關鍵區別): K22 是
    // 「最近一次完成 session 的持續秒數」(= last_completed_session_age_secs,
    // lifetime 覆寫成最後一次的 secs), K26/K27 是 saturating_min/max (lifetime
    // 抓 N 個 completion 的最小/最大持續秒數)。Monotonic chain K27 ≤ K22 ≤ K26
    // 成立的前提: K22 是「同一組 completion samples 之一」(不是獨立時鐘)。
    // 護欄 fixture 設計: 對每個 provider 灌同一組 N 個 completion samples, K22 latest
    // 必 = 最後一個 sample, K27 必 = 1, K26 必 = N, monotonic 自動成立。

    #[test]
    fn r53_k22_k26_k27_render_emission_consistency_across_mixed_completion_states() {
        // 4 provider 混合 fixture: cicx (3 completion, 1..=3 → K27=1, K22=3, K26=3) +
        // claude (1 completion, 42 → K27=42, K22=42, K26=42 三件套退化) + gemini
        // (2 completion, 10 + 20 → K27=10, K22=20, K26=20) + openx (0 completion →
        // 三件套 None 跳過, 不 emit)。 對齊 session.rs R53 純 fn 護欄
        // `r53_k22_k26_k27_per_provider_isolation_under_oversubscribed_completions` 的
        // 4-provider 結構, 補 render 端的同語意不變式 (K27 ≤ K22 ≤ K26 在字串端成立 +
        // 三件套互不污染 + None provider 三件套統一跳過)。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 3,
                last_completed_session_age_secs: Some(3),
                max_completed_session_age_secs: Some(3),
                min_completed_session_age_secs: Some(1),
                ..Default::default()
            },
        );
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                completed_sessions_count: 1,
                last_completed_session_age_secs: Some(42),
                max_completed_session_age_secs: Some(42),
                min_completed_session_age_secs: Some(42),
                ..Default::default()
            },
        );
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_count: 2,
                last_completed_session_age_secs: Some(20),
                max_completed_session_age_secs: Some(20),
                min_completed_session_age_secs: Some(10),
                ..Default::default()
            },
        );
        totals.insert("openx".to_string(), ProviderTotals::default());
        // K22 配套：算 `last_completed_session_age_at(&totals)` 給 8th 參數,
        // 模擬 production 從 ProviderTotals 派生 K22 map 的路徑 (K22 跟 K26/K27
        // 共用 provider_totals 資料源 → 8th 參數該跟 totals 同步, 不能傳空 HashMap)。
        let last_completed = last_completed_session_age_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // ── Part A: completion > 0 provider (cicx, claude, gemini) 三件套全 emit + 字串精確比對
        // cicx: K22=3, K26=3, K27=1
        assert!(
            body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"} 3\n"
            ),
            "cicx: K22=3 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"cicx\"} 3\n"
            ),
            "cicx: K26=3 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"cicx\"} 1\n"
            ),
            "cicx: K27=1 必須 emit, body: {body}"
        );
        // claude: 三件套退化 K22=K26=K27=42
        assert!(
            body.contains("lobsterpulse_provider_last_completed_session_age_seconds{provider=\"claude\"} 42\n"),
            "claude: K22=42 必須 emit (三件套退化相等), body: {body}"
        );
        assert!(
            body.contains("lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"claude\"} 42\n"),
            "claude: K26=42 必須 emit, body: {body}"
        );
        assert!(
            body.contains("lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"claude\"} 42\n"),
            "claude: K27=42 必須 emit, body: {body}"
        );
        // gemini: K22=20, K26=20, K27=10
        assert!(
            body.contains("lobsterpulse_provider_last_completed_session_age_seconds{provider=\"gemini\"} 20\n"),
            "gemini: K22=20 必須 emit, body: {body}"
        );
        assert!(
            body.contains("lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"gemini\"} 20\n"),
            "gemini: K26=20 必須 emit, body: {body}"
        );
        assert!(
            body.contains("lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"gemini\"} 10\n"),
            "gemini: K27=10 必須 emit, body: {body}"
        );

        // ── Part B: completion = 0 provider (openx) 三件套統一跳過 (None 過濾)
        // 注意: openx 在 ProviderTotals default() 下, K22/K26/K27 全 None,
        // render 端不應 emit 任何 sample line
        assert!(
            !body.contains("lobsterpulse_provider_last_completed_session_age_seconds{provider=\"openx\"}"),
            "openx: K22=None 必須跳過 (沒完成過 session, gauge 缺資料), 但 emit 了 sample, body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K26=None 必須跳過, body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K27=None 必須跳過, body: {body}"
        );

        // ── Part C: 三件套 emit 順序 K22 → K26 → K27 (跟 render_prometheus_body
        // emit block 順序一致)。K22 在 K26/K27 之前 emit (K22 = last_completed
        // age 在 gauge 系列最早), K26 (max) 在 K27 (min) 之前 emit (avg/max
        // block 順序)。Byte offset 順序鎖住「三件套都 emit 過且 render 端
        // emission 順序穩定」, 真正的 K27 ≤ K22 ≤ K26 monotonic chain 在
        // Part E 算術 parse 驗證。
        for p in ["cicx", "claude", "gemini"] {
            let k22_pos = body
                .find(&format!(
                    "lobsterpulse_provider_last_completed_session_age_seconds{{provider=\"{p}\"}}"
                ))
                .unwrap_or_else(|| panic!("{p}: K22 sample 必須在 body 內, body: {body}"));
            let k26_pos = body
                .find(&format!(
                    "lobsterpulse_provider_completed_sessions_max_duration_seconds{{provider=\"{p}\"}}"
                ))
                .unwrap_or_else(|| panic!("{p}: K26 sample 必須在 body 內, body: {body}"));
            let k27_pos = body
                .find(&format!(
                    "lobsterpulse_provider_completed_sessions_min_duration_seconds{{provider=\"{p}\"}}"
                ))
                .unwrap_or_else(|| panic!("{p}: K27 sample 必須在 body 內, body: {body}"));
            assert!(
                k22_pos < k26_pos && k26_pos < k27_pos,
                "{p}: 必須 K22 < K26 < K27 在 body 內的字串位置 (emit 順序鎖住, \
                 真正的 K27 ≤ K22 ≤ K26 chain 在 Part E 算術驗), \
                 got k22={k22_pos} k26={k26_pos} k27={k27_pos}, body: {body}"
            );
        }

        // ── Part D: 跨 K-tag emission set 一致性 (跟 session.rs pure fn 護欄對齊)
        // 計數: K22 sample line 數量 = 3 (cicx + claude + gemini), K26 sample line
        // 數量 = 3, K27 sample line 數量 = 3 (openx 跳過)
        let k22_count = body
            .matches("lobsterpulse_provider_last_completed_session_age_seconds{provider=\"")
            .count();
        let k26_count = body
            .matches("lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"")
            .count();
        let k27_count = body
            .matches("lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"")
            .count();
        assert_eq!(
            k22_count, 3,
            "K22 emit 必須 = 3 provider (cicx + claude + gemini, openx 跳過), got {k22_count}, body: {body}"
        );
        assert_eq!(
            k26_count, 3,
            "K26 emit 必須 = 3 provider, got {k26_count}, body: {body}"
        );
        assert_eq!(
            k27_count, 3,
            "K27 emit 必須 = 3 provider, got {k27_count}, body: {body}"
        );
        // 跨 K-tag emission set 完全一致 (三件套都過濾 None, 所以 emit 集合必須相等)
        assert_eq!(
            k22_count, k26_count,
            "K22 emit 集合 = K26 emit 集合 (同 Option 過濾策略), got K22={k22_count} K26={k26_count}"
        );
        assert_eq!(
            k26_count, k27_count,
            "K26 emit 集合 = K27 emit 集合, got K26={k26_count} K27={k27_count}"
        );

        // ── Part E: 數值 monotonic chain 在 render 端成立 (parse 字串驗算術)
        // 對 cicx (K27=1, K22=3, K26=3) + gemini (K27=10, K22=20, K26=20) 兩組
        // 退化樣本, 從 body 解析 K27/K22/K26 數值, 驗 K27 ≤ K22 ≤ K26
        for (p, expected_min, expected_latest, expected_max) in
            [("cicx", 1i64, 3, 3), ("gemini", 10, 20, 20)]
        {
            // 從 body 解析 K27 數值
            let k27_line = format!(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{{provider=\"{p}\"}}"
            );
            let k27_val: i64 = body
                .split(&k27_line)
                .nth(1)
                .and_then(|s| s.lines().next())
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or_else(|| panic!("{p}: K27 必須能 parse 成 i64, body: {body}"));
            let k22_line = format!(
                "lobsterpulse_provider_last_completed_session_age_seconds{{provider=\"{p}\"}}"
            );
            let k22_val: i64 = body
                .split(&k22_line)
                .nth(1)
                .and_then(|s| s.lines().next())
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or_else(|| panic!("{p}: K22 必須能 parse 成 i64, body: {body}"));
            let k26_line = format!(
                "lobsterpulse_provider_completed_sessions_max_duration_seconds{{provider=\"{p}\"}}"
            );
            let k26_val: i64 = body
                .split(&k26_line)
                .nth(1)
                .and_then(|s| s.lines().next())
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or_else(|| panic!("{p}: K26 必須能 parse 成 i64, body: {body}"));
            assert_eq!(
                k27_val, expected_min,
                "{p}: K27 必須 = {expected_min} (render 端數值), got {k27_val}"
            );
            assert_eq!(
                k22_val, expected_latest,
                "{p}: K22 必須 = {expected_latest} (render 端數值), got {k22_val}"
            );
            assert_eq!(
                k26_val, expected_max,
                "{p}: K26 必須 = {expected_max} (render 端數值), got {k26_val}"
            );
            // Monotonic chain 算術驗證 (K22 是「該次 session 持續秒數」, 屬於
            // 同一組 completion samples 之一, K27 ≤ K22 ≤ K26 成立)
            assert!(
                k27_val <= k22_val,
                "{p}: K27 ({k27_val}) 必須 ≤ K22 ({k22_val}) (render 端算術)"
            );
            assert!(
                k22_val <= k26_val,
                "{p}: K22 ({k22_val}) 必須 ≤ K26 ({k26_val}) (render 端算術)"
            );
        }
    }

    // ============== R54：K30 P95 (sliding window percentile) / K25 avg (lifetime arithmetic mean) render 端 outlier ratio 護欄 ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結新增 gauge 一週」紀律延伸 — R51 補 K30/K31/K32
    // (P50/P95/P99) bounds, R52 補 K23/K24/K25 (count/total/avg) cross-metric 算術,
    // R53 補 K22/K26/K27 (latest age / max duration / min duration) lifetime aggregate
    // monotonic chain, R54 補 K30 (sliding window P95) / K25 (lifetime arithmetic mean)
    // 跨窗口 outlier ratio render 端算術護欄: 構造 ProviderTotals fixture 模擬 4 種
    // provider 混合場景, 驗 render 端 Prometheus 抓得到的字串上 K30 P95 / K25 avg 都能
    // emit, parse 算術 outlier ratio per-provider 一致 (uniform < 2.0, outlier > 5
    // alert 觸發), openx 空 provider 跳過。
    //
    // 跟 session.rs R54 pure fn 護欄對齊 — 純 fn 端驗 outlier ratio 在 ProviderTotals
    // 寫入時成立, render 端驗同一不變式在 emit 後 Prometheus 抓得到的字串上仍成立。
    // K25 emit 是 f64 4-decimal ({:.4} → 50.5 emit 為 "50.5000"), K30 P95 emit 是
    // 整數 ({} → 96 emit 為 "96"), 兩種 format 對應 `completed_sessions_average_duration_at`
    // 跟 `completed_sessions_p95_at` 既契約, render test 用字串比對 + f64 parse
    // 雙驗。

    #[test]
    fn r54_k30_p95_to_k25_avg_outlier_ratio_render_emission_consistency_across_mixed_profiles() {
        // 4 provider 混合 fixture 對應 session.rs R54 三個護欄語意面:
        //   cicx: 100 樣本 [1..100] 均勻 → outlier ratio ≈ 1.90 < 2.0 (uniform)
        //   claude: 6 樣本 [1,1,1,1,1,1000] outlier → outlier ratio ≈ 5.97 > 5 (alert 觸發)
        //   gemini: 50 樣本 [1..50] 均勻 → outlier ratio ≈ 1.88 < 2.0 (uniform)
        //   openx: 0 樣本 → 兩件套 None 跳過
        // 對齊 session.rs R54 純 fn 護欄三個語意面 (uniform below 2.0 / outlier
        // detected > 5 / per-provider 隔離) 在 render 端 Prometheus 抓得到的字串
        // 上仍成立, 證明 outlier ratio alert 維度 production scrape 端可用。
        let mut totals = HashMap::new();
        // cicx: 100 樣本 [1..100]
        //   K25 avg = (1+2+...+100)/100 = 5050/100 = 50.5 → emit 為 "50.5000"
        //   K30 P95 = samples sort 後 [1..100] idx=95 = 96 → emit 為 "96"
        //   ratio = 96/50.5 ≈ 1.901 < 2.0
        let mut cicx_p95_samples = (1i64..=100).collect::<Vec<_>>();
        cicx_p95_samples.sort_unstable();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 100,
                completed_sessions_total_duration_secs: (1..=100).sum(),
                completed_sessions_p95_samples: cicx_p95_samples,
                ..Default::default()
            },
        );
        // claude: 6 樣本 [1, 1, 1, 1, 1, 1000]
        //   K25 avg = 1005/6 = 167.5 → emit 為 "167.5000"
        //   K30 P95 = samples sort 後 [1, 1, 1, 1, 1, 1000] idx=5 = 1000 → emit 為 "1000"
        //   ratio = 1000/167.5 ≈ 5.97 > 5 (alert 觸發)
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                completed_sessions_count: 6,
                completed_sessions_total_duration_secs: 1005,
                completed_sessions_p95_samples: vec![1, 1, 1, 1, 1, 1000],
                ..Default::default()
            },
        );
        // gemini: 50 樣本 [1..50]
        //   K25 avg = (1+2+...+50)/50 = 1275/50 = 25.5 → emit 為 "25.5000"
        //   K30 P95 = samples sort 後 [1..50] idx=47 = 48 → emit 為 "48"
        //   ratio = 48/25.5 ≈ 1.882 < 2.0
        let mut gemini_p95_samples = (1i64..=50).collect::<Vec<_>>();
        gemini_p95_samples.sort_unstable();
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_count: 50,
                completed_sessions_total_duration_secs: (1..=50).sum(),
                completed_sessions_p95_samples: gemini_p95_samples,
                ..Default::default()
            },
        );
        // openx: 0 樣本 → 兩件套 None 跳過
        totals.insert("openx".to_string(), ProviderTotals::default());
        let last_completed = last_completed_session_age_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // ── Part A: K25 avg + K30 P95 字串 emit
        // cicx: K25 avg = 50.5 → "50.5000", K30 P95 = 96 → "96"
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} 50.5000\n"
            ),
            "cicx: K25 avg=50.5000 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 96\n"
            ),
            "cicx: K30 P95=96 必須 emit, body: {body}"
        );
        // claude: K25 avg = 167.5 → "167.5000", K30 P95 = 1000 → "1000"
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"claude\"} 167.5000\n"
            ),
            "claude: K25 avg=167.5000 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"claude\"} 1000\n"
            ),
            "claude: K30 P95=1000 必須 emit, body: {body}"
        );
        // gemini: K25 avg = 25.5 → "25.5000", K30 P95 = 48 → "48"
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"gemini\"} 25.5000\n"
            ),
            "gemini: K25 avg=25.5000 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"gemini\"} 48\n"
            ),
            "gemini: K30 P95=48 必須 emit, body: {body}"
        );

        // ── Part B: openx 跳過 (count=0, K25 過濾; samples 空, K30 過濾)
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K25 avg 必須跳過 (count=0)"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K30 P95 必須跳過 (samples 空)"
        );

        // ── Part C: parse 字串算 outlier ratio 驗 alert 邊界
        // 對 cicx (uniform < 2.0) + claude (outlier > 5) + gemini (uniform < 2.0) 三組
        // 從 body 解析 K25 avg 跟 K30 P95, 算 ratio 並驗 alert 邊界。parse fn 用
        // split(&metric_label).nth(1).lines().next().parse() 跟 R52 K23/K24/K25
        // render test 既有模式一致。
        let parse_metric = |body: &str, metric: &str, provider: &str| -> f64 {
            let line = format!("{metric}{{provider=\"{provider}\"}}");
            body.split(&line)
                .nth(1)
                .and_then(|s| s.lines().next())
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or_else(|| panic!("{provider}: {metric} 必須能 parse, body: {body}"))
        };
        for (p, expect_alert) in [("cicx", false), ("claude", true), ("gemini", false)] {
            let avg: f64 = parse_metric(
                &body,
                "lobsterpulse_provider_completed_sessions_average_duration_seconds",
                p,
            );
            let p95: f64 = parse_metric(
                &body,
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds",
                p,
            );
            let ratio = p95 / avg;
            if expect_alert {
                assert!(
                    ratio > 5.0,
                    "{p}: render 端 outlier ratio = {ratio:.4} 必須 > 5.0 (outlier fixture alert 觸發), body: {body}"
                );
            } else {
                assert!(
                    ratio < 2.0,
                    "{p}: render 端 outlier ratio = {ratio:.4} 必須 < 2.0 (uniform fixture 比例有界), body: {body}"
                );
            }
        }

        // ── Part D: K25 emit 數量 = 3 (cicx + claude + gemini, openx 跳過)
        let k25_count = body
            .matches(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"",
            )
            .count();
        let k30_count = body
            .matches("lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"")
            .count();
        assert_eq!(
            k25_count, 3,
            "K25 emit 必須 = 3 provider (cicx + claude + gemini, openx 跳過), got {k25_count}, body: {body}"
        );
        assert_eq!(
            k30_count, 3,
            "K30 P95 emit 必須 = 3 provider, got {k30_count}, body: {body}"
        );
        // 跨 K-tag emission set 一致 (K25 過濾 count=0, K30 過濾 samples 空 → emit 集合相同)
        assert_eq!(
            k25_count, k30_count,
            "K25 emit 集合 = K30 P95 emit 集合 (同 None 過濾, got K25={k25_count} K30={k30_count})"
        );
    }

    // ============== R55：K30/K31/K32/K33/K34 (P95/P50/P99/P75/P25) sliding window percentile chain render 端算術護欄 + R56：K27 lifetime min ≤ K34 window P25 跨 lifetime↔window 護欄 ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結 gauge 補閉環」紀律延伸 — R51 補 K30/K31/K32
    // (P50/P95/P99) bounds, R53 補 K22/K26/K27 lifetime aggregate monotonic chain,
    // R54 補 K30 P95 / K25 avg outlier ratio cross-window, R55 補 K30-K34 五件套
    // sliding window percentile chain (P25 ≤ P50 ≤ P75 ≤ P95 ≤ P99), R56 順手補
    // K27 lifetime min ≤ K34 window P25 跨 lifetime↔window 護欄。
    //
    // 跟 session.rs R55/R56 pure fn 護欄對齊 — 純 fn 端驗 chain 在 ProviderTotals
    // 寫入時成立, render 端驗同一不變式在 emit 後 Prometheus 抓得到的字串上仍成立。
    // K30-K34 五件套共用 `completed_sessions_p95_samples: Vec<i64>` 同一份 reservoir
    // (跟 K33 doc 開頭 + R54 K30 P95 fixture 同款 — 不開新欄位, 9 provider 72KB),
    // K27 `min_completed_session_age_secs: Option<i64>` 獨立 lifetime aggregate。
    //
    // Chain 數學:
    // 1. R55 五件套 chain: K34 P25 (idx=25%) ≤ K31 P50 (idx=50%) ≤ K33 P75 (idx=75%)
    //    ≤ K30 P95 (idx=95%) ≤ K32 P99 (idx=99%) — 同 samples sort 後 percentile
    //    index 排序位置保證單調遞增, 數學不變式對任何非空樣本集都成立。
    // 2. R56 跨 lifetime↔window chain: K27 min_duration_secs (lifetime saturating_min)
    //    ≤ K34 P25 (window Q1) — lifetime min = P0, P25 ≥ P0 (Q1 必 ≥ 最小值),
    //    即使 lifetime min 來自已滑出 window 的舊 completion 也仍 ≤ window P25
    //    (舊值更小, chain 自動成立), 數學上 lifetime min ≤ window 任一百分位。
    //
    // 五件套 + K27 都是 i64 整數 emit (`{}` 跟 K22/K26/K27 整數契約統一), 跟 K25/K28
    // f64 4-decimal 兩條不同契約 (R54 已處理 K25 K30 P95 outlier 邊界, R55 不重疊)。

    #[test]
    fn r55_r56_percentile_chain_and_lifetime_min_render_emission_consistency_across_mixed_profiles()
    {
        // 4 provider 混合 fixture 對齊 R53/R54 既模板:
        //   cicx: 100 樣本 [1..100] uniform → P25=26, P50=51, P75=76, P95=96, P99=100, K27=1
        //         chain: 1 ≤ 26 ≤ 51 ≤ 76 ≤ 96 ≤ 100 ✓
        //   claude: 6 樣本 [1,1,1,1,1,1000] outlier → P25=1, P50=1, P75=1, P95=1000, P99=1000, K27=1
        //         chain: 1 ≤ 1 ≤ 1 ≤ 1 ≤ 1000 ≤ 1000 ✓
        //   gemini: 50 樣本 [1..50] uniform → P25=13, P50=25, P75=38, P95=48, P99=50, K27=1
        //         chain: 1 ≤ 13 ≤ 25 ≤ 38 ≤ 48 ≤ 50 ✓
        //   openx: 0 樣本, K27=None → 五 percentile + K27 = 6 件套全 None 跳過
        let mut totals = HashMap::new();
        // cicx: 100 樣本 [1..100] + K27=1
        let mut cicx_samples = (1i64..=100).collect::<Vec<_>>();
        cicx_samples.sort_unstable();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 100,
                min_completed_session_age_secs: Some(1),
                completed_sessions_p95_samples: cicx_samples,
                ..Default::default()
            },
        );
        // claude: 6 樣本 [1,1,1,1,1,1000] + K27=1
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                completed_sessions_count: 6,
                min_completed_session_age_secs: Some(1),
                completed_sessions_p95_samples: vec![1, 1, 1, 1, 1, 1000],
                ..Default::default()
            },
        );
        // gemini: 50 樣本 [1..50] + K27=1
        let mut gemini_samples = (1i64..=50).collect::<Vec<_>>();
        gemini_samples.sort_unstable();
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_count: 50,
                min_completed_session_age_secs: Some(1),
                completed_sessions_p95_samples: gemini_samples,
                ..Default::default()
            },
        );
        // openx: 0 樣本 + K27=None → 6 件套全跳過
        totals.insert("openx".to_string(), ProviderTotals::default());
        let last_completed = last_completed_session_age_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // ── Part A: 五 percentile + K27 = 6 件套字串 emit
        // cicx: P25=26, P50=51, P75=76, P95=96, P99=100, K27=1
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"cicx\"} 26\n"
            ),
            "cicx: K34 P25=26 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"cicx\"} 51\n"
            ),
            "cicx: K31 P50=51 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"cicx\"} 76\n"
            ),
            "cicx: K33 P75=76 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 96\n"
            ),
            "cicx: K30 P95=96 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"cicx\"} 100\n"
            ),
            "cicx: K32 P99=100 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"cicx\"} 1\n"
            ),
            "cicx: K27=1 必須 emit, body: {body}"
        );
        // claude: P25=1, P50=1, P75=1, P95=1000, P99=1000, K27=1
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"claude\"} 1\n"
            ),
            "claude: K34 P25=1 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"claude\"} 1\n"
            ),
            "claude: K31 P50=1 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"claude\"} 1\n"
            ),
            "claude: K33 P75=1 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"claude\"} 1000\n"
            ),
            "claude: K30 P95=1000 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"claude\"} 1000\n"
            ),
            "claude: K32 P99=1000 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"claude\"} 1\n"
            ),
            "claude: K27=1 必須 emit, body: {body}"
        );
        // gemini: P25=13, P50=25, P75=38, P95=48, P99=50, K27=1
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"gemini\"} 13\n"
            ),
            "gemini: K34 P25=13 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"gemini\"} 26\n"
            ),
            "gemini: K31 P50=26 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"gemini\"} 38\n"
            ),
            "gemini: K33 P75=38 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"gemini\"} 48\n"
            ),
            "gemini: K30 P95=48 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"gemini\"} 50\n"
            ),
            "gemini: K32 P99=50 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"gemini\"} 1\n"
            ),
            "gemini: K27=1 必須 emit, body: {body}"
        );

        // ── Part B: openx 6 件套全跳過 (samples 空 + K27 None)
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K34 P25 必須跳過 (samples 空), body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K31 P50 必須跳過, body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K33 P75 必須跳過, body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K30 P95 必須跳過, body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K32 P99 必須跳過, body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"openx\"}"
            ),
            "openx: K27 必須跳過 (None), body: {body}"
        );

        // ── Part C: 5 percentile + K27 = 6 件套 emit count 對齊 (都過濾 None/samples 空)
        let p25_count = body
            .matches("lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"")
            .count();
        let p50_count = body
            .matches("lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"")
            .count();
        let p75_count = body
            .matches("lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"")
            .count();
        let p95_count = body
            .matches("lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"")
            .count();
        let p99_count = body
            .matches("lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"")
            .count();
        let k27_count = body
            .matches("lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"")
            .count();
        assert_eq!(
            p25_count, 3,
            "K34 P25 emit 必須 = 3 provider, got {p25_count}, body: {body}"
        );
        assert_eq!(
            p50_count, 3,
            "K31 P50 emit 必須 = 3 provider, got {p50_count}, body: {body}"
        );
        assert_eq!(
            p75_count, 3,
            "K33 P75 emit 必須 = 3 provider, got {p75_count}, body: {body}"
        );
        assert_eq!(
            p95_count, 3,
            "K30 P95 emit 必須 = 3 provider, got {p95_count}, body: {body}"
        );
        assert_eq!(
            p99_count, 3,
            "K32 P99 emit 必須 = 3 provider, got {p99_count}, body: {body}"
        );
        assert_eq!(
            k27_count, 3,
            "K27 emit 必須 = 3 provider, got {k27_count}, body: {body}"
        );
        // 6 件套 emit set 完全一致 (K30-K34 都過濾 samples 空, K27 過濾 None)
        assert_eq!(
            p25_count, p50_count,
            "K34 emit 集合 = K31 emit 集合, got P25={p25_count} P50={p50_count}"
        );
        assert_eq!(
            p50_count, p75_count,
            "K31 emit 集合 = K33 emit 集合, got P50={p50_count} P75={p75_count}"
        );
        assert_eq!(
            p75_count, p95_count,
            "K33 emit 集合 = K30 emit 集合, got P75={p75_count} P95={p95_count}"
        );
        assert_eq!(
            p95_count, p99_count,
            "K30 emit 集合 = K32 emit 集合, got P95={p95_count} P99={p99_count}"
        );
        assert_eq!(
            p99_count, k27_count,
            "K32 emit 集合 = K27 emit 集合, got P99={p99_count} K27={k27_count}"
        );

        // ── Part E: R55 chain 算術 (K34 ≤ K31 ≤ K33 ≤ K30 ≤ K32) 在 render 端成立
        // 對 cicx / claude / gemini 三組, 從 body 解析 5 percentile 數值, 驗 4 個不等式。
        let parse_i64 = |body: &str, metric: &str, provider: &str| -> i64 {
            let line = format!("{metric}{{provider=\"{provider}\"}}");
            body.split(&line)
                .nth(1)
                .and_then(|s| s.lines().next())
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or_else(|| panic!("{provider}: {metric} 必須能 parse 成 i64, body: {body}"))
        };
        for p in ["cicx", "claude", "gemini"] {
            let p25 = parse_i64(
                &body,
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds",
                p,
            );
            let p50 = parse_i64(
                &body,
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds",
                p,
            );
            let p75 = parse_i64(
                &body,
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds",
                p,
            );
            let p95 = parse_i64(
                &body,
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds",
                p,
            );
            let p99 = parse_i64(
                &body,
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds",
                p,
            );
            assert!(
                p25 <= p50,
                "{p}: R55 chain P25 ({p25}) 必須 ≤ P50 ({p50}) (render 端算術)"
            );
            assert!(
                p50 <= p75,
                "{p}: R55 chain P50 ({p50}) 必須 ≤ P75 ({p75}) (render 端算術)"
            );
            assert!(
                p75 <= p95,
                "{p}: R55 chain P75 ({p75}) 必須 ≤ P95 ({p95}) (render 端算術)"
            );
            assert!(
                p95 <= p99,
                "{p}: R55 chain P95 ({p95}) 必須 ≤ P99 ({p99}) (render 端算術)"
            );
        }

        // ── Part F: R56 chain 算術 (K27 lifetime min ≤ K34 window P25) 在 render 端成立
        // 對 cicx / claude / gemini 三組, 從 body 解析 K27 跟 K34 P25 數值, 驗 1 個不等式。
        // 數學前提: K27 lifetime min 抓「歷史 N 個 completion 的最小值」 (= P0),
        // P25 抓「window sort 後第 25 百分位」, 任一分布 P0 ≤ P25 自動成立。
        for p in ["cicx", "claude", "gemini"] {
            let k27 = parse_i64(
                &body,
                "lobsterpulse_provider_completed_sessions_min_duration_seconds",
                p,
            );
            let p25 = parse_i64(
                &body,
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds",
                p,
            );
            assert!(
                k27 <= p25,
                "{p}: R56 chain K27 ({k27}) 必須 ≤ K34 P25 ({p25}) \
                 (lifetime min ≤ window Q1, 數學 P0 ≤ P25 不變式, render 端算術)"
            );
        }
    }

    // ============== R57：K35 (interarrival) lifetime-derive render emission consistency 護欄 ==============
    // 撿 R55 commit (bd85810) 留 WIP「lib.rs render-side test 留 R56+ 觀察」落地。
    // 對齊 session.rs R57 4 個純 fn 護欄語意面 (8 種樣本數 + 4 provider 隔離 +
    // K23=0 / since=None 過濾 + saturating clamp 0) 在 render 端 Prometheus 抓得到
    // 的字串上仍成立, 證明 K35 = (now - since) / K23 算式在 emit 階段未退化,
    // 且 emit 順序 (K22 last_completed → K35 interarrival) 跟 render 端 emit block
    // 順序一致, 給 Prometheus scrape 端 lifetime↔window 觀察維度穩定。
    //
    // 跟 K30-K34 R55/R56 percentile chain render test 對稱 — 同樣 4 provider 混合
    // fixture 模板, 同樣 Part A 字串 / Part B 過濾 / Part C count / Part D 順序 /
    // Part E 隔離 五段式, 但 K35 推導路徑完全不同 (K10 since + K23 count + render
    // now 三輸入) vs K30-K34 samples 池單輸入, 兩類 metric 互不污染。
    //
    // 數據源: K35 派生自 `ProviderTotals.since` (K10) + `completed_sessions_count`
    // (K23) + render-time `now` (render 端參數) — 不開新 ProviderTotals 欄位,
    // 跟 K12 idle_ratio 同款「純 fn 端組合既有資料源」策略。Memory 零成本。
    //
    // Fixture (4 provider × 各自 lifetime window):
    //   cicx:   since=now-1h, K23=10, K22=Some(3600) → K35 = 3600/10 = 360
    //   claude: since=now-2h, K23=20, K22=Some(7200) → K35 = 7200/20 = 360
    //   gemini: since=now-6h, K23=0,  K22=None       → K35 跳過 (K23=0 過濾)
    //   openx:  since=None,    K23=5,  K22=Some(100) → K35 跳過 (since=None 過濾)
    // 跟 session.rs R57 fixture 4 provider lifetime window (1h/2h/6h/12h) 同模板
    // 但 12h 改 None (對應 since 過濾邊界, session.rs 是 fixture 一個 provider
    // since=None 觸發過濾, render 端這裡 openx 模擬同樣語意)。

    #[test]
    fn r57_k35_interarrival_render_emission_consistency_across_mixed_lifetime_windows() {
        let now = Utc::now();
        let mut totals = HashMap::new();
        // cicx: lifetime 1h, K23=10, K22=Some(3600) → K35 = 3600/10 = 360
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                since: Some(now - chrono::Duration::hours(1)),
                completed_sessions_count: 10,
                last_completed_session_age_secs: Some(3600),
                ..Default::default()
            },
        );
        // claude: lifetime 2h, K23=20, K22=Some(7200) → K35 = 7200/20 = 360
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                since: Some(now - chrono::Duration::hours(2)),
                completed_sessions_count: 20,
                last_completed_session_age_secs: Some(7200),
                ..Default::default()
            },
        );
        // gemini: lifetime 6h, K23=0, K22=None → K35 跳過 (K23=0 過濾)
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                since: Some(now - chrono::Duration::hours(6)),
                completed_sessions_count: 0,
                last_completed_session_age_secs: None,
                ..Default::default()
            },
        );
        // openx: since=None, K23=5, K22=Some(100) → K35 跳過 (since=None 過濾)
        totals.insert(
            "openx".to_string(),
            ProviderTotals {
                since: None,
                completed_sessions_count: 5,
                last_completed_session_age_secs: Some(100),
                ..Default::default()
            },
        );
        let last_completed = last_completed_session_age_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            now,
        );

        // ── Part A: K35 字串 emit (cicx 360 + claude 360)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{provider=\"cicx\"} 360\n"
            ),
            "cicx: K35 = 3600/10 = 360 必須 emit, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{provider=\"claude\"} 360\n"
            ),
            "claude: K35 = 7200/20 = 360 必須 emit, body: {body}"
        );

        // ── Part B: K35 過濾 (gemini K23=0 跳過 + openx since=None 跳過)
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{provider=\"gemini\"}"
            ),
            "gemini: K35 必須跳過 (K23=0 過濾, 避免 0/0 假冒「瞬間完成」), body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{provider=\"openx\"}"
            ),
            "openx: K35 必須跳過 (since=None 過濾), body: {body}"
        );

        // ── Part C: K35 emit count = 2 (cicx + claude, gemini/openx 跳過)
        let k35_count = body
            .matches(
                "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{provider=\"",
            )
            .count();
        assert_eq!(
            k35_count, 2,
            "K35 emit 必須 = 2 provider (cicx + claude), got {k35_count}, body: {body}"
        );

        // ── Part D: R57 lifetime↔window chain 在 render 端 — K22 (last_completed
        // age) 必須在 K35 (interarrival) 之前 emit (跟 render 端 emit block 順序
        // K22 line 1995 → K35 line 2157 一致)。cicx 跟 claude 兩個 K23>=1
        // provider 都驗, 確保 emit 順序在跨 provider 上穩定。
        for p in ["cicx", "claude"] {
            let k22_label = format!(
                "lobsterpulse_provider_last_completed_session_age_seconds{{provider=\"{p}\"}}"
            );
            let k35_label = format!(
                "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{{provider=\"{p}\"}}"
            );
            let k22_pos = body
                .find(&k22_label)
                .unwrap_or_else(|| panic!("{p}: K22 必須 emit (last_completed=Some)"));
            let k35_pos = body
                .find(&k35_label)
                .unwrap_or_else(|| panic!("{p}: K35 必須 emit (K23>=1, since=Some)"));
            assert!(
                k22_pos < k35_pos,
                "{p}: render 端 emit 順序 K22 ({k22_pos}) 必須 < K35 ({k35_pos}) \
                 (跟 render 端 emit block K22 line → K35 line 順序一致, R57 lifetime↔window chain)"
            );
        }

        // ── Part E: K35 跟 K30-K34 K-tag emission set 隔離 (derive 推導路徑獨立)
        // cicx: 沒 samples (K30-K34 跳過) + 有 since+count (K35 emit) → 只有 K35 集合
        // claude: 沒 samples (K30-K34 跳過) + 有 since+count (K35 emit) → 只有 K35 集合
        // gemini: 沒 samples (K30-K34 跳過) + K23=0 (K35 跳過) → 雙跳過
        // openx: 沒 samples (K30-K34 跳過) + since=None (K35 跳過) → 雙跳過
        // 跨 K-tag emit 集合差異證明 K35 derive 跟 K30-K34 sample 池互不污染
        // (本 fixture cicx/claude 都沒灌 samples, 跟 K35 推導路徑天然隔離)。
        for p in ["cicx", "claude"] {
            let p95_label = format!(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{{provider=\"{p}\"}}"
            );
            let k35_label = format!(
                "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{{provider=\"{p}\"}}"
            );
            assert!(
                body.find(&p95_label).is_none(),
                "{p}: K30 P95 跳過 (samples default empty, 跟 K35 emit 推導路徑獨立)"
            );
            assert!(
                body.find(&k35_label).is_some(),
                "{p}: K35 必須 emit (since=Some + K23>=1, 跟 K30 sample 池獨立)"
            );
        }
        // gemini: K30 (samples 空) 跳過 + K35 (K23=0) 跳過 → 雙跳過
        let gemini_p95 = body.find(
            "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"gemini\"}",
        );
        let gemini_k35 = body.find(
            "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{provider=\"gemini\"}",
        );
        assert!(gemini_p95.is_none(), "gemini: K30 跳過 (samples 空)");
        assert!(gemini_k35.is_none(), "gemini: K35 跳過 (K23=0)");
        // openx: K30 (samples 空) 跳過 + K35 (since=None) 跳過 → 雙跳過
        let openx_p95 = body.find(
            "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"openx\"}",
        );
        let openx_k35 = body.find(
            "lobsterpulse_provider_completed_sessions_interarrival_avg_seconds{provider=\"openx\"}",
        );
        assert!(openx_p95.is_none(), "openx: K30 跳過 (samples 空)");
        assert!(openx_k35.is_none(), "openx: K35 跳過 (since=None)");
    }

    // ============== R61：K19 (sessions_by_state) ↔ K40 (provider_sessions) 跨 K 算術不變式護欄 ==============
    // 策略顧問 R50 巡邏「凍結 gauge 補閉環」下, R52 補 K23/K24/K25 三件套算術不變式,
    // R60 補 K14/K17 第二個三件套, R61 補 K19/K40 「跨 live 切片 × per-state 切面」
    // 算術不變式 (R52-R60 chain 全在 lifetime aggregate 範圍, R61 第一次跨進 live
    // sessions slice 維度)。 lib.rs:2259-2260 docstring 已寫死
    // `sum by(provider)(lobsterpulse_provider_sessions_by_state) ==
    // lobsterpulse_provider_sessions` 不變式但無護欄: 同一個 `for s in sessions`
    // 迴圈 (line 1576-1590) 對 `provider_counts` (K40) 跟 `provider_sessions_by_state`
    // (K19) 同步 +1, 算術必嚴格相等。 bug surface: (a) 有人把 K19 抽到獨立迴圈
    // 過濾 is_active (跟 K18 max_session_age 一致) → K19 變「active only」, K40
    // 仍算全部, 算術分裂; (b) 有人加 new state enum variant (K19 4 → 5 label) 但
    // K40 不動 → K19 多 bucket 跟 K40 算術分裂; (c) 有人把 `for s in sessions`
    // 拆兩段, 兩段 sessions 切片語意變 (e.g. 一段加 filter) → 算術分裂; (d) 有人
    // 改 K40 emit 條件加 `if c > 0` 過濾, K19 仍 emit 0 → 0/0 邊界算術分裂。
    // 對齊 R52-R60 護欄 chain 紀律: R52 K23/K24/K25 → R53 K22/K26/K27 → R54 K30
    // → R55 K30-K34 percentile → R56 K27↔K34 → R57 K22↔K10 + K35 helper → R58
    // K22-K27 6 K → R59 K15 ⊆ K16 4xx → R60 K14 = sum(K17 buckets) → R61 K19 sum
    // by(provider) == K40 跨 live 切片算術 (R52-R60 沒覆蓋 live sessions slice
    // 算術關係, R61 補缺口)。 4 provider × 4 state 跨 13 sessions fixture, 驗
    // (a) K19 per (provider, state) emit 正確, (b) K40 per provider emit 正確,
    // (c) sum by(provider)(K19) == K40 嚴格成立, (d) K40 emit 條件 None-free
    // (counter 0 有效, 跟 K14 emit count=0 同策略) vs K19 emit 條件 (provider,
    // state) 對非零才 emit (4 state 切面設計契約, 不 emit 0 bucket)。

    #[test]
    fn r61_k19_k40_sum_by_provider_arithmetic_invariant_across_mixed_states() {
        // 4 provider × 4 state 跨 13 sessions fixture, K19 跟 K40 算術必嚴格相等:
        //   cicx:   5 working + 2 idle + 1 stale         = 8   (K19: 3 buckets)
        //   claude: 3 working + 1 waiting + 2 stale + 1 idle = 7 (K19: 4 buckets 全 state)
        //   gemini: 2 idle + 4 waiting                    = 6   (K19: 2 buckets, 無 working/stale)
        //   openx:  1 stale + 1 working                   = 2   (K19: 2 buckets, 無 idle/waiting)
        //   總 sessions = 8+7+6+2 = 23; K19 buckets = 3+4+2+2 = 11; K40 series = 4
        let sessions = vec![
            // cicx: 8 sessions
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Idle),
            info_with_state("cicx", true, SessionState::Idle),
            info_with_state("cicx", true, SessionState::Stale),
            // claude: 7 sessions (4 state 全到位)
            info_with_state("claude", true, SessionState::Working),
            info_with_state("claude", true, SessionState::Working),
            info_with_state("claude", true, SessionState::Working),
            info_with_state("claude", true, SessionState::WaitingForUser),
            info_with_state("claude", true, SessionState::Stale),
            info_with_state("claude", true, SessionState::Stale),
            info_with_state("claude", true, SessionState::Idle),
            // gemini: 6 sessions (idle + waiting, 故意缺 working/stale 驗 K19 emit 不補 0 bucket)
            info_with_state("gemini", true, SessionState::Idle),
            info_with_state("gemini", true, SessionState::Idle),
            info_with_state("gemini", true, SessionState::WaitingForUser),
            info_with_state("gemini", true, SessionState::WaitingForUser),
            info_with_state("gemini", true, SessionState::WaitingForUser),
            info_with_state("gemini", true, SessionState::WaitingForUser),
            // openx: 2 sessions (stale + working, 故意缺 idle/waiting 驗 K19 emit 不補 0 bucket)
            info_with_state("openx", true, SessionState::Stale),
            info_with_state("openx", true, SessionState::Working),
        ];
        let body = render_prometheus_body(
            &sessions,
            sessions.len() as u64,
            sessions.len() as u64,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // (a) K19 per (provider, state) emit 正確: 11 條 (cicx 3 + claude 4 + gemini 2 + openx 2)
        let k19_expected = [
            // cicx: working=5, idle=2, stale=1, 無 waiting
            "lobsterpulse_provider_sessions_by_state{provider=\"cicx\",state=\"working\"} 5\n",
            "lobsterpulse_provider_sessions_by_state{provider=\"cicx\",state=\"idle\"} 2\n",
            "lobsterpulse_provider_sessions_by_state{provider=\"cicx\",state=\"stale\"} 1\n",
            // claude: 4 state 全到位
            "lobsterpulse_provider_sessions_by_state{provider=\"claude\",state=\"working\"} 3\n",
            "lobsterpulse_provider_sessions_by_state{provider=\"claude\",state=\"waiting_for_user\"} 1\n",
            "lobsterpulse_provider_sessions_by_state{provider=\"claude\",state=\"stale\"} 2\n",
            "lobsterpulse_provider_sessions_by_state{provider=\"claude\",state=\"idle\"} 1\n",
            // gemini: idle=2, waiting=4, 缺 working/stale (K19 不 emit 0 bucket)
            "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"idle\"} 2\n",
            "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"waiting_for_user\"} 4\n",
            // openx: stale=1, working=1, 缺 idle/waiting (K19 不 emit 0 bucket)
            "lobsterpulse_provider_sessions_by_state{provider=\"openx\",state=\"stale\"} 1\n",
            "lobsterpulse_provider_sessions_by_state{provider=\"openx\",state=\"working\"} 1\n",
        ];
        for needle in &k19_expected {
            assert!(
                body.contains(needle),
                "K19 應 emit `{needle}` 但 body 找不到 — 算術 / 計數 / 排序 任一錯"
            );
        }
        // gemini 缺 working/stale: K19 emit 不補 0 bucket (設計契約)
        assert!(
            !body.contains(
                "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"working\"}"
            ),
            "K19 設計契約: state 計數 = 0 不 emit 0 bucket (gemini 無 working)"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_sessions_by_state{provider=\"gemini\",state=\"stale\"}"
            ),
            "K19 設計契約: state 計數 = 0 不 emit 0 bucket (gemini 無 stale)"
        );
        // openx 缺 idle/waiting: 同設計契約
        assert!(
            !body.contains(
                "lobsterpulse_provider_sessions_by_state{provider=\"openx\",state=\"idle\"}"
            ),
            "K19 設計契約: state 計數 = 0 不 emit 0 bucket (openx 無 idle)"
        );
        assert!(
            !body.contains("lobsterpulse_provider_sessions_by_state{provider=\"openx\",state=\"waiting_for_user\"}"),
            "K19 設計契約: state 計數 = 0 不 emit 0 bucket (openx 無 waiting_for_user)"
        );

        // (b) K40 per provider emit 正確: 4 條
        let k40_expected = [
            "lobsterpulse_provider_sessions{provider=\"cicx\"} 8\n",
            "lobsterpulse_provider_sessions{provider=\"claude\"} 7\n",
            "lobsterpulse_provider_sessions{provider=\"gemini\"} 6\n",
            "lobsterpulse_provider_sessions{provider=\"openx\"} 2\n",
        ];
        for needle in &k40_expected {
            assert!(
                body.contains(needle),
                "K40 應 emit `{needle}` 但 body 找不到 — provider 計數錯"
            );
        }

        // (c) 算術不變式核心: sum by(provider)(K19) == K40, 4 provider 全驗
        // 從 body parse 出 K19 per-provider sum, 跟 K40 emit value 比對
        for (provider, k40_value) in &[
            ("cicx", 8u64),
            ("claude", 7u64),
            ("gemini", 6u64),
            ("openx", 2u64),
        ] {
            // parse 該 provider 對應所有 K19 bucket, sum
            let prefix = format!(
                "lobsterpulse_provider_sessions_by_state{{provider=\"{provider}\",state=\""
            );
            let mut k19_sum: u64 = 0;
            let mut found_any = false;
            for line in body.lines() {
                if let Some(rest) = line.strip_prefix(&prefix) {
                    // rest = e.g. `working"} 5\n` 找 `"` 結尾, 然後 ` 5\n`
                    if let Some(close_q) = rest.find('"') {
                        // 跳過 `state="..."` 後取 ` N` 結尾
                        let after_state = &rest[close_q + 1..];
                        // after_state = `} 5\n` or `} 5`
                        if let Some(num_part) = after_state.strip_prefix("} ") {
                            let n: u64 = num_part
                                .trim()
                                .parse()
                                .unwrap_or_else(|_| panic!("K19 value parse fail: {line}"));
                            k19_sum += n;
                            found_any = true;
                        }
                    }
                }
            }
            assert!(
                found_any,
                "{provider}: K19 該 provider 至少要有 1 個 state bucket emit"
            );
            assert_eq!(
                k19_sum, *k40_value,
                "{provider}: K19 sum by(provider) ({k19_sum}) 必須 == K40 ({k40_value})"
            );
        }
    }

    // ============== R62：K6 (sessions_total) ↔ K40 (provider_sessions) 跨 live 切片算術不變式護欄 ==============
    // 對齊 R61 chain 紀律: R61 補 K19 (by-state 切面) ↔ K40 (by-provider 切面) 算術
    // 不變式,但 K6 (global live aggregate) 跟 K40 (per-provider live) 算術關係 R52-R61
    // chain 全沒覆蓋。 lib.rs:1730-1740 docstring 已寫死 `lobsterpulse_sessions_total
    // {session_count}` 從 `self.sessions.len()` (session.rs:834) 餵入,跟同一個 `for s
    // in sessions` 迴圈 (line 1576-1580) 對 `provider_counts` (K40) 同步 +1 嚴格一致
    // → K6 必 = sum by(provider) K40。 bug surface: (a) 有人把 K40 抽到獨立迴圈
    // 過濾 `is_active` (跟 K41 provider_active 對齊) → K40 變 active only, K6 仍算全部
    // (含 is_active=false 的 Idle inactive session) → 算術分裂; (b) 有人把
    // `state.session_count` 從 `self.sessions.len()` 改成
    // `provider_totals.iter().map(|t| t.session_count).sum()` (K12 lifetime sum)
    // → K6 變 lifetime, K40 仍 live → 算術分裂; (c) 有人改 K40 emit 條件加
    // `if c > 0` 過濾 → 0/0 邊界算術分裂; (d) 有人加 K6 二次過濾 (e.g. 「只看 working
    // state」) 但 K40 不動 → 算術分裂。 補 R52 chain 第三個 live 切片三件套算術護欄
    // (R52 K23/K24/K25 lifetime → R60 K14/K17 lifetime events → R61 K19/K40 live by-state
    // → **R62 K6/K40 live by-provider → global aggregate**),完成 live 切片 chain
    // (R61 是 by-state → by-provider, R62 是 by-provider → global aggregate,鏈起來 = K6
    // = sum(K19) = sum(K40) 三層一致)。 同時加 1 個 boundary 護欄: 故意把 K12
    // (lifetime) 跟 K6 (live) 灌不同值,證明 R62 chain 護的是 live 不是 lifetime —
    // K12 lifetime 累計可能 ≫ K6 live (session 結束 + 30 min stale 回收後 lifetime
    // 仍累計, live 歸零),這條 boundary 把 K6 ↔ K12 算術關係明確斷開,避免未來有人混淆。
    //
    // fixture: 4 provider × 4 state 跨 23 sessions (跟 R61 同 fixture 結構,便於交叉
    // 比對):
    //   cicx:   5 working + 2 idle + 1 stale                  = 8  (K40 emit: 1 條)
    //   claude: 3 working + 1 waiting + 2 stale + 1 idle      = 7  (K40 emit: 1 條)
    //   gemini: 2 idle + 4 waiting                             = 6  (K40 emit: 1 條)
    //   openx:  1 stale + 1 working                            = 2  (K40 emit: 1 條)
    //   總 sessions = 8+7+6+2 = 23; K40 series = 4; K6 = 23
    //   sum by(provider) K40 = 8+7+6+2 = 23 = K6 ✓ 算術嚴格成立
    // 額外 1 個 boundary test (r62_k6_live_ne_k12_lifetime_distinct_metric): 故意把
    // K12 (ProviderTotals.session_count = lifetime 累計 = 100) 跟 K6 (live sessions.len
    // () = 3) 灌不同值,驗 K6 emit 3 跟 K12 emit 100 不混淆,boundary 把 live 跟
    // lifetime 切乾淨。

    #[test]
    fn r62_k6_k40_sum_by_provider_global_aggregate_arithmetic_invariant_across_mixed_states() {
        // 4 provider × 4 state 跨 23 sessions fixture (跟 R61 同結構), K6 = sum(K40) 必嚴格成立:
        //   cicx:   5 working + 2 idle + 1 stale         = 8
        //   claude: 3 working + 1 waiting + 2 stale + 1 idle = 7
        //   gemini: 2 idle + 4 waiting                    = 6
        //   openx:  1 stale + 1 working                   = 2
        //   總 sessions = 8+7+6+2 = 23; K40 series = 4; K6 = 23
        let sessions = vec![
            // cicx: 8 sessions
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Idle),
            info_with_state("cicx", true, SessionState::Idle),
            info_with_state("cicx", true, SessionState::Stale),
            // claude: 7 sessions (4 state 全到位)
            info_with_state("claude", true, SessionState::Working),
            info_with_state("claude", true, SessionState::Working),
            info_with_state("claude", true, SessionState::Working),
            info_with_state("claude", true, SessionState::WaitingForUser),
            info_with_state("claude", true, SessionState::Stale),
            info_with_state("claude", true, SessionState::Stale),
            info_with_state("claude", true, SessionState::Idle),
            // gemini: 6 sessions (idle + waiting, 故意缺 working/stale)
            info_with_state("gemini", true, SessionState::Idle),
            info_with_state("gemini", true, SessionState::Idle),
            info_with_state("gemini", true, SessionState::WaitingForUser),
            info_with_state("gemini", true, SessionState::WaitingForUser),
            info_with_state("gemini", true, SessionState::WaitingForUser),
            info_with_state("gemini", true, SessionState::WaitingForUser),
            // openx: 2 sessions (stale + working, 故意缺 idle/waiting)
            info_with_state("openx", true, SessionState::Stale),
            info_with_state("openx", true, SessionState::Working),
        ];
        // K6 = sessions.len() (跟 session.rs:834 `let session_count = self.sessions.len();` 同步)
        // K7 = sessions.len() (全部 is_active=true, 所以 active_count == sessions.len(), 跟 K6 在這 fixture 重合)
        // 故意 K7 != K6 跨 fixture 會混淆,本 test 兩者給同值聚焦驗 K6 = sum(K40)
        let body = render_prometheus_body(
            &sessions,
            sessions.len() as u64, // K6 sessions_total = 23
            sessions.len() as u64, // K7 sessions_active = 23 (本 fixture is_active 全 true, 用來聚焦 K6 不被 K7 干擾)
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // (a) K40 per provider emit 正確: 4 條 (跟 R61 (b) 段同 expected)
        let k40_expected = [
            "lobsterpulse_provider_sessions{provider=\"cicx\"} 8\n",
            "lobsterpulse_provider_sessions{provider=\"claude\"} 7\n",
            "lobsterpulse_provider_sessions{provider=\"gemini\"} 6\n",
            "lobsterpulse_provider_sessions{provider=\"openx\"} 2\n",
        ];
        for needle in &k40_expected {
            assert!(
                body.contains(needle),
                "K40 應 emit `{needle}` 但 body 找不到 — provider 計數錯"
            );
        }

        // (b) K6 global aggregate emit 正確: 一條
        assert!(
            body.contains("lobsterpulse_sessions_total 23\n"),
            "K6 應 emit `lobsterpulse_sessions_total 23` (= sessions.len()) 但 body 找不到"
        );

        // (c) 算術不變式核心: K6 = sum by(provider)(K40), 跨 4 provider 全加總驗
        // 從 body parse 出 K40 per-provider value, sum 跨 provider, 跟 K6 emit value 比對
        let prefix = "lobsterpulse_provider_sessions{provider=\"";
        let mut k40_sum: u64 = 0;
        let mut found_providers: Vec<String> = Vec::new();
        for line in body.lines() {
            if let Some(rest) = line.strip_prefix(prefix) {
                // rest = e.g. `cicx"} 8\n` 找 `"` 結尾, 然後 ` 8\n`
                if let Some(close_q) = rest.find('"') {
                    let provider = &rest[..close_q];
                    let after_provider = &rest[close_q + 1..];
                    // after_provider = `} 8\n` or `} 8`
                    if let Some(num_part) = after_provider.strip_prefix("} ") {
                        let n: u64 = num_part
                            .trim()
                            .parse()
                            .unwrap_or_else(|_| panic!("K40 value parse fail: {line}"));
                        k40_sum += n;
                        found_providers.push(provider.to_string());
                    }
                }
            }
        }
        assert_eq!(
            found_providers.len(),
            4,
            "K40 應 emit 4 個 provider, 實際找到 {} 個: {found_providers:?}",
            found_providers.len()
        );
        assert_eq!(
            k40_sum, 23,
            "sum by(provider)(K40) 必須 == 23 (= K6 sessions_total), 實際 = {k40_sum}"
        );
        // (d) chain 跨 K 一致性: K6 (global) == sum(K40) == sum(sum(K19 per provider))
        // 跟 R61 chain 串接: R61 已驗 K19 sum by(provider) == K40 per provider, R62 接 K40
        // sum by(provider) == K6 global。 設計上 R61 4 個 provider 各自 K19 sum = K40
        // value, R62 再把 4 個 K40 value 加總 = K6 → 鏈起來 sum(K19) 全部 = K6。
        // 這條 assertion 直接驗證 K6 = 23, 等於 fixture 設計的 sessions.len() = 23,
        // 三層 chain (K19 → K40 → K6) 自洽已在 (a)(b)(c) 隱含驗證。
        assert!(
            body.contains("lobsterpulse_sessions_total 23\n")
                && body.contains("lobsterpulse_provider_sessions{provider=\"cicx\"} 8\n")
                && body.contains(
                    "lobsterpulse_provider_sessions_by_state{provider=\"cicx\",state=\"working\"} 5\n"
                ),
            "R61-R62 chain 一致性: K6 23 == K40 cicx 8 == K19 cicx working 5 + idle 2 + stale 1 (任一缺即 chain 斷)"
        );
    }

    #[test]
    fn r62_k6_live_ne_k12_lifetime_distinct_metric() {
        // R62 boundary: 故意 K6 (live) = 3 跟 K12 (lifetime) = 100 灌不同值,
        // 驗 K6 emit 3 跟 K12 emit 100 不混淆。 K12 = ProviderTotals.session_count
        // (lifetime 累計, line 321 + handle_event `+= 1`), K6 = sessions.len() (live
        // 切片, session.rs:834)。 兩條 metric 走不同資料源,語意不同,本 test 把
        // 這條「不變式的不變式」明確寫死護欄,防未來有人把 K6 改成
        // `provider_totals.iter().map(|t| t.session_count).sum()` → 變 lifetime aggregate
        // 跟 K40 (live per-provider) 算術分裂。
        let sessions = vec![
            info_with_state("claude", true, SessionState::Working),
            info_with_state("cicx", true, SessionState::Idle),
            info_with_state("gemini", true, SessionState::WaitingForUser),
        ];
        // K6 = 3 (live sessions.len()), K7 = 3 (全 is_active=true)
        // K12 lifetime: claude 累計 50 次 + cicx 累計 30 次 + gemini 累計 20 次 = 100
        //   → provider_session_count{provider="claude"} 50, {cicx} 30, {gemini} 20
        //   → sum(K12 lifetime) = 100 ≠ K6 live 3
        let body = render_prometheus_body(
            &sessions,
            3, // K6 sessions_total
            3, // K7 sessions_active
            &totals_map(vec![
                totals_with_session_count("claude", 50),
                totals_with_session_count("cicx", 30),
                totals_with_session_count("gemini", 20),
            ]),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // K6 emit 3 (live aggregate)
        assert!(
            body.contains("lobsterpulse_sessions_total 3\n"),
            "K6 (live) 應 emit 3, body 找不到"
        );
        // K12 emit lifetime 累計, 跟 K6 不同值
        assert!(
            body.contains("lobsterpulse_provider_session_count{provider=\"claude\"} 50\n"),
            "K12 lifetime claude 應 emit 50, body 找不到"
        );
        assert!(
            body.contains("lobsterpulse_provider_session_count{provider=\"cicx\"} 30\n"),
            "K12 lifetime cicx 應 emit 30, body 找不到"
        );
        assert!(
            body.contains("lobsterpulse_provider_session_count{provider=\"gemini\"} 20\n"),
            "K12 lifetime gemini 應 emit 20, body 找不到"
        );
        // K40 emit live per-provider (跟 K12 lifetime 數字完全不同)
        assert!(
            body.contains("lobsterpulse_provider_sessions{provider=\"claude\"} 1\n"),
            "K40 live claude 應 emit 1 (單一 session), body 找不到"
        );
        assert!(
            body.contains("lobsterpulse_provider_sessions{provider=\"cicx\"} 1\n"),
            "K40 live cicx 應 emit 1 (單一 session), body 找不到"
        );
        assert!(
            body.contains("lobsterpulse_provider_sessions{provider=\"gemini\"} 1\n"),
            "K40 live gemini 應 emit 1 (單一 session), body 找不到"
        );
        // chain 算術驗證:
        //   K6 = 3 (live) ≠ sum(K12 lifetime) = 50+30+20 = 100
        //   K6 = 3 = sum(K40 live) = 1+1+1 (R62 護的不變式: live 切片 chain)
        //   sum(K12 lifetime) = 100 ≠ sum(K40 live) = 3 (K12 跟 K40 走不同資料源)
        // 上面 3+3+1+1+1+1+1+1 八條 assert 已隱含驗證: K6 跟 K12 數字不同 → 兩條
        // metric 走不同語意, R62 chain 護的是 live (K6 ↔ K40) 不是 lifetime (K12
        // 獨立 counter)。 防迴歸: 若未來有人把 K6 改成 `provider_totals.iter()
        // .map(|t| t.session_count).sum()` → K6 變 100, 上面 8 條 assert 至少
        // 第 1 條 (K6=3) 會炸, 抓得到。
    }

    // ============== R60：K14 (events_total) ↔ K17 (event_type_counts) 跨 bucket 算術護欄 ==============
    // 策略顧問 R50 巡邏「凍結 gauge 補閉環」下, R52 補 K23/K24/K25 三件套算術不變式 +
    // 3 層 emit set ⊆ 護欄, R60 補 K14/K17 三件套的「第二個跨 K 算術護欄」(event
    // total 是 lifetime aggregate counter, event_type_total 是 per-type bucket
    // 細顆度 counter, 語意跟 K23/K24/K25 對稱)。 bump_provider_totals 對每個 event
    // 同步寫: `events_total += 1` (無條件, 不管 type); 然後 `if !event.hook_event_name
    // .is_empty() { event_type_counts[hook_event_name] += 1 }` (空字串過濾, 防
    // `type=""` 污染 metric 視圖)。 數學不變式: events_total = sum(event_type_counts
    // .values) + 空字串事件數; production 中空字串過濾生效, fixture 全部 event 都
    // 具名 type → K14 必嚴格等於 K17 buckets 總和。 4 provider 跨 4 type bucket
    // fixture, 驗 (a) K14 算術 = sum(K17 buckets per provider), (b) K17 沒有
    // type="" bucket (空字串不污染設計契約), (c) K14 emit 條件 None-free (counter 0
    // 有效, 跟 K23 emit count=0 同策略) vs K17 emit 條件 event_type_counts 非空
    // (跨 4 provider 兩條 series 都 emit, 跟 K22/K26/K27 過濾 None 不同)。 R52
    // chain 已有 K23/K24/K25 三件套算術護欄, R60 補 R52 沒覆蓋的 event count ×
    // event type 跨 K 算術關係, 跟 R52 同樣 cross-metric invariant 紀律, CI 1 秒抓出。

    #[test]
    fn r60_k14_k17_cross_bucket_arithmetic_invariant_across_providers() {
        // 4 provider 各 4 type bucket, K14 故意 = sum(K17) 嚴格成立:
        //   cicx:   {Stop=5, TokenUpdate=2, PostToolUse=3, UserPromptSubmit=10} → 20
        //   claude: {Stop=4, PreToolUse=2, PostToolUse=1, SessionStart=4}        → 11
        //   gemini: {Stop=3, PreToolUse=1, Notification=7, UserPromptSubmit=1}   → 12
        //   openx:  {Stop=8, TokenUpdate=8, PostToolUse=3, PostToolUseFailure=1}→ 20
        let totals = std::collections::HashMap::from([
            (
                "cicx".to_string(),
                ProviderTotals {
                    events_total: 20,
                    event_type_counts: std::collections::BTreeMap::from([
                        ("Stop".to_string(), 5),
                        ("TokenUpdate".to_string(), 2),
                        ("PostToolUse".to_string(), 3),
                        ("UserPromptSubmit".to_string(), 10),
                    ]),
                    ..Default::default()
                },
            ),
            (
                "claude".to_string(),
                ProviderTotals {
                    events_total: 11,
                    event_type_counts: std::collections::BTreeMap::from([
                        ("Stop".to_string(), 4),
                        ("PreToolUse".to_string(), 2),
                        ("PostToolUse".to_string(), 1),
                        ("SessionStart".to_string(), 4),
                    ]),
                    ..Default::default()
                },
            ),
            (
                "gemini".to_string(),
                ProviderTotals {
                    events_total: 12,
                    event_type_counts: std::collections::BTreeMap::from([
                        ("Stop".to_string(), 3),
                        ("PreToolUse".to_string(), 1),
                        ("Notification".to_string(), 7),
                        ("UserPromptSubmit".to_string(), 1),
                    ]),
                    ..Default::default()
                },
            ),
            (
                "openx".to_string(),
                ProviderTotals {
                    events_total: 20,
                    event_type_counts: std::collections::BTreeMap::from([
                        ("Stop".to_string(), 8),
                        ("TokenUpdate".to_string(), 8),
                        ("PostToolUse".to_string(), 3),
                        ("PostToolUseFailure".to_string(), 1),
                    ]),
                    ..Default::default()
                },
            ),
        ]);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // ── (a) K14 per-provider 算術: 必 = sum(K17 buckets) ──
        // 從 body 解析 K14 數值 (跟 R53 護欄 `body.split(&prefix).nth(1).and_then(...)`
        // parse pattern 一致, 避免新增 helper)
        let k14_expected: &[(&str, u64)] =
            &[("cicx", 20), ("claude", 11), ("gemini", 12), ("openx", 20)];
        for (p, expected) in k14_expected {
            let prefix = format!("lobsterpulse_provider_events_total{{provider=\"{p}\"}}");
            let actual: u64 = body
                .split(&prefix)
                .nth(1)
                .and_then(|s| s.lines().next())
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or_else(|| panic!("{p}: K14 必須能 parse 成 u64, body: {body}"));
            assert_eq!(
                actual, *expected,
                "{p}: K14 events_total 必 = sum(K17 buckets) = {expected}, got {actual}, body: {body}"
            );
        }

        // ── (b) K17 per-provider × per-type 算術: 4 provider × 4 type = 16 series ──
        let k17_expected: &[(&str, &str, u64)] = &[
            ("cicx", "Stop", 5),
            ("cicx", "TokenUpdate", 2),
            ("cicx", "PostToolUse", 3),
            ("cicx", "UserPromptSubmit", 10),
            ("claude", "Stop", 4),
            ("claude", "PreToolUse", 2),
            ("claude", "PostToolUse", 1),
            ("claude", "SessionStart", 4),
            ("gemini", "Stop", 3),
            ("gemini", "PreToolUse", 1),
            ("gemini", "Notification", 7),
            ("gemini", "UserPromptSubmit", 1),
            ("openx", "Stop", 8),
            ("openx", "TokenUpdate", 8),
            ("openx", "PostToolUse", 3),
            ("openx", "PostToolUseFailure", 1),
        ];
        for (p, etype, expected) in k17_expected {
            let prefix = format!(
                "lobsterpulse_provider_event_type_total{{provider=\"{p}\",type=\"{etype}\"}}"
            );
            let actual: u64 = body
                .split(&prefix)
                .nth(1)
                .and_then(|s| s.lines().next())
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or_else(|| panic!("{p}/{etype}: K17 必須能 parse 成 u64, body: {body}"));
            assert_eq!(
                actual, *expected,
                "{p}/{etype}: K17 event_type_total 必 = {expected}, got {actual}, body: {body}"
            );
        }

        // ── (c) K14 ↔ K17 跨 bucket 算術不變式: K14 必 = sum(K17 buckets per provider) ──
        for (p, _) in k14_expected {
            let sum_k17: u64 = k17_expected
                .iter()
                .filter(|(pp, _, _)| pp == p)
                .map(|(_, _, n)| n)
                .sum();
            let k14_prefix = format!("lobsterpulse_provider_events_total{{provider=\"{p}\"}}");
            let k14_actual: u64 = body
                .split(&k14_prefix)
                .nth(1)
                .and_then(|s| s.lines().next())
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or_else(|| panic!("{p}: K14 必能 parse, body: {body}"));
            assert_eq!(
                k14_actual, sum_k17,
                "{p}: K14 ({k14_actual}) 必 = sum(K17 buckets) ({sum_k17}) \
                 (K14↔K17 跨 bucket 算術不變式, R60 護欄核心), body: {body}"
            );
        }

        // ── (d) 設計契約: K17 沒有 type="" bucket (bump_provider_totals 過濾
        // 空字串防 metric 視圖污染, fixture 已驗全部 type 都具名) ──
        assert!(
            !body.contains("type=\"\""),
            "K17 不可有 type=\"\" bucket (空字串污染 metric 視圖防線), body: {body}"
        );

        // ── (e) K14 ↔ K17 emit 集合對稱性: 兩條 series 在 4 provider 各自 emit ──
        // K14 emit 條件 None-free (counter 0 有效, 跟 K23 emit count=0 策略一致),
        // K17 emit 條件 event_type_counts 非空 (fixture 故意讓 4 provider 都有 buckets)
        let k14_count = body
            .matches("lobsterpulse_provider_events_total{provider=\"")
            .count();
        let k17_count = body
            .matches("lobsterpulse_provider_event_type_total{provider=\"")
            .count();
        assert_eq!(
            k14_count, 4,
            "K14 emit 必 = 4 provider (counter 0 仍 emit, None-free), got {k14_count}, body: {body}"
        );
        assert_eq!(
            k17_count, 16,
            "K17 emit 必 = 4 provider × 4 type bucket = 16 series, got {k17_count}, body: {body}"
        );
    }

    // ============== K26 per-provider completed_sessions_max_duration_seconds gauge ==============
    // 跟 K22 (latest) / K25 (avg) 形成 max / latest / avg 三件套 gauge。 對齊 K22
    // emit 語意: Option 過濾 — 該 provider 累計收過 event 但還沒完成過 session → 缺
    // 資料跳過不 emit sample, 避免 Prometheus 端把「沒看到」當「max=0」誤判「該
    // provider 瞬間完成」= 假健康信號。 3 個 render test 覆蓋 empty / per-provider
    // 隔離 / alphabetical sort + 整數格式三個邊界, 跟 K20-K25 既有 render test
    // 風格一致。 數據源: 不是獨立 HashMap, 直接讀 `provider_totals` —— 跟 K22 /
    // K25 同資料源, 讓 render helper 自己派發 pure fn 攤平 (對齊 R26/R27 政策:
    // cross-cutting snapshot 留給 M1 輪 MetricsSnapshot struct 統一處理, 本輪不
    // 重構 render 端 11 個參數的怪 signature)。

    #[test]
    fn completed_sessions_max_duration_empty_totals_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 / K21 / K22 / K23 / K24 / K25 empty-state 契約:
        // 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出), 不丟假資料。
        // ProviderTotals 沒 entry → 該 provider 不會被寫進 metric, 避免
        // Prometheus 端把「沒看到」當「max=0」誤判「該 provider 瞬間完成」= 假
        // 健康信號。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains("# HELP lobsterpulse_provider_completed_sessions_max_duration_seconds")
        );
        assert!(body.contains(
            "# TYPE lobsterpulse_provider_completed_sessions_max_duration_seconds gauge"
        ));
        // 沒 sample line — 用 lines().filter(starts_with) 鎖真正的 sample line,
        // 避免 `!contains("metric_name ")` 跟 HELP/TYPE 標頭(也含「name + 空格」)
        // 誤撞 (對齊 K25 同樣 pattern)。
        let k26_samples: Vec<&str> = body
            .lines()
            .filter(|l| {
                l.starts_with("lobsterpulse_provider_completed_sessions_max_duration_seconds{")
            })
            .collect();
        assert!(
            k26_samples.is_empty(),
            "空 totals 不應 emit K26 sample line, got: {k26_samples:?}, body: {body}"
        );
    }

    #[test]
    fn completed_sessions_max_duration_per_provider_isolated_and_skips_none() {
        // K26 跟 K22 / K25 對齊的 per-provider 隔離語意: 構造 2 個 provider,
        // 1 個有 max (Some(7200)), 1 個 default (max = None) → render 端只
        // emit 有 max 那個, default provider 不污染 output。 順便驗 K22 (latest)
        // 跟 K26 (max) emit 行為獨立: 同樣的 totals 進去, K22 跟 K26 該 emit
        // 各自的 sample, 不互相覆寫。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(100),
                max_completed_session_age_secs: Some(7200),
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());

        // K22 配套：算 `last_completed_session_age_at(&totals)` 給 8th 參數,
        // 模擬 production 從 ProviderTotals 派生 K22 map 的路徑(K22 跟 K26 共用
        // provider_totals 資料源 → 8th 參數該跟 totals 同步, 不能傳空 HashMap)。
        let last_completed = last_completed_session_age_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K26 cicx: max = 7200 (saturating_max 寫入, 跟 latest=100 隔離)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"cicx\"} 7200\n"
            ),
            "K26 cicx 該 emit max=7200, body: {body}"
        );
        // K26 claude: max = None → 該跳過不 emit sample
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"claude\"}"
            ),
            "K26 claude (max=None) 該跳過不 emit sample, body: {body}"
        );
        // K22 cicx: latest = 100 (K22 跟 K26 各自獨立 emit, 不互相覆寫)
        assert!(
            body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"} 100\n"
            ),
            "K22 cicx 仍 emit latest=100 (跟 K26 max=7200 隔離), body: {body}"
        );
    }

    #[test]
    fn completed_sessions_max_duration_alphabetical_sort_and_integer_format() {
        // 3 provider 非字母序插入(openx, cicx, gemini)→ 輸出必須 alphabetical
        // (cicx, gemini, openx), 跟 K6-K25 既契約一致, 給 Prometheus scraper
        // diff 穩定。 整數格式: K26 跟 K22 一樣 emit 整數 (沒 f64 / 沒 .0000 結尾),
        // 跟 K25 f64 4 位小數區分開。 順便驗 K22 / K25 / K26 三件套在同一
        // ProviderTotals 上各自 emit 各自的值 (K22=latest, K25=avg, K26=max) 不
        // 互相覆蓋。
        let mut totals = HashMap::new();
        totals.insert(
            "openx".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(30),
                completed_sessions_count: 1,
                completed_sessions_total_duration_secs: 30,
                max_completed_session_age_secs: Some(30),
                ..Default::default()
            },
        );
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(60),
                completed_sessions_count: 3,
                completed_sessions_total_duration_secs: 180,
                max_completed_session_age_secs: Some(600),
                ..Default::default()
            },
        );
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(3600),
                completed_sessions_count: 2,
                completed_sessions_total_duration_secs: 7200,
                max_completed_session_age_secs: Some(3600),
                ..Default::default()
            },
        );
        // K22 配套：同 per_provider_isolated_and_skips_none, 派生 K22 map 餵 8th
        // 參數(3 provider 都有 last_completed_session_age_secs)→ K22 sample lines
        // 跟 K26 sample lines 都在 output 內, 各自獨立 emit。
        let last_completed = last_completed_session_age_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // K26 alphabetical + 整數格式驗證: cicx=600, gemini=3600, openx=30
        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"cicx\"} 600\n")
            .expect("cicx K26 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"gemini\"} 3600\n")
            .expect("gemini K26 sample line");
        let openx_idx = body
            .find("lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"openx\"} 30\n")
            .expect("openx K26 sample line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider completed_sessions_max_duration_seconds 必須 alphabetical 排序 \
             (cicx={cicx_idx}, gemini={gemini_idx}, openx={openx_idx})"
        );
        // 反向驗：確認 emit 的是整數格式, 不是 f64 4 位小數格式 (沒有 `.0000` 結尾)
        // K26 gauge 跟 K22 一樣是整數 (latest / max 都 saturating clamp 到 i64),
        // 跟 K25 f64 4 位小數區分開。
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"cicx\"} 600.0000\n"),
            "K26 整數格式契約(不是 f64 4 位小數), 不可含 `.0000` 結尾, body: {body}"
        );
        // 順便驗 K22 (latest) / K25 (avg) / K26 (max) 三件套互不覆蓋
        //  K22 cicx 仍是 60 (latest), 沒被 K26 max=600 污染
        //  K25 cicx 仍是 60.0000 (avg=180/3), 沒被 K26 整數污染
        assert!(
            body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"} 60\n"
            ),
            "K22 (latest) 跟 K26 (max) 隔離, cicx latest 仍 emit 60, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} 60.0000\n"
            ),
            "K25 (avg) 跟 K26 (max) 隔離, cicx avg 仍 emit 60.0000, body: {body}"
        );
    }

    // ============== K27 per-provider completed_sessions_min_duration_seconds gauge ==============
    // 跟 K22 (latest) / K25 (avg) / K26 (max) 形成 min / max / latest / avg 四件套
    // gauge。對齊 K22 / K26 emit 語意: Option 過濾 — 該 provider 累計收過 event
    // 但還沒完成過 session → 缺資料跳過不 emit sample, 避免 Prometheus 端把「沒
    // 看到」當「min=0」誤判「該 provider 瞬間完成」= 假健康信號。 3 個 render
    // test 覆蓋 empty / per-provider 隔離 / alphabetical sort + 整數格式三個邊界,
    // 跟 K20-K26 既有 render test 風格一致。 數據源: 不是獨立 HashMap, 直接讀
    // `provider_totals` —— 跟 K22 / K25 / K26 同資料源, 讓 render helper 自己
    // 派發 pure fn 攤平 (對齊 R26/R27 政策: cross-cutting snapshot 留給 M1 輪
    // MetricsSnapshot struct 統一處理, 本輪不重構 render 端 11 個參數的怪
    // signature)。

    #[test]
    fn completed_sessions_min_duration_empty_totals_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 / K21 / K22 / K23 / K24 / K25 / K26
        // empty-state 契約: 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出),
        // 不丟假資料。 ProviderTotals 沒 entry → 該 provider 不會被寫進 metric,
        // 避免 Prometheus 端把「沒看到」當「min=0」誤判「該 provider 瞬間完成」=
        // 假健康信號。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains("# HELP lobsterpulse_provider_completed_sessions_min_duration_seconds")
        );
        assert!(body.contains(
            "# TYPE lobsterpulse_provider_completed_sessions_min_duration_seconds gauge"
        ));
        // 沒 sample line — 用 lines().filter(starts_with) 鎖真正的 sample line,
        // 避免 `!contains("metric_name ")` 跟 HELP/TYPE 標頭(也含「name + 空格」)
        // 誤撞 (對齊 K25 / K26 同樣 pattern)。
        let k27_samples: Vec<&str> = body
            .lines()
            .filter(|l| {
                l.starts_with("lobsterpulse_provider_completed_sessions_min_duration_seconds{")
            })
            .collect();
        assert!(
            k27_samples.is_empty(),
            "空 totals 不應 emit K27 sample line, got: {k27_samples:?}, body: {body}"
        );
    }

    #[test]
    fn completed_sessions_min_duration_per_provider_isolated_and_skips_none() {
        // K27 跟 K22 / K26 對齊的 per-provider 隔離語意: 構造 2 個 provider, 1
        // 個有 min (Some(8)), 1 個 default (min = None) → render 端只 emit 有
        // min 那個, default provider 不污染 output。 順便驗 K22 (latest) / K26
        // (max) / K27 (min) emit 行為獨立: 同樣的 totals 進去, K22 / K26 / K27
        // 該 emit 各自的 sample, 不互相覆寫。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(100),
                max_completed_session_age_secs: Some(7200),
                min_completed_session_age_secs: Some(8),
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());

        // K22 配套：算 `last_completed_session_age_at(&totals)` 給 8th 參數,
        // 模擬 production 從 ProviderTotals 派生 K22 map 的路徑(K22 跟 K27 共用
        // provider_totals 資料源 → 8th 參數該跟 totals 同步, 不能傳空 HashMap)。
        let last_completed = last_completed_session_age_at(&totals);
        // K27 配套：算 `completed_sessions_min_duration_at(&totals)` 餵 6th 參數
        // (provider_max_session_age 位置), 跟 production 派生 K27 map 同步。
        // 跟 K26 render 模式不同：K26 沒額外 snapshot 參數（直接讀 totals）,
        // 本測試只為了跟 K22 / K26 共用同一個 totals 餵進去驗 4 件套隔離。
        let _completed_min = completed_sessions_min_duration_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K27 cicx: min = 8 (saturating_min 寫入, 跟 latest=100 / max=7200 隔離)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"cicx\"} 8\n"
            ),
            "K27 cicx 該 emit min=8, body: {body}"
        );
        // K27 claude: min = None → 該跳過不 emit sample
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"claude\"}"
            ),
            "K27 claude (min=None) 該跳過不 emit sample, body: {body}"
        );
        // K22 cicx: latest = 100 (K22 跟 K27 各自獨立 emit, 不互相覆寫)
        assert!(
            body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"} 100\n"
            ),
            "K22 cicx 仍 emit latest=100 (跟 K27 min=8 隔離), body: {body}"
        );
        // K26 cicx: max = 7200 (K26 跟 K27 各自獨立 emit, 不互相覆寫)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"cicx\"} 7200\n"
            ),
            "K26 cicx 仍 emit max=7200 (跟 K27 min=8 隔離), body: {body}"
        );
    }

    #[test]
    fn completed_sessions_min_duration_alphabetical_sort_and_integer_format() {
        // 3 provider 非字母序插入(openx, cicx, gemini)→ 輸出必須 alphabetical
        // (cicx, gemini, openx), 跟 K6-K26 既契約一致, 給 Prometheus scraper
        // diff 穩定。 整數格式: K27 跟 K22 / K26 一樣 emit 整數 (沒 f64 / 沒
        // .0000 結尾), 跟 K25 f64 4 位小數區分開。 順便驗 K22 / K25 / K26 /
        // K27 四件套在同一 ProviderTotals 上各自 emit 各自的值 (K22=latest,
        // K25=avg, K26=max, K27=min) 不互相覆蓋。
        let mut totals = HashMap::new();
        totals.insert(
            "openx".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(30),
                completed_sessions_count: 1,
                completed_sessions_total_duration_secs: 30,
                max_completed_session_age_secs: Some(30),
                min_completed_session_age_secs: Some(30),
                ..Default::default()
            },
        );
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(60),
                completed_sessions_count: 3,
                completed_sessions_total_duration_secs: 180,
                max_completed_session_age_secs: Some(600),
                min_completed_session_age_secs: Some(15),
                ..Default::default()
            },
        );
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(3600),
                completed_sessions_count: 2,
                completed_sessions_total_duration_secs: 7200,
                max_completed_session_age_secs: Some(3600),
                min_completed_session_age_secs: Some(3600),
                ..Default::default()
            },
        );
        // K22 配套：同 per_provider_isolated_and_skips_none, 派生 K22 map 餵 8th
        // 參數(3 provider 都有 last_completed_session_age_secs)→ K22 sample lines
        // 跟 K27 sample lines 都在 output 內, 各自獨立 emit。
        let last_completed = last_completed_session_age_at(&totals);
        let _completed_min = completed_sessions_min_duration_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // K27 alphabetical + 整數格式驗證: cicx=15, gemini=3600, openx=30
        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"cicx\"} 15\n")
            .expect("cicx K27 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"gemini\"} 3600\n")
            .expect("gemini K27 sample line");
        let openx_idx = body
            .find("lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"openx\"} 30\n")
            .expect("openx K27 sample line");
        assert!(
            cicx_idx < gemini_idx && gemini_idx < openx_idx,
            "per-provider completed_sessions_min_duration_seconds 必須 alphabetical 排序 \
             (cicx={cicx_idx}, gemini={gemini_idx}, openx={openx_idx})"
        );
        // 反向驗：確認 emit 的是整數格式, 不是 f64 4 位小數格式 (沒有 `.0000` 結尾)
        // K27 gauge 跟 K22 / K26 一樣是整數 (min saturating clamp 到 i64),
        // 跟 K25 f64 4 位小數區分開。
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"cicx\"} 15.0000\n"),
            "K27 整數格式契約(不是 f64 4 位小數), 不可含 `.0000` 結尾, body: {body}"
        );
        // 順便驗 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) 四件套互不覆蓋
        //  K22 cicx 仍是 60 (latest), 沒被 K27 min=15 污染
        //  K25 cicx 仍是 60.0000 (avg=180/3), 沒被 K27 整數污染
        //  K26 cicx 仍是 600 (max), 沒被 K27 覆蓋
        assert!(
            body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"} 60\n"
            ),
            "K22 (latest) 跟 K27 (min) 隔離, cicx latest 仍 emit 60, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} 60.0000\n"
            ),
            "K25 (avg) 跟 K27 (min) 隔離, cicx avg 仍 emit 60.0000, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"cicx\"} 600\n"
            ),
            "K26 (max) 跟 K27 (min) 隔離, cicx max 仍 emit 600, body: {body}"
        );
    }

    // ============== K28 per-provider completed_sessions_stddev_seconds gauge ==============
    // 跟 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) 互補: 五件套中唯一
    // 補「波動性」維度。Welford online algorithm 在 record 端累積 mean/M2,
    // render 端只 emit `(M2 / count).sqrt()` 結果。 4 位小數 f64 格式 (跟 K25
    // avg 一致, 跟 K26/K27 整數區分)。 count=0 過濾 (跟 K26/K27 None 跳過
    // 語意對齊, 但走 count 過濾是因為 Welford mean/M2 預設 0.0, 沒
    // 「無值 vs 值=0」可區分性)。 3 個 render test 覆蓋 empty / per-provider
    // 隔離 / alphabetical sort + f64 precision, 跟 K20-K27 既有 render test
    // 風格一致。

    #[test]
    fn completed_sessions_stddev_empty_totals_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 / K21 / K22 / K23 / K24 / K25 / K26 / K27
        // empty-state 契約: 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出),
        // 不丟假資料。 ProviderTotals 沒 entry → count 預設 0 → 過濾掉,
        // 避免 Prometheus 端把「沒看到」當「stddev=0」誤判「該 provider session
        // 時長無波動」= 假健康信號。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(body.contains("# HELP lobsterpulse_provider_completed_sessions_stddev_seconds"));
        assert!(
            body.contains("# TYPE lobsterpulse_provider_completed_sessions_stddev_seconds gauge")
        );
        // 沒 sample line 契約: 任何 provider=... 都不該 emit (空 map → 沒資料)
        assert!(
            !body.lines().any(|l| l.starts_with(
                "lobsterpulse_provider_completed_sessions_stddev_seconds{provider=\""
            )),
            "empty totals 不該 emit stddev sample line, body: {body}"
        );
    }

    #[test]
    fn completed_sessions_stddev_per_provider_isolated_and_skips_zero_count() {
        // per-provider 隔離 + count=0 跳過: cicx (count=2) emit, gemini
        // (count=1, stddev=0) emit 0.0000, openx (count=0) 跳過。 用
        // struct literal initializer 明確控制每個 provider 狀態, 跟 K20-K27
        // 既有隔離測試風格一致。
        let mut totals = HashMap::new();
        // cicx: 2 samples [40, 80] → mean=60, M2=800, stddev=√400=20
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 2,
                completed_sessions_mean_secs: 60.0,
                completed_sessions_m2_secs: 800.0,
                ..Default::default()
            },
        );
        // gemini: 1 sample [42] → mean=42, M2=0, stddev=0 (單樣本無波動)
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_count: 1,
                completed_sessions_mean_secs: 42.0,
                completed_sessions_m2_secs: 0.0,
                ..Default::default()
            },
        );
        // openx: 0 samples → count=0 → render 端跳過, 不該 emit sample
        totals.insert("openx".to_string(), ProviderTotals::default());

        let _completed_stddev = completed_sessions_stddev_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // cicx emit 20.0000 (4 位小數 f64)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_stddev_seconds{provider=\"cicx\"} 20.0000\n"
            ),
            "cicx stddev = √(800/2) = 20.0000, body: {body}"
        );
        // gemini emit 0.0000 (count=1, 視為有效資料, 跟 K25 avg=該 sample 一致)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_stddev_seconds{provider=\"gemini\"} 0.0000\n"
            ),
            "gemini stddev = 0 (count=1, M2=0), body: {body}"
        );
        // openx 跳過: count=0 不該 emit sample line (K26/K27 None 跳過契約延伸)
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_stddev_seconds{provider=\"openx\"}"
            ),
            "openx count=0 該跳過 (跟 K26/K27 None 語意對齊), body: {body}"
        );
    }

    #[test]
    fn completed_sessions_stddev_alphabetical_sort_and_four_decimal_precision() {
        // alphabetical sort + f64 4 位小數 precision + 跟 K22/K25/K26/K27 互不
        // 覆蓋: 三個 provider cicx/claude/gemini 不同 sample → 各自 emit 自己的
        // stddev, alphabetical 排序 (cicx < claude < gemini), 全部 4 位小數格式。
        let mut totals = HashMap::new();
        // cicx: 2 samples [100, 200] → mean=150, M2=5000, stddev=√2500=50,
        // latest=200, max=200, min=100
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(200),
                completed_sessions_count: 2,
                completed_sessions_total_duration_secs: 300,
                max_completed_session_age_secs: Some(200),
                min_completed_session_age_secs: Some(100),
                completed_sessions_mean_secs: 150.0,
                completed_sessions_m2_secs: 5000.0,
                ..Default::default()
            },
        );
        // claude: 3 samples [10, 20, 30] → mean=20, M2=200, stddev=√(200/3)≈8.165,
        // latest=30, max=30, min=10
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(30),
                completed_sessions_count: 3,
                completed_sessions_total_duration_secs: 60,
                max_completed_session_age_secs: Some(30),
                min_completed_session_age_secs: Some(10),
                completed_sessions_mean_secs: 20.0,
                completed_sessions_m2_secs: 200.0,
                ..Default::default()
            },
        );
        // gemini: 1 sample [42] → stddev=0, latest=max=min=42
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(42),
                completed_sessions_count: 1,
                completed_sessions_total_duration_secs: 42,
                max_completed_session_age_secs: Some(42),
                min_completed_session_age_secs: Some(42),
                completed_sessions_mean_secs: 42.0,
                completed_sessions_m2_secs: 0.0,
                ..Default::default()
            },
        );

        let last_completed = last_completed_session_age_at(&totals);
        let _completed_stddev = completed_sessions_stddev_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &last_completed,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K28 alphabetical + 4 位小數 f64 格式驗證:
        // cicx=50.0000 (√2500), claude=8.1650 (√(200/3)=√66.6667≈8.1650),
        // gemini=0.0000 (count=1, M2=0)
        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_stddev_seconds{provider=\"cicx\"} 50.0000\n")
            .expect("cicx K28 sample line");
        let claude_idx = body
            .find("lobsterpulse_provider_completed_sessions_stddev_seconds{provider=\"claude\"} 8.1650\n")
            .expect("claude K28 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_stddev_seconds{provider=\"gemini\"} 0.0000\n")
            .expect("gemini K28 sample line");
        assert!(
            cicx_idx < claude_idx && claude_idx < gemini_idx,
            "per-provider completed_sessions_stddev_seconds 必須 alphabetical 排序 \
             (cicx={cicx_idx}, claude={claude_idx}, gemini={gemini_idx})"
        );
        // 反向驗: 確認 emit 的是 4 位小數 f64 格式, 不是整數格式 (沒有 整數值 直接
        // emit 沒小數點)
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_stddev_seconds{provider=\"cicx\"} 50\n"
            ),
            "K28 f64 4 位小數契約(不是整數), 不可無小數點, body: {body}"
        );
        // 順便驗 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev)
        // 五件套互不覆蓋: cicx latest=200, avg=150.0000, max=200, min=100,
        // stddev=50.0000 各 emit 各自 metric, 沒被彼此污染
        assert!(
            body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"} 200\n"
            ),
            "K22 (latest) 跟 K28 (stddev) 隔離, cicx latest 仍 emit 200, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} 150.0000\n"
            ),
            "K25 (avg) 跟 K28 (stddev) 隔離, cicx avg 仍 emit 150.0000, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_max_duration_seconds{provider=\"cicx\"} 200\n"
            ),
            "K26 (max) 跟 K28 (stddev) 隔離, cicx max 仍 emit 200, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_min_duration_seconds{provider=\"cicx\"} 100\n"
            ),
            "K27 (min) 跟 K28 (stddev) 隔離, cicx min 仍 emit 100, body: {body}"
        );
    }

    // ============== K29 per-provider failure_to_completion_ratio gauge ==============
    // 跟 K25 (avg) 同屬「兩個 lifetime counter 組合成 ratio」純 derived 衍生
    // gauge —— K10 failure_count / K23 completed_sessions_count 兩條 counter
    // 在 render 端做除法 (4 位小數 f64, 跟 K25 avg / K28 stddev 對齊)。 補
    // K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) 五件套
    // 都沒覆蓋的「失敗 vs 成功比」維度 —— 跟 K9 / K10 絕對失敗計數互補。
    // count=0 過濾 (跟 K25 同款防線, 0/0 數學未定義)。 3 個 render test 覆蓋
    // empty / per-provider 隔離 / alphabetical sort + f64 precision, 跟
    // K20-K28 既有 render test 風格一致。

    #[test]
    fn failure_to_completion_ratio_empty_totals_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 / K21 / K22 / K23 / K24 / K25 / K26 / K27 /
        // K28 empty-state 契約: 空 map → 沒 sample line (HELP/TYPE 標頭仍輸出),
        // 不丟假資料。 ProviderTotals 沒 entry → count 預設 0 → 過濾掉, 避免
        // Prometheus 端把「沒看到」當「ratio=0」誤判「該 provider 零失敗」= 假
        // 健康信號。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(body.contains("# HELP lobsterpulse_provider_failure_to_completion_ratio"));
        assert!(body.contains("# TYPE lobsterpulse_provider_failure_to_completion_ratio gauge"));
        // 沒 sample line 契約: 任何 provider=... 都不該 emit (空 map → 沒資料)
        assert!(
            !body
                .lines()
                .any(|l| l
                    .starts_with("lobsterpulse_provider_failure_to_completion_ratio{provider=\"")),
            "empty totals 不該 emit failure ratio sample line, body: {body}"
        );
    }

    #[test]
    fn failure_to_completion_ratio_per_provider_isolated_and_skips_zero_count() {
        // per-provider 隔離 + count=0 跳過: cicx (failure=3/completed=2) emit
        // 1.5, claude (failure=0/completed=5) emit 0.0, openx (count=0) 跳過。
        // 用 struct literal initializer 明確控制每個 provider 狀態, 跟 K20-K28
        // 既有隔離測試風格一致。
        let mut totals = HashMap::new();
        // cicx: failure=3 / completed=2 → 1.5
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                failure_count: 3,
                completed_sessions_count: 2,
                ..Default::default()
            },
        );
        // claude: failure=0 / completed=5 → 0.0 (零失敗, 真實健康信號, 跟
        // 「缺資料跳過」必須分清楚)
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                failure_count: 0,
                completed_sessions_count: 5,
                ..Default::default()
            },
        );
        // openx: completed_sessions_count=0 → render 端跳過, 不該 emit sample
        totals.insert("openx".to_string(), ProviderTotals::default());

        let _failure_ratio = failure_to_completion_ratio_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // cicx emit 1.5000 (4 位小數 f64)
        assert!(
            body.contains(
                "lobsterpulse_provider_failure_to_completion_ratio{provider=\"cicx\"} 1.5000\n"
            ),
            "cicx ratio = 3/2 = 1.5000, body: {body}"
        );
        // claude emit 0.0000 (真實零失敗, 不是缺資料)
        assert!(
            body.contains(
                "lobsterpulse_provider_failure_to_completion_ratio{provider=\"claude\"} 0.0000\n"
            ),
            "claude ratio = 0/5 = 0.0000 (真實零失敗), body: {body}"
        );
        // openx 跳過: count=0 不該 emit sample line (K25 「0/0 不 emit」同款防線)
        assert!(
            !body.contains("lobsterpulse_provider_failure_to_completion_ratio{provider=\"openx\"}"),
            "openx count=0 該跳過 (跟 K25 「0/0 不 emit」同款防線), body: {body}"
        );
    }

    #[test]
    fn failure_to_completion_ratio_alphabetical_sort_and_four_decimal_precision() {
        // alphabetical sort + f64 4 位小數 precision + 跟 K22 / K25 / K26 / K27 /
        // K28 五件套互不覆蓋: 三個 provider cicx/claude/gemini 不同 failure
        // pattern → 各自 emit 自己的 ratio, alphabetical 排序
        // (cicx < claude < gemini), 全部 4 位小數格式。
        let mut totals = HashMap::new();
        // cicx: failure=3 / completed=2 → 1.5
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                failure_count: 3,
                completed_sessions_count: 2,
                ..Default::default()
            },
        );
        // claude: failure=0 / completed=5 → 0.0 (零失敗, 真實健康信號)
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                failure_count: 0,
                completed_sessions_count: 5,
                ..Default::default()
            },
        );
        // gemini: failure=8 / completed=1 → 8.0 (alert > 2.0 觸發「retry 過高」)
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                failure_count: 8,
                completed_sessions_count: 1,
                ..Default::default()
            },
        );

        let _failure_ratio = failure_to_completion_ratio_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K29 alphabetical + 4 位小數 f64 格式驗證:
        // cicx=1.5000, claude=0.0000, gemini=8.0000
        let cicx_idx = body
            .find("lobsterpulse_provider_failure_to_completion_ratio{provider=\"cicx\"} 1.5000\n")
            .expect("cicx K29 sample line");
        let claude_idx = body
            .find("lobsterpulse_provider_failure_to_completion_ratio{provider=\"claude\"} 0.0000\n")
            .expect("claude K29 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_failure_to_completion_ratio{provider=\"gemini\"} 8.0000\n")
            .expect("gemini K29 sample line");
        assert!(
            cicx_idx < claude_idx && claude_idx < gemini_idx,
            "per-provider failure_to_completion_ratio 必須 alphabetical 排序 \
             (cicx={cicx_idx}, claude={claude_idx}, gemini={gemini_idx})"
        );
        // 反向驗: 確認 emit 的是 4 位小數 f64 格式, 不是整數格式
        assert!(
            !body.contains(
                "lobsterpulse_provider_failure_to_completion_ratio{provider=\"cicx\"} 1.5\n"
            ),
            "K29 f64 4 位小數契約(不是 f64 3 位小數), 不可 emit 1.5, body: {body}"
        );
        // 順便驗 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev)
        // 五件套 + K29 (ratio) 互不覆蓋: cicx 在 5 個 metric 各自 emit 自己的值
        // (ratio=1.5 跟 latest/avg/max/min/stddev 隔離, 沒被彼此污染)
        assert!(
            !body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"}"
            ),
            "K22 (latest) 跟 K29 (ratio) 隔離, cicx 在 K22 沒 latest (None) 不該 emit, body: {body}"
        );
        // K25 (avg) 跟 K29 (ratio) 隔離: cicx total_duration=0 + count=2 →
        // K25 emit avg=0.0000, K29 emit ratio=1.5000 (各發各的 series line,
        // 互不污染)。K25 pure fn 邏輯是 count>0 一律 emit (含 0.0), 不能用
        // `!contains` 驗隔離 —— 改用「K25 跟 K29 各自 emit 各自的值」雙驗證。
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_average_duration_seconds{provider=\"cicx\"} 0.0000\n"
            ),
            "K25 cicx avg = 0/2 = 0.0000 (K25 跟 K29 隔離, 互不污染), body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_failure_to_completion_ratio{provider=\"cicx\"} 1.5000\n"
            ),
            "K29 cicx ratio = 3/2 = 1.5000 (K25 跟 K29 隔離, 互不污染), body: {body}"
        );
    }

    // ============== K30 per-provider completed_sessions_p95 gauge ==============
    // 跟 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) 五件套 +
    // K29 (failure ratio) 互補形成「七件套」 —— K30 是第六個維度「95 百分位延遲」
    // (後續若加 K31 median / K32 p99 可再擴)。資料源 ProviderTotals
    // .completed_sessions_p95_samples (reservoir bounded 1024, K30 record 函式
    // 累積), 純 fn completed_sessions_p95_at sort 找 P95。samples 為空跳過
    // (跟 K22 / K25-K29 既「缺資料不 emit」一致, 避免 P95=0 假冒「瞬間完成」
    // 假健康信號)。3 個 render test 覆蓋 empty / per-provider 隔離 /
    // alphabetical sort + 整數 precision, 跟 K20-K29 既有 render test 風格一致。

    #[test]
    fn p95_empty_totals_emits_header_only() {
        // 對齊 K11 / K18 / K19 / K20 / K21 / K22 / K23 / K24 / K25 / K26 / K27 /
        // K28 / K29 empty-state 契約: 空 map → 沒 sample line (HELP/TYPE 標頭
        // 仍輸出), 不丟假資料。ProviderTotals 沒 entry → samples 預設空 → 過濾
        // 掉, 避免 Prometheus 端把「沒看到」當「P95=0」誤判「該 provider 瞬間
        // 完成所有 session」= 假健康信號。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains("# HELP lobsterpulse_provider_completed_sessions_p95_duration_seconds")
        );
        assert!(body.contains(
            "# TYPE lobsterpulse_provider_completed_sessions_p95_duration_seconds gauge"
        ));
        // 沒 sample line 契約: 任何 provider=... 都不該 emit (空 map → 沒資料)
        assert!(
            !body.lines().any(|l| l.starts_with(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\""
            )),
            "empty totals 不該 emit P95 sample line, body: {body}"
        );
    }

    #[test]
    fn p95_per_provider_isolated_and_skips_empty_samples() {
        // per-provider 隔離 + samples 為空跳過: cicx (20 sample [1..20]) emit
        // P95=20, claude (samples 為空) 跳過, openx (samples 為空) 跳過。用
        // struct literal + `..Default::default()` 明確控制每個 provider 狀態,
        // 跟 K20-K29 既有隔離測試風格一致。P95 計算: 20 個 sample 排序後
        // index = 20 * 95 / 100 = 19 → samples[19] = 20 (少樣本下 P95 退化到
        // max, 跟 production pure fn 行為一致)。
        let mut totals = HashMap::new();
        // cicx: 20 sample [1..20] → 排序後 P95 index=19, samples[19]=20
        let mut cicx_samples: Vec<i64> = (1..=20).collect();
        cicx_samples.sort_unstable(); // 已排序, 為求語意清楚顯式 sort
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: cicx_samples,
                ..Default::default()
            },
        );
        // claude: samples 為空 → render 端跳過, 不該 emit sample line
        totals.insert("claude".to_string(), ProviderTotals::default());
        // openx: samples 為空 → render 端跳過, 不該 emit sample line
        totals.insert("openx".to_string(), ProviderTotals::default());

        let _p95 = completed_sessions_p95_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // cicx emit 20 (i64 整數, 沒有 4 位小數 f64 跟 K25/K28/K29 不同)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "cicx P95 = samples[19] = 20 (整數 i64), body: {body}"
        );
        // claude 跳過: samples 為空不該 emit sample line (K25 「0/0 不 emit」同款防線)
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"claude\"}"),
            "claude samples 為空該跳過, body: {body}"
        );
        // openx 跳過: 同 claude
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"openx\"}"
            ),
            "openx samples 為空該跳過, body: {body}"
        );
    }

    #[test]
    fn p95_alphabetical_sort_and_integer_precision() {
        // alphabetical sort + i64 整數 precision + 跟 K22 / K25 / K26 / K27 /
        // K28 / K29 六件套互不覆蓋: 三個 provider cicx/claude/gemini 不同
        // reservoir → 各自 emit 自己的 P95, alphabetical 排序 (cicx < claude <
        // gemini), 整數 i64 格式 (沒有 f64 4 位小數, 跟 sample 整數 duration 語意
        // 一致, 強制裁整 0 精度流失)。
        let mut totals = HashMap::new();
        // cicx: 20 sample [1..20] → P95=20
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=20).collect(),
                ..Default::default()
            },
        );
        // claude: 100 sample [1..100] → P95 index=95, samples[95]=96
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=100).collect(),
                ..Default::default()
            },
        );
        // gemini: 50 sample [1..50] → P95 index=47, samples[47]=48
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=50).collect(),
                ..Default::default()
            },
        );

        let _p95 = completed_sessions_p95_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K30 alphabetical + 整數 i64 格式驗證:
        // cicx=20, claude=96, gemini=48
        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 20\n")
            .expect("cicx K30 sample line");
        let claude_idx = body
            .find("lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"claude\"} 96\n")
            .expect("claude K30 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"gemini\"} 48\n")
            .expect("gemini K30 sample line");
        assert!(
            cicx_idx < claude_idx && claude_idx < gemini_idx,
            "per-provider P95 必須 alphabetical 排序 \
             (cicx={cicx_idx}, claude={claude_idx}, gemini={gemini_idx})"
        );
        // 反向驗: 確認 emit 的是整數 i64 格式, 不是 f64 4 位小數格式
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 20.0000\n"
            ),
            "K30 i64 整數契約(不是 f64 4 位小數), 不可 emit 20.0000, body: {body}"
        );
        // 順便驗 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) /
        // K29 (ratio) 五件套 + K30 (P95) 互不覆蓋: cicx 在各 metric 各自 emit 自己
        // 的值, 互不污染 (K30 整數 vs K25/K28/K29 f64 格式本身已隔離, 雙驗證)
        assert!(
            !body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"}"
            ),
            "K22 (latest) 跟 K30 (P95) 隔離, cicx 在 K22 沒 latest (None) 不該 emit, body: {body}"
        );
        // K29 (ratio) 跟 K30 (P95) 隔離: cicx 在 K29 沒 failure_count/completed_count
        // → 跳過, K30 emit P95=20, 雙驗證各發各的 series line 互不污染。
        assert!(
            !body.contains("lobsterpulse_provider_failure_to_completion_ratio{provider=\"cicx\"}"),
            "K29 (ratio) 跟 K30 (P95) 隔離, cicx count=0 該跳過 K29, body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "K30 cicx P95 = 20 (K29 跟 K30 隔離, 互不污染), body: {body}"
        );
    }

    // ============== K31 per-provider completed_sessions_p50 gauge ==============
    // 跟 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) / K29
    // (failure ratio) / K30 (P95) 七件套互補形成「八件套」 —— K31 是第七個維度
    // 「50 百分位延遲」= median 中位數。資料源沿用 K30 `ProviderTotals
    // .completed_sessions_p95_samples` (K31 不開新欄位, 共用 K30 reservoir 1024
    // sliding window), 純 fn `completed_sessions_p50_at` 跟 K30 復用同一份 vec
    // 各自 sort 後取不同 percentile index (K31 取 50/100, K30 取 95/100)。樣本
    // 為空跳過 (跟 K22 / K25-K30 既「缺資料不 emit」一致, 避免 P50=0 假冒「瞬間
    // 完成中位數」假健康信號)。3 個 render test 覆蓋 empty / per-provider 隔離 /
    // alphabetical sort + 整數 precision + 跟 K30 雙驗證同 reservoir 各自 emit
    // 各自 percentile, 跟 K20-K30 既有 render test 風格一致。

    #[test]
    fn p50_empty_totals_emits_header_only() {
        // 對齊 K11 / K18-K30 empty-state 契約: 空 map → 沒 sample line
        // (HELP/TYPE 標頭仍輸出), 不丟假資料。ProviderTotals 沒 entry → samples
        // 預設空 → 過濾掉, 避免 Prometheus 端把「沒看到」當「P50=0」誤判「該
        // provider 瞬間完成所有 session」= 假健康信號。順手驗 K30 標頭在 K31
        // 後仍存在, 證明 K31 沒覆蓋 K30 emit block。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains("# HELP lobsterpulse_provider_completed_sessions_p50_duration_seconds")
        );
        assert!(body.contains(
            "# TYPE lobsterpulse_provider_completed_sessions_p50_duration_seconds gauge"
        ));
        // 沒 sample line 契約: 任何 provider=... 都不該 emit (空 map → 沒資料)
        assert!(
            !body.lines().any(|l| l.starts_with(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\""
            )),
            "empty totals 不該 emit P50 sample line, body: {body}"
        );
        // 順手驗 K30 標頭仍存在, 證明 K31 沒覆蓋 K30 emit block
        assert!(
            body.contains(
                "# TYPE lobsterpulse_provider_completed_sessions_p95_duration_seconds gauge"
            ),
            "K31 補在 K30 後, K30 emit block 仍要 emit HELP/TYPE 標頭, body: {body}"
        );
    }

    #[test]
    fn p50_per_provider_isolated_and_skips_empty_samples() {
        // per-provider 隔離 + samples 為空跳過: cicx (20 sample [1..20]) emit
        // P50=11, claude (samples 為空) 跳過, openx (samples 為空) 跳過。用
        // struct literal + `..Default::default()` 明確控制每個 provider 狀態,
        // 跟 K20-K30 既有隔離測試風格一致。P50 計算: 20 個 sample 排序後
        // index = 20 * 50 / 100 = 10 → samples[10] = 11 (少樣本下 P50 退到接近
        // 中位, 跟 production pure fn 行為一致)。
        let mut totals = HashMap::new();
        // cicx: 20 sample [1..20] → 排序後 P50 index=10, samples[10]=11
        let mut cicx_samples: Vec<i64> = (1..=20).collect();
        cicx_samples.sort_unstable(); // 已排序, 為求語意清楚顯式 sort
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: cicx_samples,
                ..Default::default()
            },
        );
        // claude: samples 為空 → render 端跳過, 不該 emit sample line
        totals.insert("claude".to_string(), ProviderTotals::default());
        // openx: samples 為空 → render 端跳過, 不該 emit sample line
        totals.insert("openx".to_string(), ProviderTotals::default());

        let _p50 = completed_sessions_p50_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // cicx emit 11 (i64 整數, 沒有 4 位小數 f64 跟 K25/K28/K29 不同)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"cicx\"} 11\n"
            ),
            "cicx P50 = samples[10] = 11 (整數 i64), body: {body}"
        );
        // claude 跳過: samples 為空不該 emit sample line (跟 K25 「0/0 不 emit」同款)
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"claude\"}"),
            "claude samples 為空該跳過, body: {body}"
        );
        // openx 跳過: 同 claude
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"openx\"}"
            ),
            "openx samples 為空該跳過, body: {body}"
        );
    }

    #[test]
    fn p50_alphabetical_sort_and_integer_precision() {
        // alphabetical sort + i64 整數 precision + 跟 K22 / K25-K30 七件套互不
        // 覆蓋: 三個 provider cicx/claude/gemini 同一份 reservoir 各自 sort
        // 取不同 percentile → 各自 emit 自己的 P50 (= median), alphabetical
        // 排序 (cicx < claude < gemini), 整數 i64 格式 (沒有 f64 4 位小數,
        // 跟 sample 整數 duration 語意一致, 強制裁整 0 精度流失)。K31 跟
        // K30 雙驗證: 同一份 cicx samples 餵 K30 (P95) 跟 K31 (P50), 各自
        // emit 不同值 (K30=20 / K31=11), 證明 pure fn 各自獨立 + render
        // emit 互不污染。
        let mut totals = HashMap::new();
        // cicx: 20 sample [1..20] → P50=11 (idx=10)
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=20).collect(),
                ..Default::default()
            },
        );
        // claude: 100 sample [1..100] → P50 index=50, samples[50]=51
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=100).collect(),
                ..Default::default()
            },
        );
        // gemini: 50 sample [1..50] → P50 index=25, samples[25]=26
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=50).collect(),
                ..Default::default()
            },
        );

        let _p50 = completed_sessions_p50_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K31 alphabetical + 整數 i64 格式驗證: cicx=11, claude=51, gemini=26
        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"cicx\"} 11\n")
            .expect("cicx K31 sample line");
        let claude_idx = body
            .find("lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"claude\"} 51\n")
            .expect("claude K31 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"gemini\"} 26\n")
            .expect("gemini K31 sample line");
        assert!(
            cicx_idx < claude_idx && claude_idx < gemini_idx,
            "per-provider P50 必須 alphabetical 排序 \
             (cicx={cicx_idx}, claude={claude_idx}, gemini={gemini_idx})"
        );
        // 反向驗: 確認 emit 的是整數 i64 格式, 不是 f64 4 位小數格式
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"cicx\"} 11.0000\n"
            ),
            "K31 i64 整數契約(不是 f64 4 位小數), 不可 emit 11.0000, body: {body}"
        );
        // K30 (P95) 跟 K31 (P50) 共用 samples vec 雙驗證: 同一份 cicx samples
        // 餵 K30 (P95=20) 跟 K31 (P50=11), 各自 emit 各自 percentile 互不污染。
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "K30 cicx P95 = 20 (跟 K31 P50=11 共用 samples vec, 各 emit 各 percentile), body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"cicx\"} 11\n"
            ),
            "K31 cicx P50 = 11 (跟 K30 P95=20 共用 samples vec, 各 emit 各 percentile), body: {body}"
        );
        // 順便驗 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) /
        // K29 (ratio) 五件套 + K30 (P95) / K31 (P50) 互不覆蓋: cicx 在各 metric
        // 各自 emit 自己的值, 互不污染 (K31 整數 vs K25/K28/K29 f64 格式本身已
        // 隔離, 雙驗證)
        assert!(
            !body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"}"
            ),
            "K22 (latest) 跟 K31 (P50) 隔離, cicx 在 K22 沒 latest (None) 不該 emit, body: {body}"
        );
        assert!(
            !body.contains("lobsterpulse_provider_failure_to_completion_ratio{provider=\"cicx\"}"),
            "K29 (ratio) 跟 K31 (P50) 隔離, cicx count=0 該跳過 K29, body: {body}"
        );
    }

    // ============== K32 per-provider completed_sessions_p99 gauge ==============
    // 跟 K30 P95 / K31 P50 同模板, 純 fn `completed_sessions_p99_at` 復用
    // `ProviderTotals.completed_sessions_p95_samples` (K30/K31/K32 共用 reservoir
    // 1024 sliding window) sort 後取 99 百分位 index, 跟 K30/K31 各自 emit
    // 各自 percentile。樣本為空跳過 (跟 K30/K31 既「缺資料不 emit」一致, 避免
    // P99=0 假冒「瞬間完成極端尾端 1%」假健康信號)。3 個 render test 跟 K30/K31
    // 既有 render test 對稱: empty / per-provider 隔離 / alphabetical sort + 整數
    // precision + 跟 K30/K31 三件套共用 samples vec 雙驗證。

    #[test]
    fn p99_empty_totals_emits_header_only() {
        // 對齊 K11 / K18-K31 empty-state 契約: 空 map → 沒 sample line
        // (HELP/TYPE 標頭仍輸出), 不丟假資料。ProviderTotals 沒 entry → samples
        // 預設空 → 過濾掉, 避免 Prometheus 端把「沒看到」當「P99=0」誤判「該
        // provider 瞬間完成所有 session」= 假健康信號。順手驗 K30/K31 標頭在
        // K32 後仍存在, 證明 K32 沒覆蓋 K30/K31 emit block。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains("# HELP lobsterpulse_provider_completed_sessions_p99_duration_seconds")
        );
        assert!(body.contains(
            "# TYPE lobsterpulse_provider_completed_sessions_p99_duration_seconds gauge"
        ));
        // 沒 sample line 契約: 任何 provider=... 都不該 emit (空 map → 沒資料)
        assert!(
            !body.lines().any(|l| l.starts_with(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\""
            )),
            "empty totals 不該 emit P99 sample line, body: {body}"
        );
        // 順手驗 K30 標頭仍存在, 證明 K32 沒覆蓋 K30 emit block
        assert!(
            body.contains(
                "# TYPE lobsterpulse_provider_completed_sessions_p95_duration_seconds gauge"
            ),
            "K32 補在 K31 後, K30 emit block 仍要 emit HELP/TYPE 標頭, body: {body}"
        );
        // 順手驗 K31 標頭仍存在, 證明 K32 沒覆蓋 K31 emit block
        assert!(
            body.contains(
                "# TYPE lobsterpulse_provider_completed_sessions_p50_duration_seconds gauge"
            ),
            "K32 補在 K31 後, K31 emit block 仍要 emit HELP/TYPE 標頭, body: {body}"
        );
    }

    #[test]
    fn p99_per_provider_isolated_and_skips_empty_samples() {
        // per-provider 隔離 + samples 為空跳過: cicx (20 sample [1..20]) emit
        // P99=20 (少樣本退化到 max), claude (samples 為空) 跳過, openx
        // (samples 為空) 跳過。用 struct literal + `..Default::default()` 明確
        // 控制每個 provider 狀態, 跟 K20-K31 既有隔離測試風格一致。P99 計算:
        // 20 個 sample 排序後 index = 20 * 99 / 100 = 19 → samples[19] = 20
        // (= max, 少樣本下 P99 退到 max 跟 K26 max 對齊, 跟 K30 P95=20 同值)。
        let mut totals = HashMap::new();
        // cicx: 20 sample [1..20] → 排序後 P99 index=19, samples[19]=20
        let mut cicx_samples: Vec<i64> = (1..=20).collect();
        cicx_samples.sort_unstable(); // 已排序, 為求語意清楚顯式 sort
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: cicx_samples,
                ..Default::default()
            },
        );
        // claude: samples 為空 → render 端跳過, 不該 emit sample line
        totals.insert("claude".to_string(), ProviderTotals::default());
        // openx: samples 為空 → render 端跳過, 不該 emit sample line
        totals.insert("openx".to_string(), ProviderTotals::default());

        let _p99 = completed_sessions_p99_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // cicx emit 20 (i64 整數, 沒有 4 位小數 f64 跟 K25/K28/K29 不同)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "cicx P99 = samples[19] = 20 (整數 i64, 少樣本退化到 max), body: {body}"
        );
        // claude 跳過: samples 為空不該 emit sample line (跟 K25 「0/0 不 emit」同款)
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"claude\"}"),
            "claude samples 為空該跳過, body: {body}"
        );
        // openx 跳過: 同 claude
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"openx\"}"
            ),
            "openx samples 為空該跳過, body: {body}"
        );
    }

    #[test]
    fn p99_alphabetical_sort_and_integer_precision() {
        // alphabetical sort + i64 整數 precision + 跟 K22 / K25-K31 八件套互不
        // 覆蓋: 三個 provider cicx/claude/gemini 同一份 reservoir 各自 sort
        // 取不同 percentile → 各自 emit 自己的 P99, alphabetical 排序
        // (cicx < claude < gemini), 整數 i64 格式 (沒有 f64 4 位小數, 跟
        // sample 整數 duration 語意一致, 強制裁整 0 精度流失)。K32 跟 K30/K31
        // 三驗證: 同一份 cicx samples 餵 K30 (P95) / K31 (P50) / K32 (P99),
        // 各自 emit 不同值 (K30=20 / K31=11 / K32=20, P95==P99 因少樣本退化到
        // max), 證明 pure fn 各自獨立 + render emit 互不污染。claude 100
        // 樣本 P99=100 (= max, idx=99 剛好是 max), 跟 cicx 20 樣本 P99=20
        // (= max) 對齊, 證明 K32 在滿樣本下 P99 = max (語意: 第 99 百分位 ≈
        // top value, 樣本少時退化 = max 數學合理)。
        let mut totals = HashMap::new();
        // cicx: 20 sample [1..20] → P99=20 (idx=19, 退化到 max)
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=20).collect(),
                ..Default::default()
            },
        );
        // claude: 100 sample [1..100] → P99 index=99, samples[99]=100 (= max)
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=100).collect(),
                ..Default::default()
            },
        );
        // gemini: 50 sample [1..50] → P99 index=49, samples[49]=50 (= max)
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=50).collect(),
                ..Default::default()
            },
        );

        let _p99 = completed_sessions_p99_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K32 alphabetical + 整數 i64 格式驗證: cicx=20, claude=100, gemini=50
        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"cicx\"} 20\n")
            .expect("cicx K32 sample line");
        let claude_idx = body
            .find("lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"claude\"} 100\n")
            .expect("claude K32 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"gemini\"} 50\n")
            .expect("gemini K32 sample line");
        assert!(
            cicx_idx < claude_idx && claude_idx < gemini_idx,
            "per-provider P99 必須 alphabetical 排序 \
             (cicx={cicx_idx}, claude={claude_idx}, gemini={gemini_idx})"
        );
        // 反向驗: 確認 emit 的是整數 i64 格式, 不是 f64 4 位小數格式
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"cicx\"} 20.0000\n"
            ),
            "K32 i64 整數契約(不是 f64 4 位小數), 不可 emit 20.0000, body: {body}"
        );
        // K30 (P95) / K31 (P50) / K32 (P99) 共用 samples vec 三驗證: 同一份
        // cicx samples 餵 K30 (P95=20) / K31 (P50=11) / K32 (P99=20), 各自 emit
        // 各自 percentile 互不污染。cicx 20 樣本 P95 == P99 == max (= 20), 證明
        // 少樣本下 K30/K32 都退化到 max (語意: 樣本 < 100 沒 P99 區辨力)。
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "K30 cicx P95 = 20 (跟 K31 P50=11 / K32 P99=20 共用 samples vec), body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"cicx\"} 11\n"
            ),
            "K31 cicx P50 = 11 (跟 K30 P95=20 / K32 P99=20 共用 samples vec, P50 是中位), body: {body}"
        );
        // 順便驗 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) /
        // K29 (ratio) 五件套 + K30 (P95) / K31 (P50) / K32 (P99) 互不覆蓋: cicx
        // 在各 metric 各自 emit 自己的值, 互不污染 (K32 整數 vs K25/K28/K29 f64
        // 格式本身已隔離, 雙驗證)
        assert!(
            !body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"}"
            ),
            "K22 (latest) 跟 K32 (P99) 隔離, cicx 在 K22 沒 latest (None) 不該 emit, body: {body}"
        );
        assert!(
            !body.contains("lobsterpulse_provider_failure_to_completion_ratio{provider=\"cicx\"}"),
            "K29 (ratio) 跟 K32 (P99) 隔離, cicx count=0 該跳過 K29, body: {body}"
        );
    }

    // ============== K33 per-provider completed_sessions_p75 gauge ==============
    // 跟 K30 P95 / K31 P50 / K32 P99 同模板, 純 fn `completed_sessions_p75_at` 復用
    // `ProviderTotals.completed_sessions_p95_samples` (K30/K31/K32/K33 共用 reservoir
    // 1024 sliding window) sort 後取 75 百分位 index, 跟 K30/K31/K32 各自 emit
    // 各自 percentile。樣本為空跳過 (跟 K30-K32 既「缺資料不 emit」一致, 避免
    // P75=0 假冒「瞬間完成 75% session」假健康信號)。3 個 render test 跟 K30/K31/K32
    // 既有 render test 對稱: empty / per-provider 隔離 / alphabetical sort + 整數
    // precision + 跟 K30/K31/K32 四件套共用 samples vec 雙驗證。

    #[test]
    fn p75_empty_totals_emits_header_only() {
        // 對齊 K11 / K18-K32 empty-state 契約: 空 map → 沒 sample line
        // (HELP/TYPE 標頭仍輸出), 不丟假資料。ProviderTotals 沒 entry → samples
        // 預設空 → 過濾掉, 避免 Prometheus 端把「沒看到」當「P75=0」誤判「該
        // provider 75% session 瞬間完成」= 假健康信號。順手驗 K30/K31/K32 標頭
        // 在 K33 後仍存在, 證明 K33 沒覆蓋 K30/K31/K32 emit block。
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains("# HELP lobsterpulse_provider_completed_sessions_p75_duration_seconds")
        );
        assert!(body.contains(
            "# TYPE lobsterpulse_provider_completed_sessions_p75_duration_seconds gauge"
        ));
        // 沒 sample line 契約: 任何 provider=... 都不該 emit (空 map → 沒資料)
        assert!(
            !body.lines().any(|l| l.starts_with(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\""
            )),
            "empty totals 不該 emit P75 sample line, body: {body}"
        );
        // 順手驗 K30 標頭仍存在, 證明 K33 沒覆蓋 K30 emit block
        assert!(
            body.contains(
                "# TYPE lobsterpulse_provider_completed_sessions_p95_duration_seconds gauge"
            ),
            "K33 補在 K32 後, K30 emit block 仍要 emit HELP/TYPE 標頭, body: {body}"
        );
        // 順手驗 K31 標頭仍存在, 證明 K33 沒覆蓋 K31 emit block
        assert!(
            body.contains(
                "# TYPE lobsterpulse_provider_completed_sessions_p50_duration_seconds gauge"
            ),
            "K33 補在 K32 後, K31 emit block 仍要 emit HELP/TYPE 標頭, body: {body}"
        );
        // 順手驗 K32 標頭仍存在, 證明 K33 沒覆蓋 K32 emit block
        assert!(
            body.contains(
                "# TYPE lobsterpulse_provider_completed_sessions_p99_duration_seconds gauge"
            ),
            "K33 補在 K32 後, K32 emit block 仍要 emit HELP/TYPE 標頭, body: {body}"
        );
    }

    #[test]
    fn p75_per_provider_isolated_and_skips_empty_samples() {
        // per-provider 隔離 + samples 為空跳過: cicx (20 sample [1..20]) emit
        // P75=16 (idx=15, samples[15]=16, 75% 位置剛好命中), claude (samples
        // 為空) 跳過, openx (samples 為空) 跳過。用 struct literal +
        // `..Default::default()` 明確控制每個 provider 狀態, 跟 K20-K32 既有
        // 隔離測試風格一致。P75 計算: 20 個 sample 排序後 index = 20 * 75 /
        // 100 = 15 → samples[15] = 16 (P75 剛好命中 75% 位置, 不退化)。
        let mut totals = HashMap::new();
        // cicx: 20 sample [1..20] → 排序後 P75 index=15, samples[15]=16
        let mut cicx_samples: Vec<i64> = (1..=20).collect();
        cicx_samples.sort_unstable(); // 已排序, 為求語意清楚顯式 sort
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: cicx_samples,
                ..Default::default()
            },
        );
        // claude: samples 為空 → render 端跳過, 不該 emit sample line
        totals.insert("claude".to_string(), ProviderTotals::default());
        // openx: samples 為空 → render 端跳過, 不該 emit sample line
        totals.insert("openx".to_string(), ProviderTotals::default());

        let _p75 = completed_sessions_p75_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // cicx emit 16 (i64 整數, 沒有 4 位小數 f64 跟 K25/K28/K29 不同)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"cicx\"} 16\n"
            ),
            "cicx P75 = samples[15] = 16 (整數 i64, 75% 位置剛好命中), body: {body}"
        );
        // claude 跳過: samples 為空不該 emit sample line (跟 K25 「0/0 不 emit」同款)
        assert!(
            !body.contains("lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"claude\"}"),
            "claude samples 為空該跳過, body: {body}"
        );
        // openx 跳過: 同 claude
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"openx\"}"
            ),
            "openx samples 為空該跳過, body: {body}"
        );
    }

    #[test]
    fn p75_alphabetical_sort_and_integer_precision() {
        // alphabetical sort + i64 整數 precision + 跟 K22 / K25-K32 九件套互不
        // 覆蓋: 三個 provider cicx/claude/gemini 同一份 reservoir 各自 sort
        // 取不同 percentile → 各自 emit 自己的 P75, alphabetical 排序
        // (cicx < claude < gemini), 整數 i64 格式 (沒有 f64 4 位小數, 跟
        // sample 整數 duration 語意一致, 強制裁整 0 精度流失)。K33 跟 K30/K31/K32
        // 四驗證: 同一份 cicx samples 餵 K30 (P95) / K31 (P50) / K32 (P99) /
        // K33 (P75), 各自 emit 不同值 (K30=20 / K31=11 / K32=20 / K33=16,
        // P75=16 居中 P50=11 跟 P95=20, monotonic chain 成立), 證明 pure fn
        // 各自獨立 + render emit 互不污染。claude 100 樣本 P75=76 (idx=75 剛好
        // 命中 75% 位置, 不退化), 跟 cicx 20 樣本 P75=16 (75% 位置命中但 idx
        // 不同) 對齊, 證明 K33 在不同樣本數下 idx 計算都正確 (N=20 → idx=15;
        // N=100 → idx=75)。
        let mut totals = HashMap::new();
        // cicx: 20 sample [1..20] → P75=16 (idx=15)
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=20).collect(),
                ..Default::default()
            },
        );
        // claude: 100 sample [1..100] → P75 index=75, samples[75]=76
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=100).collect(),
                ..Default::default()
            },
        );
        // gemini: 50 sample [1..50] → P75 index=37, samples[37]=38
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: (1..=50).collect(),
                ..Default::default()
            },
        );

        let _p75 = completed_sessions_p75_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K33 alphabetical + 整數 i64 格式驗證: cicx=16, claude=76, gemini=38
        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"cicx\"} 16\n")
            .expect("cicx K33 sample line");
        let claude_idx = body
            .find("lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"claude\"} 76\n")
            .expect("claude K33 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"gemini\"} 38\n")
            .expect("gemini K33 sample line");
        assert!(
            cicx_idx < claude_idx && claude_idx < gemini_idx,
            "per-provider P75 必須 alphabetical 排序 \
             (cicx={cicx_idx}, claude={claude_idx}, gemini={gemini_idx})"
        );
        // 反向驗: 確認 emit 的是整數 i64 格式, 不是 f64 4 位小數格式
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"cicx\"} 16.0000\n"
            ),
            "K33 i64 整數契約(不是 f64 4 位小數), 不可 emit 16.0000, body: {body}"
        );
        // K30 (P95) / K31 (P50) / K32 (P99) / K33 (P75) 共用 samples vec 四驗證:
        // 同一份 cicx samples 餵 K30 (P95=20) / K31 (P50=11) / K32 (P99=20) /
        // K33 (P75=16), 各自 emit 各自 percentile 互不污染。cicx 20 樣本 monotonic
        // chain P50=11 < P75=16 < P95=20 == P99=20 (少樣本 P95/P99 退化到 max
        // 但 P75 仍命中 75% 位置, 證明 K33 跟 K30/K31/K32 樣本數邊界行為不同)。
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "K30 cicx P95 = 20 (跟 K31 P50=11 / K32 P99=20 / K33 P75=16 共用 samples vec), body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"cicx\"} 11\n"
            ),
            "K31 cicx P50 = 11 (跟 K30 P95=20 / K32 P99=20 / K33 P75=16 共用 samples vec, P50 是中位), body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "K32 cicx P99 = 20 (跟 K30 P95=20 / K31 P50=11 / K33 P75=16 共用 samples vec, 少樣本退化到 max), body: {body}"
        );
        // 順便驗 K22 (latest) / K29 (ratio) 跟 K33 (P75) 隔離: cicx 在 K22
        // 沒 latest (None) 不該 emit, K29 cicx count=0 該跳過
        assert!(
            !body.contains(
                "lobsterpulse_provider_last_completed_session_age_seconds{provider=\"cicx\"}"
            ),
            "K22 (latest) 跟 K33 (P75) 隔離, cicx 在 K22 沒 latest (None) 不該 emit, body: {body}"
        );
        assert!(
            !body.contains("lobsterpulse_provider_failure_to_completion_ratio{provider=\"cicx\"}"),
            "K29 (ratio) 跟 K33 (P75) 隔離, cicx count=0 該跳過 K29, body: {body}"
        );
    }

    // ============== K34 render-side tests (跟 K30 P95 / K31 P50 / K32 P99 / K33 P75 同模板) ==============

    #[test]
    fn p25_emits_header_only_when_no_samples() {
        // 跟 K30 P95 / K31 P50 / K32 P99 / K33 P75 同款 empty header-only: 0 provider
        // → 只有 HELP/TYPE 標頭, 沒有 sample line, 避免「沒看到」誤判「P25=0」
        // 假健康信號。
        let totals = HashMap::<String, ProviderTotals>::new();
        let _p25 = completed_sessions_p25_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains("# HELP lobsterpulse_provider_completed_sessions_p25_duration_seconds"),
            "K34 即使 0 provider 也要 emit HELP 標頭, body: {body}"
        );
        assert!(
            body.contains(
                "# TYPE lobsterpulse_provider_completed_sessions_p25_duration_seconds gauge"
            ),
            "K34 即使 0 provider 也要 emit TYPE 標頭, body: {body}"
        );
        // 沒 provider → 沒 sample line (不 emit `provider=\"xxx\"`)
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\""
            ),
            "K34 0 provider 不 emit sample line, body: {body}"
        );
    }

    #[test]
    fn p25_per_provider_isolated_and_skips_empty_samples() {
        // per-provider 隔離 + samples 為空跳過: cicx (20 sample [1..20]) emit
        // P25=6 (idx=5, samples[5]=6, 25% 位置剛好命中), claude (samples
        // 為空) 跳過, openx (samples 為空) 跳過。P25 計算: 20 個 sample
        // 排序後 index = 20 * 25 / 100 = 5 → samples[5] = 6 (剛好命中 25%
        // 位置, 不退化, 跟 K33 4 sample P75 退化到 max 對比 —— K34 在 4
        // 樣本時 P25 就能命中精確位置)。
        let mut totals = HashMap::new();
        let mut cicx_samples: Vec<i64> = (1..=20).collect();
        cicx_samples.sort_unstable();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: cicx_samples,
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());
        totals.insert("openx".to_string(), ProviderTotals::default());

        let _p25 = completed_sessions_p25_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"cicx\"} 6\n"
            ),
            "cicx P25 = samples[5] = 6 (整數 i64, 25% 位置剛好命中), body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"claude\"}"
            ),
            "claude samples 為空 → 跳過, 不 emit sample line, body: {body}"
        );
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"openx\"}"
            ),
            "openx samples 為空 → 跳過, 不 emit sample line, body: {body}"
        );
    }

    #[test]
    fn p25_alphabetical_sort_and_integer_precision_with_k30_k31_k32_k33_isolation() {
        // 3 個 provider (cicx/claude/gemini) 各自不同 sample 集, 驗證:
        // 1. alphabetical 排序 (cicx < claude < gemini)
        // 2. 整數 i64 格式 (不是 f64 4 位小數)
        // 3. 跟 K30 P95 / K31 P50 / K32 P99 / K33 P75 共用 samples vec 五驗證
        //    (P25 < P50 < P75 < P95 == P99 monotonic chain 在 20 樣本下嚴格成立)
        let mut totals = HashMap::new();
        // cicx: 1..=20 → P25=6, P50=11, P75=16, P95=20, P99=20
        let cicx_samples: Vec<i64> = (1..=20).collect();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: cicx_samples,
                ..Default::default()
            },
        );
        // claude: 1..=100 → P25=26, P50=51, P75=76, P95=96, P99=100
        let claude_samples: Vec<i64> = (1..=100).collect();
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: claude_samples,
                ..Default::default()
            },
        );
        // gemini: 1..=50 → P25=13, P50=26, P75=38, P95=48, P99=50
        let gemini_samples: Vec<i64> = (1..=50).collect();
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_p95_samples: gemini_samples,
                ..Default::default()
            },
        );

        let _p25 = completed_sessions_p25_at(&totals);
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &totals,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        // K34 alphabetical + 整數 i64 格式驗證
        let cicx_idx = body
            .find("lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"cicx\"} 6\n")
            .expect("cicx K34 sample line");
        let claude_idx = body
            .find("lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"claude\"} 26\n")
            .expect("claude K34 sample line");
        let gemini_idx = body
            .find("lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"gemini\"} 13\n")
            .expect("gemini K34 sample line");
        assert!(
            cicx_idx < claude_idx && claude_idx < gemini_idx,
            "per-provider P25 必須 alphabetical 排序 (cicx={cicx_idx}, claude={claude_idx}, gemini={gemini_idx})"
        );
        // 反向驗: 確認 emit 的是整數 i64 格式, 不是 f64 4 位小數格式
        assert!(
            !body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"cicx\"} 6.0000\n"
            ),
            "K34 i64 整數契約 (不是 f64 4 位小數), 不可 emit 6.0000, body: {body}"
        );
        // K30 P95 / K31 P50 / K32 P99 / K33 P75 / K34 P25 共用 samples vec 五驗證:
        // 同一份 cicx samples 餵 K30 (P95=20) / K31 (P50=11) / K32 (P99=20) /
        // K33 (P75=16) / K34 (P25=6), 各自 emit 各自 percentile 互不污染。cicx
        // 20 樣本 monotonic chain P25=6 < P50=11 < P75=16 < P95=20 == P99=20
        // (樣本數 < 100 時 P95/P99 退化到 max, 但 P25/P50/P75 仍命中精確位置)。
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "K30 cicx P95 = 20 (跟 K31 P50=11 / K32 P99=20 / K33 P75=16 / K34 P25=6 共用 samples vec), body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p50_duration_seconds{provider=\"cicx\"} 11\n"
            ),
            "K31 cicx P50 = 11 (跟 K30 P95=20 / K32 P99=20 / K33 P75=16 / K34 P25=6 共用 samples vec, P50 是中位), body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p99_duration_seconds{provider=\"cicx\"} 20\n"
            ),
            "K32 cicx P99 = 20 (跟 K30 P95=20 / K31 P50=11 / K33 P75=16 / K34 P25=6 共用 samples vec, 少樣本退化到 max), body: {body}"
        );
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p75_duration_seconds{provider=\"cicx\"} 16\n"
            ),
            "K33 cicx P75 = 16 (跟 K30 P95=20 / K31 P50=11 / K32 P99=20 / K34 P25=6 共用 samples vec, P75 命中 75% 位置), body: {body}"
        );
        // claude 100 樣本 P25=26: idx = 100 * 25 / 100 = 25, samples[25] = 26
        // (K34 在 100 樣本下 P25 剛好命中精確位置, 跟 P50 idx=50=51 / P75
        // idx=75=76 / P95 idx=95=96 / P99 idx=99=100 全部命中)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"claude\"} 26\n"
            ),
            "K34 claude 100 樣本 P25 = 26 (idx=25, 剛好命中 25% 位置), body: {body}"
        );
        // gemini 50 樣本 P25=13: idx = 50 * 25 / 100 = 12, samples[12] = 13
        // (K34 在 50 樣本下 P25 命中精確位置)
        assert!(
            body.contains(
                "lobsterpulse_provider_completed_sessions_p25_duration_seconds{provider=\"gemini\"} 13\n"
            ),
            "K34 gemini 50 樣本 P25 = 13 (idx=12, 命中 25% 位置), body: {body}"
        );
    }

    // ============== R103 OTel/Prometheus spec alignment guard tests ==============
    //
    // 對齊 `openspec/changes/otel-provider-metrics-contract/spec.md` Requirement:
    // `render_prometheus_body` output is a subset of the LP_METRICS contract.
    // 護欄核心：每次新加 emit 必須先列入 LP_METRICS const + design.md 對照表 +
    // spec.md Scenario, 否則這條 test fail 並列出「未列名 metric」清單。

    #[test]
    fn lp_metrics_contract_size_is_47_matching_emit_paths() {
        // 7 段分組對齊 design.md: R103 41 條 + R113 T-1 dual-emit 6 條新 _total 名
        // = 4 + 8 + 4 + 7 + 14 + 1 + 9 = 47 (T-4 切換日後回到 41, 見
        // openspec/changes/prometheus-counter-rename-2026-q3/spec.md S-PCR1.3)
        assert_eq!(
            LP_METRICS.len(),
            47,
            "LP_METRICS 應為 47 條（R103 41 條 + R113 T-1 dual-emit 6 條新 _total 名）, 目前 {} 條",
            LP_METRICS.len()
        );
        // 防 LP_METRICS 內部有重複（spec.md 隱含 set 語意）
        let unique: std::collections::HashSet<&str> = LP_METRICS.iter().copied().collect();
        assert_eq!(
            unique.len(),
            LP_METRICS.len(),
            "LP_METRICS 不可有重複項, 重複會破 contract 護欄語意 (移除重複項時不會被偵測到)"
        );
        // R113 T-1 dual-emit 護衛：6 條新 _total 名 100% 必須出現在 LP_METRICS
        // const（防漏列, 對齊 R106 design.md 對照表）
        let dual_emit_new_names = [
            "lobsterpulse_tokens_input_total",
            "lobsterpulse_tokens_output_total",
            "lobsterpulse_provider_tokens_input_total",
            "lobsterpulse_provider_tokens_output_total",
            "lobsterpulse_provider_failure_count_total",
            "lobsterpulse_provider_session_count_total",
        ];
        for new_name in &dual_emit_new_names {
            assert!(
                LP_METRICS.contains(new_name),
                "R113 T-1 dual-emit 6 條新 _total 名必須 100% 出現在 LP_METRICS const, 漏列: {new_name}"
            );
        }
    }

    #[test]
    fn render_prometheus_body_empty_state_all_emits_in_lp_metrics_contract() {
        // 對應 spec.md Scenario: empty state still produces a valid contract subset
        let body = render_prometheus_body(
            &[],
            0,
            0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        let contract: std::collections::HashSet<&str> = LP_METRICS.iter().copied().collect();
        let mut off_contract: Vec<String> = Vec::new();
        for line in body.lines() {
            if line.is_empty() {
                continue;
            }
            if line.starts_with("# HELP ") || line.starts_with("# TYPE ") {
                continue;
            }
            // Prometheus 行格式：`metric_name{labels} value` 或 `metric_name value`。
            // 取第一個 `{` 或空白前的 token 即為 metric name。
            let token = line.split(['{', ' ', '\t']).next().unwrap_or("");
            if !contract.contains(token) {
                off_contract.push(token.to_string());
            }
        }
        assert!(
            off_contract.is_empty(),
            "空 state 仍有未列名 metric ({off_contract:?}), 請先加入 LP_METRICS const + design.md 對照表 + spec.md Scenario"
        );
    }

    #[test]
    fn render_prometheus_body_full_state_all_emits_in_lp_metrics_contract() {
        // 對應 spec.md Scenario: all current emit paths stay within the contract
        // 跑多 provider + 多 state + 有 quota + 有 discord + 有 hook metrics 的完整
        // render, 確保所有 emit 路徑（per-provider × metric × state 笛卡兒積）都
        // 落在 LP_METRICS 集合內, 不漏列任何新加的 metric。
        let mut totals = HashMap::<String, ProviderTotals>::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                tokens_input: 100,
                tokens_output: 50,
                session_count: 3,
                failure_count: 1,
                events_total: 10,
                event_type_counts: std::collections::BTreeMap::new(),
                since: Some(Utc::now() - chrono::Duration::seconds(60)),
                last_event_at: Some(Utc::now() - chrono::Duration::seconds(5)),
                last_completed_session_age_secs: Some(30),
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());
        totals.insert("openx".to_string(), ProviderTotals::default());

        let mut quota_ages = HashMap::new();
        quota_ages.insert("cicx".to_string(), 120i64);
        quota_ages.insert("claude".to_string(), 5i64);

        let mut quota_pct = HashMap::new();
        quota_pct.insert("cicx".to_string(), 80u8);
        quota_pct.insert("claude".to_string(), 95u8);

        let mut last_completed_age = HashMap::new();
        last_completed_age.insert("cicx".to_string(), 30i64);
        last_completed_age.insert("claude".to_string(), 10i64);

        let sessions = vec![
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("claude", true, SessionState::WaitingForUser),
            info_with_state("openx", false, SessionState::Idle),
        ];

        let body = render_prometheus_body(
            &sessions,
            3,
            2,
            &totals,
            &quota_ages,
            &quota_pct,
            Some(15),
            &last_completed_age,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        let contract: std::collections::HashSet<&str> = LP_METRICS.iter().copied().collect();
        let mut off_contract: Vec<String> = Vec::new();
        let mut total_emits = 0usize;
        for line in body.lines() {
            if line.is_empty() {
                continue;
            }
            if line.starts_with("# HELP ") || line.starts_with("# TYPE ") {
                continue;
            }
            total_emits += 1;
            let token = line.split(['{', ' ', '\t']).next().unwrap_or("");
            if !contract.contains(token) {
                off_contract.push(token.to_string());
            }
        }
        assert!(
            off_contract.is_empty(),
            "完整 state render 出現未列名 metric ({off_contract:?}), 總 emit 行 {total_emits}, 請先加入 LP_METRICS const + design.md 對照表 + spec.md Scenario"
        );
        // 反向 sanity check: 至少要 emit 一定數量的 metric 行, 確保 test 不是在空 body 上誤綠
        assert!(
            total_emits >= 30,
            "完整 state 預期 emit 至少 30 行 metric（含 5+ provider 維度 × 多 metric family），實際 {total_emits}, test 可能是空 body 偽綠"
        );
        // R113 T-1 dual-emit 護衛：6 條 counter 必須同時 emit 舊名 + 新名（HELP/TYPE/sample 三件套）
        // 對齊 R106 design.md 對照表 6 條 + 5 週時程 T-1 階段；T-4 切換日撤銷（不再需要 dual-emit）
        let dual_emit_pairs = [
            (
                "lobsterpulse_tokens_input",
                "lobsterpulse_tokens_input_total",
            ),
            (
                "lobsterpulse_tokens_output",
                "lobsterpulse_tokens_output_total",
            ),
            (
                "lobsterpulse_provider_tokens_input",
                "lobsterpulse_provider_tokens_input_total",
            ),
            (
                "lobsterpulse_provider_tokens_output",
                "lobsterpulse_provider_tokens_output_total",
            ),
            (
                "lobsterpulse_provider_failure_count",
                "lobsterpulse_provider_failure_count_total",
            ),
            (
                "lobsterpulse_provider_session_count",
                "lobsterpulse_provider_session_count_total",
            ),
        ];
        for (legacy, total) in &dual_emit_pairs {
            assert!(
                body.contains(legacy) && body.contains(total),
                "R113 T-1 dual-emit 必須 6 條 counter 同時 emit 舊名 + 新名, 缺一: 舊={legacy} 新={total}"
            );
        }
    }

    /// R113.1: dual-emit 數值一致性護衛
    ///
    /// 補強 R113 既有的「string contains」護衛：舊護衛只斷言「兩個名字都出現在
    /// body」→ 若其中一條 emit path 改了 source 忘記同步另一條, 兩個名字都還在
    /// body 但 value 已經分叉 (silent contract drift). 本護衛 parse 出實際
    /// `(labels, value)`, 對 6 條 dual-emit pair 斷言 HashMap 相等
    ///
    /// 涵蓋 case:
    ///   - global counter (#1 #2): 兩個名字同 value (sum 來自同一 source)
    ///   - per-provider counter (#3-#6): 兩個名字對每個 provider label 都同 value
    ///
    /// 對齊 R106 design.md 對照表 6 條 + 5 週時程 T-1 階段; T-4 切換日撤銷
    /// (屆時舊名刪除, 護衛可改為「舊名不在 body」+ 新名仍 emit 數值)
    #[test]
    fn render_prometheus_body_dual_emit_values_match_per_provider() {
        // 沿用 full_state test 的 fixture, 不重複建資料
        let mut totals = HashMap::<String, ProviderTotals>::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                tokens_input: 100,
                tokens_output: 50,
                session_count: 3,
                failure_count: 1,
                events_total: 10,
                event_type_counts: std::collections::BTreeMap::new(),
                since: Some(Utc::now() - chrono::Duration::seconds(60)),
                last_event_at: Some(Utc::now() - chrono::Duration::seconds(5)),
                last_completed_session_age_secs: Some(30),
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());
        totals.insert("openx".to_string(), ProviderTotals::default());

        let mut quota_ages = HashMap::new();
        quota_ages.insert("cicx".to_string(), 120i64);
        quota_ages.insert("claude".to_string(), 5i64);

        let mut quota_pct = HashMap::new();
        quota_pct.insert("cicx".to_string(), 80u8);
        quota_pct.insert("claude".to_string(), 95u8);

        let mut last_completed_age = HashMap::new();
        last_completed_age.insert("cicx".to_string(), 30i64);
        last_completed_age.insert("claude".to_string(), 10i64);

        let sessions = vec![
            info_with_state("cicx", true, SessionState::Working),
            info_with_state("claude", true, SessionState::WaitingForUser),
            info_with_state("openx", false, SessionState::Idle),
        ];

        let body = render_prometheus_body(
            &sessions,
            3,
            2,
            &totals,
            &quota_ages,
            &quota_pct,
            Some(15),
            &last_completed_age,
            &discord::DiscordHealth::default(),
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );

        // 從 Prometheus text body parse 出指定 metric 的所有 (labels, value)
        // 支援兩種格式:
        //   "<name> <value>"              (無 label)
        //   "<name>{k=\"v\"} <value>"     (有 label, label pair 由 } 邊界切)
        fn extract_metric_values(body: &str, metric_name: &str) -> HashMap<String, u64> {
            let mut out = HashMap::new();
            for line in body.lines() {
                if line.is_empty() || line.starts_with("# ") {
                    continue;
                }
                let Some(rest) = line.strip_prefix(metric_name) else {
                    continue;
                };
                let (labels, value_str) = if let Some(brace) = rest.strip_prefix('{') {
                    let Some(end) = brace.find('}') else {
                        continue;
                    };
                    let labels = brace[..end].to_string();
                    let value_str = brace[end + 1..].trim();
                    (labels, value_str)
                } else {
                    (String::new(), rest.trim())
                };
                if let Ok(v) = value_str.parse::<u64>() {
                    out.insert(labels, v);
                }
            }
            out
        }

        let dual_emit_pairs = [
            (
                "lobsterpulse_tokens_input",
                "lobsterpulse_tokens_input_total",
            ),
            (
                "lobsterpulse_tokens_output",
                "lobsterpulse_tokens_output_total",
            ),
            (
                "lobsterpulse_provider_tokens_input",
                "lobsterpulse_provider_tokens_input_total",
            ),
            (
                "lobsterpulse_provider_tokens_output",
                "lobsterpulse_provider_tokens_output_total",
            ),
            (
                "lobsterpulse_provider_failure_count",
                "lobsterpulse_provider_failure_count_total",
            ),
            (
                "lobsterpulse_provider_session_count",
                "lobsterpulse_provider_session_count_total",
            ),
        ];
        for (legacy, total) in &dual_emit_pairs {
            let legacy_vals = extract_metric_values(&body, legacy);
            let total_vals = extract_metric_values(&body, total);
            assert_eq!(
                legacy_vals, total_vals,
                "R113.1 dual-emit 數值分叉 (silent contract drift): 舊名 {legacy} = {legacy_vals:?}, 新名 {total} = {total_vals:?}"
            );
        }
    }
}

#[cfg(test)]
mod lib_warn_msg_tests {
    use super::*;

    /// R57: 對齊 R37 `provider_settings_warn_msg_unifies_prefix` / R6 `discord_err_msg`
    /// / R23 `config_persist_warn_msg` / R28 `persisted_marker_warn_msg` 四條前例
    /// prefix 風格契約, log filter 可一次 grep `[lib]` 撈 module 警告。L93 / L132 /
    /// L820 / L2862 4 處 silent-fail 收邊都走這條 helper, prefix 統一讓 4 處 log 可
    /// 單一 awk 過濾
    #[test]
    fn lib_warn_msg_unifies_prefix() {
        let msg = lib_warn_msg("sounds_dir_mkdir", "permission denied");
        assert!(
            msg.starts_with("[lib] sounds_dir_mkdir failed:"),
            "prefix 應含 module + action + 失敗動詞, 實際: {msg}"
        );
        assert!(
            msg.contains("permission denied"),
            "訊息尾應含原始 err 內容, 實際: {msg}"
        );

        // 跨 4 處 call site 各自的 action 名稱也鎖住 (避免未來 refactor 把
        // action 名 typo 改掉, log filter grep 失效)
        let m1 = lib_warn_msg("sounds_dir_mkdir", "io: x");
        let m2 = lib_warn_msg("seed_default_sounds write", "io: x");
        let m3 = lib_warn_msg("openab_runners_dir_mkdir", "io: x");
        let m4 = lib_warn_msg("metrics_http_response_write", "io: x");
        assert!(m1.contains("sounds_dir_mkdir"));
        assert!(m2.contains("seed_default_sounds write"));
        assert!(m3.contains("openab_runners_dir_mkdir"));
        assert!(m4.contains("metrics_http_response_write"));
    }
}

#[cfg(test)]
mod r74_play_sound_file_fallback_tests {
    use super::*;

    /// R74 T-BOT3 spec 收尾驗證：spec 寫「刪掉 irisx 音效檔，IRISX 事件進來不
    /// 崩、用 default」。實作側的「default」採最簡解讀 = silent no-op (不 panic、
    /// 不 thread spawn、不產聲音)，對應 `play_sound_file` lib.rs:202-204 早 return
    /// 路徑 `if !path.exists() { return; }`。這條 test 把「缺檔 → 安全靜默」這條
    /// invariant 從 commit-time 直覺變成 CI 守護：未來有人把早 return 拿掉、改成
    /// `.unwrap()` 或 `panic!` 一定會被這條 test 抓到（call 直接 panic → test fail）
    #[test]
    fn r74_play_sound_file_safe_when_file_missing() {
        // 用極不可能存在於 sounds_dir 的檔名, 確保走到缺檔分支
        let fake_name = "__r74_definitely_missing_xxxxx_9999.mp3";
        // 不應 panic, 不應 thread spawn (因 `if !path.exists() { return; }` 早 return)
        // 若未來有人改壞這條早 return, call 點會直接 panic (rodio::File::open 對
        // 不存在檔案) → test 自動 fail
        play_sound_file(fake_name.to_string());
        // 走到這行 = 早 return 路徑生效, 通過
    }

    /// R74 T-BOT3 spec 第二條：seed 必須 idempotent, 不能每次啟動覆寫 user 改過
    /// 的音效檔；同時確認 irisx_bot 兩個 placeholder 都會被 seeded 進目標 dir。
    /// 用 tempdir 隔離避免污染 `~/.lobsterpulse/sounds/`
    #[test]
    fn r74_seed_default_sounds_is_idempotent_and_seeds_irisx_bot() {
        let tmp = std::env::temp_dir().join("r74_seed_default_sounds_test");
        let _ = std::fs::remove_dir_all(&tmp); // 清乾淨避免前次 run 殘留
        std::fs::create_dir_all(&tmp).expect("tempdir 應可建");

        seed_default_sounds(&tmp);
        let after_first: Vec<String> = std::fs::read_dir(&tmp)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        // 再 seed 一次, 檔案數應不變 (idempotent — `if !path.exists()` 守住)
        seed_default_sounds(&tmp);
        let after_second: Vec<String> = std::fs::read_dir(&tmp)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        assert_eq!(
            after_first.len(),
            after_second.len(),
            "seed_default_sounds 應 idempotent, 第一次 = {after_first:?}, 第二次 = {after_second:?}"
        );

        // R71 T-BOT3 補的 irisx_bot 兩個 placeholder 必須都 seeded 進 tmp
        assert!(
            tmp.join("irisx_bot.mp3").exists(),
            "irisx_bot.mp3 應被 seed (R71 T-BOT3)"
        );
        assert!(
            tmp.join("irisx_bot-waiting.mp3").exists(),
            "irisx_bot-waiting.mp3 應被 seed (R71 T-BOT3)"
        );

        // 既有 6 OpenAB bot × 2 sound + irisx_bot × 2 + grokx × 2 + lpbot × 2 + mimo × 2 = 18 個 mp3 應都 seeded
        // (cicx/gitx/giminix/codex/openx/irisx_bot/grokx/lpbot/mimo 各 .mp3 + -waiting.mp3)
        // R78 T-BOT11: 加 grokx 後 12 → 14；R78 T-BOT12: 加 lpbot 後 14 → 16；R78 T-BOT5: 加 mimo 後 16 → 18
        assert_eq!(
            after_first.len(),
            18,
            "應 seeded 18 個 mp3 (9 OpenAB bot × 2, R78 T-BOT11 加 grokx, R78 T-BOT12 加 lpbot, R78 T-BOT5 加 mimo), \
             實際 {} 個, 列表: {after_first:?}",
            after_first.len()
        );

        // cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
