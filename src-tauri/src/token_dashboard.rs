//! Token 戰情室（TokenUsageInsights）按需啟動。
//! 它常駐時每 5 秒把成長中的 session 紀錄重讀一遍，實測單核 30–40% CPU（2026-10-20），所以平常不開。
//! LobsterPulse 自己守著 http://127.0.0.1:3003（閒置時只是一個 accept 迴圈），有瀏覽器連進來才在
//! 背景啟動看板（綁本機另一個空 port），再把連線原封不動轉過去：書籤、網址列、重新整理都能直接開，
//! 閒置被關掉之後再操作也會自動叫回來。瀏覽器閒置一段時間、或 LobsterPulse 結束（含當掉）就關。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 對外的固定網址（書籤用這個）；看板本身綁每次啟動時挑的空 port
const PORT: u16 = 3003;
/// 連續這麼久沒有瀏覽器連線就關。看板平常只在切換頁面時發請求，
/// 瀏覽器留著的閒置連線幾分鐘內就會斷，所以這差不多是「最後一次操作後 15 分鐘」。
const IDLE_LIMIT: Duration = Duration::from_secs(15 * 60);
const POLL_EVERY: Duration = Duration::from_secs(30);
/// 啟動後要先開資料庫、做一次增量同步才開始監聽，實測約 3 秒
const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// 新連線等第一個請求的上限。瀏覽器會先開好連線、晚點才用，太早關掉會害它報錯
const HEAD_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_HEAD: usize = 32 * 1024;
/// 「啟動中」頁面輪詢用；由 LobsterPulse 自己回答，不會叫醒看板
const STATUS_PATH: &str = "/__lobsterpulse/status";

struct Running {
    child: Child,
    /// LobsterPulse 結束（含當掉）或這個值被 drop 時 handle 關閉，看板跟著結束
    #[cfg(windows)]
    _job: crate::quota::KillOnCloseJob,
}

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    Stopped,
    Starting,
    /// 看板實際監聽的 port
    Ready(u16),
    Failed(String),
}

struct Hub {
    phase: Phase,
    running: Option<Running>,
    /// 正在轉送的連線數；大於 0 就不算閒置
    active: usize,
    last_active: Option<Instant>,
    /// 每次啟動加一；上一輪的閒置監看看到編號變了就收手
    generation: u64,
}

impl Hub {
    const fn new() -> Self {
        Hub { phase: Phase::Stopped, running: None, active: 0, last_active: None, generation: 0 }
    }
}

/// 啟動看板並等到就緒（阻塞），回傳程序與它監聽的 port。測試換成假的
type Launch = fn() -> Result<(Running, u16), String>;

struct Shared {
    hub: Mutex<Hub>,
    launch: Launch,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Hub> {
        self.hub.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static SHARED: Shared = Shared { hub: Mutex::new(Hub::new()), launch };
/// 3003 是否已經由這個程序在守
static LISTENING: Mutex<bool> = Mutex::new(false);

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

fn idle_expired(last_active: Instant, now: Instant) -> bool {
    now.saturating_duration_since(last_active) >= IDLE_LIMIT
}

fn dashboard_url() -> String {
    format!("http://127.0.0.1:{PORT}/?agent=all&tab=monthly")
}

fn open_browser() -> Result<(), String> {
    // 網址是常數，不含 `"`
    crate::browser_command(&dashboard_url())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("開瀏覽器失敗：{e}"))
}

/// 讓系統挑一個本機空 port 給看板。放掉到看板綁上之間有短暫空窗，
/// 萬一被別的程式搶走，就會等不到就緒而回報失敗，不會轉到別人那裡（probe 會驗身分）
fn free_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("找不到可用的本機 port：{e}"))
}

fn spawn(exe: &Path, port: u16) -> Result<Running, String> {
    let mut cmd = Command::new(exe);
    // 只綁本機（它預設 0.0.0.0，看板能還原完整對話內容）；不自動下載新版替換執行檔
    cmd.arg("--no-auto-update")
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
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

/// 實際的啟動：挑 port、開看板、等 `/api/version` 回應（阻塞，最多 READY_TIMEOUT）
fn launch() -> Result<(Running, u16), String> {
    let exe = exe_path().ok_or("尚未安裝 Token 戰情室")?;
    let port = free_port()?;
    let mut running = spawn(&exe, port)?;
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if probe(port) == PortState::Dashboard {
            return Ok((running, port));
        }
        if let Ok(Some(status)) = running.child.try_wait() {
            return Err(format!("Token 戰情室啟動後立刻結束（{status}）"));
        }
        if Instant::now() >= deadline {
            stop(running);
            return Err(format!("Token 戰情室 {} 秒內沒有就緒", READY_TIMEOUT.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// 呼叫端持有鎖、且目前不在 Starting／Ready。真正的啟動在背景執行緒，結果寫回 phase
fn begin_start(shared: &'static Shared, hub: &mut Hub) {
    hub.phase = Phase::Starting;
    hub.generation += 1;
    let generation = hub.generation;
    let stale = hub.running.take();
    std::thread::spawn(move || {
        if let Some(r) = stale {
            stop(r);
        }
        let result = (shared.launch)();
        let mut hub = shared.lock();
        match result {
            Ok((running, port)) => {
                let pid = running.child.id();
                hub.running = Some(running);
                hub.phase = Phase::Ready(port);
                hub.last_active = Some(Instant::now());
                drop(hub);
                log::info!("[token-dashboard] 已啟動 PID {pid}（port {port}）");
                watch_idle(shared, generation);
            }
            Err(e) => {
                log::warn!("[token-dashboard] 啟動失敗：{e}");
                hub.phase = Phase::Failed(e);
            }
        }
    });
}

/// 沒在跑就開始啟動；上次失敗的也重試（使用者主動按的）。回傳呼叫前的狀態
fn kick(shared: &'static Shared) -> Phase {
    let mut hub = shared.lock();
    let before = hub.phase.clone();
    if matches!(before, Phase::Stopped | Phase::Failed(_)) {
        begin_start(shared, &mut hub);
    }
    before
}

enum Check {
    Keep,
    Quit,
    Stop(Running),
}

/// 閒置監看的一次檢查（持鎖呼叫；要關的程序交回給呼叫端在鎖外關）
fn check(hub: &mut Hub, generation: u64, now: Instant) -> Check {
    if hub.generation != generation || !matches!(hub.phase, Phase::Ready(_)) {
        return Check::Quit;
    }
    if let Some(r) = hub.running.as_mut() {
        let pid = r.child.id();
        match r.child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                log::info!("[token-dashboard] PID {pid} 已自行結束（{status}）");
                hub.running = None;
                hub.phase = Phase::Stopped;
                return Check::Quit;
            }
            Err(e) => {
                // 查不到狀態就當成失控，確實關掉，不讓狀態和實際程序脫節
                log::warn!("[token-dashboard] 查詢 PID {pid} 狀態失敗：{e}，直接關閉");
                hub.phase = Phase::Stopped;
                return hub.running.take().map_or(Check::Quit, Check::Stop);
            }
        }
    }
    if hub.active > 0 {
        hub.last_active = Some(now);
        return Check::Keep;
    }
    if idle_expired(hub.last_active.unwrap_or(now), now) {
        let pid = hub.running.as_ref().map_or(0, |r| r.child.id());
        log::info!("[token-dashboard] 閒置 {} 分鐘，關閉 PID {pid}", IDLE_LIMIT.as_secs() / 60);
        hub.phase = Phase::Stopped;
        return hub.running.take().map_or(Check::Quit, Check::Stop);
    }
    Check::Keep
}

/// 每 30 秒檢查一次；一次啟動配一條執行緒，換下一輪啟動或關掉後自己結束
fn watch_idle(shared: &'static Shared, generation: u64) {
    std::thread::spawn(move || loop {
        std::thread::sleep(POLL_EVERY);
        let outcome = check(&mut shared.lock(), generation, Instant::now());
        match outcome {
            Check::Keep => {}
            Check::Quit => return,
            Check::Stop(r) => {
                stop(r);
                return;
            }
        }
    });
}

/// 連線上第一個 HTTP 請求的標頭（只取需要的欄位）
#[derive(Debug, Default)]
struct Request {
    method: String,
    target: String,
    host: Option<String>,
    accept: Option<String>,
    fetch_site: Option<String>,
    fetch_dest: Option<String>,
    /// 出現兩個以上 Host：檢查的和看板看到的可能不是同一個，直接拒絕
    duplicate_host: bool,
}

fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

impl Request {
    fn parse(buf: &[u8]) -> Option<Self> {
        let head = std::str::from_utf8(&buf[..head_end(buf)?]).ok()?;
        let mut lines = head.split("\r\n");
        let mut first = lines.next()?.split(' ');
        let mut req = Request {
            method: first.next()?.to_string(),
            target: first.next()?.to_string(),
            ..Default::default()
        };
        for line in lines {
            let Some((name, value)) = line.split_once(':') else { continue };
            let value = Some(value.trim().to_string());
            match name.trim().to_ascii_lowercase().as_str() {
                "host" => {
                    req.duplicate_host |= req.host.is_some();
                    req.host = value;
                }
                "accept" => req.accept = value,
                "sec-fetch-site" => req.fetch_site = value,
                "sec-fetch-dest" => req.fetch_dest = value,
                _ => {}
            }
        }
        Some(req)
    }

    fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    /// 只收指名本機的請求：擋 DNS rebinding（Host 是別的網域），
    /// 也擋其他網站在背景戳它（Sec-Fetch-Site: cross-site），免得隨便一個網頁就能把看板叫起來
    fn allowed(&self) -> bool {
        let host_ok = self.host.as_deref().map_or(true, |h| {
            let name = match h.rsplit_once(':') {
                Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
                _ => h,
            };
            name.eq_ignore_ascii_case("127.0.0.1") || name.eq_ignore_ascii_case("localhost")
        });
        host_ok && !self.duplicate_host && self.fetch_site.as_deref() != Some("cross-site")
    }

    /// 瀏覽器開頁面（不是頁面裡的 fetch／圖片）：還沒就緒時可以先回「啟動中」頁
    fn is_navigation(&self) -> bool {
        self.method == "GET"
            && match self.fetch_dest.as_deref() {
                Some(dest) => dest == "document",
                None => self.accept.as_deref().is_some_and(|a| a.contains("text/html")),
            }
    }
}

/// 讀到第一個請求的標頭結束。讀到的全部位元組都要原樣轉給看板（可能已含 body 或下一個請求）
async fn read_head(sock: &mut tokio::net::TcpStream) -> Option<Vec<u8>> {
    let mut buf = Vec::with_capacity(2048);
    let read = async {
        let mut chunk = [0u8; 4096];
        loop {
            let n = sock.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(end) = head_end(&buf) {
                return (end <= MAX_HEAD).then_some(());
            }
            if buf.len() > MAX_HEAD {
                return None;
            }
        }
    };
    tokio::time::timeout(HEAD_TIMEOUT, read).await.ok()??;
    Some(buf)
}

async fn respond(sock: &mut tokio::net::TcpStream, status: &str, content_type: &str, body: &str) {
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = sock.write_all(resp.as_bytes()).await;
    let _ = sock.shutdown().await;
}

fn status_json(phase: &Phase) -> String {
    let (name, error) = match phase {
        Phase::Stopped => ("stopped", None),
        Phase::Starting => ("starting", None),
        Phase::Ready(_) => ("ready", None),
        Phase::Failed(e) => ("failed", Some(e.as_str())),
    };
    serde_json::json!({ "phase": name, "error": error }).to_string()
}

enum Route {
    Forward(u16),
    Loading,
    Unavailable(String),
}

/// 決定這條連線怎麼處理。Ready 時在同一把鎖下登記為轉送中（閒置檢查不會剛好把它關掉）。
/// 頁面背景請求不重試上次失敗的啟動，只有使用者開頁面才重試，免得一直重開
async fn decide(shared: &'static Shared, navigation: bool) -> Route {
    let deadline = Instant::now() + READY_TIMEOUT + Duration::from_secs(5);
    let mut kicked = false;
    loop {
        {
            let mut hub = shared.lock();
            match &hub.phase {
                Phase::Ready(port) => {
                    let port = *port;
                    hub.active += 1;
                    hub.last_active = Some(Instant::now());
                    return Route::Forward(port);
                }
                Phase::Failed(e) if kicked || !navigation => return Route::Unavailable(e.clone()),
                Phase::Stopped | Phase::Failed(_) => {
                    begin_start(shared, &mut hub);
                    kicked = true;
                    if navigation {
                        return Route::Loading;
                    }
                }
                Phase::Starting if navigation => return Route::Loading,
                Phase::Starting => {}
            }
        }
        if Instant::now() >= deadline {
            return Route::Unavailable("Token 戰情室還沒準備好".into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// 轉送中的連線；結束（含出錯）時扣回計數並記下活動時間
struct ActiveConn(&'static Shared);

impl Drop for ActiveConn {
    fn drop(&mut self) {
        let mut hub = self.0.lock();
        hub.active = hub.active.saturating_sub(1);
        hub.last_active = Some(Instant::now());
    }
}

/// 連不上看板：程序已經自己結束就記為 Stopped（下一個請求自動重開）；
/// 程序還在卻連不上就當成壞掉，關掉並記為失敗（重新開頁面會重試）
fn give_up_on(shared: &'static Shared, port: u16, err: &std::io::Error) {
    let stale = {
        let mut hub = shared.lock();
        if hub.phase != Phase::Ready(port) {
            return;
        }
        let exited = hub.running.as_mut().is_some_and(|r| matches!(r.child.try_wait(), Ok(Some(_))));
        if exited {
            log::info!("[token-dashboard] 看板已自行結束，下次請求時重新啟動");
            hub.phase = Phase::Stopped;
            hub.running = None;
            return;
        }
        hub.phase = Phase::Failed(format!("Token 戰情室沒有回應（{err}）"));
        hub.running.take()
    };
    log::warn!("[token-dashboard] 連不上 port {port}：{err}，關閉看板");
    if let Some(r) = stale {
        std::thread::spawn(move || stop(r));
    }
}

async fn handle(shared: &'static Shared, mut client: tokio::net::TcpStream) {
    let Some(buf) = read_head(&mut client).await else {
        return;
    };
    let Some(req) = Request::parse(&buf) else {
        respond(&mut client, "400 Bad Request", "text/plain; charset=utf-8", "bad request").await;
        return;
    };
    if !req.allowed() {
        let body = "為了安全，Token 戰情室只接受直接開啟（書籤、網址列或 LobsterPulse 的按鈕）。";
        respond(&mut client, "403 Forbidden", "text/plain; charset=utf-8", body).await;
        return;
    }
    let json = "application/json; charset=utf-8";
    if req.path() == STATUS_PATH {
        let body = status_json(&shared.lock().phase);
        respond(&mut client, "200 OK", json, &body).await;
        return;
    }
    let navigation = req.is_navigation();
    let port = match decide(shared, navigation).await {
        Route::Forward(port) => port,
        Route::Loading => {
            respond(&mut client, "200 OK", "text/html; charset=utf-8", LOADING_PAGE).await;
            return;
        }
        Route::Unavailable(e) => {
            let body = serde_json::json!({ "error": e }).to_string();
            respond(&mut client, "503 Service Unavailable", json, &body).await;
            return;
        }
    };
    let _active = ActiveConn(shared);
    match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
        Ok(mut upstream) => {
            if upstream.write_all(&buf).await.is_ok() {
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            }
        }
        Err(e) => {
            give_up_on(shared, port, &e);
            if navigation {
                // 「啟動中」頁會讀到失敗狀態並顯示重試鈕
                respond(&mut client, "200 OK", "text/html; charset=utf-8", LOADING_PAGE).await;
            } else {
                let body = serde_json::json!({ "error": e.to_string() }).to_string();
                respond(&mut client, "502 Bad Gateway", json, &body).await;
            }
        }
    }
}

async fn accept_loop(shared: &'static Shared, listener: tokio::net::TcpListener) {
    loop {
        match listener.accept().await {
            Ok((sock, _)) => {
                tokio::spawn(handle(shared, sock));
            }
            Err(e) => {
                log::warn!("[token-dashboard] accept 失敗：{e}");
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

/// 在目前的 tokio runtime 上開始服務（listener 已綁好）
fn serve(shared: &'static Shared, listener: std::net::TcpListener) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(listener)?;
    tokio::spawn(accept_loop(shared, listener));
    Ok(())
}

/// 開始守 3003；已經在守就直接回 Ok。要在 async runtime 裡呼叫
fn ensure_listening() -> std::io::Result<()> {
    let mut listening = LISTENING.lock().unwrap_or_else(|e| e.into_inner());
    if *listening {
        return Ok(());
    }
    serve(&SHARED, std::net::TcpListener::bind(("127.0.0.1", PORT))?)?;
    *listening = true;
    log::info!("[token-dashboard] 守在 http://127.0.0.1:{PORT}（看板用到才啟動）");
    Ok(())
}

/// LobsterPulse 啟動時開始守 3003。剛「重新啟動」時舊程序可能還沒放掉 port，所以重試一陣子；
/// 都失敗也沒關係，按下按鈕時會再試
pub async fn listen_on_startup() {
    if exe_path().is_none() {
        return;
    }
    for attempt in 1..=30 {
        match ensure_listening() {
            Ok(()) => return,
            Err(e) if attempt == 30 => log::warn!("[token-dashboard] 守不到 port {PORT}：{e}"),
            Err(_) => tokio::time::sleep(Duration::from_secs(2)).await,
        }
    }
}

/// 額度面板按鈕與系統匣選單：確保在守 3003、看板沒在跑就開始啟動，然後馬上開瀏覽器
/// （還沒就緒時瀏覽器先看到「啟動中」頁，好了自動切過去）。
#[tauri::command]
pub async fn open_token_dashboard() -> Result<String, String> {
    let listening = match exe_path() {
        Some(_) => ensure_listening().map_err(|e| format!("無法使用 port {PORT}：{e}")),
        None => Err("尚未安裝 Token 戰情室".to_string()),
    };
    let outcome = match listening {
        Ok(()) => match kick(&SHARED) {
            Phase::Ready(_) => "opened",
            _ => "started",
        },
        Err(why) => {
            // 3003 上已經有看板（例如手動開的）就直接開網頁；不是看板就回報原因
            let state = tauri::async_runtime::spawn_blocking(|| probe(PORT)).await.map_err(|e| e.to_string())?;
            match state {
                PortState::Dashboard => "opened",
                PortState::Other if exe_path().is_some() => return Err(format!("port {PORT} 被其他程式佔用")),
                _ => return Err(why),
            }
        }
    };
    tauri::async_runtime::spawn_blocking(open_browser).await.map_err(|e| e.to_string())??;
    Ok(outcome.to_string())
}

const LOADING_PAGE: &str = r#"<!doctype html>
<html lang="zh-Hant">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Token 戰情室 · 啟動中</title>
<style>
  :root { color-scheme: light dark; --bg: #f4f5f7; --fg: #1f2328; --muted: #59636e; --accent: #3d9bff; --err: #d1242f; }
  @media (prefers-color-scheme: dark) { :root { --bg: #0f1419; --fg: #e6edf3; --muted: #8b949e; --err: #ff8a80; } }
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: var(--bg); color: var(--fg);
         font: 15px/1.6 system-ui, "Microsoft JhengHei", sans-serif; }
  main { text-align: center; padding: 24px 16px; max-width: 420px; }
  .spin { width: 28px; height: 28px; margin: 0 auto 16px; border-radius: 50%;
          border: 3px solid color-mix(in srgb, var(--accent) 25%, transparent); border-top-color: var(--accent);
          animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  h1 { font-size: 18px; margin: 0 0 6px; }
  p { margin: 0; color: var(--muted); overflow-wrap: anywhere; }
  .err h1 { color: var(--err); }
  .err .spin { display: none; }
  button { margin-top: 16px; padding: 6px 18px; border-radius: 6px; border: 1px solid var(--muted);
           background: transparent; color: var(--fg); font: inherit; cursor: pointer; }
  button[hidden] { display: none; }
</style>
</head>
<body>
<main id="box">
  <div class="spin"></div>
  <h1 id="title">正在啟動 Token 戰情室…</h1>
  <p id="msg">通常 3 秒左右，好了會自動切過去。</p>
  <button id="retry" hidden>重試</button>
</main>
<script>
  const box = document.getElementById("box");
  const retry = document.getElementById("retry");
  retry.onclick = () => location.reload();
  function fail(title, msg) {
    box.classList.add("err");
    document.getElementById("title").textContent = title;
    document.getElementById("msg").textContent = msg;
    retry.hidden = false;
  }
  let misses = 0;
  async function tick() {
    try {
      const s = await (await fetch("/__lobsterpulse/status", { cache: "no-store" })).json();
      misses = 0;
      if (s.phase === "ready" || s.phase === "stopped") return location.reload();
      if (s.phase === "failed") return fail("Token 戰情室啟動失敗", s.error || "");
    } catch (e) {
      if (++misses >= 5) return fail("連不到 LobsterPulse", "請確認 LobsterPulse 有在執行，再按重試。");
    }
    setTimeout(tick, 400);
  }
  tick();
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    fn parse(head: &str) -> Request {
        Request::parse(head.as_bytes()).expect("標頭要能解析")
    }

    #[test]
    fn request_guards_and_navigation_detection() {
        let nav = parse(
            "GET /?agent=all&tab=monthly HTTP/1.1\r\nHost: 127.0.0.1:3003\r\nAccept: text/html,*/*\r\nSec-Fetch-Site: none\r\nSec-Fetch-Dest: document\r\n\r\n",
        );
        assert!(nav.allowed() && nav.is_navigation());
        assert_eq!(nav.path(), "/");

        let api = parse("GET /api/version HTTP/1.1\r\nhost: localhost:3003\r\nsec-fetch-site: same-origin\r\nsec-fetch-dest: empty\r\naccept: */*\r\n\r\n");
        assert!(api.allowed() && !api.is_navigation(), "頁面裡的 fetch 不是開頁面");

        assert!(parse("GET / HTTP/1.1\r\nAccept: text/html\r\n\r\n").is_navigation(), "沒有 Sec-Fetch-Dest 時看 Accept");
        assert!(!parse("POST / HTTP/1.1\r\nAccept: text/html\r\n\r\n").is_navigation());
        assert!(parse("GET /api/version HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").allowed(), "不帶 port 的本機 Host 可以");
        assert!(parse("GET / HTTP/1.0\r\n\r\n").allowed(), "沒有 Host 的舊式請求不是瀏覽器，放行");

        assert!(!parse("GET / HTTP/1.1\r\nHost: evil.example:3003\r\n\r\n").allowed(), "DNS rebinding");
        assert!(!parse("GET / HTTP/1.1\r\nHost: 127.0.0.1.evil.example\r\n\r\n").allowed());
        assert!(!parse("GET / HTTP/1.1\r\nHost: 127.0.0.1:3003\r\nSec-Fetch-Site: cross-site\r\n\r\n").allowed(), "別的網站戳過來");
        assert!(!parse("GET / HTTP/1.1\r\nHost: evil.example\r\nHost: 127.0.0.1:3003\r\n\r\n").allowed(), "重複 Host：最後一個是本機也不行");
        assert!(Request::parse(b"GET / HTTP/1.1\r\nHost: 127.0.0.1").is_none(), "標頭沒讀完不解析");
    }

    #[test]
    fn idle_check_only_stops_when_nothing_is_connected() {
        let t0 = Instant::now();
        let mut hub = Hub::new();
        hub.phase = Phase::Ready(1);
        hub.generation = 7;
        hub.last_active = Some(t0);

        hub.active = 1;
        assert!(matches!(check(&mut hub, 7, t0 + IDLE_LIMIT * 2), Check::Keep), "有連線就不關");
        assert_eq!(hub.last_active, Some(t0 + IDLE_LIMIT * 2), "有連線時刷新活動時間");

        hub.active = 0;
        let later = t0 + IDLE_LIMIT * 2;
        assert!(matches!(check(&mut hub, 7, later + IDLE_LIMIT - Duration::from_secs(1)), Check::Keep));
        assert!(matches!(check(&mut hub, 6, later + IDLE_LIMIT), Check::Quit), "上一輪的監看直接收手");
        assert_eq!(hub.phase, Phase::Ready(1), "收手的舊監看不能動到狀態");
        assert!(matches!(check(&mut hub, 7, later + IDLE_LIMIT), Check::Quit));
        assert_eq!(hub.phase, Phase::Stopped, "閒置到期就關");
    }

    fn test_shared(launch: Launch) -> &'static Shared {
        Box::leak(Box::new(Shared { hub: Mutex::new(Hub::new()), launch }))
    }

    fn start_proxy(shared: &'static Shared) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        serve(shared, listener).unwrap();
        port
    }

    async fn send(port: u16, req: &str) -> String {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), s.read_to_end(&mut out)).await.unwrap().unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    fn never_launch() -> Result<(Running, u16), String> {
        panic!("這個情境不該啟動看板")
    }

    /// 假看板：收滿 `expect` 個位元組後回 hello，交回收到的內容
    async fn fake_upstream(expect: usize) -> (u16, tokio::task::JoinHandle<String>) {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = upstream.local_addr().unwrap().port();
        let got = tokio::spawn(async move {
            let (mut s, _) = upstream.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            while buf.len() < expect {
                let n = s.read(&mut chunk).await.unwrap();
                assert!(n > 0, "還沒收完就斷線");
                buf.extend_from_slice(&chunk[..n]);
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello").await.unwrap();
            String::from_utf8_lossy(&buf).into_owned()
        });
        (port, got)
    }

    #[tokio::test]
    async fn forwards_the_connection_and_counts_it_as_activity() {
        let req = "GET /api/summary?x=1 HTTP/1.1\r\nHost: 127.0.0.1:3003\r\nSec-Fetch-Dest: empty\r\n\r\n";
        let (up_port, got) = fake_upstream(req.len()).await;
        let shared = test_shared(never_launch);
        shared.lock().phase = Phase::Ready(up_port);
        let port = start_proxy(shared);

        let resp = send(port, req).await;
        assert!(resp.ends_with("hello"), "{resp}");
        assert_eq!(got.await.unwrap(), req, "已讀走的標頭要原樣轉給看板");

        for _ in 0..50 {
            if shared.lock().active == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let hub = shared.lock();
        assert_eq!(hub.active, 0, "連線結束要扣回");
        assert!(hub.last_active.is_some(), "轉送過就算活動");
    }

    #[tokio::test]
    async fn bytes_read_past_the_head_are_forwarded_too() {
        // 同一次寫入帶著 body 和下一個請求：讀標頭時一起讀走的部分不能掉
        let req = "POST /api/x HTTP/1.1\r\nHost: 127.0.0.1:3003\r\nContent-Length: 4\r\n\r\nbodyGET /next HTTP/1.1\r\nHost: 127.0.0.1:3003\r\n\r\n";
        let (up_port, got) = fake_upstream(req.len()).await;
        let shared = test_shared(never_launch);
        shared.lock().phase = Phase::Ready(up_port);
        let port = start_proxy(shared);

        assert!(send(port, req).await.ends_with("hello"));
        assert_eq!(got.await.unwrap(), req);
    }

    #[tokio::test]
    async fn oversized_heads_are_dropped_without_waking_the_dashboard() {
        let shared = test_shared(never_launch);
        let port = start_proxy(shared);
        let req = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:3003\r\nX-Pad: {}\r\n\r\n", "a".repeat(MAX_HEAD));
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let _ = s.write_all(req.as_bytes()).await;
        let mut out = Vec::new();
        // 代理直接關線（可能是 EOF 或 RST），不回任何東西
        let _ = tokio::time::timeout(Duration::from_secs(10), s.read_to_end(&mut out)).await.unwrap();
        assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out));
        assert_eq!(shared.lock().phase, Phase::Stopped);
    }

    static FAILS: AtomicUsize = AtomicUsize::new(0);
    fn failing_launch() -> Result<(Running, u16), String> {
        FAILS.fetch_add(1, Ordering::SeqCst);
        Err("測試：沒有看板".into())
    }

    #[tokio::test]
    async fn opening_the_page_starts_it_and_failures_only_retry_on_navigation() {
        let shared = test_shared(failing_launch);
        let port = start_proxy(shared);
        let nav = "GET / HTTP/1.1\r\nHost: 127.0.0.1:3003\r\nSec-Fetch-Dest: document\r\n\r\n";
        let status = "GET /__lobsterpulse/status HTTP/1.1\r\nHost: 127.0.0.1:3003\r\n\r\n";

        let page = send(port, nav).await;
        assert!(page.starts_with("HTTP/1.1 200"), "{page}");
        assert!(page.contains("Cache-Control: no-store"), "「啟動中」頁不能被當成看板快取");
        assert!(page.contains("正在啟動 Token 戰情室"));

        let mut last = String::new();
        for _ in 0..100 {
            last = send(port, status).await;
            if last.contains("\"failed\"") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(last.contains("測試：沒有看板"), "{last}");
        assert_eq!(FAILS.load(Ordering::SeqCst), 1);

        let api = send(port, "GET /api/x HTTP/1.1\r\nHost: 127.0.0.1:3003\r\nSec-Fetch-Dest: empty\r\n\r\n").await;
        assert!(api.starts_with("HTTP/1.1 503"), "{api}");
        assert_eq!(FAILS.load(Ordering::SeqCst), 1, "頁面背景請求不重試");

        send(port, nav).await;
        for _ in 0..100 {
            if FAILS.load(Ordering::SeqCst) == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(FAILS.load(Ordering::SeqCst), 2, "使用者重新開頁面才重試");
    }

    #[tokio::test]
    async fn foreign_hosts_are_refused_without_waking_the_dashboard() {
        let shared = test_shared(never_launch);
        let port = start_proxy(shared);
        let resp = send(port, "GET / HTTP/1.1\r\nHost: evil.example:3003\r\nAccept: text/html\r\n\r\n").await;
        assert!(resp.starts_with("HTTP/1.1 403"), "{resp}");
        assert_eq!(shared.lock().phase, Phase::Stopped);
    }
}
