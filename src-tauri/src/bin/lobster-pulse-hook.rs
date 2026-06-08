// Sidecar binary invoked by CLI hook configs. Reads the event JSON from
// stdin and forwards it as an HTTP POST to the running LobsterPulse server.
//
// Usage: lobster-pulse-hook <provider_id>
//
// Shell-agnostic by design: no bash, no PowerShell, no cmd syntax. Any
// host CLI that can spawn a process (on any OS) can invoke this. Errors
// are surfaced to stderr so a hook misfire is debuggable from the parent
// CLI's captured output, but the sidecar always exits 0 so a hook failure
// never breaks the parent CLI.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

const DEFAULT_PORT: u16 = 19280;
const TIMEOUT: Duration = Duration::from_secs(2);
const LOG_PREFIX: &str = "[lobster-pulse-hook]";

fn main() {
    let provider = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "unknown".to_string());

    let mut body = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut body) {
        // R34 surface: 原 `let _ = ...` silent 吞 stdin read 錯誤，operator 看到
        // 「LP 沒反應」但分不清是 hook 完全沒被 invoke 還是 stdin pipe 斷。改為 eprintln
        // 到 stderr（多數 CLI 會 capture 子進程 stderr）。仍繼續送空 body 給 server，
        // 由 server 端 validate — 維持 sidecar exit 0 不破壞 parent CLI。
        eprintln!(
            "{LOG_PREFIX} stdin read failed: {e} — proceeding with empty body \
             (parent CLI captured this; check pipe/EOF)"
        );
    }

    let port = read_port().unwrap_or_else(|| {
        eprintln!(
            "{LOG_PREFIX} read_port failed — falling back to DEFAULT_PORT={DEFAULT_PORT} \
             (LP may not be running or port file corrupt)"
        );
        DEFAULT_PORT
    });
    if let Err(e) = post(port, &provider, &body) {
        // R34 + R164 surface: `post` 之前 silent 吞網路錯誤,operator 看到
        // 「event 沒到 LP」分不清 LP 沒跑 / port 不通 / write 失敗。R164 進一步
        // 區分「server 拒收 (4xx/5xx)」: ErrorKind::Other = server 端邏輯拒,
        // 其他 ErrorKind = 網路層失敗。訊息分流避免「check LP running」誤導 4xx。
        match e.kind() {
            std::io::ErrorKind::Other => {
                eprintln!("{LOG_PREFIX} event for provider={provider} dropped: {e}");
            }
            _ => {
                eprintln!(
                    "{LOG_PREFIX} post to 127.0.0.1:{port} provider={provider} failed: {e} — \
                     event dropped, parent CLI continues (check LP running on this port)"
                );
            }
        }
    }
}

/// Pure fn: 給定 port file path，讀 + parse u16。
/// 跟 openab_bridge 的 `read_offset_at` / `write_offset_at` 同 pattern，讓 caller
/// 端統一處理 fallback。回 None 代表「該路徑不可用」,由 caller 決定 log + fallback。
///
/// R34: 原 `read_to_string(...).ok()?` chain 吞 2 條 silent fail:
/// 1. port file 不存在 / 讀失敗（磁碟問題 / 權限）— 屬 expected（LP 沒跑就沒有）
/// 2. port file 內容不是 u16（corrupt / 半截寫入）— 屬 unexpected，應可觀察
///
/// 第一條走 stderr note（first-run 預期），第二條走 stderr warn。
fn read_port_at(path: &Path) -> Option<u16> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "{LOG_PREFIX} read_port: no port file at {} (LP not running?)",
                path.display()
            );
            return None;
        }
        Err(e) => {
            eprintln!(
                "{LOG_PREFIX} read_port: read failed for {}: {e} — falling back to None",
                path.display()
            );
            return None;
        }
    };
    let trimmed = content.trim();
    match trimmed.parse::<u16>() {
        Ok(n) => Some(n),
        Err(e) => {
            eprintln!(
                "{LOG_PREFIX} read_port: parse failed for {:?}: {e} (content: {trimmed:?}) \
                 — falling back to None",
                path
            );
            None
        }
    }
}

fn read_port() -> Option<u16> {
    let home = dirs::home_dir()?;
    read_port_at(&home.join(".lobsterpulse").join("port"))
}

fn post(port: u16, provider: &str, body: &str) -> std::io::Result<()> {
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .expect("valid socket addr");

    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.set_read_timeout(Some(TIMEOUT))?;

    let request = format!(
        "POST /hook/{provider} HTTP/1.0\r\n\
         Host: 127.0.0.1\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );

    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    // R164：必須 parse response status line,不可 silent 吞 4xx/5xx。
    //
    // 對齊 hook_server.rs:272-277 — bad JSON / unknown provider / malformed request
    // 都回 `HTTP/1.1 400 Bad Request`,server 端 K16 `responses_4xx` counter 已 ++,
    // server log 也 warn 過 (line 268-271)。但原本 `post` 用 `let _ = stream.read(...)`
    // 丟棄 response,4xx/5xx 一律 `Ok(())` → caller 端 eprintln 不觸發 → operator 看
    // 到「event 沒到 LP」分不清是 LP 沒跑還是 server 拒收。Silent event loss。
    //
    // 修法: 讀 status line → parse u16 → >=400 回 Err(ErrorKind::Other, status 訊息)。
    // `Connection: close` 已設,server 寫完 status line 會 flush 後 close,read 一次
    // 就能拿到 `HTTP/1.X NNN Reason\r\n` (最長 ~32 byte, 64 byte buffer 足夠)。
    let mut status_buf = [0u8; 64];
    let n = match stream.read(&mut status_buf) {
        Ok(n) if n > 0 => n,
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "server closed connection without sending a response status line",
            ));
        }
        Err(e) => return Err(e),
    };
    let status_line = String::from_utf8_lossy(&status_buf[..n]);
    let status_code = parse_status_code(&status_line).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("malformed HTTP status line: {status_line:?}"),
        )
    })?;
    if status_code >= 400 {
        return Err(std::io::Error::other(format!(
            "server rejected event with HTTP {status_code} (provider={provider}) \
                 — check event JSON format & provider whitelist in hook_server::KNOWN_PROVIDERS"
        )));
    }
    Ok(())
}

/// Parse HTTP status code from response status line.
///
/// Accepts both `HTTP/1.0` and `HTTP/1.1`, and tolerates the reason phrase
/// being missing or non-ASCII (some servers truncate to just `HTTP/1.0 200\r\n`).
/// Returns `None` for any line that doesn't start with `HTTP/` followed by a
/// valid u16, so caller surfaces a clear "malformed status" error rather
/// than silently treating it as 200.
fn parse_status_code(status_line: &str) -> Option<u16> {
    let mut parts = status_line.split_whitespace();
    let version = parts.next()?;
    if !version.starts_with("HTTP/") {
        return None;
    }
    let code = parts.next()?.parse::<u16>().ok()?;
    Some(code)
}

#[cfg(test)]
mod read_port_at_tests {
    //! R34 regression:`read_port` 之前用 `.ok()?` chain 吞 read + parse 兩條 silent fail
    //! 路徑,corrupt port file(磁碟損壞 / 寫入半截 / 非 u16 字串)會回 None → fallback
    //! DEFAULT_PORT,但 operator 看到「event 送到錯的 port」完全無 log 可查。改為
    //! `read_port_at(path) -> Option<u16>` pure fn + caller 統一 stderr note,跟
    //! openab_bridge 的 `read_offset_at` 同 pattern。
    //!
    //! 本 module 鎖 4 條契約:
    //! 1. 檔案不存在(NotFound)→ 回 None + stderr note
    //! 2. 檔案存在但內容是 garbage(parse 失敗)→ 回 None + stderr warn
    //! 3. 檔案存在且內容是合法 u16 → 原樣回傳
    //! 4. 檔案存在且內容含 whitespace / newline → trim 後正確解析

    use super::*;

    fn tmp_port(tag: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("lp-hook-port-{tag}-{nonce}-{}", std::process::id()));
        p
    }

    #[test]
    fn read_port_at_missing_file_returns_none() {
        let path = tmp_port("missing");
        // 不建立檔案
        assert_eq!(read_port_at(&path), None, "不存在檔案應回 None");
    }

    #[test]
    fn read_port_at_garbage_content_returns_none() {
        let path = tmp_port("garbage");
        std::fs::write(&path, b"not-a-port-at-all").expect("write fixture");
        assert_eq!(read_port_at(&path), None, "非 u16 內容應 parse 失敗回 None");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_port_at_valid_u16_returns_value() {
        let path = tmp_port("valid");
        std::fs::write(&path, b"19280").expect("write fixture");
        assert_eq!(read_port_at(&path), Some(19280), "合法 u16 應原樣回傳");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_port_at_trims_whitespace_and_newlines() {
        let path = tmp_port("trim");
        std::fs::write(&path, b"  19285\n").expect("write fixture");
        assert_eq!(read_port_at(&path), Some(19285), "trim 後 u16 應正確解析");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_port_at_rejects_out_of_range_u16() {
        // 超過 u16::MAX 的字串應 parse 失敗(非 silent overflow)
        let path = tmp_port("overflow");
        std::fs::write(&path, b"99999").expect("write fixture");
        assert_eq!(
            read_port_at(&path),
            None,
            "超過 65535 的值應 parse 失敗,不能 silent 截斷"
        );
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod post_tests {
    //! R34 + R164 regression:`post` 之前在 `main` 用 `let _ = post(...)` silent 吞網路
    //! 錯誤,改為 caller 端 `if let Err(e) = ... { eprintln!(...) }` 後,本 module 鎖 post
    //! 端契約:
    //! 1. happy path:連到 ephemeral TCP listener → 寫入的 request 應含 provider/body,
    //!    server 回 200 → post 回 Ok
    //! 2. connect 失敗:綁到不存在的 port → 回 Err(讓 caller log 警告,不 silent)
    //! 3. R164: server 回 4xx → post 必須 propagate 為 Err,不能 silent 吞成 Ok

    use super::*;
    use std::net::TcpListener;

    #[test]
    fn post_sends_provider_and_body_to_listener() {
        // 綁 ephemeral port(127.0.0.1:0)→ 取得實際綁定的 port
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().expect("local_addr").port();

        let body = r#"{"hook_event_name":"UserPromptSubmit","session_id":"s1"}"#;
        // 在另一個 thread accept,R164 起 `post` 會等 server status line → server thread
        // 必須先 write 200 OK 再 read request(TCP duplex 允許);原本 read_to_end 先 read
        // 會 block 到 client close,但 client 在 read status 階段不會主動 close,死結。
        let expected_provider = "claude".to_string();
        let expected_body = body.to_string();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            // 先 write response,讓 client read 立刻拿到 status line
            let _ = sock
                .write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            // 再 read request(verification 用)
            let mut buf = Vec::new();
            let _ = sock.read_to_end(&mut buf);
            let s = String::from_utf8_lossy(&buf).to_string();
            (s, expected_provider, expected_body)
        });

        post(port, "claude", body).expect("happy post 應成功");
        let (received, provider, body) = handle.join().expect("thread join");

        assert!(
            received.contains(&format!("/hook/{provider}")),
            "request 應含 /hook/{{provider}},實際:{received}"
        );
        assert!(
            received.contains(&body),
            "request 應含 body,實際:{received}"
        );
        assert!(
            received.contains("Content-Length: "),
            "request 應有 Content-Length header"
        );
    }

    #[test]
    fn post_returns_err_on_unreachable_port() {
        // 綁一個 port 然後 drop listener → 該 port 立刻空出,但若沒人搶,
        // OS 短窗內仍會丟 RST/ConnectionRefused。
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("local_addr").port();
        drop(listener); // 立刻關閉 listener

        // 用一個高機率空着的 port 連線(直接連剛 drop 的 port 或 1)
        let result = post(port, "claude", "{}");
        assert!(result.is_err(), "連到已關閉的 port 應回 Err,不能 silent 吞");
    }

    #[test]
    fn post_returns_err_on_4xx_response() {
        // R164 regression:server 回 400 Bad Request 必須 propagate 為 Err。
        // 模擬 hook_server::process_body 失敗路徑(JSON parse 失敗 / unknown provider /
        // 沒 body),原本 `let _ = stream.read(...)` 直接吞 → post 回 Ok → 父 CLI log
        // 顯示「hook fired」但 LP 沒收到 event,silent event loss。
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().expect("local_addr").port();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            // 先 write 400,讓 client read 立刻拿到
            let _ = sock.write_all(
                b"HTTP/1.0 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            let mut discard = [0u8; 1024];
            let _ = sock.read(&mut discard);
        });

        let result = post(port, "claude", "{}");
        let _ = handle.join();

        let err = result.expect_err("server 400 應 propagate 為 Err,不能 silent 吞成 Ok");
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::Other,
            "4xx 走 ErrorKind::Other"
        );
        assert!(
            err.to_string().contains("400"),
            "err msg 應含 status code 400,實際:{}",
            err
        );
        assert!(
            err.to_string().contains("claude"),
            "err msg 應含 provider id,實際:{}",
            err
        );
    }

    #[test]
    fn post_returns_err_on_5xx_response() {
        // 對齊 4xx 契約:server 5xx (目前永遠 0,保留供未來) 也走同路徑
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().expect("local_addr").port();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            let _ = sock.write_all(
                b"HTTP/1.0 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            let mut discard = [0u8; 1024];
            let _ = sock.read(&mut discard);
        });

        let result = post(port, "codex", "{}");
        let _ = handle.join();

        let err = result.expect_err("server 500 應 propagate 為 Err");
        assert!(
            err.to_string().contains("500"),
            "err msg 應含 500,實際:{}",
            err
        );
    }

    #[test]
    fn post_returns_err_on_malformed_status_line() {
        // server 回了非 HTTP 開頭的 garbage → parse_status_code 應回 None → post 走 InvalidData
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().expect("local_addr").port();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            let _ = sock.write_all(b"NOT-AN-HTTP-RESPONSE\r\n");
            let mut discard = [0u8; 1024];
            let _ = sock.read(&mut discard);
        });

        let result = post(port, "claude", "{}");
        let _ = handle.join();

        let err = result.expect_err("malformed status 應 propagate 為 Err");
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::InvalidData,
            "malformed 走 InvalidData"
        );
    }

    #[test]
    fn post_returns_err_on_eof_without_response() {
        // server accept 後立刻 drop。OS 表現兩條路徑:
        //   - Windows: drop 觸發 RST → ConnectionReset (os 10054) — `stream.read` 直接回 Err
        //   - Linux/macOS: drop 觸發 FIN → UnexpectedEof — `stream.read` 回 Ok(0),我們走
        //     UnexpectedEof 分支自己 Err
        // 兩條路徑都是 Err,契約是「server 沒給 status line → propagate Err」,不 hardcode
        // kind (OS 本地化字串會變)。
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().expect("local_addr").port();
        let handle = std::thread::spawn(move || {
            let (sock, _) = listener.accept().expect("accept");
            drop(sock);
        });

        let result = post(port, "claude", "{}");
        let _ = handle.join();

        assert!(
            result.is_err(),
            "server 沒回 status line 應回 Err,不是 silent Ok"
        );
    }
}

#[cfg(test)]
mod parse_status_code_tests {
    //! R164:`parse_status_code` 是 post 內部 helper,獨立抽到 mod level 方便 unit test。
    //! 鎖 4 條契約:
    //! 1. 標準 status line → 正確 parse u16
    //! 2. 沒 reason phrase → 仍 parse u16
    //! 3. 非 HTTP/ 開頭 → None
    //! 4. HTTP/ 開頭但第二欄不是 u16 → None

    use super::*;

    #[test]
    fn parses_standard_200() {
        assert_eq!(parse_status_code("HTTP/1.1 200 OK"), Some(200));
    }

    #[test]
    fn parses_400_with_reason() {
        assert_eq!(parse_status_code("HTTP/1.0 400 Bad Request"), Some(400));
    }

    #[test]
    fn parses_500_with_reason() {
        assert_eq!(
            parse_status_code("HTTP/1.1 500 Internal Server Error"),
            Some(500)
        );
    }

    #[test]
    fn parses_204_no_reason_phrase() {
        // 某些 server / proxy 只回 `HTTP/1.0 204\r\n`,沒 reason phrase,仍要 parse
        assert_eq!(parse_status_code("HTTP/1.0 204"), Some(204));
    }

    #[test]
    fn rejects_non_http_prefix() {
        assert_eq!(parse_status_code("NOT-AN-HTTP-RESPONSE"), None);
    }

    #[test]
    fn rejects_http_prefix_but_non_numeric_code() {
        // 防 server bug / 攻擊 payload: status code 不是 u16 → None
        assert_eq!(parse_status_code("HTTP/1.1 5xx Server Error"), None);
    }

    #[test]
    fn rejects_empty_line() {
        assert_eq!(parse_status_code(""), None);
    }

    #[test]
    fn rejects_http_only_no_code() {
        // 只有 `HTTP/1.1` 沒 code → None
        assert_eq!(parse_status_code("HTTP/1.1"), None);
    }
}
