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
    save_config(&config).ok();
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
        save_config(&config).ok();
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
    // K8 落地：傳入 `now` 給 pure fn 計算 per-provider idle_seconds，
    // 把時鐘從 pure fn 隔離出來、unit test 注入固定時間驗證 idle 數學。
    render_prometheus_body(
        &state.sessions,
        state.session_count as u64,
        state.active_count as u64,
        &state.provider_totals,
        Utc::now(),
    )
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
fn render_prometheus_body(
    sessions: &[session::SessionInfo],
    session_count: u64,
    active_count: u64,
    provider_totals: &std::collections::HashMap<String, session::ProviderTotals>,
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
    use std::collections::HashMap;

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
                since: None,
                last_event_at: Some(Utc::now()),
            },
        )
    }

    #[test]
    fn empty_state_emits_zero_counters_and_no_provider_lines() {
        let body = render_prometheus_body(&[], 0, 0, &HashMap::new(), Utc::now());

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
    }

    #[test]
    fn single_inactive_session_reported_as_total_only() {
        let sessions = vec![info("claude", false, 100, 50)];
        let body = render_prometheus_body(
            &sessions,
            1,
            0,
            &totals_map(vec![totals("claude", 100, 50)]),
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
        let body = render_prometheus_body(&sessions, 4, 2, &HashMap::new(), Utc::now());

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
            &[], // 0 個 live session
            0,
            0,
            &totals_map(vec![
                totals_with_failures("claude", 0, 0, 0), // 沒失敗
                totals_with_failures("codex", 0, 0, 5),  // 5 次失敗
                totals_with_failures("cicx", 0, 0, 2),   // 2 次失敗
            ]),
            Utc::now(),
        );

        // 即使 live sessions = []，failure metric 仍要反映出 ProviderTotals 累計
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"claude\"} 0\n"));
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"codex\"} 5\n"));
        assert!(body.contains("lobsterpulse_provider_failure_count{provider=\"cicx\"} 2\n"));
    }

    #[test]
    fn idle_seconds_empty_state_emits_header_only() {
        // 沒有任何 provider → idle 段只有 HELP/TYPE、沒有 sample line。
        // 對齊 K6/K7「empty state 不假裝 0 秒 idle」語意。
        let body = render_prometheus_body(&[], 0, 0, &HashMap::new(), Utc::now());

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
            &[], // 0 個 live session
            0,
            0,
            &totals_map(vec![
                totals_at("claude", 0, 0, 0, now - chrono::Duration::seconds(60)),
                totals_at("cicx", 0, 0, 0, now - chrono::Duration::seconds(30)),
            ]),
            now,
        );

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
            now,
        );

        // clamp 為 0 而非 -1
        assert!(body.contains("lobsterpulse_provider_idle_seconds{provider=\"claude\"} 0\n"));
    }

    #[test]
    fn session_count_empty_state_emits_header_only() {
        // 沒任何 provider → K9 段只有 HELP/TYPE、沒有 sample line。
        // 對齊 K6/K7/K8「empty state 不假裝 0 session」語意。
        let body = render_prometheus_body(&[], 0, 0, &HashMap::new(), Utc::now());

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
            &[], // 0 個 live session
            0,
            0,
            &totals_map(vec![
                totals_with_session_count("cicx", 5),   // 5 個 session 累計
                totals_with_session_count("codex", 1),  // 1 個 session
                totals_with_session_count("claude", 0), // 0（理論不會出現，但驗 0 也輸出）
            ]),
            Utc::now(),
        );

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
}
