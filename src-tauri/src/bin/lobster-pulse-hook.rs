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
        // R34 surface: 原 `let _ = post(...)` silent 吞網路錯誤，operator 看到
        // 「event 沒到 LP」但完全沒線索區分「LP 沒啟動 / port 不通 / write 失敗」。
        eprintln!(
            "{LOG_PREFIX} post to 127.0.0.1:{port} failed: {e} — \
             event dropped, parent CLI continues (check LP running on this port)"
        );
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

    // Drain server response best-effort（要 connection: close 收尾乾淨）;
    // 讀失敗不阻斷 — 我們不 care response body,只關心 server 有 accept 連線。
    let mut discard = [0u8; 64];
    let _ = stream.read(&mut discard);
    Ok(())
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
    //! R34 regression:`post` 之前在 `main` 用 `let _ = post(...)` silent 吞網路錯誤,
    //! 改為 caller 端 `if let Err(e) = ... { eprintln!(...) }` 後,本 module 鎖 post
    //! 端 2 條契約:
    //! 1. happy path:連到 ephemeral TCP listener → 寫入的 request 應含 provider/body
    //! 2. connect 失敗:綁到不存在的 port → 回 Err(讓 caller log 警告,不 silent)

    use super::*;
    use std::net::TcpListener;

    #[test]
    fn post_sends_provider_and_body_to_listener() {
        // 綁 ephemeral port(127.0.0.1:0)→ 取得實際綁定的 port
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().expect("local_addr").port();

        let body = r#"{"hook_event_name":"UserPromptSubmit","session_id":"s1"}"#;
        // 在另一個 thread accept,避免 post() block(本來 read 60s timeout)
        let expected_provider = "claude".to_string();
        let expected_body = body.to_string();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
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
}
