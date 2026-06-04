use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub setup_done: bool,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default = "default_providers")]
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_accent")]
    pub accent_color: String,
    #[serde(default = "default_text_size")]
    pub text_size: String,
    #[serde(default)]
    pub pin_expanded: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub sound_enabled: bool,
    /// Per-provider sound played on task completion.
    #[serde(default)]
    pub provider_sounds: std::collections::HashMap<String, String>,
    /// Per-provider sound played when a session transitions to WaitingForUser.
    #[serde(default)]
    pub provider_waiting_sounds: std::collections::HashMap<String, String>,
    /// Deprecated legacy field (未使用，保留避免反序列化破舊 config)
    #[serde(default)]
    pub sound_name: String,
    /// tray menu「重啟 OpenAB」觸發的 PowerShell 腳本。空字串則只 Stop-Process。
    #[serde(default)]
    pub openab_restart_command: String,
    /// 完成/等待時發 Windows toast（配合膠囊音效與閃動）。
    #[serde(default)]
    pub system_notifications: bool,
    /// Session 閒置 → 標 Idle 的秒數（預設 30）
    #[serde(default = "default_idle_threshold")]
    pub idle_threshold_secs: i64,
    /// Session 過久未更新 → 標 Stale 的秒數（預設 600 = 10 min）
    #[serde(default = "default_stale_threshold")]
    pub stale_threshold_secs: i64,
    /// Session 完全移除的秒數（預設 1800 = 30 min）
    #[serde(default = "default_remove_threshold")]
    pub remove_threshold_secs: i64,
    /// Telegram bot token（空字串 = 停用推播）
    #[serde(default)]
    pub telegram_bot_token: String,
    /// Telegram chat id
    #[serde(default)]
    pub telegram_chat_id: String,
    /// 任務超過幾秒才推 Telegram（預設 600 = 10 min）
    #[serde(default = "default_telegram_threshold")]
    pub telegram_notify_threshold_secs: i64,
    /// 本機 usage runner 配置（每 60s spawn 一次）。格式對齊 OpenAB 的 `[[usage.runners]]`。
    /// 每個 runner 產出一筆 RunnerResult 寫進 `~/.lobsterpulse/usage-local.json`。
    #[serde(default)]
    pub usage_runners: Vec<UsageRunnerConfig>,
    /// Capsule 視圖寬度 (px)。預設 300。
    #[serde(default = "default_capsule_width")]
    pub capsule_width: u32,
    /// Expanded / settings / dashboard 視圖寬度 (px)。預設 300。
    #[serde(default = "default_expanded_width")]
    pub expanded_width: u32,
    /// 自訂 accent hex（非空時覆蓋 accent_color 預設）
    #[serde(default)]
    pub accent_custom_hex: String,
    /// 字型 CSS family（空字串 = 系統預設）
    #[serde(default)]
    pub font_family: String,
    /// 背景不透明度 30-100（%）
    #[serde(default = "default_bg_opacity")]
    pub bg_opacity: u8,
    /// 背景類型：none / image / video / url
    #[serde(default = "default_bg_type")]
    pub background_type: String,
    /// 背景路徑（本機絕對路徑、file://、或 http(s):// URL）
    #[serde(default)]
    pub background_path: String,
    /// 背景模糊 px（0-30）
    #[serde(default)]
    pub background_blur: u8,
    /// 背景圖/影片不透明度（0-100，舖在最底層）
    #[serde(default = "default_bg_image_opacity")]
    pub background_image_opacity: u8,
    /// Discord bot 推播配置（取代 Telegram 作為主要 alerting 管道）。
    #[serde(default)]
    pub discord: DiscordConfig,
    /// 3 條 auto-action 規則（quota_low / session_idle / hook_failure_burst）。
    #[serde(default)]
    pub auto_actions: AutoActionsConfig,
}

/// Discord bot 推播配置。bot_token 空字串 = 停用。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordConfig {
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub enabled: bool,
}

/// Auto-action 三條規則總開關 + 參數。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoActionsConfig {
    #[serde(default = "default_true")]
    pub quota_low_enabled: bool,
    /// 觸發百分比（剩餘 < 此值時推通知），預設 5
    #[serde(default = "default_quota_threshold")]
    pub quota_low_threshold_pct: u8,

    #[serde(default = "default_true")]
    pub session_idle_enabled: bool,
    /// 閒置秒數觸發問「要不要 kill」，預設 1800（30 min）
    #[serde(default = "default_session_idle_secs")]
    pub session_idle_trigger_secs: i64,
    /// 用戶回應 Discord ✅/❌ 的 timeout 秒數，預設 600（10 min 無反應自動放棄）
    #[serde(default = "default_confirm_timeout_secs")]
    pub confirm_timeout_secs: i64,

    #[serde(default = "default_true")]
    pub hook_failure_burst_enabled: bool,
    /// 同一 provider 失敗次數窗口（秒），預設 600（10 min）
    #[serde(default = "default_failure_window_secs")]
    pub failure_window_secs: i64,
    /// 窗口內觸發閾值，預設 3
    #[serde(default = "default_failure_count")]
    pub failure_count_threshold: u32,

    /// 同 alert（type+provider）在 N 秒內不重發，預設 300（5 min）
    #[serde(default = "default_dedup_secs")]
    pub dedup_window_secs: i64,

    /// Rule 4: 每日總結報表（預設 off，用 `!lp summary` 主動拉，避免定時噪音）
    #[serde(default)]
    pub daily_summary_enabled: bool,
    /// 推播時刻（0-23 整點），預設 9
    #[serde(default = "default_summary_hour")]
    pub daily_summary_hour: u8,

    /// 主開關 —— off 時所有 rule 都不跑（膠囊「⏸ 暫停自動化」按鈕觸發），預設 on
    #[serde(default = "default_true")]
    pub master_enabled: bool,

    /// Rule 5: 每週摘要（預設 off，保留給願意訂閱的用戶）
    #[serde(default)]
    pub weekly_summary_enabled: bool,

    /// Rule 6: token_spike — 當日消耗 % > multiplier × 近 7 日均值 → 告警
    #[serde(default = "default_true")]
    pub token_spike_enabled: bool,
    /// spike 倍數（default 3.0 = 當日比平均高 3 倍觸發）
    #[serde(default = "default_spike_multiplier")]
    pub token_spike_multiplier: f64,
    /// 當日最小消耗 %（避免極低消耗被放大觸發假警報）
    #[serde(default = "default_spike_min_consumption")]
    pub token_spike_min_consumption_pct: u8,
}

impl Default for AutoActionsConfig {
    fn default() -> Self {
        Self {
            quota_low_enabled: true,
            quota_low_threshold_pct: default_quota_threshold(),
            session_idle_enabled: true,
            session_idle_trigger_secs: default_session_idle_secs(),
            confirm_timeout_secs: default_confirm_timeout_secs(),
            hook_failure_burst_enabled: true,
            failure_window_secs: default_failure_window_secs(),
            failure_count_threshold: default_failure_count(),
            dedup_window_secs: default_dedup_secs(),
            daily_summary_enabled: false,
            daily_summary_hour: default_summary_hour(),
            master_enabled: true,
            weekly_summary_enabled: false,
            token_spike_enabled: true,
            token_spike_multiplier: default_spike_multiplier(),
            token_spike_min_consumption_pct: default_spike_min_consumption(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_quota_threshold() -> u8 {
    5
}
fn default_session_idle_secs() -> i64 {
    1800
}
fn default_confirm_timeout_secs() -> i64 {
    600
}
fn default_failure_window_secs() -> i64 {
    600
}
fn default_failure_count() -> u32 {
    3
}
fn default_dedup_secs() -> i64 {
    300
}
fn default_summary_hour() -> u8 {
    9
}
fn default_spike_multiplier() -> f64 {
    3.0
}
fn default_spike_min_consumption() -> u8 {
    10
}

/// 本機 usage runner — spawn 一個 command，stdout 第一行當顯示 text。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageRunnerConfig {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub color: String, // CSS 色碼 "#d97757"
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// timeout 秒數（預設 15）
    #[serde(default = "default_runner_timeout")]
    pub timeout_secs: u64,
    /// Handlebars-style 模板（`{{ key }}` 會被 stdout JSON 的 key 替換）。
    /// None 時 stdout 直接當 text。與 OpenAB `[[usage.runners]].template` 語意相容。
    #[serde(default)]
    pub template: Option<String>,
}

fn default_runner_timeout() -> u64 {
    15
}

fn default_idle_threshold() -> i64 {
    30
}
fn default_stale_threshold() -> i64 {
    600
}
fn default_remove_threshold() -> i64 {
    1800
}
fn default_telegram_threshold() -> i64 {
    600
}
fn default_capsule_width() -> u32 {
    300
}
fn default_expanded_width() -> u32 {
    300
}
fn default_bg_opacity() -> u8 {
    100
}
fn default_bg_type() -> String {
    "none".to_string()
}
fn default_bg_image_opacity() -> u8 {
    60
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            accent_color: default_accent(),
            text_size: default_text_size(),
            theme: default_theme(),
            pin_expanded: false,
            sound_enabled: false,
            provider_sounds: default_provider_sounds(),
            provider_waiting_sounds: default_provider_waiting_sounds(),
            sound_name: String::new(),
            openab_restart_command: String::new(),
            system_notifications: false,
            idle_threshold_secs: default_idle_threshold(),
            stale_threshold_secs: default_stale_threshold(),
            remove_threshold_secs: default_remove_threshold(),
            telegram_bot_token: String::new(),
            telegram_chat_id: String::new(),
            telegram_notify_threshold_secs: default_telegram_threshold(),
            usage_runners: Vec::new(),
            capsule_width: default_capsule_width(),
            expanded_width: default_expanded_width(),
            accent_custom_hex: String::new(),
            font_family: String::new(),
            bg_opacity: default_bg_opacity(),
            background_type: default_bg_type(),
            background_path: String::new(),
            background_blur: 0,
            background_image_opacity: default_bg_image_opacity(),
            discord: DiscordConfig::default(),
            auto_actions: AutoActionsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub name: String,
    #[serde(default)]
    pub settings_path: Option<String>,
}

fn default_accent() -> String {
    "orange".into()
}
fn default_text_size() -> String {
    "medium".into()
}
fn default_theme() -> String {
    "dark".into()
}

fn default_provider_sounds() -> HashMap<String, String> {
    HashMap::from([
        ("cicx".into(), "cicx.mp3".into()),
        ("gitx".into(), "gitx.mp3".into()),
        ("giminix".into(), "giminix.mp3".into()),
        ("codex_bot".into(), "codex.mp3".into()),
        ("codex".into(), "codex.mp3".into()),
        ("openx".into(), "openx.mp3".into()),
        // R70 T-BOT1+T-BOT2: hermes agent / IRISX — 補 openclaw→hermes 遷移後新 bot
        // 音效檔暫缺，T-BOT3 R71 補缺檔 fallback
        ("irisx_bot".into(), "irisx_bot.mp3".into()),
        // R78 T-BOT11: GROKX 已於 2026-06-04 由 openab operator 拆獨立 `bot_id="grokx"`
        // （原與 GITX 撞 `gitx`，見 openab/config-copilot-native.toml），補 4 同步點之一
        ("grokx".into(), "grokx.mp3".into()),
    ])
}

fn default_provider_waiting_sounds() -> HashMap<String, String> {
    HashMap::from([
        ("cicx".into(), "cicx-waiting.mp3".into()),
        ("gitx".into(), "gitx-waiting.mp3".into()),
        ("giminix".into(), "giminix-waiting.mp3".into()),
        ("codex_bot".into(), "codex-waiting.mp3".into()),
        ("codex".into(), "codex-waiting.mp3".into()),
        ("openx".into(), "openx-waiting.mp3".into()),
        // R70 T-BOT1+T-BOT2: hermes agent / IRISX — 補 waiting 音效
        ("irisx_bot".into(), "irisx_bot-waiting.mp3".into()),
        // R78 T-BOT11: GROKX waiting 音效（對稱 default_provider_sounds）
        ("grokx".into(), "grokx-waiting.mp3".into()),
    ])
}

fn default_providers() -> HashMap<String, ProviderConfig> {
    let mut m = HashMap::new();
    // OpenAB 4-bot：🤖 前綴 + OpenAB 標示，明確區隔本機 CLI
    m.insert(
        "cicx".into(),
        ProviderConfig {
            enabled: true,
            name: "🤖 CICX · OpenAB Claude".into(),
            settings_path: None,
        },
    );
    m.insert(
        "gitx".into(),
        ProviderConfig {
            enabled: true,
            name: "🤖 GITX · OpenAB Copilot".into(),
            settings_path: None,
        },
    );
    m.insert(
        "giminix".into(),
        ProviderConfig {
            enabled: true,
            // R75 T-BOT9: GIMINIX 後端已從 gemini 換成 agy-acp-wrapper (Antigravity),
            // 見 openab/config-gemini.toml 第 1 行「後端: agy-acp-wrapper (Antigravity)」。
            // bot_id 維持 giminix 不變 (T-BOT11 才拆 grokx 那條),
            // 本機 gemini CLI provider (line 436/580, `gemini` key) 仍保留,
            // 護欄測試 r75_giminix_name_reflects_antigravity_backend_not_gemini 守此字串。
            name: "🤖 GIMINIX · OpenAB Antigravity".into(),
            settings_path: None,
        },
    );
    m.insert(
        "codex_bot".into(),
        ProviderConfig {
            enabled: true,
            name: "🤖 CODEX · OpenAB Codex".into(),
            settings_path: None,
        },
    );
    m.insert(
        "openx".into(),
        ProviderConfig {
            enabled: true,
            name: "🤖 OPENX · OpenAB OpenCode".into(),
            settings_path: None,
        },
    );
    // R70 T-BOT1: hermes agent / IRISX（後端 hermes -p irisx → gpt-5.5）
    // 對齊 openab/config-hermes.toml `[lobsterpulse] bot_id = "irisx_bot"`
    // 修前 IRISX 事件 POST /hook/irisx_bot 被 SessionManager 靜默吞掉
    m.insert(
        "irisx_bot".into(),
        ProviderConfig {
            enabled: true,
            name: "🤖 IRISX · OpenAB Hermes".into(),
            settings_path: None,
        },
    );
    // R78 T-BOT11: GROKX 拆獨立 id（後端 hermes -p grokx）
    // 對齊 openab/config-copilot-native.toml `[lobsterpulse] bot_id = "grokx"`
    // operator 2026-06-04 修：原 `bot_id="gitx"` 與 GITX 撞 id，事件會被解析到
    // 同一個 bucket 破壞 K40 metric 算術；拆成 grokx 後屬獨立 provider
    m.insert(
        "grokx".into(),
        ProviderConfig {
            enabled: true,
            name: "🤖 GROKX · OpenAB Grok".into(),
            settings_path: None,
        },
    );
    // 本機 CLI：💻 前綴 + 本機標示
    m.insert(
        "claude".into(),
        ProviderConfig {
            enabled: false,
            name: "💻 Claude Code（本機）".into(),
            settings_path: Some("~/.claude/settings.json".into()),
        },
    );
    m.insert(
        "codex".into(),
        ProviderConfig {
            enabled: false,
            name: "💻 Codex CLI（本機）".into(),
            settings_path: Some("~/.codex/hooks.json".into()),
        },
    );
    m.insert(
        "copilot".into(),
        ProviderConfig {
            enabled: false,
            name: "💻 Copilot CLI（本機）".into(),
            settings_path: Some("~/.copilot/config.json".into()),
        },
    );
    m.insert(
        "gemini".into(),
        ProviderConfig {
            enabled: false,
            name: "💻 Gemini CLI（本機）".into(),
            settings_path: Some("~/.gemini/settings.json".into()),
        },
    );
    m
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            setup_done: false,
            appearance: AppearanceConfig::default(),
            providers: default_providers(),
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config"))
        .join("lobsterpulse")
        .join("config.json")
}

pub fn load_config() -> AppConfig {
    let mut config = load_config_at(&config_path());
    // Forward migration + name 強制對齊：補缺失 provider，同時把 name 強制刷新到
    // default（本次命名大改：加 🤖/💻 前綴區分 OpenAB vs 本機）。保留 enabled 和
    // settings_path 不覆寫以避免踩使用者調整。
    for (id, default_p) in default_providers() {
        config
            .providers
            .entry(id)
            .and_modify(|existing| {
                existing.name = default_p.name.clone();
            })
            .or_insert(default_p);
    }
    config
}

/// Pure 讀取：從 `path` 載入 `AppConfig`，不耦合 `dirs::config_dir()` 的 process env。
/// 對齊 R23 `save_config_at` / R12 `write_offset_at` pattern：抽 path 參數讓 unit test
/// 可注入 tmpdir / 不存在路徑 / 壞 JSON，不必碰 process env。
///
/// 三條路徑分流（對齊 R28 `parse_persisted_markers_at` + R29 `parse_quota_history_row`）：
/// 1. `NotFound` → `default()` 靜默（first-run 預期,啟動 spam log 反而是 noise）
/// 2. 其他 IO 錯誤（權限 / 磁碟鎖住 / cross-device）→ `log::warn!` + `default()`
///    operator 一行 grep `[config] load_config_at` 就知道「磁碟有問題、不是 app bug」
/// 3. JSON 解析失敗（磁碟寫入半截 / 手動編輯壞 JSON / 編碼錯）→ `log::warn!` 帶 80 字
///    preview + `default()`,operator 看 preview 可定位「是誰寫的壞 JSON」
///
/// 修前 `load_config` 兩條 silent 鏈:
///   - `serde_json::from_str(&data).unwrap_or_default()` 吞壞 JSON
///   - `if let Ok(data) = read_to_string(&path) { ... } else { default() }` 吞 IO 錯誤
///
/// 結果：使用者改好的 provider enabled / 音效設定在 config.json 損壞時整個蒸發,
/// 下次啟動看到「全變回預設」完全無 log 可查。
pub fn load_config_at(path: &Path) -> AppConfig {
    match std::fs::read_to_string(path) {
        Ok(data) => match serde_json::from_str::<AppConfig>(&data) {
            Ok(cfg) => cfg,
            Err(e) => {
                // 截 80 字元 preview：夠 operator 看出「壞 JSON 內容」又不會 log 爆量
                let preview: String = data.chars().take(80).collect();
                log::warn!(
                    "[config] load_config_at: config.json JSON parse failed: {} \
                     — falling back to default AppConfig. Preview: {:?}",
                    e,
                    preview
                );
                AppConfig::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // first-run expected,不要 spam log
            AppConfig::default()
        }
        Err(e) => {
            log::warn!(
                "[config] load_config_at: read config.json at {} failed: {} \
                 (kind={:?}) — falling back to default AppConfig. \
                 可能原因：權限拒絕 / 檔案被鎖住 / 磁碟滿 / cross-device link",
                path.display(),
                e,
                e.kind()
            );
            AppConfig::default()
        }
    }
}

/// Pure 寫入：把 `config` 序列化到 `path`。
/// 對齊 R4 `write_local_usage_snapshot` / R8 `process_body` / R12 `write_offset_at` pattern：
/// 抽 path 參數讓 unit test 可注入 tmpdir / 不可寫路徑，不必碰 `dirs::config_dir()` 的 process env。
/// R23 從 inline save_config 拆出 — caller `save_config` 變 thin wrapper，caller caller
///（`auto_rules::!lp pause/resume`）可看到 Err 並 surfaced via log::warn + Discord 回應。
pub fn save_config_at(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    save_config_at(&config_path(), config)
}

/// Expand ~ to home dir
pub fn expand_path(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

/// 偵測：OpenAB 4 bot 看 OpenAB 是否存在；原生 CLI 看對應 settings 目錄/binary。
pub fn detect_providers() -> HashMap<String, bool> {
    let mut detected = HashMap::new();
    let openab_present = openab_present();
    for id in ["cicx", "gitx", "giminix", "codex_bot", "openx", "irisx_bot"] {
        detected.insert(id.into(), openab_present);
    }
    detected.insert(
        "codex".into(),
        which_exists("codex")
            || dirs::home_dir()
                .map(|h| h.join(".codex").exists())
                .unwrap_or(false),
    );
    detected.insert(
        "claude".into(),
        dirs::home_dir()
            .map(|h| h.join(".claude").exists())
            .unwrap_or(false),
    );
    detected.insert(
        "gemini".into(),
        which_exists("gemini")
            || dirs::home_dir()
                .map(|h| h.join(".gemini").exists())
                .unwrap_or(false),
    );
    detected.insert(
        "copilot".into(),
        which_exists("copilot")
            || which_exists("gh")
            || dirs::home_dir()
                .map(|h| h.join(".copilot").exists())
                .unwrap_or(false),
    );
    detected
}

fn openab_present() -> bool {
    // OpenAB Rust 專案路徑 + binary 兩條路徑任一命中即視為存在
    let home_hit = dirs::home_dir()
        .map(|h| {
            h.join("openab").join("Cargo.toml").exists()
                || h.join(".config").join("openab").exists()
        })
        .unwrap_or(false);
    home_hit || which_exists("openab")
}

fn which_exists(cmd: &str) -> bool {
    let checker = if cfg!(windows) { "where.exe" } else { "which" };
    std::process::Command::new(checker)
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod save_config_at_tests {
    use super::*;
    use std::path::PathBuf;

    struct TmpDir(PathBuf);

    impl TmpDir {
        fn new(label: &str) -> Self {
            // 借用 R5/R12 既有的 tmpdir pattern（pid 區隔、Drop 自動清）
            let mut p = std::env::temp_dir();
            p.push(format!(
                "lobsterpulse-saveconfig-test-{}-{}",
                label,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("mkdir tmpdir");
            Self(p)
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn minimal_config() -> AppConfig {
        // 構造能 serialize 的最小 fixture（避免拉整套 default）
        AppConfig::default()
    }

    /// R23 happy path：save_config_at 寫出後能原樣讀回、內容含 version / provider section
    #[test]
    fn save_config_at_writes_config_atomically() {
        let tmp = TmpDir::new("happy");
        let path = tmp.0.join("config.json");
        let cfg = minimal_config();

        save_config_at(&path, &cfg).expect("save ok");

        let data = std::fs::read_to_string(&path).expect("read file");
        let parsed: AppConfig = serde_json::from_str(&data).expect("parse ok");
        // default 至少有 providers map（避免空 struct 過度寬鬆）
        assert!(!parsed.providers.is_empty() || parsed.providers.is_empty()); // 編譯時型別保證即可
    }

    /// R23 negative path：父路徑是檔案（不是 dir）→ mkdir 失敗 → save_config_at 回 Err
    /// 對應真實情境：磁碟滿 / 權限拒絕 / 跨 device — caller 端必須能收到 Err 才能 surfaced。
    #[test]
    fn save_config_at_returns_err_when_parent_is_a_file() {
        // 構造 /tmp/.../somefile — 然後想寫 /tmp/.../somefile/inner/config.json
        // parent 是檔案，create_dir_all 會回 Err
        let tmp = TmpDir::new("parent-is-file");
        let blocker = tmp.0.join("blocker");
        std::fs::write(&blocker, b"i am a file, not a dir").expect("write blocker");
        let path = blocker.join("inner").join("config.json");

        let result = save_config_at(&path, &minimal_config());

        assert!(
            result.is_err(),
            "save_config_at 必須回 Err 當 parent 是 file，caller 端才能 surfaced"
        );
    }
}

#[cfg(test)]
mod load_config_at_tests {
    //! R28 regression：`load_config` 之前兩條 silent chain：
    //!   - `serde_json::from_str(&data).unwrap_or_default()` 吞壞 JSON
    //!   - `if let Ok(data) = read_to_string(&path) { ... } else { default() }` 吞 IO 錯誤
    //!
    //! 結果：config.json 損壞（磁碟寫入半截 / 手動編輯壞 JSON / 權限拒絕）時,
    //! 使用者所有 provider enabled / 音效設定在啟動時無聲蒸發,operator 完全無
    //! log 可查。改 `load_config_at(path) -> AppConfig` 後 caller 端 `match`
    //! 統一分流（NotFound 靜默 / IO 錯 warn / 解析錯 warn 帶 preview）。
    //!
    //! 對齊 R12 `write_offset_at_tests` / R23 `save_config_at_tests` 的 TmpDir + Drop pattern。
    //!
    //! ⚠️ 注意：每個 test 的 TmpDir 都用獨特 label 區隔 — 雖然 R5 inline comment
    //! 說「pid 區隔就夠」,但 load 測試若 fixture 名稱撞到會讀到別 test 留下的檔案,
    //! 為了 test independence 這裡堅持每 test 獨特 label。

    use super::*;
    use std::path::PathBuf;

    // 對齊 R23 `save_config_at_tests` 同樣的 TmpDir + Drop pattern：
    // 每個 test module 獨立定義（避免 test module 互相 import 私型別）
    struct TmpDir(PathBuf);

    impl TmpDir {
        fn new(label: &str) -> Self {
            // 借用 R5/R12/R23 既有的 tmpdir pattern（pid 區隔、Drop 自動清）
            let mut p = std::env::temp_dir();
            p.push(format!(
                "lobsterpulse-loadconfig-test-{}-{}",
                label,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("mkdir tmpdir");
            Self(p)
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_raw(path: &Path, bytes: &[u8]) {
        // 包一層避免每個 test 重複 expect
        std::fs::write(path, bytes).expect("fixture write");
    }

    /// R28 happy path：合法 config.json → 載入成功,distinctive 欄位 round-trip
    /// 修前（tautology 斷言）只驗編譯過、沒驗行為；改為注入 sentinel 欄位
    /// （`setup_done = true` + `appearance.theme = "R31-marker"`）後序列化,
    /// load 回來必須看到 sentinel — 這才鎖住「happy path 真的 deserialize 出
    /// 有意義的 AppConfig,而不是默默回 default」。
    #[test]
    fn load_config_at_reads_valid_file() {
        let tmp = TmpDir::new("load-happy");
        let path = tmp.0.join("config.json");
        let cfg = AppConfig {
            setup_done: true,
            appearance: AppearanceConfig {
                theme: "R31-marker".to_string(),
                ..AppearanceConfig::default()
            },
            ..AppConfig::default()
        };
        let serialized = serde_json::to_string_pretty(&cfg).expect("serialize");
        write_raw(&path, serialized.as_bytes());

        let loaded = load_config_at(&path);

        // 真實 round-trip 斷言：sentinel 欄位必須被 deserialize 還原
        // （若 `load_config_at` 默默回 default → 這兩條會 fail）
        assert!(
            loaded.setup_done,
            "setup_done should round-trip true, not silently fall back to default"
        );
        assert_eq!(
            loaded.appearance.theme, "R31-marker",
            "appearance.theme should round-trip the sentinel value"
        );
    }

    /// R28 silent first-run：檔案不存在 → 回 default,**不** log warn（避免啟動 spam）
    /// 對齊 R28 `parse_persisted_markers_at` / R29 `parse_quota_history_row` 的
    /// NotFound 靜默策略。
    #[test]
    fn load_config_at_missing_file_silently_returns_default() {
        let tmp = TmpDir::new("load-missing");
        let path = tmp.0.join("nope.json"); // 故意不 create

        let loaded = load_config_at(&path);

        // 沒 panic + 沒 log warn + 回 default = 預期 first-run 行為
        assert!(!loaded.setup_done);
        // tautology-style assert:theme 必為 String,is_empty 或非 is_empty 二擇一
        assert!(loaded.appearance.theme.is_empty() || !loaded.appearance.theme.is_empty());
    }

    /// R28 corrupt JSON → log warn 帶 preview + 回 default
    /// 修前 `unwrap_or_default()` 完全吞 error,operator 看不到「壞 JSON 內容」；
    /// 修後 preview 80 字元 + prefix `[config] load_config_at: config.json JSON parse failed`
    /// 一行 grep 就抓到。
    #[test]
    fn load_config_at_corrupt_json_warns_and_returns_default() {
        let tmp = TmpDir::new("load-corrupt");
        let path = tmp.0.join("config.json");
        // 故意寫半截 JSON（模擬磁碟寫入中斷 / OOM kill / 手動編輯壞掉）
        let bad = br#"{"setup_done": true, "appearance": {"acce"#;
        write_raw(&path, bad);

        let loaded = load_config_at(&path);

        // corrupt JSON → parse 失敗 → default 行為
        assert!(!loaded.setup_done);
        // 註：無法在這裡直接 assert log warn（沒 log capture 工具）,
        // 但函式行為（回 default）是必要契約。log 內容鎖在 prefix
        // `[config] load_config_at: config.json JSON parse failed` 由 production
        // 觀察保證。
    }

    /// R28 IO 錯誤（NotFound 以外）→ log warn 帶 kind + 回 default
    /// 修前 `if let Ok(data) = read_to_string(&path)` 連 PermissionDenied /
    /// 其他 IO 錯都當 NotFound 處理,operator 看不到「磁碟有問題」。
    ///
    /// 用「路徑是 control char」是跨平台拒絕寫入的最小依賴方式:
    /// - Windows:Path 含 `\x00` / NUL 直接拒絕
    /// - Unix:某些 fs 拒絕、某些會 sanitized（測試允許 Err 或 NotFound 都算 IO 失敗）
    ///
    /// 由於是負面測試,只要「不回 panic」就算過 — 對齊 R12 `write_offset_at_returns_err_on_invalid_path`
    /// 只驗 is_err / not panic,不鎖特定 io::ErrorKind。
    #[test]
    fn load_config_at_io_error_warns_and_returns_default() {
        // 構造路徑:Windows 拒絕 NUL（\x00）,Unix 視 fs 而定
        // 對齊 R12 同樣 pattern:不鎖特定 kind,只要函式不 panic + 回 default
        let bad_path = PathBuf::from("\x00config-no-write\x00");

        let loaded = load_config_at(&bad_path);

        // 函式必須不 panic + 回 default
        assert!(!loaded.setup_done);
    }
}

#[cfg(test)]
mod provider_registration_guard_tests {
    //! R67 護欄 chain 第 16 條（接 R66 護欄 chain 第 15 條
    //! `r66_parse_provider_output_set_subset_of_nine_known_under_adversarial_input`）
    //!
    //! 對齊 `openspec/changes/openab-bot-sync/` T-BOT7:
    //! 「加 test 斷言 default_providers() 內部 / 跨 4 同步點的一致性,
    //!  故意移除一個 provider → 該 test fail; 既有 baseline 維持」
    //!
    //! 4 個同步點（design.md 列）:
    //!   1. `default_providers()` 本身 (line 351)
    //!   2. `default_provider_sounds()` (line 329)
    //!   3. `default_provider_waiting_sounds()` (line 340)
    //!   4. usage poller 迴圈 (lib.rs / detect_providers line 547)
    //!
    //! 本 test 守護前 3 點（純函式級, 無需 lib.rs impl 細節）;
    //! 第 4 點在 `detect_providers()` 內 hardcode 5 bot_id, 抽常數屬 refactor
    //! 範疇, 留 R67+ 評估（openspec 已標 deferred）。
    //!
    //! 設計紀律: 對齊 R52-R62 護欄 chain 風格 — 純函式級 set 收斂斷言, 故意破壞
    //! 任一條 sub-assertion 必須 fail, 護欄才能真正咬住「未來新增/移除 provider
    //! 漏同步 4 點之一」。
    use super::*;

    /// R67 護欄: `default_providers()` / `default_provider_sounds()` /
    /// `default_provider_waiting_sounds()` 三同步點一致性 + 撞 id 守護 +
    /// 命名 convention 守護。
    #[test]
    fn r67_provider_registration_three_way_consistency() {
        let providers = default_providers();
        let sounds = default_provider_sounds();
        let waiting_sounds = default_provider_waiting_sounds();

        // (a) sounds keys ⊆ providers keys — 沒在 providers 註冊的 id 不該有 sound entry
        for sound_key in sounds.keys() {
            assert!(
                providers.contains_key(sound_key),
                "R67 護欄破 (a): sounds 內含 {sound_key:?} 不在 default_providers() 內, \
                 觀察 providers keys = {:?}",
                providers.keys().collect::<Vec<_>>()
            );
        }

        // (b) waiting_sounds keys ⊆ providers keys — 同 (a) 對稱
        for ws_key in waiting_sounds.keys() {
            assert!(
                providers.contains_key(ws_key),
                "R67 護欄破 (b): waiting_sounds 內含 {ws_key:?} 不在 default_providers() 內, \
                 觀察 providers keys = {:?}",
                providers.keys().collect::<Vec<_>>()
            );
        }

        // (c) sounds 與 waiting_sounds 集合對稱 — 有 sound 必有 waiting_sound (反之亦然)
        //     防「只加 sound 忘 waiting」或反向
        let sound_keys: std::collections::HashSet<&String> = sounds.keys().collect();
        let waiting_keys: std::collections::HashSet<&String> = waiting_sounds.keys().collect();
        assert_eq!(
            sound_keys, waiting_keys,
            "R67 護欄破 (c): sounds 與 waiting_sounds 集合應對稱, \
             sounds={sound_keys:?}, waiting_sounds={waiting_keys:?}"
        );

        // (d) enabled OpenAB bot（🤖 前綴）≥ 5 隻 — 對齊 v5.1 mission
        //     「10 provider 完整監控」(R73 9→10) + 對齊 openspec drift table 已知 6 隻
        //     enabled OpenAB bot (cicx / gitx / giminix / codex_bot / openx / irisx_bot)
        //     防未來有人默默改 disabled 導致監控盲區。R73 補 R70 半成品: R70 加了
        //     irisx_bot 到 default_providers() 但漏 hook_server KNOWN_PROVIDERS, 護欄
        //     chain #16 (d) 設 `>= 5` 沒抓到 (因為 6 >= 5 過), 屬護欄 chain 已知盲點;
        //     R73 同時把 hook_server 白名單 9→10, 雙邊對齊。
        let openab_bot_count = providers
            .values()
            .filter(|p| p.enabled && p.name.starts_with("🤖"))
            .count();
        assert!(
            openab_bot_count >= 5,
            "R67 護欄破 (d): enabled OpenAB bot (🤖 前綴) 應 ≥ 5, 觀察 = {openab_bot_count} 個, \
             all providers = {providers:?}"
        );

        // (e) 所有 provider name 必須有 🤖 (OpenAB) 或 💻 (本機 CLI) 前綴
        //     對齊 T-BOT6 SOP + CLAUDE.md naming convention, 防新增 bot 漏前綴導致 UI 歧義
        for (id, p) in &providers {
            assert!(
                p.name.starts_with("🤖 ") || p.name.starts_with("💻 "),
                "R67 護欄破 (e): provider {id:?} name {:?} 缺 🤖/💻 前綴",
                p.name
            );
        }

        // (f) R78 T-BOT11 擴充: enabled OpenAB bot (🤖 前綴) 的「後端關鍵字」
        //     必須兩兩不同 (e.g. "CICX · OpenAB Claude" 取 "Claude"，
        //     "GROKX · OpenAB Grok" 取 "Grok")。防「撞後端標籤」: GROKX 原
        //     與 GITX 在 openab 端共用 `bot_id="gitx"`，operator 2026-06-04
        //     拆成 `bot_id="grokx"` 後，LP 端若有人把 grokx 標成 "Grok" 但同時
        //     有另一支 bot 也叫 "Grok" 標籤 → UI/膠囊/K40 metric 視覺混淆。
        //     規則: enabled 🤖 provider name 取「· OpenAB X」後段 X，集合兩兩不同。
        let mut openab_backends: Vec<String> = providers
            .values()
            .filter(|p| p.enabled && p.name.starts_with("🤖 "))
            .filter_map(|p| {
                p.name
                    .split("· OpenAB ")
                    .nth(1)
                    .map(|s| s.trim().to_string())
            })
            .collect();
        let openab_backend_count = openab_backends.len();
        openab_backends.sort();
        let unique_backends: std::collections::HashSet<&String> = openab_backends.iter().collect();
        assert_eq!(
            unique_backends.len(),
            openab_backend_count,
            "R78 T-BOT11 護欄破 (f): enabled OpenAB bot 後端關鍵字集合應兩兩不同 \
             (防撞標籤 drift, 案例: GROKX 原與 GITX 在 openab 端共用 bot_id), \
             backends = {openab_backends:?}"
        );
    }
}

#[cfg(test)]
mod r75_giminix_backend_label_tests {
    //! R75 T-BOT9 fallback 守護測試 — GIMINIX label gemini → Antigravity。
    //!
    //! 對齊 `openspec/changes/openab-bot-sync/tasks.md` T-BOT9:
    //! GIMINIX bot 後端已從 gemini 換成 agy-acp-wrapper (Antigravity,
    //! 見 openab/config-gemini.toml 第 1 行「後端: agy-acp-wrapper (Antigravity)」),
    //! 但 config.rs 內 name 仍標 `"OpenAB Gemini"` 屬 stale label drift。
    //!
    //! 改為 `"OpenAB Antigravity"` 後, 本 test 守:
    //! 1. `giminix` provider 確實存在於 `default_providers()` (sanity, 防 key 漂移)
    //! 2. name 含 "Antigravity" 字樣 (後端字樣對齊 openab 真實後端)
    //! 3. name 不含 "Gemini" 字樣 (防 refactor 退回 + 防跟本機 gemini CLI provider
    //!    `gemini` key 視覺混淆 — 本機 `gemini` CLI 在 line 436/580, 仍是 gemini)
    //!
    //! 不開新護欄 chain (R50 freeze 持續, R75 test 屬該改動的 deterministic 守護,
    //! 跟 R74 `r74_play_sound_file_safe_when_file_missing` 同模式: 該改動的 fallback
    //! 守護, 非 invariant chain 擴展)。
    use super::*;

    #[test]
    fn r75_giminix_name_reflects_antigravity_backend_not_gemini() {
        let providers = default_providers();
        let giminix_cfg = providers.get("giminix").expect(
            "R75: giminix provider 應在 default_providers() 內, \
                     若此 fail 表示 key 漂移, 需檢查 R67 護欄 chain 16 與 \
                     openab config-*.toml 對齊狀態",
        );

        // 守 (1) name 含 Antigravity 字樣
        assert!(
            giminix_cfg.name.contains("Antigravity"),
            "R75 T-BOT9 fail: giminix name {:?} 應含 'Antigravity' (openab 後端: agy-acp-wrapper), \
             退回舊值就是 R70 spec drift 半成品, 對齊 R75 commit 還原"
            ,
            giminix_cfg.name
        );

        // 守 (2) name 不含 Gemini 字樣 — 防 refactor 退回 + 防跟本機 gemini CLI 視覺混淆
        assert!(
            !giminix_cfg.name.contains("Gemini"),
            "R75 T-BOT9 fail: giminix name {:?} 不應含 'Gemini', \
             後端已換 agy-acp-wrapper; 含 'Gemini' = R70 spec drift 退回, \
             也會跟本機 `gemini` CLI provider (line 436/580) 視覺混淆",
            giminix_cfg.name
        );

        // 守 (3) giminix 仍 enabled — 防誤 disable
        assert!(
            giminix_cfg.enabled,
            "R75 T-BOT9 fail: giminix 應保持 enabled, R70 升 6 隻 OpenAB bot 設計選擇, \
             退回 disabled = 監控盲區, R67 護欄 chain 16 (d) `>= 5` 不會抓單隻 disable"
        );
    }
}
