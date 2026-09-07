use crate::config::{expand_path, ProviderConfig};
use log::info;
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
                "{} — settings.json at {} must be repaired before install",
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

fn is_lobsterpulse_hook(hook: &Value) -> bool {
    let command = hook.get("command").and_then(Value::as_str).unwrap_or("");
    let bash = hook.get("bash").and_then(Value::as_str).unwrap_or("");
    command.contains(MARKER) || bash.contains(MARKER)
}

/// Remove LobsterPulse-owned commands while preserving surrounding entries,
/// matchers, unknown fields, and all third-party hooks.
fn remove_lobsterpulse_hooks(root: &mut Value) -> Result<bool, String> {
    let before = root.clone();
    let Some(hooks_value) = root.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks = hooks_value
        .as_object_mut()
        .ok_or("hooks is not an object; refusing to modify it")?;

    for entries_value in hooks.values_mut() {
        let Some(entries) = entries_value.as_array_mut() else {
            continue;
        };
        entries.retain_mut(|entry| {
            if let Some(nested_value) = entry.get_mut("hooks") {
                let Some(nested) = nested_value.as_array_mut() else {
                    return true;
                };
                nested.retain(|hook| !is_lobsterpulse_hook(hook));
                !nested.is_empty()
            } else {
                !is_lobsterpulse_hook(entry)
            }
        });
    }

    Ok(*root != before)
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

    if remove_lobsterpulse_hooks(&mut root)? {
        save_json(&path, &root)?;
    }
    info!("Removed LobsterPulse hooks for {provider_id}");
    Ok(())
}

/// Install hooks for a provider, removing any previous LobsterPulse hooks first.
pub fn install_provider(provider_id: &str, config: &ProviderConfig) -> Result<(), String> {
    // Clean up any existing LobsterPulse hooks first.
    // R37：原 `let _ =` 沉默吞 cleanup 失敗（corrupt JSON / 權限拒絕 / 寫入失敗）,
    // operator 看到「install 成功」但舊 hook 可能還在, 沒 log 可查。
    // 非 Codex provider 維持既有 cleanup 流程；Codex 會在記憶體內移除自有 hook，
    // 先驗證完整 JSON，再以一次原子替換寫回。
    if provider_id != "codex" {
        if let Err(e) = remove_provider(provider_id, config) {
            log::warn!(
                "{} — install will proceed and overwrite, stale hooks may remain",
                provider_settings_warn_msg(provider_id, "install_provider cleanup", &e)
            );
        }
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

fn toml_line_without_comment(line: &str) -> Result<&str, String> {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;

    for (index, ch) in line.char_indices() {
        if double_quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                double_quoted = false;
            }
        } else if single_quoted {
            if ch == '\'' {
                single_quoted = false;
            }
        } else {
            match ch {
                '\'' => single_quoted = true,
                '"' => double_quoted = true,
                '#' => return Ok(&line[..index]),
                _ => {}
            }
        }
    }

    if single_quoted || double_quoted || escaped {
        return Err("config.toml contains an unterminated quoted string".to_string());
    }
    Ok(line)
}

fn toml_find_equals(line: &str) -> Option<usize> {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;

    for (index, ch) in line.char_indices() {
        if double_quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                double_quoted = false;
            }
        } else if single_quoted {
            if ch == '\'' {
                single_quoted = false;
            }
        } else {
            match ch {
                '\'' => single_quoted = true,
                '"' => double_quoted = true,
                '=' => return Some(index),
                _ => {}
            }
        }
    }
    None
}

fn toml_bracket_delta(line: &str) -> i32 {
    let mut delta = 0;
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;

    for ch in line.chars() {
        if double_quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                double_quoted = false;
            }
        } else if single_quoted {
            if ch == '\'' {
                single_quoted = false;
            }
        } else {
            match ch {
                '\'' => single_quoted = true,
                '"' => double_quoted = true,
                '[' => delta += 1,
                ']' => delta -= 1,
                _ => {}
            }
        }
    }
    delta
}

fn toml_key_is_codex_hooks(raw_key: &str) -> bool {
    let key = raw_key.trim();
    key == "codex_hooks"
        || key == "\"codex_hooks\""
        || key == "'codex_hooks'"
}

fn enable_codex_hooks_feature(config_toml: &Path) -> Result<bool, String> {
    let mut content =
        std::fs::read_to_string(config_toml).map_err(|e| format!("{}: {e}", config_toml.display()))?;
    let mut section = String::new();
    let mut features_seen = false;
    let mut features_header_end = None;
    let mut feature_value_span = None;
    let mut bracket_depth = 0i32;
    let mut offset = 0usize;
    let newline = if content.contains("\r\n") { "\r\n" } else { "\n" };

    for raw_line in content.split_inclusive('\n') {
        let line_without_newline = raw_line
            .strip_suffix('\n')
            .unwrap_or(raw_line)
            .strip_suffix('\r')
            .unwrap_or_else(|| raw_line.strip_suffix('\n').unwrap_or(raw_line));
        let clean = toml_line_without_comment(line_without_newline)?;
        let trimmed = clean.trim();

        if trimmed.is_empty() {
            offset += raw_line.len();
            continue;
        }

        if trimmed.starts_with('[') {
            if !trimmed.ends_with(']')
                || trimmed.starts_with("[[")
                || trimmed.len() < 3
                || trimmed[1..trimmed.len() - 1].trim().is_empty()
            {
                return Err(format!("{}: malformed table header", config_toml.display()));
            }
            let name = trimmed[1..trimmed.len() - 1].trim();
            if name == "features" {
                if features_seen {
                    return Err(format!("{}: duplicate [features] table", config_toml.display()));
                }
                features_seen = true;
                features_header_end = Some(offset + raw_line.len());
            }
            section.clear();
            section.push_str(name);
            bracket_depth = 0;
            offset += raw_line.len();
            continue;
        }

        if bracket_depth > 0 {
            bracket_depth += toml_bracket_delta(clean);
            if bracket_depth < 0 {
                return Err(format!("{}: malformed array", config_toml.display()));
            }
            offset += raw_line.len();
            continue;
        }

        let equals = toml_find_equals(clean).ok_or_else(|| {
            format!(
                "{}: malformed assignment near {}",
                config_toml.display(),
                trimmed
            )
        })?;
        if clean[..equals].trim().is_empty() {
            return Err(format!("{}: assignment has no key", config_toml.display()));
        }

        if section == "features" && toml_key_is_codex_hooks(&clean[..equals]) {
            if feature_value_span.is_some() {
                return Err(format!(
                    "{}: duplicate codex_hooks in [features]",
                    config_toml.display()
                ));
            }
            let value = clean[equals + 1..].trim();
            if value != "true" && value != "false" {
                return Err(format!(
                    "{}: [features].codex_hooks must be a boolean",
                    config_toml.display()
                ));
            }
            let value_offset = clean[equals + 1..]
                .find(value)
                .ok_or_else(|| format!("{}: invalid codex_hooks value", config_toml.display()))?;
            let value_start = offset + equals + 1 + value_offset;
            feature_value_span = Some((value_start, value_start + value.len(), value == "true"));
        }

        bracket_depth += toml_bracket_delta(clean);
        if bracket_depth < 0 {
            return Err(format!("{}: malformed array", config_toml.display()));
        }
        offset += raw_line.len();
    }

    if bracket_depth != 0 {
        return Err(format!("{}: unterminated array", config_toml.display()));
    }

    match feature_value_span {
        Some((start, end, true)) => {
            let _ = (start, end);
            Ok(false)
        }
        Some((start, end, false)) => {
            content.replace_range(start..end, "true");
            save_text_atomically(config_toml, &content)?;
            Ok(true)
        }
        None => {
            if let Some(insert_at) = features_header_end {
                content.insert_str(insert_at, &format!("codex_hooks = true{newline}"));
            } else {
                if !content.is_empty() && !content.ends_with(newline) {
                    content.push_str(newline);
                }
                content.push_str(&format!("[features]{newline}codex_hooks = true{newline}"));
            }
            save_text_atomically(config_toml, &content)?;
            Ok(true)
        }
    }
}

/// Codex CLI: hooks in ~/.codex/hooks.json + enable feature flag in config.toml
fn install_codex_hooks(path: &PathBuf) -> Result<(), String> {
    // Parse and validate before touching either user configuration file.
    let mut root = load_or_create_json(path)?;
    remove_lobsterpulse_hooks(&mut root)?;

    let hooks = root
        .as_object_mut()
        .ok_or("hooks.json root is not an object; refusing to modify it")?
        .entry("hooks")
        .or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or("hooks is not an object; refusing to modify it")?;

    let cmd = hook_cmd_powershell("codex");
    let events = [
        ("SessionStart", false),
        ("UserPromptSubmit", false),
        ("PreToolUse", true),
        ("PostToolUse", true),
        ("Stop", false),
    ];

    for (event, needs_matcher) in events {
        let mut entry = json!({
            "hooks": [{ "type": "command", "command": cmd.clone() }]
        });
        if needs_matcher {
            entry
                .as_object_mut()
                .expect("new hook entry is always an object")
                .insert("matcher".to_string(), json!(""));
        }

        let event_hooks = hooks.entry(event).or_insert_with(|| json!([]));
        event_hooks
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{event} is not an array; refusing to modify it"))?
            .push(entry);
    }

    let config_toml = path
        .parent()
        .ok_or("Invalid hooks.json path")?
        .join("config.toml");
    if config_toml.exists() && enable_codex_hooks_feature(&config_toml)? {
        info!("Enabled codex_hooks feature flag in config.toml");
    }

    save_json(path, &root)?;
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

fn load_or_create_json(path: &Path) -> Result<Value, String> {
    if path.exists() {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data)
            .map_err(|e| format!("{} contains malformed JSON: {e}", path.display()))
    } else {
        Ok(json!({}))
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("Invalid settings path: {}", path.display()))?;
    let mut suffixed = file_name.to_os_string();
    suffixed.push(suffix);
    Ok(path.with_file_name(suffixed))
}

fn backup_path(path: &Path) -> Result<PathBuf, String> {
    sibling_with_suffix(path, ".bak")
}

fn temporary_path(path: &Path) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    sibling_with_suffix(
        path,
        &format!(".lobsterpulse-{}-{nonce}.tmp", std::process::id()),
    )
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn atomic_write_target(path: &Path) -> Result<PathBuf, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => std::fs::canonicalize(path).map_err(|e| {
            format!(
                "failed to resolve symlinked settings path {}: {e}",
                path.display()
            )
        }),
        Ok(_) => Ok(path.to_path_buf()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(format!(
            "failed to inspect settings path {}: {error}",
            path.display()
        )),
    }
}

fn save_text_atomically(path: &Path, content: &str) -> Result<(), String> {
    save_text_atomically_with(path, content, replace_file)
}

fn save_text_atomically_with<F>(
    path: &Path,
    content: &str,
    replace: F,
) -> Result<(), String>
where
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    save_text_atomically_with_observer(path, content, |_| Ok(()), replace)
}

fn save_text_atomically_with_observer<B, F>(
    path: &Path,
    content: &str,
    before_write: B,
    replace: F,
) -> Result<(), String>
where
    B: FnOnce(&Path) -> std::io::Result<()>,
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    // Renaming over a symlink replaces the link itself. Resolve an existing
    // link first so dotfile-managed configuration keeps the link and updates
    // the canonical target atomically instead.
    let destination = atomic_write_target(path)?;

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let existing_permissions = if destination.exists() {
        let backup = backup_path(&destination)?;
        std::fs::copy(&destination, &backup)
            .map_err(|e| format!("failed to back up {}: {e}", destination.display()))?;
        Some(
            std::fs::metadata(&destination)
                .map_err(|e| e.to_string())?
                .permissions(),
        )
    } else {
        None
    };

    let temporary = temporary_path(&destination)?;
    let write_result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Provider settings may contain credentials. Make a new temporary
            // file owner-only before it can contain any bytes.
            options.mode(0o600);
        }

        let mut file = options.open(&temporary)?;
        if let Some(permissions) = existing_permissions {
            // Match an existing destination before writing its contents so
            // there is never a wider-permission exposure window.
            file.set_permissions(permissions)?;
        }
        // Keep this observation point immediately before the first write. It
        // makes the pre-write permission invariant directly testable without
        // exposing it in the production API.
        before_write(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "failed to write temporary settings file for {}: {error}",
            destination.display()
        ));
    }

    if let Err(error) = replace(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "failed to atomically replace {}: {error}",
            destination.display()
        ));
    }
    Ok(())
}

fn save_json(path: &Path, value: &Value) -> Result<(), String> {
    let formatted = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    save_text_atomically(path, &formatted)
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
    fn marker_count(value: &Value) -> usize {
        match value {
            Value::String(text) => usize::from(text.contains(MARKER)),
            Value::Array(values) => values.iter().map(marker_count).sum(),
            Value::Object(values) => values.values().map(marker_count).sum(),
            _ => 0,
        }
    }

    #[test]
    fn codex_install_reinstall_remove_preserves_unrelated_configuration() {
        let directory = tmp_settings_path("codex-roundtrip");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let original = json!({
            "schemaVersion": 7,
            "futureTopLevel": {"keep": true},
            "hooks": {
                "SessionStart": [],
                "UserPromptSubmit": [],
                "PreToolUse": [
                    {
                        "matcher": "third-party",
                        "futureEntryField": 42,
                        "hooks": [
                            {"type": "command", "command": "third-party-pre", "timeout": 9}
                        ]
                    },
                    {
                        "matcher": "mixed",
                        "hooks": [
                            {"type": "command", "command": "old-lobster-pulse-hook codex"},
                            {"type": "command", "command": "third-party-mixed"}
                        ]
                    }
                ],
                "PostToolUse": [],
                "Stop": [],
                "FutureEvent": [
                    {"type": "command", "command": "future-command", "unknown": "keep"}
                ]
            }
        });
        write_raw(
            &path,
            serde_json::to_string_pretty(&original)
                .expect("serialize fixture")
                .as_bytes(),
        );
        write_raw(&directory.join("config.toml"), b"[features]\nexisting = true\n");
        let config = provider_with_path(&path);

        install_provider("codex", &config).expect("first install");
        let first: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read first"))
                .expect("parse first");
        assert_eq!(first["schemaVersion"], json!(7));
        assert_eq!(first["futureTopLevel"], json!({"keep": true}));
        assert_eq!(first["hooks"]["FutureEvent"], original["hooks"]["FutureEvent"]);
        assert_eq!(first["hooks"]["PreToolUse"][0], original["hooks"]["PreToolUse"][0]);
        assert_eq!(
            first["hooks"]["PreToolUse"][1]["hooks"],
            json!([{"type": "command", "command": "third-party-mixed"}])
        );
        assert_eq!(marker_count(&first), 5);
        assert!(
            backup_path(&path).expect("backup path").exists(),
            "install must keep one bounded backup"
        );

        install_provider("codex", &config).expect("idempotent reinstall");
        let second: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read second"))
                .expect("parse second");
        assert_eq!(marker_count(&second), 5);
        assert_eq!(first, second);

        remove_provider("codex", &config).expect("remove provider");
        let removed: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read removed"))
                .expect("parse removed");
        let mut expected = original.clone();
        remove_lobsterpulse_hooks(&mut expected).expect("clean expected fixture");
        assert_eq!(removed, expected);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_enables_existing_false_feature_flag() {
        let directory = tmp_settings_path("codex-feature-false");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(
            &config_path,
            br#"# codex_hooks = false
[other]
description = "codex_hooks = false"
[features]
codex_hooks = false # preserve this comment
"#,
        );

        install_provider("codex", &provider_with_path(&path)).expect("install codex hooks");

        let config = std::fs::read_to_string(&config_path).expect("read config");
        assert!(config.contains("codex_hooks = true # preserve this comment"));
        assert!(config.contains("# codex_hooks = false"));
        assert!(config.contains(r#"description = "codex_hooks = false""#));
        assert_eq!(config.matches("codex_hooks = true").count(), 1);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_adds_feature_flag_when_only_other_section_mentions_it() {
        let directory = tmp_settings_path("codex-feature-other-section");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(
            &config_path,
            br#"[other]
description = "codex_hooks = false"
[features]
existing = true
"#,
        );

        install_provider("codex", &provider_with_path(&path)).expect("install codex hooks");

        let config = std::fs::read_to_string(&config_path).expect("read config");
        assert!(config.contains("[features]\ncodex_hooks = true\nexisting = true"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_fails_closed_on_malformed_toml() {
        let directory = tmp_settings_path("codex-malformed-toml");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        let original_hooks = br#"{"hooks":{}}"#;
        let original_config = b"[features]\ncodex_hooks = false\nnot a valid assignment\n";
        write_raw(&path, original_hooks);
        write_raw(&config_path, original_config);

        let result = install_provider("codex", &provider_with_path(&path));
        assert!(result.is_err(), "malformed TOML must fail closed");
        assert_eq!(std::fs::read(&path).expect("read hooks"), original_hooks);
        assert_eq!(
            std::fs::read(&config_path).expect("read config"),
            original_config
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_rejects_duplicate_feature_flags() {
        let directory = tmp_settings_path("codex-duplicate-feature");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        let original_hooks = br#"{"hooks":{}}"#;
        let original_config = b"[features]\ncodex_hooks = false\ncodex_hooks = true\n";
        write_raw(&path, original_hooks);
        write_raw(&config_path, original_config);

        let result = install_provider("codex", &provider_with_path(&path));
        assert!(result.is_err(), "duplicate feature flags must fail closed");
        assert_eq!(std::fs::read(&path).expect("read hooks"), original_hooks);
        assert_eq!(
            std::fs::read(&config_path).expect("read config"),
            original_config
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_fails_closed_on_malformed_json() {
        let directory = tmp_settings_path("codex-malformed");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        let original = b"{ malformed user hooks";
        let original_config = b"[features]\nexisting = true\n";
        write_raw(&path, original);
        write_raw(&config_path, original_config);
        let config = provider_with_path(&path);

        let result = install_provider("codex", &config);
        assert!(result.is_err(), "malformed JSON must fail closed");
        assert_eq!(std::fs::read(&path).expect("read hooks"), original);
        assert_eq!(
            std::fs::read(&config_path).expect("read config"),
            original_config
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn atomic_replace_failure_keeps_original_and_backup() {
        let directory = tmp_settings_path("atomic-failure");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let original = b"{\"original\":true}";
        write_raw(&path, original);

        let result = save_text_atomically_with(
            &path,
            "{\"replacement\":true}",
            |_temporary, _destination| {
                Err(std::io::Error::other("injected replace failure"))
            },
        );

        assert!(result.is_err(), "injected replace failure must surface");
        assert_eq!(std::fs::read(&path).expect("read original"), original);
        assert_eq!(
            std::fs::read(backup_path(&path).expect("backup path")).expect("read backup"),
            original
        );
        let leftovers = std::fs::read_dir(&directory)
            .expect("read fixture dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0, "failed replace must clean temporary file");
        let _ = std::fs::remove_dir_all(&directory);
    }


    #[cfg(unix)]
    #[test]
    fn atomic_save_applies_private_permissions_before_first_write() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tmp_settings_path("private-temporary");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let original = b"{\"secret\":\"existing\"}";
        write_raw(&path, original);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("restrict original permissions");

        save_text_atomically_with_observer(
            &path,
            "{\"secret\":\"replacement\"}",
            |temporary| {
                let metadata = std::fs::metadata(temporary)?;
                let temporary_mode = metadata.permissions().mode() & 0o777;
                if temporary_mode != 0o600 {
                    return Err(std::io::Error::other(format!(
                        "pre-write temporary file mode was {temporary_mode:o}, expected 600"
                    )));
                }
                if metadata.len() != 0 {
                    return Err(std::io::Error::other(
                        "pre-write temporary file already contained content",
                    ));
                }
                Ok(())
            },
            |temporary, destination| std::fs::rename(temporary, destination),
        )
        .expect("save private settings");

        assert_eq!(
            std::fs::metadata(&path)
                .expect("inspect replacement")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read replacement"),
            "{\"secret\":\"replacement\"}"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_creates_new_temporary_file_as_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tmp_settings_path("new-private-temporary");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");

        save_text_atomically_with_observer(
            &path,
            "{\"secret\":\"new\"}",
            |temporary| {
                let metadata = std::fs::metadata(temporary)?;
                let temporary_mode = metadata.permissions().mode() & 0o777;
                if temporary_mode != 0o600 {
                    return Err(std::io::Error::other(format!(
                        "pre-write new temporary file mode was {temporary_mode:o}, expected 600"
                    )));
                }
                if metadata.len() != 0 {
                    return Err(std::io::Error::other(
                        "pre-write new temporary file already contained content",
                    ));
                }
                Ok(())
            },
            |temporary, destination| std::fs::rename(temporary, destination),
        )
        .expect("save new private settings");

        assert_eq!(
            std::fs::metadata(&path)
                .expect("inspect new settings")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_preserves_symlink_and_updates_managed_target() {
        use std::os::unix::fs::symlink;

        let directory = tmp_settings_path("symlink-target");
        let managed_directory = directory.join("managed");
        std::fs::create_dir_all(&managed_directory).expect("mkdir managed fixture");
        let target = managed_directory.join("hooks.json");
        let link = directory.join("hooks.json");
        let original = b"{\"managed\":true}";
        write_raw(&target, original);
        symlink(Path::new("managed/hooks.json"), &link).expect("create relative symlink");

        save_text_atomically(&link, "{\"replacement\":true}").expect("save through symlink");

        assert!(
            std::fs::symlink_metadata(&link)
                .expect("inspect symlink")
                .file_type()
                .is_symlink(),
            "atomic save must preserve the dotfile-managed symlink"
        );
        assert_eq!(
            std::fs::read_to_string(&target).expect("read managed target"),
            "{\"replacement\":true}"
        );
        assert_eq!(
            std::fs::read(backup_path(&target).expect("target backup path"))
                .expect("read target backup"),
            original
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_fails_closed_for_broken_symlink() {
        use std::os::unix::fs::symlink;

        let directory = tmp_settings_path("broken-symlink");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let link = directory.join("hooks.json");
        let missing_target = directory.join("missing.json");
        symlink(Path::new("missing.json"), &link).expect("create broken symlink");

        let result = save_text_atomically(&link, "{\"replacement\":true}");

        assert!(result.is_err(), "broken symlink must fail closed");
        assert!(
            std::fs::symlink_metadata(&link)
                .expect("inspect broken symlink")
                .file_type()
                .is_symlink()
        );
        assert!(!missing_target.exists(), "must not create the missing target");
        let _ = std::fs::remove_dir_all(&directory);
    }

}
