//! Token 戰情室（TokenUsageInsights）按需啟動。
//! 它常駐時每 5 秒把成長中的 session 紀錄重讀一遍，實測單核 30–40% CPU（2026-10-20），
//! 所以只在使用者按下額度面板的按鈕時才開；瀏覽器閒置一段時間、或 LobsterPulse 結束就關。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PORT: u16 = 3003;
/// 連續這麼久沒有瀏覽器連線就關。看板平常只在切換頁面時發請求，
/// 瀏覽器留著的閒置連線幾分鐘內就會斷，所以這差不多是「最後一次操作後 15 分鐘」。
const IDLE_LIMIT: Duration = Duration::from_secs(15 * 60);
const POLL_EVERY: Duration = Duration::from_secs(30);
/// 啟動後要先開資料庫、做一次增量同步才開始監聽，實測約 3 秒
const READY_TIMEOUT: Duration = Duration::from_secs(30);

struct Running {
    child: Child,
    /// LobsterPulse 結束（含當掉）時 handle 被系統關掉，看板跟著結束
    #[cfg(windows)]
    _job: crate::quota::KillOnCloseJob,
    last_active: Instant,
}

/// 由 LobsterPulse 啟動、還沒關掉的那一個看板
static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
/// 「檢查→啟動→等就緒」整段序列化，連點兩下不會開出兩個
static OPENING: Mutex<()> = Mutex::new(());

#[derive(Debug, PartialEq)]
enum PortState {
    Free,
    Dashboard,
    Other,
}

/// Windows 一行安裝的位置（%LOCALAPPDATA%\TokenUsageInsights）；其他平台不支援
fn exe_path() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let p = dirs::data_local_dir()?.join("TokenUsageInsights").join("token-usage-insights.exe");
    p.is_file().then_some(p)
}

/// port 上是不是 Token 戰情室（`/api/version` 回 version＋build_date），還是被別的程式佔走
fn probe(port: u16) -> PortState {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return PortState::Free;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
    if s.write_all(b"GET /api/version HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").is_err() {
        return PortState::Other;
    }
    // 讀到對方關線或逾時為止；回應不完整時 JSON 解析不過，就不會誤判成看板
    let mut buf = Vec::new();
    let _ = s.take(64 * 1024).read_to_end(&mut buf);
    if is_dashboard_version_response(&String::from_utf8_lossy(&buf)) {
        PortState::Dashboard
    } else {
        PortState::Other
    }
}

/// 狀態碼 200，且 body 是含字串 `version`、`build_date` 的 JSON 物件
fn is_dashboard_version_response(resp: &str) -> bool {
    let Some((head, body)) = resp.split_once("\r\n\r\n") else {
        return false;
    };
    let status_ok = head
        .lines()
        .next()
        .is_some_and(|l| l.starts_with("HTTP/1.") && l.split_whitespace().nth(1) == Some("200"));
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body.trim()) else {
        return false;
    };
    status_ok && json["version"].is_string() && json["build_date"].is_string()
}

/// 有沒有瀏覽器連在這個看板程序的 port 上
#[cfg(windows)]
fn has_client(port: u16, pid: u32) -> bool {
    crate::terminal::tcp_rows().is_some_and(|rows| {
        rows.iter().any(|r| {
            r.state == crate::terminal::TCP_STATE_ESTABLISHED && r.local_port == port && r.pid == pid
        })
    })
}

/// 其他平台讀不到 TCP table，一律當沒有連線（按下後 IDLE_LIMIT 就關）
#[cfg(not(windows))]
fn has_client(_port: u16, _pid: u32) -> bool {
    false
}

fn idle_expired(last_active: Instant, now: Instant) -> bool {
    now.saturating_duration_since(last_active) >= IDLE_LIMIT
}

fn dashboard_url() -> String {
    format!("http://127.0.0.1:{PORT}/?agent=all&tab=monthly")
}

fn open_browser() -> Result<(), String> {
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        // explorer 自己解析命令列：沒加引號、又含 `?…&` 的網址會被默默丟掉、瀏覽器不會開
        // （2026-10-20 實測）。std 只替含空白的參數加引號，所以這裡整段自己包。網址是常數。
        let mut c = Command::new("explorer");
        c.raw_arg(format!("\"{}\"", dashboard_url()));
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
        c.arg(dashboard_url());
        c
    };
    cmd.spawn().map(|_| ()).map_err(|e| format!("開瀏覽器失敗：{e}"))
}

fn spawn(exe: &Path) -> Result<Running, String> {
    let mut cmd = Command::new(exe);
    // 只綁本機（它預設 0.0.0.0，看板能還原完整對話內容）；不自動下載新版替換執行檔
    cmd.arg("--no-auto-update")
        .env("HOST", "127.0.0.1")
        .env("PORT", PORT.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut child = cmd.spawn().map_err(|e| format!("啟動 Token 戰情室失敗：{e}"))?;
    // 放不進 Job 就沒辦法保證 LobsterPulse 結束時它跟著結束——寧可不開，也不要留下常駐程序
    #[cfg(windows)]
    let job = match crate::quota::KillOnCloseJob::attach(&child) {
        Some(job) => job,
        None => {
            if child.kill().is_ok() {
                let _ = child.wait();
            } else {
                log::warn!("[token-dashboard] 放不進 Job，且結束 PID {} 失敗", child.id());
            }
            return Err("無法把 Token 戰情室綁到 LobsterPulse 的生命週期，已取消啟動".into());
        }
    };
    Ok(Running {
        child,
        #[cfg(windows)]
        _job: job,
        last_active: Instant::now(),
    })
}

/// kill 失敗時不 wait（避免卡住）；Windows 上 Job 隨 `r` 一起 drop，仍會把它收掉
fn stop(mut r: Running) {
    if r.child.kill().is_ok() {
        let _ = r.child.wait();
    } else {
        log::warn!("[token-dashboard] 結束 PID {} 失敗", r.child.id());
    }
}

fn lock_running() -> std::sync::MutexGuard<'static, Option<Running>> {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner())
}

/// 每 30 秒看一次：有瀏覽器連著就記為活動；連續 IDLE_LIMIT 沒活動就關。
/// 一個看板程序配一條執行緒，RUNNING 換成別的程序（或清空）時自己結束。
fn watch_idle(pid: u32) {
    std::thread::spawn(move || loop {
        std::thread::sleep(POLL_EVERY);
        let mut guard = lock_running();
        let Some(r) = guard.as_mut().filter(|r| r.child.id() == pid) else {
            return;
        };
        match r.child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                log::info!("[token-dashboard] PID {pid} 已自行結束（{status}）");
                *guard = None;
                return;
            }
            Err(e) => {
                // 查不到狀態就當成失控，確實關掉，不讓狀態和實際程序脫節
                log::warn!("[token-dashboard] 查詢 PID {pid} 狀態失敗：{e}，直接關閉");
                if let Some(r) = guard.take() {
                    stop(r);
                }
                return;
            }
        }
        let now = Instant::now();
        if has_client(PORT, pid) {
            r.last_active = now;
        } else if idle_expired(r.last_active, now) {
            log::info!("[token-dashboard] 閒置 {} 分鐘，關閉 PID {pid}", IDLE_LIMIT.as_secs() / 60);
            if let Some(r) = guard.take() {
                stop(r);
            }
            return;
        }
    });
}

fn open_blocking() -> Result<&'static str, String> {
    let _serial = OPENING.lock().unwrap_or_else(|e| e.into_inner());
    match probe(PORT) {
        PortState::Dashboard => {
            // 已在跑（我們開的，或使用者自己手動開的）→ 只開網頁
            if let Some(r) = lock_running().as_mut() {
                r.last_active = Instant::now();
            }
            open_browser()?;
            return Ok("opened");
        }
        PortState::Other => return Err(format!("port {PORT} 被其他程式佔用")),
        PortState::Free => {}
    }
    let exe = exe_path().ok_or("尚未安裝 Token 戰情室")?;
    // 之前開的那個已經不在服務（當掉或卡住）→ 收掉再開
    if let Some(old) = lock_running().take() {
        stop(old);
    }
    let running = spawn(&exe)?;
    let pid = running.child.id();
    *lock_running() = Some(running);

    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if probe(PORT) == PortState::Dashboard {
            break;
        }
        {
            let mut guard = lock_running();
            let exited = match guard.as_mut() {
                Some(r) if r.child.id() == pid => r.child.try_wait().ok().flatten(),
                _ => None,
            };
            if let Some(status) = exited {
                *guard = None;
                return Err(format!("Token 戰情室啟動後立刻結束（{status}）"));
            }
            if Instant::now() >= deadline {
                if let Some(r) = guard.take() {
                    stop(r);
                }
                return Err(format!("Token 戰情室 {} 秒內沒有就緒", READY_TIMEOUT.as_secs()));
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    log::info!("[token-dashboard] 已啟動 PID {pid}");
    watch_idle(pid);
    open_browser()?;
    Ok("started")
}

/// 額度面板的「Token 戰情室」按鈕：需要時才啟動看板，然後用預設瀏覽器打開。
#[tauri::command]
pub async fn open_token_dashboard() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(open_blocking)
        .await
        .map_err(|e| e.to_string())?
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(body: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut req = [0u8; 1024];
                let _ = s.read(&mut req);
                let _ = s.write_all(body.as_bytes());
            }
        });
        port
    }

    #[test]
    fn probe_tells_dashboard_from_other_programs_and_free_ports() {
        let dash = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"version\":\"1.3.2\",\"build_date\":\"2026-10-15\",\"syncing\":false}",
        );
        assert_eq!(probe(dash), PortState::Dashboard);

        let other = serve_once("HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\nnope");
        assert_eq!(probe(other), PortState::Other);

        let free = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert_eq!(probe(free), PortState::Free);
    }

    #[test]
    fn version_response_must_be_200_with_complete_json() {
        let ok = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"version\":\"1.3.2\",\"build_date\":\"2026-10-15\"}";
        assert!(is_dashboard_version_response(ok));
        assert!(!is_dashboard_version_response(&ok.replace("200 OK", "404 Not Found")), "非 200 不算");
        assert!(!is_dashboard_version_response(&ok[..ok.len() - 10]), "截斷的 JSON 不算");
        assert!(!is_dashboard_version_response(
            "HTTP/1.1 200 OK\r\n\r\n<html>\"version\" \"build_date\"</html>"
        ), "只是含有字串不算");
        assert!(!is_dashboard_version_response(
            "HTTP/1.1 200 OK\r\n\r\n{\"version\":1,\"build_date\":\"x\"}"
        ), "欄位型別不對不算");
    }

    #[test]
    fn idle_limit_counts_from_last_activity() {
        let start = Instant::now();
        assert!(!idle_expired(start, start + IDLE_LIMIT - Duration::from_secs(1)));
        assert!(idle_expired(start, start + IDLE_LIMIT));
        // 時鐘不會倒退，但萬一 last_active 比 now 晚也不能誤判成閒置
        assert!(!idle_expired(start + Duration::from_secs(5), start));
    }

    #[test]
    fn dashboard_url_is_local_only() {
        assert!(dashboard_url().starts_with("http://127.0.0.1:3003/"));
    }

    #[cfg(windows)]
    #[test]
    fn has_client_sees_established_connections_to_the_owner_pid() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let me = std::process::id();
        assert!(!has_client(port, me), "還沒有人連進來");
        let _client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (_server_side, _) = listener.accept().unwrap();
        assert!(has_client(port, me), "連線建立後要看得到");
        assert!(!has_client(port, me.wrapping_add(1)), "別的 PID 的連線不算");
    }
}
