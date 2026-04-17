// Sidecar binary invoked by CLI hook configs. Reads the event JSON from
// stdin and forwards it as an HTTP POST to the running LobsterPulse server.
//
// Usage: lobster-pulse-hook <provider_id>
//
// Shell-agnostic by design: no bash, no PowerShell, no cmd syntax. Any
// host CLI that can spawn a process (on any OS) can invoke this. Errors
// are swallowed so a hook misfire never breaks the parent CLI.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const DEFAULT_PORT: u16 = 19280;
const TIMEOUT: Duration = Duration::from_secs(2);

fn main() {
    let provider = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "unknown".to_string());

    let mut body = String::new();
    let _ = std::io::stdin().read_to_string(&mut body);

    let port = read_port().unwrap_or(DEFAULT_PORT);
    let _ = post(port, &provider, &body);
}

fn read_port() -> Option<u16> {
    let home = dirs::home_dir()?;
    let content = std::fs::read_to_string(home.join(".lobsterpulse").join("port")).ok()?;
    content.trim().parse().ok()
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

    let mut discard = [0u8; 64];
    let _ = stream.read(&mut discard);
    Ok(())
}
