mod auto_rules;
mod config;
mod discord;
mod hook_event;
mod hook_server;
mod hooks_configurator;
mod openab_bridge;
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

/// Get the sounds directory, creating it if needed
fn sounds_dir() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config"))
        .join("lobsterpulse")
        .join("sounds");
    let _ = std::fs::create_dir_all(&dir);
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
    ];
    for (name, bytes) in defaults {
        let path = dir.join(name);
        if !path.exists() {
            let _ = std::fs::write(&path, bytes);
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

/// 讀取 OpenAB 5 個 bot 的 snapshot + LobsterPulse 自建 local snapshot。
/// 路徑：~/.lobsterpulse/usage-{bot_id}.json + usage-local.json。
/// 前端 refreshQuotas 會優先用 __local__（LobsterPulse 自跑的）作全域 quota 來源。
#[tauri::command]
fn read_usage_snapshots() -> std::collections::HashMap<String, Option<serde_json::Value>> {
    let mut out = std::collections::HashMap::new();
    let Some(home) = dirs::home_dir() else {
        return out;
    };
    let dir = home.join(".lobsterpulse");
    for bot in ["cicx", "gitx", "giminix", "codex_bot", "openx"] {
        let path = dir.join(format!("usage-{bot}.json"));
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        out.insert(bot.to_string(), data);
    }
    // Legacy：OpenAB BackendType::Other 寫 usage-bot.json 當 OPENX
    if out.get("openx").and_then(|v| v.as_ref()).is_none() {
        let path = dir.join("usage-bot.json");
        if let Some(data) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        {
            out.insert("openx".to_string(), Some(data));
        }
    }
    // LobsterPulse 自跑的 usage runner 寫 usage-local.json
    let local_path = dir.join("usage-local.json");
    let local_data = std::fs::read_to_string(&local_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    out.insert("__local__".to_string(), local_data);
    out
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
    let _ = std::fs::create_dir_all(&dir);

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
        // 5 個 OpenAB bot + 1 個 local runner 全填 None，caller 端 filter 後不出現
        // 在 metric map（age 段就只 emit header、沒 sample）。
        for p in ["cicx", "gitx", "giminix", "codex_bot", "openx", "__local__"] {
            out.insert(p.to_string(), None);
        }
        return out;
    };
    for bot in ["cicx", "gitx", "giminix", "codex_bot", "openx"] {
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
// `render_prometheus_body` 累積 8 個正交輸入（sessions / counts / provider_totals /
// quota_snapshot_ages / discord_health / hook_parse_failures / now），每個都是
// 來自不同 process-level state 的純 snapshot。把它們打包成 struct 沒比較乾淨
// —— render 端是純函式,沒有 mut / no allocation 切換,純粹 string 構造。
// 對齊 R26/R27 政策：cross-cutting snapshot 整合留給 M1 輪(K15 candidate:
// `MetricsSnapshot` struct 餵前端,render 端也順手用同個 struct),不在本輪
// 重構。
#[allow(clippy::too_many_arguments)]
fn render_prometheus_body(
    sessions: &[session::SessionInfo],
    session_count: u64,
    active_count: u64,
    provider_totals: &std::collections::HashMap<String, session::ProviderTotals>,
    quota_snapshot_ages: &std::collections::HashMap<String, i64>,
    discord_health: &discord::DiscordHealth,
    hook_metrics: hook_server::HookServerMetrics,
    now: DateTime<Utc>,
) -> String {
    let mut provider_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut provider_active: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for s in sessions {
        *provider_counts.entry(s.provider.clone()).or_default() += 1;
        if s.is_active {
            *provider_active.entry(s.provider.clone()).or_default() += 1;
        }
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
    // K13 落地：per-provider lifetime event 計數（counter）—— 該 provider 累計
    // 收過幾個 event。`ProviderTotals.events_total` 在 `bump_provider_totals` 內對
    // 任何 event 類型（SessionStart / PostToolUse / PostToolUseFailure / Stop /
    // Notification / TokenUpdate / ...）都 `+= 1`。對齊 K6/K7/K9 lifetime
    // aggregate：session 結束 / 30 min stale 回收後仍保留 → Prometheus 不會誤判
    // counter 倒退。operator 端 `rate(events_total[5m])` = 該 provider ingest
    // 吞吐量，補 K7 failure / K9 session 沒覆蓋的「整體事件流量」信號。
    let mut provider_events_total: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
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
    }

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
    out.push_str("# HELP lobsterpulse_tokens_input Lifetime input tokens across all providers\n# TYPE lobsterpulse_tokens_input counter\n");
    out.push_str(&format!("lobsterpulse_tokens_input {tot_in}\n"));
    out.push_str("# HELP lobsterpulse_tokens_output Lifetime output tokens across all providers\n# TYPE lobsterpulse_tokens_output counter\n");
    out.push_str(&format!("lobsterpulse_tokens_output {tot_out}\n"));
    out.push_str("# HELP lobsterpulse_provider_tokens_input Lifetime input tokens per provider\n# TYPE lobsterpulse_provider_tokens_input counter\n");
    for (p, n) in &provider_in_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_tokens_input{{provider=\"{p}\"}} {n}\n"
        ));
    }
    out.push_str("# HELP lobsterpulse_provider_tokens_output Lifetime output tokens per provider\n# TYPE lobsterpulse_provider_tokens_output counter\n");
    for (p, n) in &provider_out_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_tokens_output{{provider=\"{p}\"}} {n}\n"
        ));
    }
    // K7 落地：per-provider 失敗計數（lifetime aggregate）。
    // `failure_count` 來源是 `ProviderTotals`，由 `bump_provider_totals` 在
    // `PostToolUseFailure` 事件時 `+= 1` 累加；不依賴 live session（失敗事件
    // 之後 session 仍會轉 idle/移除，但累計保留在 ProviderTotals 不蒸發）。
    out.push_str("# HELP lobsterpulse_provider_failure_count Lifetime tool/post failure count per provider\n# TYPE lobsterpulse_provider_failure_count counter\n");
    for (p, n) in &provider_fail_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_failure_count{{provider=\"{p}\"}} {n}\n"
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
    out.push_str("# HELP lobsterpulse_provider_session_count Lifetime session count per provider\n# TYPE lobsterpulse_provider_session_count counter\n");
    for (p, n) in &provider_session_count_sorted {
        out.push_str(&format!(
            "lobsterpulse_provider_session_count{{provider=\"{p}\"}} {n}\n"
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
    quota_history::load_history().unwrap_or_default()
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
                                let transition = {
                                    let mut m = mgr.0.lock().unwrap();
                                    m.handle_event(&event)
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
                            let _ = sock.write_all(resp.as_bytes()).await;
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
    use crate::session::{ProviderTotals, SessionInfo, SessionState};
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
                since: None,
                last_event_at: Some(Utc::now()),
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
                since: None,
                last_event_at: Some(Utc::now()),
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
                since: None,
                last_event_at: Some(last_at),
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
                since: None,
                last_event_at: None,
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
                since: None,
                last_event_at: Some(Utc::now()),
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
                since: Some(since),
                last_event_at: Some(since),
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
                since: None,
                last_event_at: Some(Utc::now()),
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
                since: Some(since),
                last_event_at: Some(last_event_at),
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
                since: Some(since),
                last_event_at: None,
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
                since: Some(now),
                last_event_at: Some(now),
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
                since: None,
                last_event_at: Some(Utc::now()),
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
        // K10 落地：per-provider since_timestamp 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_since_timestamp{"));
        // K11 落地：per-provider quota_snapshot_age 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_quota_snapshot_age_seconds{"));
        // K12 落地：per-provider idle_ratio 段同樣：空 map → 沒 sample line
        assert!(!body.contains("lobsterpulse_provider_idle_ratio{"));
        // K15 落地：lifetime parse failures counter
        assert!(body.contains("lobsterpulse_hook_parse_failures_total 0\n"));
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
            // K13 新增：per-provider lifetime event counter（任何 event 都 +1）
            "# HELP lobsterpulse_provider_events_total",
            "# TYPE lobsterpulse_provider_events_total counter",
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
        // 5 個 OpenAB bot + 1 個 local runner 全填 None，caller 端 filter 後不出現在
        // metric map（age 段就只 emit header、沒 sample）。
        let out = collect_quota_snapshot_mtimes(&None);

        assert_eq!(out.len(), 6, "5 OpenAB bot + __local__ 共 6 個 key");
        for p in ["cicx", "gitx", "giminix", "codex_bot", "openx", "__local__"] {
            assert_eq!(out.get(p).copied(), Some(None), "{p} 應該是 None");
        }
    }

    #[test]
    fn collect_quota_snapshot_mtimes_returns_mtime_for_existing_files() {
        // 主軸：home 存在 → 對 `~/.lobsterpulse/usage-{bot}.json` 與 `usage-local.json`
        // 做 `metadata()`，有檔案 → Some(mtime)、沒檔案 → None。
        // 寫 2 個假檔（cicx + __local__）→ 該 2 個 key 有 mtime、其他 4 個 None。
        // 注意：helper 內部會 `home.join(".lobsterpulse")` 當資料目錄，所以測試要把檔案寫
        // 在 `<tmp>/.lobsterpulse/` 下對齊 production shape。
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
        // 沒寫的 4 個 key → None
        for p in ["gitx", "giminix", "codex_bot", "openx"] {
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
            &h_5xx,
            hook_server::HookServerMetrics::default(),
            Utc::now(),
        );
        assert!(
            body_5xx.contains("lobsterpulse_discord_health 2\n"),
            "Server5xx → gauge 2, body: {body_5xx}"
        );
    }
}
