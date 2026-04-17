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
    /// Legacy field, kept for backward compat
    #[serde(default)]
    pub sound_name: String,
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

fn default_accent() -> String { "orange".into() }
fn default_text_size() -> String { "medium".into() }
fn default_theme() -> String { "dark".into() }

fn default_provider_sounds() -> HashMap<String, String> {
    HashMap::from([
        ("claude".into(), "claude.mp3".into()),
        ("codex".into(), "codex.mp3".into()),
        ("copilot".into(), "copilot.mp3".into()),
        ("gemini".into(), "__none__".into()),
    ])
}

fn default_provider_waiting_sounds() -> HashMap<String, String> {
    HashMap::from([
        ("claude".into(), "claude-waiting.mp3".into()),
        ("codex".into(), "codex-waiting.mp3".into()),
        ("copilot".into(), "copilot-waiting.mp3".into()),
        ("gemini".into(), "__none__".into()),
    ])
}

fn default_providers() -> HashMap<String, ProviderConfig> {
    let mut m = HashMap::new();
    m.insert("claude".into(), ProviderConfig {
        enabled: false,
        name: "Claude Code".into(),
        settings_path: Some("~/.claude/settings.json".into()),
    });
    m.insert("gemini".into(), ProviderConfig {
        enabled: false,
        name: "Gemini CLI".into(),
        settings_path: Some("~/.gemini/settings.json".into()),
    });
    m.insert("copilot".into(), ProviderConfig {
        enabled: false,
        name: "GitHub Copilot CLI".into(),
        settings_path: Some("~/.copilot/config.json".into()),
    });
    m.insert("codex".into(), ProviderConfig {
        enabled: false,
        name: "Codex CLI".into(),
        settings_path: Some("~/.codex/hooks.json".into()),
    });
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
    if let Ok(data) = std::fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        AppConfig::default()
    }
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
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

/// Detect which providers are installed on this system
pub fn detect_providers() -> HashMap<String, bool> {
    let mut detected = HashMap::new();

    // Claude: check if ~/.claude/ exists
    detected.insert("claude".into(), dirs::home_dir()
        .map(|h| h.join(".claude").exists())
        .unwrap_or(false));

    // Gemini: check if gemini binary exists or ~/.gemini/ exists
    detected.insert("gemini".into(),
        which_exists("gemini") ||
        dirs::home_dir().map(|h| h.join(".gemini").exists()).unwrap_or(false));

    // Copilot: check binary, GitHub CLI, or config dir
    detected.insert("copilot".into(),
        which_exists("copilot") ||
        which_exists("gh") ||
        dirs::home_dir().map(|h| h.join(".copilot").exists()).unwrap_or(false));

    // Codex: check binary or config dir
    detected.insert("codex".into(),
        which_exists("codex") ||
        dirs::home_dir().map(|h| h.join(".codex").exists()).unwrap_or(false));

    detected
}

fn which_exists(cmd: &str) -> bool {
    let checker = if cfg!(windows) { "where.exe" } else { "which" };
    std::process::Command::new(checker)
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
