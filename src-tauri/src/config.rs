use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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
            name: "🤖 GIMINIX · OpenAB Gemini".into(),
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
    let path = config_path();
    let mut config: AppConfig = if let Ok(data) = std::fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        AppConfig::default()
    };
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

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(())
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
    for id in ["cicx", "gitx", "giminix", "codex_bot", "openx"] {
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
