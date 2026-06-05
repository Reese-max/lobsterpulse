//! OpenAI (Codex CLI) live quota fetch：讀 `~/.codex/auth.json` 拿 access_token、
//! 解析 JWT exp 拿 token 到期日、打 `/v1/models` 探 token 有效性、從 `config.toml`
//! 拿 model 名、顯示 ChatGPT 訂閱類型。R86 落地，R89 經 Tauri command 接入
//! (`quota::codex::fetch`)。

use super::RunnerQuota;
use serde::Deserialize;
use std::path::Path;

const OPENAI_MODELS_API: &str = "https://api.openai.com/v1/models";

#[derive(Debug, Deserialize)]
struct CodexAuth {
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
    tokens: Option<CodexTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexTokens {
    #[serde(rename = "id_token")]
    id_token: Option<String>,
    #[serde(rename = "access_token")]
    access_token: Option<String>,
    #[serde(rename = "account_id")]
    account_id: Option<String>,
}

/// 解析 JWT payload 拿 `exp` claim（不驗簽，只讀 base64 payload）
fn jwt_exp(token: &str) -> Option<u64> {
    let json = decode_jwt_payload(token)?;
    json.get("exp").and_then(|v| v.as_u64())
}

/// 解析 JWT payload 為 serde_json::Value（不驗簽，只讀 base64 payload）
fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload_b64 = token.split('.').nth(1)?;
    let mut b64 = payload_b64.replace('-', "+").replace('_', "/");
    match b64.len() % 4 {
        2 => b64.push_str("=="),
        3 => b64.push('='),
        _ => {}
    }
    let bytes = base64_decode(&b64).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 最小 base64 解碼（不引 base64 crate，避無謂 dep）
/// -1 = 非法 char；0..=63 = base64 值
fn base64_lookup(c: u8) -> i8 {
    match c {
        b'A'..=b'Z' => (c - b'A') as i8,
        b'a'..=b'z' => (c - b'a' + 26) as i8,
        b'0'..=b'9' => (c - b'0' + 52) as i8,
        b'+' => 62,
        b'/' => 63,
        _ => -1,
    }
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let v = base64_lookup(c);
        if v < 0 {
            return Err(format!("invalid base64 char {c}"));
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// 從 `epoch_secs` 算「XhYm」剩餘時間，過期回 "expired"
fn fmt_countdown(epoch_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if epoch_secs <= now {
        return "expired".to_string();
    }
    let delta = epoch_secs - now;
    let h = delta / 3600;
    let m = (delta % 3600) / 60;
    if h > 0 {
        format!("{h}h{m}m")
    } else {
        format!("{m}m")
    }
}

/// 讀 `~/.codex/auth.json`，回 (access_token, account_id, plan_type)
fn read_credentials(home: &Path) -> Result<(String, String, String), String> {
    let path = home.join(".codex").join("auth.json");
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read auth.json: {e}"))?;
    let auth: CodexAuth =
        serde_json::from_str(&data).map_err(|e| format!("parse auth.json: {e}"))?;

    // API key path（純 API 用戶）
    if let Some(key) = auth.openai_api_key.as_deref() {
        if !key.is_empty() && key != "null" {
            return Ok((key.to_string(), "—".to_string(), "API key".to_string()));
        }
    }

    // OAuth path（ChatGPT 訂閱用戶）
    let tokens = auth.tokens.ok_or("missing tokens")?;
    let access = tokens.access_token.ok_or("missing access_token")?;
    let account = tokens.account_id.unwrap_or_else(|| "—".to_string());

    // 訂閱類型從 id_token JWT 解（包在 `https://api.openai.com/auth` 物件下）
    let plan = tokens
        .id_token
        .as_deref()
        .and_then(decode_jwt_payload)
        .and_then(|v| {
            v.get("https://api.openai.com/auth")
                .and_then(|a| a.get("chatgpt_plan_type"))
                .and_then(|p| p.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| "ChatGPT".to_string());

    Ok((access, account, plan))
}

/// 從 `~/.codex/config.toml` 抓 `model = "..."`（容錯 grep，TOML 解析非必要）
///
/// 邊界：必須 `model` 開頭且後面只能是空白或 `=`，避免誤吃 `model_reasoning_effort`。
fn read_model(home: &Path) -> String {
    let path = home.join(".codex").join("config.toml");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| {
            for line in s.lines() {
                let trimmed = line.trim();
                let after_model = match trimmed.strip_prefix("model") {
                    Some(r)
                        if r.is_empty()
                            || r.starts_with(|c: char| c.is_whitespace() || c == '=') =>
                    {
                        r
                    }
                    _ => continue,
                };
                let after_eq = match after_model.find('=') {
                    Some(i) => &after_model[i + 1..],
                    None => continue,
                };
                let value = after_eq.trim().trim_matches('"');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
            None
        })
        .unwrap_or_else(|| "—".to_string())
}

/// 呼叫 OpenAI API 探 token 有效性
pub async fn fetch(home: &Path) -> RunnerQuota {
    let label = "💻 Codex CLI（本機）".to_string();
    let color = "#10a37f".to_string();
    let name = "codex".to_string();

    let (token, account, plan) = match read_credentials(home) {
        Ok(v) => v,
        Err(e) => {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: format!("⚠ {e}"),
                raw: None,
            };
        }
    };

    let model = read_model(home);
    let token_exp = jwt_exp(&token)
        .map(fmt_countdown)
        .unwrap_or_else(|| "—".to_string());

    // 探 token 有效（200 = ok，401 = expired/invalid）
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let resp = client
        .get(OPENAI_MODELS_API)
        .bearer_auth(&token)
        .send()
        .await;

    let (api_ok, status_code) = match resp {
        Ok(r) => (r.status().is_success(), r.status().as_u16()),
        Err(e) => {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: format!("⚠ API error: {e}"),
                raw: None,
            }
        }
    };

    let text = if api_ok {
        format!("✓ {plan} · model {model}\n⏱ token {token_exp}")
    } else {
        format!("⚠ token rejected ({status_code})\nmodel {model}")
    };

    let raw = serde_json::json!({
        "ok": api_ok,
        "plan": plan,
        "model": model,
        "token_expires_in": token_exp,
        "account_id": account,
        "status_code": status_code,
        "ts": chrono::Utc::now().to_rfc3339(),
    });

    RunnerQuota {
        name,
        label,
        color,
        ok: api_ok,
        text,
        raw: Some(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decode_known_value() {
        // "hello" base64 = "aGVsbG8="
        let decoded = base64_decode("aGVsbG8=").expect("decode");
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn base64_decode_empty_string() {
        let decoded = base64_decode("").expect("decode");
        assert!(decoded.is_empty());
    }

    #[test]
    fn base64_decode_rejects_invalid_char() {
        assert!(base64_decode("aGVs!G8=").is_err());
    }

    #[test]
    fn base64_decode_handles_url_safe_via_caller() {
        // url-safe 轉換是 caller 的責任，這裡只測標準 base64
        let decoded = base64_decode("aGVsbG8=").expect("decode");
        assert_eq!(std::str::from_utf8(&decoded).unwrap(), "hello");
    }

    #[test]
    fn jwt_exp_returns_none_for_malformed_token() {
        assert!(jwt_exp("not-a-jwt").is_none());
        assert!(jwt_exp("only.two").is_none());
        assert!(jwt_exp("").is_none());
    }

    #[test]
    fn jwt_exp_extracts_real_exp() {
        // 真實 JWT: header={"alg":"RS256"} payload={"exp":1780459263,"sub":"user"}
        // header base64: eyJhbGciOiJSUzI1NiJ9
        // payload base64: eyJleHAiOjE3ODA0NTkyNjMsInN1YiI6InVzZXIifQ
        let token = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjE3ODA0NTkyNjMsInN1YiI6InVzZXIifQ.signature";
        assert_eq!(jwt_exp(token), Some(1780459263));
    }

    #[test]
    fn fmt_countdown_past_epoch_returns_expired() {
        assert_eq!(fmt_countdown(0), "expired");
    }

    #[test]
    fn fmt_countdown_under_one_hour_renders_minutes() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let s = fmt_countdown(now + 30 * 60);
        assert!(s.ends_with('m'));
        assert!(s.contains("30m"), "got {s:?}");
    }

    #[test]
    fn fmt_countdown_over_one_hour_renders_hm() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let s = fmt_countdown(now + 2 * 3600 + 15 * 60);
        assert!(s.starts_with("2h"));
        assert!(s.contains("15m"));
    }

    #[test]
    fn read_model_parses_standard_config() {
        let dir = std::env::temp_dir().join("lp_codex_test_model");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".codex")).expect("mkdir");
        std::fs::write(
            dir.join(".codex").join("config.toml"),
            "model = \"gpt-5.5\"\n",
        )
        .expect("write");
        assert_eq!(read_model(&dir), "gpt-5.5");
    }

    #[test]
    fn read_model_missing_file_returns_dash() {
        let dir = std::env::temp_dir().join("lp_codex_test_model_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        assert_eq!(read_model(&dir), "—");
    }

    #[test]
    fn read_model_handles_comments_and_other_keys() {
        let dir = std::env::temp_dir().join("lp_codex_test_model_comments");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".codex")).expect("mkdir");
        std::fs::write(
            dir.join(".codex").join("config.toml"),
            "# comment\nmodel_reasoning_effort = \"medium\"\nmodel = \"gpt-5.4-mini\"\n",
        )
        .expect("write");
        assert_eq!(read_model(&dir), "gpt-5.4-mini");
    }
}
