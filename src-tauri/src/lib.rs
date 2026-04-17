mod config;
mod hook_event;
mod hook_server;
mod hooks_configurator;
mod session;

use config::{AppConfig, load_config, save_config, detect_providers};
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

#[tauri::command]
fn get_config(config_state: tauri::State<AppConfigState>) -> AppConfig {
    config_state.0.lock().unwrap().clone()
}

#[tauri::command]
fn save_app_config(config_state: tauri::State<AppConfigState>, new_config: AppConfig) -> Result<(), String> {
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
        ("claude.mp3", include_bytes!("../../sounds/claude.mp3")),
        ("gemini.mp3", include_bytes!("../../sounds/gemini.mp3")),
        ("codex.mp3", include_bytes!("../../sounds/codex.mp3")),
        ("copilot.mp3", include_bytes!("../../sounds/copilot.mp3")),
        ("claude-waiting.mp3", include_bytes!("../../sounds/claude-waiting.mp3")),
        ("gemini-waiting.mp3", include_bytes!("../../sounds/gemini-waiting.mp3")),
        ("codex-waiting.mp3", include_bytes!("../../sounds/codex-waiting.mp3")),
        ("copilot-waiting.mp3", include_bytes!("../../sounds/copilot-waiting.mp3")),
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
                    if lower.ends_with(".mp3") || lower.ends_with(".wav") || lower.ends_with(".ogg") {
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
    if !path.exists() { return; }

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
    let opener = if cfg!(target_os = "macos") { "open" }
                 else if cfg!(target_os = "windows") { "explorer" }
                 else { "xdg-open" };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_app_config() -> Result<(), String> {
    let path = config::config_path();
    let opener = if cfg!(target_os = "macos") { "open" }
                 else if cfg!(target_os = "windows") { "explorer" }
                 else { "xdg-open" };
    std::process::Command::new(opener)
        .arg(path.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_sounds_folder() -> Result<(), String> {
    let dir = sounds_dir();
    let opener = if cfg!(target_os = "macos") { "open" }
                 else if cfg!(target_os = "windows") { "explorer" }
                 else { "xdg-open" };
    std::process::Command::new(opener)
        .arg(dir.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_provider_settings(provider_id: String, config_state: tauri::State<AppConfigState>) -> Result<(), String> {
    let config = config_state.0.lock().unwrap();
    let provider = config.providers.get(&provider_id)
        .ok_or(format!("Unknown provider: {provider_id}"))?;
    let path = provider.settings_path.as_ref()
        .ok_or("No settings path for this provider")?;
    let expanded = config::expand_path(path);

    // Use xdg-open on Linux, open on macOS, start on Windows
    let opener = if cfg!(target_os = "macos") { "open" }
                 else if cfg!(target_os = "windows") { "explorer" }
                 else { "xdg-open" };

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
    let provider = config.providers.get(&provider_id)
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


#[tauri::command]
fn bounce_window(window: tauri::WebviewWindow) {
    let win = window.clone();
    std::thread::spawn(move || {
        if let Ok(pos) = win.outer_position() {
            let scale = win.scale_factor().unwrap_or(1.0);
            let orig_y = pos.y as f64 / scale;
            let orig_x = pos.x as f64 / scale;

            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(orig_x, orig_y + 8.0)));
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(orig_x, orig_y - 3.0)));
            std::thread::sleep(std::time::Duration::from_millis(40));
            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(orig_x, orig_y + 2.0)));
            std::thread::sleep(std::time::Duration::from_millis(30));
            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(orig_x, orig_y)));
        }
    });
}

#[tauri::command]
fn resize_window(window: tauri::WebviewWindow, width: f64, height: f64) {
    let _ = window.set_resizable(true);
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let _ = window.set_always_on_top(true);
}

#[tauri::command]
fn is_cursor_inside(window: tauri::WebviewWindow) -> bool {
    let cursor = match window.cursor_position() { Ok(c) => c, Err(_) => return false };
    let pos = match window.outer_position() { Ok(p) => p, Err(_) => return false };
    let size = match window.outer_size() { Ok(s) => s, Err(_) => return false };
    let margin = 2.0;
    cursor.x >= (pos.x as f64 - margin)
        && cursor.x <= (pos.x as f64 + size.width as f64 + margin)
        && cursor.y >= (pos.y as f64 - margin)
        && cursor.y <= (pos.y as f64 + size.height as f64 + margin)
}

struct ServerPort(u16);

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
            app.manage(AppConfigState(Mutex::new(config)));

            // Window setup
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(300.0, 46.0)));

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

            // Staleness checker
            let handle2 = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(10));
                let mgr = handle2.state::<AppSessionManager>();
                mgr.0.lock().unwrap().check_staleness();
                let _ = handle2.emit("session-update", ());
            });

            // System tray
            let show = MenuItemBuilder::with_id("show", "顯示 / 隱藏").build(app)?;
            let settings = MenuItemBuilder::with_id("settings", "開啟設定").build(app)?;
            let toggle_theme = MenuItemBuilder::with_id("toggle_theme", "切換明暗主題").build(app)?;
            let open_config = MenuItemBuilder::with_id("open_config", "開啟設定檔").build(app)?;
            let restart = MenuItemBuilder::with_id("restart", "重新啟動").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "結束龍蝦監控").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show)
                .item(&settings)
                .item(&toggle_theme)
                .separator()
                .item(&open_config)
                .item(&restart)
                .item(&quit)
                .build()?;

            let icon_bytes = include_bytes!("../icons/32x32.png");
            let icon = Image::from_bytes(icon_bytes)?;

            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("龍蝦監控")
                .menu(&menu)
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
                        let _ = std::process::Command::new(opener)
                            .arg(path.to_string_lossy().to_string())
                            .spawn();
                    }
                    "restart" => {
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = std::process::Command::new(exe)
                                .spawn();
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
            resize_window,
            bounce_window,
            is_cursor_inside,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LobsterPulse");
}
