use crate::config::{expand_path, ProviderConfig};
use log::info;
use serde_json::{json, Value};
use std::path::PathBuf;

/// R37：`provider_needs_setup` 之前 `Err(_) => return true` 連 IO 錯（PermissionDenied
/// / disk full）和 parse 錯（corrupt JSON / encoding 損壞）一併沉默吞；壞檔
/// 場景下前端只看到「needs setup = true」→ install 重跑 → `install_provider` 內
/// `remove_provider` cleanup 又 `let _ =` 吞 error → 同一個 corrupt 檔留著，
/// operator 完全沒 log 串起來定位。改 typed enum 後 caller 端 match 統一分流
/// （NotFound 靜默 / Io 跟 Parse 都 log warn 帶 path）。
#[derive(Debug)]
pub(crate) enum ReadProviderSettingsError {
    /// 首次啟動 / 該 provider 還沒建 settings 檔 → 預期, 不 log
    NotFound,
    /// 權限拒絕 / 磁碟滿 / 其它 OS 層級 fs 錯
    Io(std::io::Error),
    /// 檔案存在但內容不是合法 JSON
    Parse(String),
}

impl std::fmt::Display for ReadProviderSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "settings file not found"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Parse(e) => write!(f, "malformed JSON: {e}"),
        }
    }
}

/// Pure fn: 從指定 path 讀 provider settings.json, 解析成 serde_json::Value。
/// 對齊 R28 `parse_persisted_markers_at` pattern — 把 fs 跟 parse 兩條失敗路徑
/// 收斂到同一個 enum, 讓 caller 端 1 個 `match` 統一 log 處理。
fn read_provider_settings_at(path: &std::path::Path) -> Result<Value, ReadProviderSettingsError> {
    let data = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ReadProviderSettingsError::NotFound
        } else {
            ReadProviderSettingsError::Io(e)
        }
    })?;
    serde_json::from_str(&data).map_err(|e| ReadProviderSettingsError::Parse(e.to_string()))
}

/// 統一 log prefix 風格 helper — 對齊 R6 `discord_err_msg` / R23 `config_persist_warn_msg` /
/// R28 `persisted_marker_warn_msg` 三條前例, log filter 可一次 grep `[hooks_configurator]`。
fn provider_settings_warn_msg(provider_id: &str, action: &str, err: &str) -> String {
    format!("[hooks_configurator] {action} for {provider_id} failed: {err}")
}

/// Check if a provider's hooks are already configured
pub fn provider_needs_setup(provider_id: &str, config: &ProviderConfig) -> bool {
    let path = match &config.settings_path {
        Some(p) => expand_path(p),
        None => return true,
    };

    let json: Value = match read_provider_settings_at(&path) {
        Ok(v) => v,
        Err(ReadProviderSettingsError::NotFound) => return true, // first-run: 預期, 靜默
        Err(e) => {
            // R37 surface: 壞檔 / 權限拒絕 → log warn 帶 path, 仍回 true
            // (前端顯示「needs setup」正確, 因為壞檔就是要重 setup)
            log::warn!(
                "{} — settings.json at {} will be overwritten on next install",
                provider_settings_warn_msg(provider_id, "provider_needs_setup", &e.to_string()),
                path.display()
            );
            return true;
        }
    };

    // Look for the LobsterPulse sidecar marker in hooks
    let hooks_obj = json.get("hooks").unwrap_or(&json);
    let hooks = match hooks_obj {
        Value::Object(h) => h,
        _ => return true,
    };

    for (_event, entries) in hooks {
        if let Value::Array(entries) = entries {
            for entry in entries {
                // Check both nested hooks array and direct hook objects
                let hook_list = if let Some(Value::Array(hl)) = entry.get("hooks") {
                    hl.clone()
                } else {
                    vec![entry.clone()]
                };

                for hook in &hook_list {
                    if let Some(cmd) = hook.get("command").and_then(|v| v.as_str()) {
                        if cmd.contains(MARKER) {
                            return false;
                        }
                    }
                }
            }
        }
    }

    true
}

// Substring that uniquely identifies LobsterPulse-installed hooks. Matches
// the sidecar binary filename across all shells + OSes (lobster-pulse-hook
// on unix, lobster-pulse-hook.exe on windows). Previously "agentpulse" —
// which never matched anything, because the binary name is hyphenated.
const MARKER: &str = "lobster-pulse-hook";

/// Absolute path to the sidecar binary, expected next to the main exe.
/// Shipping a binary (not a shell one-liner) keeps hook commands
/// shell-agnostic across bash / PowerShell / cmd.exe.
fn sidecar_path() -> PathBuf {
    let exe_name = if cfg!(windows) {
        "lobster-pulse-hook.exe"
    } else {
        "lobster-pulse-hook"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe_name)))
        .unwrap_or_else(|| PathBuf::from(exe_name))
}

/// Build the hook command string. The sidecar reads stdin + port file
/// itself, so no shell substitution is needed.
fn hook_cmd(provider_id: &str) -> String {
    format!("\"{}\" {provider_id}", sidecar_path().display())
}

/// Some Windows hook runners execute command strings through PowerShell.
/// PowerShell parses `"path\to\exe.exe" arg` as a bare string expression
/// (ParserError: UnexpectedToken at `arg`), not a call, so the `&` call
/// operator is required. cmd.exe and bash don't accept the prefix, so only
/// emit it for providers whose Windows hook runner needs PowerShell syntax.
fn hook_cmd_powershell(provider_id: &str) -> String {
    if cfg!(windows) {
        format!("& {}", hook_cmd(provider_id))
    } else {
        hook_cmd(provider_id)
    }
}

/// Remove only LobsterPulse hooks from a provider's config.
pub fn remove_provider(provider_id: &str, config: &ProviderConfig) -> Result<(), String> {
    let path = match &config.settings_path {
        Some(p) => expand_path(p),
        None => return Ok(()),
    };

    if !path.exists() {
        return Ok(());
    }

    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut root: Value = serde_json::from_str(&data)
        .map_err(|e| format!("malformed JSON in {}: {e}", path.display()))?;

    if let Some(Value::Object(hooks)) = root.get_mut("hooks") {
        for (_event, entries) in hooks.iter_mut() {
            if let Value::Array(arr) = entries {
                arr.retain(|entry| {
                    // Check if this entry contains a LobsterPulse hook
                    let hook_list = if let Some(Value::Array(hl)) = entry.get("hooks") {
                        hl.clone()
                    } else {
                        vec![entry.clone()]
                    };
                    !hook_list.iter().any(|h| {
                        let cmd_str = h.get("command").and_then(|v| v.as_str()).unwrap_or("");
                        let bash_str = h.get("bash").and_then(|v| v.as_str()).unwrap_or("");
                        cmd_str.contains(MARKER) || bash_str.contains(MARKER)
                    })
                });
            }
        }
    }

    let formatted = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    std::fs::write(&path, formatted).map_err(|e| e.to_string())?;
    info!("Removed LobsterPulse hooks for {provider_id}");
    Ok(())
}

/// Install hooks for a provider, removing any previous LobsterPulse hooks first.
pub fn install_provider(provider_id: &str, config: &ProviderConfig) -> Result<(), String> {
    // Clean up any existing LobsterPulse hooks first.
    // R37：原 `let _ =` 沉默吞 cleanup 失敗（corrupt JSON / 權限拒絕 / 寫入失敗）,
    // operator 看到「install 成功」但舊 hook 可能還在, 沒 log 可查。
    // install 仍繼續走（overwrite 行為, 壞檔本來就會被新寫入覆蓋）— 只 log warn。
    if let Err(e) = remove_provider(provider_id, config) {
        log::warn!(
            "{} — install will proceed and overwrite, stale hooks may remain",
            provider_settings_warn_msg(provider_id, "install_provider cleanup", &e)
        );
    }

    let path = match &config.settings_path {
        Some(p) => expand_path(p),
        None => return Err(format!("No settings path for provider {provider_id}")),
    };

    match provider_id {
        "claude" => install_claude_hooks(&path),
        "gemini" => install_gemini_hooks(&path),
        "codex" => install_codex_hooks(&path),
        "copilot" => install_copilot_hooks(&path),
        _ => Err(format!("Unknown provider: {provider_id}")),
    }
}

/// Claude Code: hooks in ~/.claude/settings.json
fn install_claude_hooks(path: &PathBuf) -> Result<(), String> {
    let mut root = load_or_create_json(path)?;

    let hooks = root
        .as_object_mut()
        .ok_or("settings.json root is not an object")?
        .entry("hooks")
        .or_insert_with(|| json!({}));

    let cmd = hook_cmd("claude");

    let events = [
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "PermissionRequest",
        "Stop",
    ];

    for event in events {
        let entry = json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": cmd,
                "async": true
            }]
        });

        let event_hooks = hooks
            .as_object_mut()
            .ok_or("hooks is not an object")?
            .entry(event)
            .or_insert_with(|| json!([]));

        if let Value::Array(ref mut arr) = event_hooks {
            arr.push(entry);
        }
    }

    save_json(path, &root)?;
    info!("Claude Code hooks configured");
    Ok(())
}

/// Gemini CLI: hooks in ~/.gemini/settings.json
fn install_gemini_hooks(path: &PathBuf) -> Result<(), String> {
    let mut root = load_or_create_json(path)?;

    let hooks = root
        .as_object_mut()
        .ok_or("settings.json root is not an object")?
        .entry("hooks")
        .or_insert_with(|| json!({}));

    let cmd = hook_cmd_powershell("gemini");

    let events = [
        "SessionStart",
        "SessionEnd",
        "BeforeAgent",
        "AfterAgent",
        "BeforeModel",
        "AfterModel",
        "BeforeTool",
        "AfterTool",
        "Notification",
    ];

    for event in events {
        let entry = json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": cmd,
                "async": true
            }]
        });

        let event_hooks = hooks
            .as_object_mut()
            .ok_or("hooks is not an object")?
            .entry(event)
            .or_insert_with(|| json!([]));

        if let Value::Array(ref mut arr) = event_hooks {
            arr.push(entry);
        }
    }

    save_json(path, &root)?;
    info!("Gemini CLI hooks configured");
    Ok(())
}

/// Codex CLI: hooks in ~/.codex/hooks.json + enable feature flag in config.toml
fn install_codex_hooks(path: &PathBuf) -> Result<(), String> {
    // 1. Enable codex_hooks feature flag in config.toml
    let config_toml = path
        .parent()
        .ok_or("Invalid hooks.json path")?
        .join("config.toml");

    if config_toml.exists() {
        let mut content = std::fs::read_to_string(&config_toml).map_err(|e| e.to_string())?;
        if !content.contains("codex_hooks") {
            // Add [features] section with codex_hooks = true
            if content.contains("[features]") {
                content = content.replace("[features]", "[features]\ncodex_hooks = true");
            } else {
                content.push_str("\n[features]\ncodex_hooks = true\n");
            }
            std::fs::write(&config_toml, content).map_err(|e| e.to_string())?;
            info!("Enabled codex_hooks feature flag in config.toml");
        }
    }

    // 2. Write hooks.json
    let cmd = hook_cmd_powershell("codex");

    let hooks_json = json!({
        "hooks": {
            "SessionStart": [{
                "hooks": [{ "type": "command", "command": cmd }]
            }],
            "UserPromptSubmit": [{
                "hooks": [{ "type": "command", "command": cmd }]
            }],
            "PreToolUse": [{
                "matcher": "",
                "hooks": [{ "type": "command", "command": cmd }]
            }],
            "PostToolUse": [{
                "matcher": "",
                "hooks": [{ "type": "command", "command": cmd }]
            }],
            "Stop": [{
                "hooks": [{ "type": "command", "command": cmd }]
            }]
        }
    });

    save_json(path, &hooks_json)?;
    info!("Codex CLI hooks configured");
    Ok(())
}

/// GitHub Copilot CLI: hooks in ~/.copilot/config.json
fn install_copilot_hooks(path: &PathBuf) -> Result<(), String> {
    let mut root = load_or_create_json(path)?;

    let hooks = root
        .as_object_mut()
        .ok_or("config.json root is not an object")?
        .entry("hooks")
        .or_insert_with(|| json!({}));

    let cmd = hook_cmd("copilot");

    let events = [
        "sessionStart",
        "sessionEnd",
        "userPromptSubmitted",
        "preToolUse",
        "postToolUse",
        "agentStop",
    ];

    for event in events {
        let hook_entry = json!({
            "type": "command",
            "bash": cmd
        });

        let event_hooks = hooks
            .as_object_mut()
            .ok_or("hooks is not an object")?
            .entry(event)
            .or_insert_with(|| json!([]));

        if let Value::Array(ref mut arr) = event_hooks {
            arr.push(hook_entry);
        }
    }

    save_json(path, &root)?;
    info!("GitHub Copilot CLI hooks configured");
    Ok(())
}

fn load_or_create_json(path: &PathBuf) -> Result<Value, String> {
    if path.exists() {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data)
            .map_err(|e| format!("{} contains malformed JSON: {e}", path.display()))
    } else {
        Ok(json!({}))
    }
}

fn save_json(path: &PathBuf, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let formatted = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, formatted).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod r37_silent_fail_surfacing_tests {
    //! R37 regression: 收斂 `provider_needs_setup` 跟 `install_provider` 兩條
    //! silent fail 路徑到 typed enum + 統一 warn prefix。
    //!
    //! 修前:
    //! - `provider_needs_setup` 兩處 `Err(_) => return true` 連 PermissionDenied /
    //!   磁碟滿 / 壞 JSON 一併吞
    //! - `install_provider` cleanup 用 `let _ = remove_provider(...)` 吞 error
    //!
    //! 修後: NotFound 仍靜默 (first-run 預期), Io/Parse 走 `log::warn!` 帶 path 跟統一
    //! `[hooks_configurator]` prefix, install 失敗不阻擋 overwrite 行為。
    //!
    //! 本 module 鎖三條契約:
    //! 1. `read_provider_settings_at` 三條 path (missing / corrupt / valid) 各回對應 variant
    //! 2. `provider_settings_warn_msg` prefix 格式 (R6/R23/R28 prefix 風格對齊)
    //! 3. `install_provider` 對壞 settings.json 仍回 Ok (overwrite 行為) + 新 hook 寫入

    use super::*;
    use crate::config::ProviderConfig;

    fn tmp_settings_path(tag: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("lp-r37-{tag}-{nonce}-{}", std::process::id()));
        p
    }

    fn write_raw(path: &PathBuf, body: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parents");
        }
        std::fs::write(path, body).expect("write fixture");
    }

    fn provider_with_path(path: &std::path::Path) -> ProviderConfig {
        // ProviderConfig 無 Default derive, 顯式構造避免改 config.rs
        ProviderConfig {
            enabled: false,
            name: String::new(),
            settings_path: Some(path.to_string_lossy().into_owned()),
        }
    }

    #[test]
    fn read_provider_settings_at_missing_file_returns_not_found() {
        let path = tmp_settings_path("missing");
        let r = read_provider_settings_at(&path);
        assert!(
            matches!(r, Err(ReadProviderSettingsError::NotFound)),
            "missing 檔應回 NotFound variant (first-run 預期), 實際: {r:?}"
        );
    }

    #[test]
    fn read_provider_settings_at_corrupt_json_returns_parse_with_message() {
        let path = tmp_settings_path("corrupt");
        write_raw(&path, b"{ not valid json at all");
        let r = read_provider_settings_at(&path);
        match r {
            Err(ReadProviderSettingsError::Parse(msg)) => {
                assert!(
                    !msg.is_empty(),
                    "Parse variant 應帶 serde_json 錯誤訊息給 log 端 grep, 實際空字串"
                );
            }
            other => panic!("corrupt JSON 應回 Parse variant, 實際: {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_provider_settings_at_valid_file_returns_parsed_value() {
        let path = tmp_settings_path("valid");
        write_raw(&path, br#"{"hooks":{}}"#);
        let r = read_provider_settings_at(&path);
        let v = r.expect("valid JSON 應回 Ok");
        assert!(v.is_object(), "valid JSON 應 parse 成 Value::Object");
        assert!(v.get("hooks").is_some(), "應保留 hooks 欄位");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn provider_settings_warn_msg_unifies_prefix() {
        // 對齊 R6 `discord_err_msg` / R23 `config_persist_warn_msg` /
        // R28 `persisted_marker_warn_msg` 三條 prefix 風格, 讓 log filter
        // 可以一次 grep `[hooks_configurator]` 撈全 module 警告
        let msg = provider_settings_warn_msg("claude", "provider_needs_setup", "io error: denied");
        assert!(
            msg.starts_with("[hooks_configurator] provider_needs_setup for claude failed:"),
            "prefix 應含 module + action + provider_id + 失敗動詞, 實際: {msg}"
        );
        assert!(msg.contains("io error: denied"), "訊息尾應含原始 err 內容");
    }

    #[test]
    fn provider_needs_setup_missing_file_returns_true_silently() {
        // NotFound 路徑: 前端顯示 "needs setup = true" 是正確語意, 不應 log warn
        // (first-run 預期, 每次啟動都會走到, log 會被洗爆)
        let path = tmp_settings_path("needs-missing");
        let cfg = provider_with_path(&path);
        assert!(
            provider_needs_setup("claude", &cfg),
            "missing 檔應回 true 讓前端走 install flow"
        );
    }

    #[test]
    fn provider_needs_setup_corrupt_json_still_returns_true() {
        // 壞 JSON 路徑: 前端仍顯示 "needs setup = true" (壞檔就是要重 setup),
        // 並發 log warn 給 operator 知道壞檔位置 (本 test 鎖 return value, log
        // 內容靠 R6 的 log-file 觀察契約, 跟 R28 一致)
        let path = tmp_settings_path("needs-corrupt");
        write_raw(&path, b"<<not json>>");
        let cfg = provider_with_path(&path);
        assert!(
            provider_needs_setup("claude", &cfg),
            "壞 JSON 仍應回 true (前端走 install → overwrite)"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn install_provider_propagates_load_error_after_cleanup_warn() {
        // 鎖 R37 收斂後的契約邊界:
        //  - 壞 settings.json → `remove_provider` cleanup 回 Err → R37 新邏輯 `if let Err`
        //    log warn 但 install 繼續走 (不再 silent `let _ =` 吞 error)
        //  - 接著 `install_claude_hooks` → `load_or_create_json` 也回 Err (parse fail) →
        //    `?` 正常 propagate 出 `install_provider` 給 caller (前端 install 命令)
        //  - 函式不 panic, 不丟 silent, caller 拿到的 Err 訊息帶 path
        //
        // 範圍說明 (為什麼本 test 不鎖 "overwrite corrupt file" 行為):
        //  讓 install 對壞 JSON 直接 overwrite 是另一條獨立的 product 決策
        // (覆寫會丟失使用者其它自訂 hooks), 不屬 R37 M0 silent-fail surfacing
        // 範圍。R37 只負責「cleanup silent → log warn」, load silent 早就是
        // `?` propagate 沒 silent, 不在本輪治理線。
        let path = tmp_settings_path("install-corrupt");
        write_raw(&path, b"this is not json {{{ broken");
        let cfg = provider_with_path(&path);

        let r = install_provider("claude", &cfg);
        assert!(
            r.is_err(),
            "壞 JSON → install_provider 應回 Err (load propagate), 實際: {r:?}"
        );
        let err_msg = r.unwrap_err();
        assert!(
            err_msg.contains("malformed JSON") || err_msg.contains("parse"),
            "Err 訊息應指明是 parse/JSON 問題 (operator 可定位), 實際: {err_msg}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
