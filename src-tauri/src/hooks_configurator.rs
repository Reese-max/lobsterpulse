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
    if provider_id == "codex" {
        let parent = path.parent().ok_or("Invalid hooks.json path")?;
        let config_toml = parent.join("config.toml");
        let state_path = codex_flag_state_path(&config_toml)?;
        let lock_path = parent.join("lobsterpulse-codex-hooks.lock");
        if !path.exists() && !state_path.exists() && !lock_path.exists() {
            // No managed file or owned feature flag exists. In particular,
            // disabling Codex before its home exists must not create it. An
            // existing lock may belong to an install that has not written
            // either settings file yet, so acquire it before returning.
            return Ok(());
        }
    }
    let _codex_lock = if provider_id == "codex" {
        Some(lock_codex_hooks(&path)?)
    } else {
        None
    };

    if !path.exists() {
        if provider_id == "codex" {
            restore_codex_hooks_feature(&path)?;
        }
        return Ok(());
    }

    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut root: Value = serde_json::from_str(&data)
        .map_err(|e| format!("malformed JSON in {}: {e}", path.display()))?;

    if remove_lobsterpulse_hooks(&mut root)? {
        save_json(&path, &root)?;
    }
    if provider_id == "codex" {
        restore_codex_hooks_feature(&path)?;
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
        "codex" => {
            let _lock = lock_codex_hooks(&path)?;
            install_codex_hooks(&path)
        },
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

fn enable_codex_hooks_feature(config_toml: &Path) -> Result<bool, String> {
    let target = atomic_write_target(config_toml)?;
    let target = if target.exists() {
        std::fs::canonicalize(&target).map_err(|e| format!("{}: {e}", target.display()))?
    } else {
        target
    };
    let original_content = if target.exists() {
        Some(std::fs::read_to_string(&target)
            .map_err(|e| format!("{}: {e}", config_toml.display()))?)
    } else {
        None
    };
    let mut document = if let Some(content) = &original_content {
        content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| format!("{}: malformed TOML: {e}", config_toml.display()))?
    } else {
        // A new Codex home has hooks.json but no config.toml yet. Treat the
        // missing file as an empty TOML document so install cannot report
        // success while leaving the feature capability absent.
        toml_edit::DocumentMut::new()
    };

    if let Some(features) = document.get("features") {
        if !features.is_table_like() {
            return Err(format!(
                "{}: features must be a table or inline table",
                config_toml.display()
            ));
        }
    }

    // Codex now calls this feature `hooks`; `codex_hooks` remains a deprecated
    // alias. The canonical key wins when both exist, so enabling only the
    // alias can still leave hooks disabled. Validate both before any write.
    let mut present = false;
    for key in ["hooks", "codex_hooks"] {
        if let Some(feature) = document
            .get("features")
            .and_then(|features| features.get(key))
        {
            present = true;
            if feature.as_bool().is_none() {
                return Err(format!(
                    "{}: [features].{key} must be a boolean",
                    config_toml.display()
                ));
            }
        }
    }

    let state_path = codex_flag_state_path(config_toml)?;
    let prior_state = read_codex_flag_state(&state_path)?;
    if let Some(state) = &prior_state {
        verify_codex_flag_state(config_toml, state)?;
    }
    let mut owned = prior_state
        .as_ref()
        .map(|state| state.enabled_from_false.clone())
        .unwrap_or_default();
    let mut changed = false;
    for key in ["hooks", "codex_hooks"] {
        if let Some(feature) = document
            .get_mut("features")
            .and_then(|features| features.get_mut(key))
        {
            if feature.as_bool() == Some(false) {
                if !owned.iter().any(|owned_key| owned_key.as_str() == key) {
                    owned.push(key.to_string());
                }
                let decor = feature
                    .as_value()
                    .expect("boolean TOML item is a value")
                    .decor()
                    .clone();
                let mut enabled = toml_edit::Value::from(true);
                *enabled.decor_mut() = decor;
                *feature = toml_edit::Item::Value(enabled);
                changed = true;
            }
        }
    }

    if !present {
        document["features"]["hooks"] = toml_edit::value(true);
        changed = true;
    }
    if changed {
        // Write the verified target, not a symlink that might be repointed
        // between validation and the atomic replacement.
        let state_to_stage = if owned.is_empty() {
            None
        } else {
            let target_text = target.to_str()
                .ok_or_else(|| format!("{}: target path is not UTF-8", config_toml.display()))?;
            let identity = config_file_identity(&target)?;
            let identity_owned = prior_state
                .as_ref()
                .and_then(|state| state_owns_identity(state, &identity))
                .unwrap_or(false);
            Some(CodexFlagState {
                enabled_from_false: owned,
                target: target_text.to_owned(),
                identity,
                identity_owned,
                pending_identity: None,
                pending_owned: true,
            })
        };
        save_text_atomically_with(&target, &document.to_string(), |temporary, destination| {
            if let Some(content) = &original_content {
                verify_config_snapshot(config_toml, &target, content)
                    .map_err(std::io::Error::other)?;
            }
            let mut staged_state = None;
            if let Some(mut state) = state_to_stage {
                // The temporary file already has its final identity. Persist
                // both it and the prior target before replacing config.toml:
                // a failed state write cannot enable hooks, and either side
                // of a failed rename remains recognizable on retry.
                state.pending_identity = Some(
                    config_file_identity(temporary).map_err(std::io::Error::other)?
                );
                write_codex_flag_state(&state_path, &state)
                    .map_err(std::io::Error::other)?;
                verify_codex_flag_state(config_toml, &state)
                    .map_err(std::io::Error::other)?;
                staged_state = Some(state);
            }
            if let Some(content) = &original_content {
                verify_config_snapshot(config_toml, &target, content)
                    .map_err(std::io::Error::other)?;
            }
            if let Some(state) = &staged_state {
                verify_codex_flag_state(config_toml, state)
                    .map_err(std::io::Error::other)?;
            }
            if original_content.is_none() {
                // A concurrent creator must win without losing its config.
                // hard_link is atomic and fails if destination now exists.
                create_file_if_absent(temporary, destination)
            } else {
                replace_file(temporary, destination)
            }
        })?;
    }
    Ok(changed)
}

fn codex_flag_state_path(config_toml: &Path) -> Result<PathBuf, String> {
    sibling_with_suffix(config_toml, ".lobsterpulse-hooks-state.json")
}

fn lock_codex_hooks(hooks_json: &Path) -> Result<std::fs::File, String> {
    use fs2::FileExt;
    let parent = hooks_json.parent().ok_or("Invalid hooks.json path")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|e| format!("{}: {e}", parent.display()))?;
    let lock_path = canonical_parent.join("lobsterpulse-codex-hooks.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&lock_path)
        .map_err(|e| format!("{}: {e}", lock_path.display()))?;
    file.try_lock_exclusive()
        .map_err(|e| format!("{}: Codex hooks update already in progress: {e}", lock_path.display()))?;
    Ok(file)
}

struct CodexFlagState {
    enabled_from_false: Vec<String>,
    target: String,
    identity: String,
    identity_owned: bool,
    pending_identity: Option<String>,
    pending_owned: bool,
}

fn read_codex_flag_state(path: &Path) -> Result<Option<CodexFlagState>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let state: Value = serde_json::from_str(&data)
        .map_err(|e| format!("{}: malformed state: {e}", path.display()))?;
    if state.get("version").and_then(Value::as_u64) != Some(3) {
        return Err(format!("{}: unsupported state version", path.display()));
    }
    let target = state.get("target").and_then(Value::as_str)
        .filter(|target| Path::new(target).is_absolute())
        .ok_or_else(|| format!("{}: invalid target", path.display()))?;
    let identity = state.get("identity").and_then(Value::as_str)
        .filter(|identity| !identity.is_empty())
        .ok_or_else(|| format!("{}: invalid identity", path.display()))?;
    let identity_owned = state.get("identity_owned").and_then(Value::as_bool)
        .ok_or_else(|| format!("{}: invalid identity_owned", path.display()))?;
    let pending_identity = match state.get("pending_identity") {
        Some(Value::String(identity)) if !identity.is_empty() => Some(identity.clone()),
        Some(Value::Null) | None => None,
        _ => return Err(format!("{}: invalid pending_identity", path.display())),
    };
    let pending_owned = state.get("pending_owned").and_then(Value::as_bool)
        .ok_or_else(|| format!("{}: invalid pending_owned", path.display()))?;
    let entries = state
        .get("enabled_from_false")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{}: missing enabled_from_false", path.display()))?;
    let mut owned = Vec::new();
    for entry in entries {
        let key = entry
            .as_str()
            .ok_or_else(|| format!("{}: invalid feature key", path.display()))?;
        if !["hooks", "codex_hooks"].contains(&key)
            || owned.iter().any(|item: &String| item.as_str() == key)
        {
            return Err(format!("{}: invalid or duplicate feature key", path.display()));
        }
        owned.push(key.to_string());
    }
    Ok(Some(CodexFlagState {
        enabled_from_false: owned,
        target: target.to_string(),
        identity: identity.to_string(),
        identity_owned,
        pending_identity,
        pending_owned,
    }))
}

fn write_codex_flag_state(path: &Path, state: &CodexFlagState) -> Result<(), String> {
    save_json(path, &json!({
        "version": 3,
        "enabled_from_false": state.enabled_from_false,
        "target": state.target,
        "identity": state.identity,
        "identity_owned": state.identity_owned,
        "pending_identity": state.pending_identity,
        "pending_owned": state.pending_owned,
    }))
}

fn state_owns_identity(state: &CodexFlagState, identity: &str) -> Option<bool> {
    if state.pending_identity.as_deref() == Some(identity) {
        Some(state.pending_owned)
    } else if state.identity == identity {
        Some(state.identity_owned)
    } else {
        None
    }
}

fn verify_config_snapshot(config_toml: &Path, target: &Path, expected: &str) -> Result<(), String> {
    let current_target = std::fs::canonicalize(config_toml)
        .map_err(|e| format!("{}: {e}", config_toml.display()))?;
    if current_target != target {
        return Err(format!("{}: config target changed during update", config_toml.display()));
    }
    let current = std::fs::read_to_string(target)
        .map_err(|e| format!("{}: {e}", target.display()))?;
    if current != expected {
        return Err(format!("{}: config content changed during update", config_toml.display()));
    }
    Ok(())
}

fn verify_codex_flag_state(config_toml: &Path, state: &CodexFlagState) -> Result<(), String> {
    let current_target = std::fs::canonicalize(config_toml)
        .map_err(|e| format!("{}: cannot restore changed config target: {e}", config_toml.display()))?;
    let saved_target = Path::new(&state.target);
    let identity = config_file_identity(&current_target)?;
    let ownership = state_owns_identity(state, &identity);
    if current_target != saved_target || ownership.is_none() {
        return Err(format!(
            "{}: config target or file identity changed; refusing to restore owned feature flags",
            config_toml.display()
        ));
    }
    if ownership == Some(false) {
        // A pending first install did not commit. Its original inode is not
        // owned; only the original disabled values may be retried or cleaned.
        let content = std::fs::read_to_string(&current_target)
            .map_err(|e| format!("{}: {e}", current_target.display()))?;
        let document = content.parse::<toml_edit::DocumentMut>()
            .map_err(|e| format!("{}: malformed TOML: {e}", current_target.display()))?;
        if state.enabled_from_false.iter().any(|key| {
            document.get("features").and_then(|features| features.get(key))
                .and_then(|feature| feature.as_bool()) != Some(false)
        }) {
            return Err(format!(
                "{}: uncommitted original feature flags changed; refusing to claim ownership",
                config_toml.display()
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn config_file_identity(path: &Path) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn config_file_identity(path: &Path) -> Result<String, String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(format!("{}: {}", path.display(), std::io::Error::last_os_error()));
    }
    Ok(format!(
        "windows:{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

fn restore_codex_hooks_feature(hooks_json: &Path) -> Result<(), String> {
    let config_toml = hooks_json
        .parent()
        .ok_or("Invalid hooks.json path")?
        .join("config.toml");
    let state_path = codex_flag_state_path(&config_toml)?;
    if !state_path.exists() {
        return Ok(());
    }
    let state = read_codex_flag_state(&state_path)?
        .ok_or_else(|| format!("{}: missing ownership state", state_path.display()))?;
    verify_codex_flag_state(&config_toml, &state)?;
    let current_identity = config_file_identity(Path::new(&state.target))?;
    if state_owns_identity(&state, &current_identity) == Some(false) {
        // Either the enable never committed, or a prior restore committed
        // but removal of the sidecar failed. The config is already disabled.
        std::fs::remove_file(&state_path)
            .map_err(|e| format!("{}: {e}", state_path.display()))?;
        return Ok(());
    }
    let owned = &state.enabled_from_false;
    {
        let content = std::fs::read_to_string(&state.target)
            .map_err(|e| format!("{}: {e}", state.target))?;
        let mut document = content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| format!("{}: malformed TOML: {e}", config_toml.display()))?;
        let mut changed = false;
        for key in owned {
            if let Some(feature) = document
                .get_mut("features")
                .and_then(|features| features.get_mut(key))
            {
                if feature.as_bool() == Some(true) {
                    let decor = feature
                        .as_value()
                        .expect("boolean TOML item is a value")
                        .decor()
                        .clone();
                    let mut disabled = toml_edit::Value::from(false);
                    *disabled.decor_mut() = decor;
                    *feature = toml_edit::Item::Value(disabled);
                    changed = true;
                } else if feature.as_bool().is_none() {
                    return Err(format!(
                        "{}: [features].{key} must be a boolean",
                        config_toml.display()
                    ));
                }
            }
        }
        if changed {
            save_text_atomically_with(
                Path::new(&state.target),
                &document.to_string(),
                |temporary, destination| {
                    // Stage the restoring inode before the rename. If the
                    // process stops or state deletion fails afterward, the
                    // next removal recognizes an already-restored config.
                    verify_codex_flag_state(&config_toml, &state)
                        .map_err(std::io::Error::other)?;
                    let staged = CodexFlagState {
                        enabled_from_false: state.enabled_from_false.clone(),
                        target: state.target.clone(),
                        identity: current_identity.clone(),
                        identity_owned: true,
                        pending_identity: Some(
                            config_file_identity(temporary).map_err(std::io::Error::other)?
                        ),
                        pending_owned: false,
                    };
                    verify_config_snapshot(&config_toml, destination, &content)
                        .map_err(std::io::Error::other)?;
                    write_codex_flag_state(&state_path, &staged)
                        .map_err(std::io::Error::other)?;
                    verify_codex_flag_state(&config_toml, &staged)
                        .map_err(std::io::Error::other)?;
                    verify_config_snapshot(&config_toml, destination, &content)
                        .map_err(std::io::Error::other)?;
                    verify_codex_flag_state(&config_toml, &staged)
                        .map_err(std::io::Error::other)?;
                    replace_file(temporary, destination)
                },
            )?;
        }
    }
    std::fs::remove_file(&state_path).map_err(|e| format!("{}: {e}", state_path.display()))?;
    Ok(())
}

/// Codex CLI: hooks in ~/.codex/hooks.json + enable feature flag in config.toml
fn install_codex_hooks(path: &PathBuf) -> Result<(), String> {
    // Parse and validate before touching either user configuration file.
    let hooks_target = atomic_write_target(path)?;
    let hooks_target = if hooks_target.exists() {
        std::fs::canonicalize(&hooks_target)
            .map_err(|error| format!("{}: {error}", hooks_target.display()))?
    } else {
        let parent = hooks_target.parent().ok_or("Invalid hooks.json path")?;
        std::fs::canonicalize(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?
            .join(hooks_target.file_name().ok_or("Invalid hooks.json path")?)
    };
    let original_hooks = match std::fs::read_to_string(&hooks_target) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("{}: {error}", hooks_target.display())),
    };
    let original_identity = if original_hooks.is_some() {
        Some(config_file_identity(&hooks_target)?)
    } else {
        None
    };
    let mut root = match &original_hooks {
        Some(content) => serde_json::from_str(content)
            .map_err(|error| format!("{} contains malformed JSON: {error}", path.display()))?,
        None => json!({}),
    };
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
    // Persist hooks first. Bind both the write and any rollback to the target
    // we read, so repointing a dotfile symlink cannot overwrite another file.
    let installed_hooks = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
    let mut installed_identity = None;
    save_text_atomically_with(&hooks_target, &installed_hooks, |temporary, destination| {
        verify_hooks_snapshot(path, destination, original_hooks.as_deref(), original_identity.as_deref())
            .map_err(std::io::Error::other)?;
        installed_identity = Some(config_file_identity(temporary).map_err(std::io::Error::other)?);
        if original_hooks.is_some() {
            replace_file(temporary, destination)
        } else {
            create_file_if_absent(temporary, destination)
        }
    })?;
    let installed_identity = installed_identity
        .ok_or_else(|| format!("{}: missing installed hooks identity", hooks_target.display()))?;
    if let Err(error) = verify_hooks_snapshot(path, &hooks_target, Some(&installed_hooks), Some(&installed_identity)) {
        return match rollback_codex_hooks_json(&hooks_target, original_hooks.as_deref(), &installed_hooks, &installed_identity) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!("{error}; failed to restore hooks.json: {rollback_error}")),
        };
    }
    match enable_codex_hooks_feature(&config_toml) {
        Ok(true) => info!("Enabled Codex hooks feature flag in config.toml"),
        Ok(false) => {}
        Err(error) => {
            return match rollback_codex_hooks_json(&hooks_target, original_hooks.as_deref(), &installed_hooks, &installed_identity) {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(format!(
                    "{error}; failed to restore hooks.json after installation error: {rollback_error}"
                )),
            };
        }
    }
    info!("Codex CLI hooks configured");
    Ok(())
}

fn rollback_codex_hooks_json(
    target: &Path,
    original: Option<&str>,
    installed: &str,
    installed_identity: &str,
) -> Result<(), String> {
    verify_written_hooks(target, installed, installed_identity)?;
    if let Some(original) = original {
        save_text_atomically_with(target, original, |temporary, destination| {
            verify_written_hooks(destination, installed, installed_identity)
                .map_err(std::io::Error::other)?;
            replace_file(temporary, destination)
        })
    } else {
        remove_new_codex_hooks_if_owned(target, installed, installed_identity)
    }
}

fn remove_new_codex_hooks_if_owned(
    target: &Path,
    installed: &str,
    installed_identity: &str,
) -> Result<(), String> {
    // Rename first, then verify the moved inode before deleting it. If an
    // external editor replaced hooks.json after the earlier verification, its
    // file is restored (or retained at the reported quarantine path), never
    // deleted as though it were our newly created file.
    let quarantine = temporary_path(target)?;
    std::fs::rename(target, &quarantine)
        .map_err(|error| format!("{}: {error}", target.display()))?;
    if let Err(error) = verify_written_hooks(&quarantine, installed, installed_identity) {
        return match create_file_if_absent(&quarantine, target) {
            Ok(()) => Err(error),
            Err(restore_error) => Err(format!(
                "{error}; replacement retained at {} because restoration failed: {restore_error}",
                quarantine.display()
            )),
        };
    }
    std::fs::remove_file(&quarantine)
        .map_err(|error| format!("{}: {error}", quarantine.display()))
}

fn verify_written_hooks(target: &Path, expected: &str, identity: &str) -> Result<(), String> {
    if config_file_identity(target)? != identity {
        return Err(format!("{}: hooks file identity changed; refusing rollback", target.display()));
    }
    let content = std::fs::read_to_string(target)
        .map_err(|error| format!("{}: {error}", target.display()))?;
    if content != expected {
        return Err(format!("{}: hooks changed during install; refusing rollback", target.display()));
    }
    Ok(())
}

fn verify_hooks_snapshot(
    path: &Path,
    target: &Path,
    expected: Option<&str>,
    identity: Option<&str>,
) -> Result<(), String> {
    if let Some(expected) = expected {
        let current_target = std::fs::canonicalize(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if current_target != target {
            return Err(format!("{}: hooks target changed during install", path.display()));
        }
        verify_written_hooks(target, expected, identity.ok_or("missing hooks identity")?)
    } else {
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(format!("{}: hooks file appeared during install", path.display())),
            Err(error) => Err(format!("{}: {error}", path.display())),
        }
    }
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

fn create_file_if_absent(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::hard_link(source, destination)?;
    if let Err(error) = std::fs::remove_file(source) {
        log::warn!(
            "created {} but could not clean temporary {}: {error}",
            destination.display(),
            source.display()
        );
    }
    Ok(())
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
        // Acceptance gate: the enable flow must also write the real hook entries
        // (5 events) into hooks.json, not just flip the feature flag.
        let hooks: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read hooks"))
                .expect("hooks remain valid JSON");
        assert_eq!(marker_count(&hooks), 5);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_enables_canonical_hooks_flag_and_reconciles_alias() {
        for (name, original) in [
            (
                "canonical-false",
                "# keep this comment\n[features]\nhooks = false # canonical\nother = 42\n",
            ),
            (
                "canonical-false-alias-true",
                "# keep this comment\n[features]\nhooks = false # canonical\ncodex_hooks = true # legacy\n",
            ),
            (
                "canonical-true-alias-false",
                "# keep this comment\n[features]\nhooks = true # canonical\ncodex_hooks = false # legacy\n",
            ),
            (
                "both-false",
                "# keep this comment\n[features]\nhooks = false # canonical\ncodex_hooks = false # legacy\n",
            ),
        ] {
            let directory = tmp_settings_path(name);
            std::fs::create_dir_all(&directory).expect("mkdir fixture");
            let hooks_path = directory.join("hooks.json");
            let config_path = directory.join("config.toml");
            write_raw(&hooks_path, br#"{"hooks":{}}"#);
            write_raw(&config_path, original.as_bytes());
            let provider = provider_with_path(&hooks_path);

            install_provider("codex", &provider).expect("install hooks");
            let installed = std::fs::read_to_string(&config_path).expect("read config");
            let document = installed
                .parse::<toml_edit::DocumentMut>()
                .expect("valid TOML after install");
            assert_eq!(document["features"]["hooks"].as_bool(), Some(true), "{name}");
            if original.contains("codex_hooks =") {
                assert_eq!(
                    document["features"]["codex_hooks"].as_bool(),
                    Some(true),
                    "{name}"
                );
                assert!(installed.contains("# legacy"), "{name}: legacy decor lost");
            }
            assert!(installed.contains("# canonical"), "{name}: canonical decor lost");
            assert!(installed.contains("# keep this comment"), "{name}");

            install_provider("codex", &provider).expect("reinstall hooks");
            assert_eq!(
                std::fs::read_to_string(&config_path).expect("read reinstalled config"),
                installed,
                "{name}: reinstall must leave config unchanged"
            );
            let state_path = codex_flag_state_path(&config_path).expect("state path");
            let owned = read_codex_flag_state(&state_path)
                .expect("owned flags")
                .expect("state after enable")
                .enabled_from_false;
            assert_eq!(
                owned.contains(&"hooks".to_string()),
                original.contains("hooks = false # canonical"),
                "{name}: canonical ownership"
            );
            assert_eq!(
                owned.contains(&"codex_hooks".to_string()),
                original.contains("codex_hooks = false # legacy"),
                "{name}: alias ownership"
            );
            remove_provider("codex", &provider).expect("remove hooks");
            assert_eq!(
                std::fs::read_to_string(&config_path).expect("read removed config"),
                original,
                "{name}: remove must restore only flags changed from false"
            );
            assert!(!state_path.exists(), "{name}: ownership state must be removed");
            let hooks: Value = serde_json::from_str(
                &std::fs::read_to_string(&hooks_path).expect("read removed hooks"),
            )
            .expect("valid hooks JSON");
            assert_eq!(marker_count(&hooks), 0, "{name}");
            let _ = std::fs::remove_dir_all(&directory);
        }
    }

    #[test]
    fn codex_install_rejects_non_boolean_canonical_hooks_flag() {
        let directory = tmp_settings_path("codex-canonical-nonbool");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let hooks_path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        let original_hooks = br#"{"hooks":{}}"#;
        let original_config = b"[features]\nhooks = \"yes\"\ncodex_hooks = false\n";
        write_raw(&hooks_path, original_hooks);
        write_raw(&config_path, original_config);

        let error = install_provider("codex", &provider_with_path(&hooks_path))
            .expect_err("non-boolean canonical feature must fail closed");
        assert!(error.contains("[features].hooks must be a boolean"));
        assert_eq!(
            std::fs::read(&hooks_path).expect("read hooks"),
            original_hooks
        );
        assert_eq!(
            std::fs::read(&config_path).expect("read config"),
            original_config
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_leaves_already_enabled_flag_byte_identical() {
        // Issue #3 fixture: flag already `true`. Install must not rewrite
        // config.toml at all (no spurious .bak churn, no decor loss risk).
        let directory = tmp_settings_path("codex-feature-already-true");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        let original_config = b"# my codex config\n[features]\ncodex_hooks = true # user enabled\nother = 1\n";
        write_raw(&config_path, original_config);

        install_provider("codex", &provider_with_path(&path)).expect("install codex hooks");

        assert_eq!(
            std::fs::read(&config_path).expect("read config"),
            original_config,
            "already-enabled config.toml must be left byte-identical"
        );
        let hooks: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read hooks"))
                .expect("hooks remain valid JSON");
        assert_eq!(marker_count(&hooks), 5);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_rejects_non_boolean_feature_flag() {
        // Issue #3 fixture: `codex_hooks` present but not a boolean. Fail
        // closed — never append a second/conflicting key, never touch files.
        let directory = tmp_settings_path("codex-feature-nonbool");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        let original_hooks = br#"{"hooks":{}}"#;
        let original_config = b"[features]\ncodex_hooks = \"yes\"\n";
        write_raw(&path, original_hooks);
        write_raw(&config_path, original_config);

        let result = install_provider("codex", &provider_with_path(&path));
        assert!(result.is_err(), "non-boolean codex_hooks must fail closed");
        assert!(
            result.unwrap_err().contains("must be a boolean"),
            "error should name the boolean contract"
        );
        assert_eq!(std::fs::read(&path).expect("read hooks"), original_hooks);
        assert_eq!(
            std::fs::read(&config_path).expect("read config"),
            original_config
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_does_not_confuse_other_section_codex_hooks_key() {
        // Issue #3 fixture: a *real* `codex_hooks` key under a different table
        // is not the feature flag — `[features]` must still get its own entry.
        let directory = tmp_settings_path("codex-feature-other-key");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(
            &config_path,
            b"[other]\ncodex_hooks = false\n[features]\nexisting = true\n",
        );

        install_provider("codex", &provider_with_path(&path)).expect("install codex hooks");

        let config = std::fs::read_to_string(&config_path).expect("read config");
        let document = config
            .parse::<toml_edit::DocumentMut>()
            .expect("updated config remains valid TOML");
        assert_eq!(document["features"]["hooks"].as_bool(), Some(true));
        assert_eq!(
            document["other"]["codex_hooks"].as_bool(),
            Some(false),
            "unrelated [other].codex_hooks key must not be modified"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_remove_preserves_user_owned_true_flag() {
        // Issue #3 contract: remove/disable must not clobber a user-owned
        // `codex_hooks = true`. Removal leaves user-owned true byte-identical.
        let directory = tmp_settings_path("codex-feature-user-true");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        let original_config = b"[features]\ncodex_hooks = true # user-owned\n";
        write_raw(&config_path, original_config);
        let provider = provider_with_path(&path);

        install_provider("codex", &provider).expect("install codex hooks");
        remove_provider("codex", &provider).expect("remove LobsterPulse hooks");

        assert_eq!(
            std::fs::read(&config_path).expect("read config"),
            original_config,
            "remove must not rewrite user-owned true"
        );
        let hooks: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read hooks"))
                .expect("hooks remain valid JSON");
        assert_eq!(marker_count(&hooks), 0);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_remove_restores_owned_flag_after_unrelated_config_edit() {
        let directory = tmp_settings_path("codex-owned-flag-unrelated-edit");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(
            &config_path,
            b"[features]\nhooks = false # user disabled\n[other]\nname = \"first\"\n",
        );
        let provider = provider_with_path(&path);

        install_provider("codex", &provider).expect("enable hooks");
        let enabled = std::fs::read_to_string(&config_path).expect("read enabled config");
        assert!(enabled.contains("hooks = true # user disabled"));
        std::fs::write(&config_path, enabled.replace("name = \"first\"", "name = \"later\""))
            .expect("user edits unrelated setting");

        remove_provider("codex", &provider).expect("remove LobsterPulse hooks");
        let removed = std::fs::read_to_string(&config_path).expect("read restored config");
        assert!(removed.contains("hooks = false # user disabled"));
        assert!(removed.contains("name = \"later\""));
        assert!(!codex_flag_state_path(&config_path).expect("state path").exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_remove_rejects_wholesale_config_replacement() {
        let directory = tmp_settings_path("codex-owned-flag-replaced-config");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let hooks_path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&hooks_path, br#"{"hooks":{}}"#);
        write_raw(&config_path, b"[features]\nhooks = false\n");
        let provider = provider_with_path(&hooks_path);

        install_provider("codex", &provider).expect("enable hooks");
        let replacement = "[features]\nhooks = true # managed by user\n";
        save_text_atomically(&config_path, replacement).expect("replace config wholesale");
        let error = remove_provider("codex", &provider).expect_err("must reject replacement");
        assert!(error.contains("file identity changed"));
        assert_eq!(std::fs::read_to_string(&config_path).expect("read config"), replacement);
        assert!(codex_flag_state_path(&config_path).expect("state path").exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_retry_commits_after_prepared_enable_did_not_replace_config() {
        let directory = tmp_settings_path("codex-owned-flag-retry");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let hooks_path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&hooks_path, br#"{"hooks":{}}"#);
        write_raw(&config_path, b"[features]\nhooks = false\n");
        let target = std::fs::canonicalize(&config_path).expect("resolve target");
        let state_path = codex_flag_state_path(&config_path).expect("state path");
        let original_identity = config_file_identity(&target).expect("original identity");
        write_codex_flag_state(
            &state_path,
            &CodexFlagState {
                enabled_from_false: vec!["hooks".to_string()],
                target: target.to_str().expect("UTF-8 fixture path").to_string(),
                identity: original_identity.clone(),
                identity_owned: false,
                pending_identity: Some("uninstalled-temporary-inode".to_string()),
                pending_owned: true,
            },
        )
        .expect("write preliminary state");
        let provider = provider_with_path(&hooks_path);

        install_provider("codex", &provider).expect("retry enable");
        let state = read_codex_flag_state(&state_path)
            .expect("read state")
            .expect("state remains after enable");
        assert_eq!(state.identity, original_identity);
        assert_eq!(
            state.pending_identity.as_deref(),
            Some(config_file_identity(&target).expect("installed identity").as_str())
        );
        assert!(state.pending_owned);
        remove_provider("codex", &provider).expect("restore after retry");
        assert_eq!(
            std::fs::read_to_string(&config_path).expect("read restored config"),
            "[features]\nhooks = false\n"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_remove_cleans_committed_restore_state() {
        let directory = tmp_settings_path("codex-owned-flag-restored-state");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let hooks_path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&hooks_path, br#"{"hooks":{}}"#);
        write_raw(&config_path, b"[features]\nhooks = false\n");
        let provider = provider_with_path(&hooks_path);
        install_provider("codex", &provider).expect("enable hooks");

        let target = std::fs::canonicalize(&config_path).expect("resolve target");
        let enabled_identity = config_file_identity(&target).expect("enabled identity");
        save_text_atomically(&target, "[features]\nhooks = false\n")
            .expect("simulate completed restore replacement");
        let restored_identity = config_file_identity(&target).expect("restored identity");
        let state_path = codex_flag_state_path(&config_path).expect("state path");
        write_codex_flag_state(
            &state_path,
            &CodexFlagState {
                enabled_from_false: vec!["hooks".to_string()],
                target: target.to_str().expect("UTF-8 fixture path").to_string(),
                identity: enabled_identity,
                identity_owned: true,
                pending_identity: Some(restored_identity),
                pending_owned: false,
            },
        )
        .expect("simulate state deletion failure after restore");

        remove_provider("codex", &provider).expect("finish pending restoration");
        assert_eq!(
            std::fs::read_to_string(&config_path).expect("read config"),
            "[features]\nhooks = false\n"
        );
        assert!(!state_path.exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_and_remove_reject_another_in_progress_update() {
        let directory = tmp_settings_path("codex-config-lock");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let hooks_path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&hooks_path, br#"{"hooks":{}}"#);
        write_raw(&config_path, b"[features]\nhooks = false\n");
        let provider = provider_with_path(&hooks_path);
        let lock = lock_codex_hooks(&hooks_path).expect("hold lock");

        assert!(install_provider("codex", &provider)
            .expect_err("concurrent install must fail")
            .contains("already in progress"));
        assert!(remove_provider("codex", &provider)
            .expect_err("concurrent remove must fail")
            .contains("already in progress"));
        assert_eq!(
            std::fs::read_to_string(&config_path).expect("read config"),
            "[features]\nhooks = false\n"
        );

        drop(lock);
        install_provider("codex", &provider).expect("install after lock release");
        remove_provider("codex", &provider).expect("remove after lock release");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_new_config_commit_preserves_concurrent_creator() {
        let directory = tmp_settings_path("codex-concurrent-config-create");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let config_path = directory.join("config.toml");
        let external = "[features]\nhooks = false # created concurrently\n";
        let result = save_text_atomically_with(
            &config_path,
            "[features]\nhooks = true\n",
            |temporary, destination| {
                std::fs::write(destination, external)?;
                create_file_if_absent(temporary, destination)
            },
        );
        assert!(result.is_err(), "atomic create must reject existing config");
        assert_eq!(
            std::fs::read_to_string(&config_path).expect("read external config"),
            external
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn codex_remove_rejects_repointed_config_symlink() {
        use std::os::unix::fs::symlink;

        let directory = tmp_settings_path("codex-owned-flag-repointed-config");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let hooks_path = directory.join("hooks.json");
        let original = directory.join("original.toml");
        let replacement = directory.join("replacement.toml");
        let config_path = directory.join("config.toml");
        write_raw(&hooks_path, br#"{"hooks":{}}"#);
        write_raw(&original, b"[features]\nhooks = false\n");
        write_raw(&replacement, b"[features]\nhooks = true # user-owned\n");
        symlink(&original, &config_path).expect("create config symlink");
        let provider = provider_with_path(&hooks_path);

        install_provider("codex", &provider).expect("enable original target");
        std::fs::remove_file(&config_path).expect("unlink original target");
        symlink(&replacement, &config_path).expect("repoint config symlink");
        let error = remove_provider("codex", &provider).expect_err("must reject repointed link");
        assert!(error.contains("config target or file identity changed"));
        assert_eq!(
            std::fs::read_to_string(&replacement).expect("read new target"),
            "[features]\nhooks = true # user-owned\n"
        );
        assert!(std::fs::read_to_string(&original)
            .expect("read original target")
            .contains("hooks = true"));
        assert!(codex_flag_state_path(&config_path).expect("state path").exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_remove_restores_owned_flag_when_hooks_file_is_missing() {
        let directory = tmp_settings_path("codex-owned-flag-missing-hooks");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(&config_path, b"[features]\nhooks = false\n");
        let provider = provider_with_path(&path);

        install_provider("codex", &provider).expect("enable hooks");
        std::fs::remove_file(&path).expect("simulate missing hooks.json");
        remove_provider("codex", &provider).expect("restore flag without hooks file");
        assert_eq!(
            std::fs::read_to_string(&config_path).expect("read restored config"),
            "[features]\nhooks = false\n"
        );
        assert!(!codex_flag_state_path(&config_path).expect("state path").exists());
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
        let document = config
            .parse::<toml_edit::DocumentMut>()
            .expect("updated config remains valid TOML");
        assert_eq!(document["features"]["hooks"].as_bool(), Some(true));
        assert_eq!(document["features"]["existing"].as_bool(), Some(true));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_handles_eof_features_header_without_newline() {
        let directory = tmp_settings_path("codex-feature-eof-header");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(&config_path, b"[features] # preserve header comment");

        install_provider("codex", &provider_with_path(&path)).expect("install codex hooks");

        let config = std::fs::read_to_string(&config_path).expect("read config");
        let document = config
            .parse::<toml_edit::DocumentMut>()
            .expect("updated config remains valid TOML");
        assert_eq!(document["features"]["hooks"].as_bool(), Some(true));
        assert!(config.contains("# preserve header comment"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_enables_dotted_feature_assignment() {
        let directory = tmp_settings_path("codex-feature-dotted");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(
            &config_path,
            b"features.codex_hooks = false # preserve dotted comment\n",
        );

        install_provider("codex", &provider_with_path(&path)).expect("install codex hooks");

        let config = std::fs::read_to_string(&config_path).expect("read config");
        let document = config
            .parse::<toml_edit::DocumentMut>()
            .expect("updated config remains valid TOML");
        assert_eq!(document["features"]["codex_hooks"].as_bool(), Some(true));
        assert!(config.contains("# preserve dotted comment"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_enables_inline_feature_assignment() {
        let directory = tmp_settings_path("codex-feature-inline");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(
            &config_path,
            b"features = { codex_hooks = false, existing = true } # preserve inline comment\n",
        );

        install_provider("codex", &provider_with_path(&path)).expect("install codex hooks");

        let config = std::fs::read_to_string(&config_path).expect("read config");
        let document = config
            .parse::<toml_edit::DocumentMut>()
            .expect("updated config remains valid TOML");
        assert_eq!(document["features"]["codex_hooks"].as_bool(), Some(true));
        assert_eq!(document["features"]["existing"].as_bool(), Some(true));
        assert!(config.contains("# preserve inline comment"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_preserves_multiline_toml_strings() {
        let directory = tmp_settings_path("codex-feature-multiline");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        let original_config = br#"[other]
basic = """
line one
# text inside the string
"""
literal = '''
line two
'''
[features]
codex_hooks = false
"#;
        write_raw(&config_path, original_config);

        install_provider("codex", &provider_with_path(&path)).expect("install codex hooks");

        let config = std::fs::read_to_string(&config_path).expect("read config");
        let document = config
            .parse::<toml_edit::DocumentMut>()
            .expect("updated config remains valid TOML");
        assert_eq!(document["features"]["codex_hooks"].as_bool(), Some(true));
        assert_eq!(
            document["other"]["basic"].as_str(),
            Some("line one\n# text inside the string\n")
        );
        assert_eq!(
            document["other"]["literal"].as_str(),
            Some("line two\n")
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_reinstall_remove_restores_owned_false_flag() {
        let directory = tmp_settings_path("codex-feature-lifecycle");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        write_raw(&path, br#"{"hooks":{}}"#);
        write_raw(&config_path, b"[features]\ncodex_hooks = false\n");
        let provider = provider_with_path(&path);

        install_provider("codex", &provider).expect("first install");
        install_provider("codex", &provider).expect("idempotent reinstall");
        remove_provider("codex", &provider).expect("remove LobsterPulse hooks");

        let config = std::fs::read_to_string(&config_path).expect("read config");
        let document = config
            .parse::<toml_edit::DocumentMut>()
            .expect("config remains valid TOML");
        assert_eq!(
            document["features"]["codex_hooks"].as_bool(),
            Some(false),
            "remove restores the explicit false that LobsterPulse changed"
        );
        assert!(!codex_flag_state_path(&config_path).expect("state path").exists());
        let hooks: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read hooks"))
                .expect("hooks remain valid JSON");
        assert!(!hooks.to_string().contains(MARKER));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_creates_missing_config_and_roundtrips_hooks() {
        let home = tmp_settings_path("missing-codex-config");
        let codex_dir = home.join(".codex");
        std::fs::create_dir_all(&codex_dir).expect("mkdir isolated Codex home");
        let hooks_path = codex_dir.join("hooks.json");
        let config_path = codex_dir.join("config.toml");
        write_raw(
            &hooks_path,
            br#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"third-party-start"}]}]}}"#,
        );
        assert!(!config_path.exists(), "fixture must model a fresh Codex home");
        let provider = provider_with_path(&hooks_path);

        install_provider("codex", &provider).expect("install creates missing config");
        assert!(config_path.exists(), "install must create config.toml");
        let first_config = std::fs::read_to_string(&config_path).expect("read created config");
        let first_document = first_config
            .parse::<toml_edit::DocumentMut>()
            .expect("created config remains valid TOML");
        assert_eq!(
            first_document["features"]["hooks"].as_bool(),
            Some(true)
        );
        let first_hooks = std::fs::read_to_string(&hooks_path).expect("read installed hooks");
        assert!(first_hooks.contains(MARKER));
        assert!(first_hooks.contains("third-party-start"));

        install_provider("codex", &provider).expect("reinstall with created config");
        assert_eq!(
            std::fs::read_to_string(&hooks_path).expect("read reinstalled hooks"),
            first_hooks
        );
        assert_eq!(
            std::fs::read_to_string(&config_path).expect("read reinstalled config"),
            first_config
        );

        remove_provider("codex", &provider).expect("remove generated hooks");
        let removed_hooks: Value =
            serde_json::from_str(&std::fs::read_to_string(&hooks_path).expect("read removed hooks"))
                .expect("removed hooks remain valid JSON");
        assert!(removed_hooks.to_string().contains("third-party-start"));
        assert!(!removed_hooks.to_string().contains(MARKER));
        let removed_config = std::fs::read_to_string(&config_path).expect("read removed config");
        let removed_document = removed_config
            .parse::<toml_edit::DocumentMut>()
            .expect("removed config remains valid TOML");
        assert_eq!(
            removed_document["features"]["hooks"].as_bool(),
            Some(true),
            "remove must not disable shared Codex capability"
        );
        let _ = std::fs::remove_dir_all(&home);
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
    fn codex_install_json_write_failure_does_not_enable_feature() {
        let directory = tmp_settings_path("codex-hooks-write-failure");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let hooks_path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        let original_hooks = br#"{"hooks":{}}"#;
        let original_config = b"[features]\nhooks = false\n";
        write_raw(&hooks_path, original_hooks);
        write_raw(&config_path, original_config);
        let backup = backup_path(&hooks_path).expect("backup path");
        std::fs::create_dir(&backup).expect("block hooks backup copy");

        let error = install_provider("codex", &provider_with_path(&hooks_path))
            .expect_err("hooks write must fail");
        assert!(error.contains("failed to back up"), "{error}");
        assert_eq!(std::fs::read(&hooks_path).expect("read hooks"), original_hooks);
        assert_eq!(std::fs::read(&config_path).expect("read config"), original_config);
        assert!(!codex_flag_state_path(&config_path).expect("state path").exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_remove_without_settings_does_not_create_home_or_lock() {
        let directory = tmp_settings_path("codex-remove-noop");
        let hooks_path = directory.join("hooks.json");
        assert!(!directory.exists());
        remove_provider("codex", &provider_with_path(&hooks_path))
            .expect("missing Codex settings are a no-op");
        assert!(!directory.exists(), "no-op removal must not create Codex home");
    }

    #[test]
    fn codex_remove_observes_in_progress_first_install_lock() {
        let directory = tmp_settings_path("codex-remove-first-install-lock");
        let hooks_path = directory.join("hooks.json");
        let provider = provider_with_path(&hooks_path);
        let lock = lock_codex_hooks(&hooks_path).expect("first install holds lock");
        assert!(!hooks_path.exists());
        let error = remove_provider("codex", &provider)
            .expect_err("removal must see in-progress first install");
        assert!(error.contains("already in progress"), "{error}");
        drop(lock);
        remove_provider("codex", &provider).expect("no-op after installer releases lock");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_install_flag_error_removes_new_hooks_file() {
        let directory = tmp_settings_path("codex-enable-failure-new-hooks");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let hooks_path = directory.join("hooks.json");
        let config_path = directory.join("config.toml");
        let original_config = b"[features]\nhooks = \"invalid\"\n";
        write_raw(&config_path, original_config);

        let error = install_provider("codex", &provider_with_path(&hooks_path))
            .expect_err("invalid feature must fail");
        assert!(error.contains("must be a boolean"), "{error}");
        assert!(!hooks_path.exists(), "new hooks file must be rolled back");
        assert_eq!(std::fs::read(&config_path).expect("read config"), original_config);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_hooks_rollback_rejects_replaced_file_with_same_text() {
        let directory = tmp_settings_path("codex-hooks-replaced-rollback");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let target = directory.join("hooks.json");
        let installed = r#"{"hooks":{"SessionStart":[]}}"#;
        write_raw(&target, installed.as_bytes());
        let identity = config_file_identity(&target).expect("original identity");
        save_text_atomically(&target, installed).expect("replace with identical bytes");

        let error = rollback_codex_hooks_json(&target, Some("{}"), installed, &identity)
            .expect_err("replacement must not be overwritten");
        assert!(error.contains("identity changed"), "{error}");
        assert_eq!(std::fs::read_to_string(&target).expect("read replacement"), installed);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn codex_hooks_quarantine_preserves_replacement_after_verification_race() {
        let directory = tmp_settings_path("codex-hooks-quarantine-race");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let target = directory.join("hooks.json");
        let installed = r#"{"hooks":{"SessionStart":[]}}"#;
        write_raw(&target, installed.as_bytes());
        let owned_identity = config_file_identity(&target).expect("owned identity");
        save_text_atomically(&target, installed).expect("external same-text replacement");
        let replacement_identity = config_file_identity(&target).expect("replacement identity");
        assert_ne!(owned_identity, replacement_identity);

        let error = remove_new_codex_hooks_if_owned(&target, installed, &owned_identity)
            .expect_err("replacement must survive quarantine check");
        assert!(error.contains("identity changed"), "{error}");
        assert_eq!(std::fs::read_to_string(&target).expect("read replacement"), installed);
        assert_eq!(config_file_identity(&target).expect("restored identity"), replacement_identity);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn codex_hooks_repointed_symlink_cannot_change_install_target() {
        use std::os::unix::fs::symlink;
        let directory = tmp_settings_path("codex-hooks-repointed-link");
        std::fs::create_dir_all(&directory).expect("mkdir fixture");
        let target_a = directory.join("hooks-a.json");
        let target_b = directory.join("hooks-b.json");
        let link = directory.join("hooks.json");
        let original = r#"{"original":true}"#;
        let installed = r#"{"hooks":{"SessionStart":[]}}"#;
        write_raw(&target_a, original.as_bytes());
        write_raw(&target_b, installed.as_bytes());
        symlink(&target_a, &link).expect("link to original target");
        let identity = config_file_identity(&target_a).expect("original identity");
        std::fs::remove_file(&link).expect("unlink first target");
        symlink(&target_b, &link).expect("repoint link");

        let error = verify_hooks_snapshot(&link, &target_a, Some(original), Some(&identity))
            .expect_err("repointed link must fail install");
        assert!(error.contains("target changed"), "{error}");
        assert_eq!(std::fs::read_to_string(&target_b).expect("read new target"), installed);

        // A rollback after the write is bound to target A, never the current
        // symlink target B, even when B has the same installed JSON text.
        save_text_atomically(&target_a, installed).expect("simulate installed file");
        let installed_identity = config_file_identity(&target_a).expect("installed identity");
        rollback_codex_hooks_json(&target_a, Some(original), installed, &installed_identity)
            .expect("restore original target");
        assert_eq!(std::fs::read_to_string(&target_a).expect("read old target"), original);
        assert_eq!(std::fs::read_to_string(&target_b).expect("read new target"), installed);
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
