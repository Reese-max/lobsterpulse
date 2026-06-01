//! Discord bot REST helper —— 走 Windows 內建 curl.exe，不加 crate 依賴。
//! Button interaction 靠 reaction polling 實現（不開 Gateway WSS）。

use serde::Deserialize;
use std::process::Command;

const API: &str = "https://discord.com/api/v10";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn auth_ok(token: &str, channel_id: &str) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("token empty".into());
    }
    if channel_id.trim().is_empty() {
        return Err("channel_id empty".into());
    }
    Ok(())
}

fn curl(method: &str, url: &str, token: &str, body: Option<&str>) -> Result<Vec<u8>, String> {
    let auth = format!("Authorization: Bot {}", token.trim());
    let mut cmd = Command::new("curl.exe");
    cmd.args([
        "-s",
        "-S",
        "-f",
        "-X",
        method,
        "-H",
        &auth,
        "-H",
        "User-Agent: LobsterPulse/0.6",
    ]);
    if let Some(b) = body {
        cmd.args(["-H", "Content-Type: application/json", "--data-raw", b]);
    }
    cmd.arg(url);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| format!("curl spawn fail: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}

/// 發純文字訊息 → 回傳 message_id。
pub fn send_message(token: &str, channel_id: &str, content: &str) -> Result<String, String> {
    auth_ok(token, channel_id)?;
    let body = serde_json::json!({ "content": content }).to_string();
    let url = format!("{API}/channels/{channel_id}/messages");
    let resp = curl("POST", &url, token, Some(&body))?;
    parse_msg_id(&resp)
}

/// 發 embed 訊息（標題+描述+顏色）→ 回傳 message_id。color 是 0xRRGGBB。
pub fn send_embed(
    token: &str,
    channel_id: &str,
    title: &str,
    desc: &str,
    color: u32,
) -> Result<String, String> {
    auth_ok(token, channel_id)?;
    let body = serde_json::json!({
        "embeds": [{
            "title": title,
            "description": desc,
            "color": color,
        }]
    })
    .to_string();
    let url = format!("{API}/channels/{channel_id}/messages");
    let resp = curl("POST", &url, token, Some(&body))?;
    parse_msg_id(&resp)
}

fn parse_msg_id(resp: &[u8]) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_slice(resp)
        .map_err(|e| format!("parse resp fail ({e}): {}", String::from_utf8_lossy(resp)))?;
    v.get("id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no message id: {}", String::from_utf8_lossy(resp)))
}

/// Bot 自己加 reaction（讓用戶一鍵點選）。emoji 用 unicode 字元。
pub fn add_reaction(
    token: &str,
    channel_id: &str,
    message_id: &str,
    emoji: &str,
) -> Result<(), String> {
    auth_ok(token, channel_id)?;
    // curl 用 --data-urlencode-like 不方便，直接 percent-encode。
    let encoded = percent_encode(emoji);
    let url = format!("{API}/channels/{channel_id}/messages/{message_id}/reactions/{encoded}/@me");
    curl("PUT", &url, token, Some(""))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct DiscordUser {
    #[allow(dead_code)]
    pub id: String,
    #[serde(default)]
    pub bot: bool,
}

/// 檢查訊息是否有非 bot 的用戶按了 emoji → 回傳 true（有人點）。
pub fn has_human_reactor(
    token: &str,
    channel_id: &str,
    message_id: &str,
    emoji: &str,
) -> Result<bool, String> {
    auth_ok(token, channel_id)?;
    let encoded = percent_encode(emoji);
    let url =
        format!("{API}/channels/{channel_id}/messages/{message_id}/reactions/{encoded}?limit=20");
    let resp = curl("GET", &url, token, None)?;
    let users: Vec<DiscordUser> = serde_json::from_slice(&resp)
        .map_err(|e| format!("parse users fail ({e}): {}", String::from_utf8_lossy(&resp)))?;
    Ok(users.iter().any(|u| !u.bot))
}

/// 列最近 N 筆訊息（for command polling）。
pub fn list_messages(
    token: &str,
    channel_id: &str,
    limit: u32,
    after: Option<&str>,
) -> Result<Vec<serde_json::Value>, String> {
    auth_ok(token, channel_id)?;
    let mut url = format!("{API}/channels/{channel_id}/messages?limit={limit}");
    if let Some(a) = after {
        url.push_str(&format!("&after={a}"));
    }
    let resp = curl("GET", &url, token, None)?;
    serde_json::from_slice::<Vec<serde_json::Value>>(&resp).map_err(|e| {
        format!(
            "parse messages fail ({e}): {}",
            String::from_utf8_lossy(&resp)
        )
    })
}

/// RFC3986 percent-encode 給 URL path 片段用（emoji 等多 byte 字元）。
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
