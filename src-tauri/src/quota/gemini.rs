//! Gemini CLI (本機) live quota fetch：讀 `~/.gemini/oauth_creds.json` 拿
//! access_token + expiry，呼叫 `https://generativelanguage.googleapis.com/v1beta/models`
//! 探 token 有效性。R108 落地（K0 Quota 推進：8/13 → 9/13）。
//!
//! 對齊 anthropic.rs / codex.rs 既有 contract：RunnerQuota 結構、fmt_countdown
//! helper、read_credentials 早返 (ok=false) 不打 API 防 timeout。
//!
//! 邊界：
//! - oauth_creds.json 不存在 / 0 bytes（gemini CLI 未登入） → read_credentials 早返 ⚠
//! - 解析失敗（不是 JSON / 缺 access_token） → read_credentials 早返 ⚠
//! - API 401 / 網路 timeout → 末尾 match 早返 ⚠，不打後續
//! - API 200 → ok=true + 顯示 token 剩餘時間

use super::RunnerQuota;
use serde::Deserialize;
use std::path::Path;

const GEMINI_MODELS_API: &str = "https://generativelanguage.googleapis.com/v1beta/models";

#[derive(Debug, Deserialize)]
struct GeminiCredentials {
    access_token: Option<String>,
    expiry: Option<String>,
    #[serde(rename = "token_type")]
    _token_type: Option<String>,
}

/// 解析 RFC3339 expiry 字串回 unix epoch 秒。
/// Gemini CLI oauth_creds.json 寫法例: `"2026-12-31T23:59:59.000Z"`。
fn parse_expiry(s: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp().max(0) as u64)
}

/// 從 `epoch_secs` 算「XhYm」剩餘時間，過期回 "expired"。
/// 對齊 anthropic.rs / codex.rs 同名 helper,複製而非抽共用,避免 quota/
/// 模組之間的 cyclic dep 風險。
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

/// 讀 `~/.gemini/oauth_creds.json`，回 (access_token, expiry_rfc3339)。
/// 0 bytes 視同「未登入」(gemini CLI 登出後會清空檔案),早返 ⚠ 友善提示。
fn read_credentials(home: &Path) -> Result<(String, Option<String>), String> {
    let path = home.join(".gemini").join("oauth_creds.json");
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read oauth_creds.json: {e}"))?;
    if data.trim().is_empty() {
        return Err("oauth_creds.json is empty (gemini CLI not logged in)".to_string());
    }
    let creds: GeminiCredentials =
        serde_json::from_str(&data).map_err(|e| format!("parse oauth_creds.json: {e}"))?;
    let token = creds.access_token.ok_or("missing access_token")?;
    Ok((token, creds.expiry))
}

/// 呼叫 Google Generative Language API 探 token 有效性（200 = ok，401 = expired/invalid）。
pub async fn fetch(home: &Path) -> RunnerQuota {
    let label = "💻 Gemini CLI（本機）".to_string();
    let color = "#4285f4".to_string();
    let name = "gemini".to_string();

    let (token, expiry) = match read_credentials(home) {
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

    let token_exp = expiry
        .as_deref()
        .and_then(parse_expiry)
        .map(fmt_countdown)
        .unwrap_or_else(|| "—".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let resp = client
        .get(GEMINI_MODELS_API)
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
            };
        }
    };

    let text = if api_ok {
        format!("✓ Gemini CLI · token {token_exp}")
    } else {
        format!("⚠ token rejected ({status_code})\ntoken {token_exp}")
    };

    let raw = serde_json::json!({
        "ok": api_ok,
        "token_expires_in": token_exp,
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
        assert!(s.ends_with('m'), "expected trailing 'm', got {s:?}");
        assert!(s.contains("30m"), "expected '30m' in {s:?}");
    }

    #[test]
    fn fmt_countdown_over_one_hour_renders_hm() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let s = fmt_countdown(now + 2 * 3600 + 15 * 60);
        assert!(s.starts_with("2h"), "expected '2h' prefix, got {s:?}");
        assert!(s.contains("15m"), "expected '15m' in {s:?}");
    }

    #[test]
    fn parse_expiry_rfc3339_returns_unix_seconds() {
        // 2026-01-01T00:00:00Z = 1767225600
        assert_eq!(parse_expiry("2026-01-01T00:00:00Z"), Some(1767225600));
    }

    #[test]
    fn parse_expiry_with_milliseconds_works() {
        // 2026-06-01T00:00:00.000Z = 1780272000 (151 天 from 2026-01-01 1767225600)
        // chrono parse_from_rfc3339 寬鬆接受 .000Z 形式
        assert_eq!(
            parse_expiry("2026-06-01T00:00:00.000Z"),
            Some(1_780_272_000)
        );
    }

    #[test]
    fn parse_expiry_invalid_string_returns_none() {
        assert!(parse_expiry("not-a-date").is_none());
        assert!(parse_expiry("").is_none());
    }

    #[test]
    fn read_credentials_missing_file_returns_error() {
        let dir = std::env::temp_dir().join("lp_gemini_test_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let err = read_credentials(&dir).expect_err("missing file 應失敗");
        assert!(err.contains("read oauth_creds.json"), "got: {err}");
    }

    #[test]
    fn read_credentials_empty_file_returns_friendly_error() {
        let dir = std::env::temp_dir().join("lp_gemini_test_empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".gemini")).expect("mkdir");
        std::fs::write(dir.join(".gemini").join("oauth_creds.json"), "").expect("write");
        let err = read_credentials(&dir).expect_err("empty file 應失敗");
        assert!(err.contains("empty"), "got: {err}");
        assert!(err.contains("not logged in"), "got: {err}");
    }

    #[test]
    fn read_credentials_whitespace_only_file_treated_as_empty() {
        let dir = std::env::temp_dir().join("lp_gemini_test_ws");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".gemini")).expect("mkdir");
        std::fs::write(dir.join(".gemini").join("oauth_creds.json"), "   \n\t  ").expect("write");
        let err = read_credentials(&dir).expect_err("whitespace 應視同 empty");
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn read_credentials_valid_file_returns_token() {
        let dir = std::env::temp_dir().join("lp_gemini_test_valid");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".gemini")).expect("mkdir");
        std::fs::write(
            dir.join(".gemini").join("oauth_creds.json"),
            r#"{"access_token":"ya29.test","expiry":"2026-12-31T23:59:59Z"}"#,
        )
        .expect("write");
        let (token, expiry) = read_credentials(&dir).expect("valid 應成功");
        assert_eq!(token, "ya29.test");
        assert_eq!(expiry, Some("2026-12-31T23:59:59Z".to_string()));
    }

    #[test]
    fn read_credentials_missing_token_field_returns_error() {
        let dir = std::env::temp_dir().join("lp_gemini_test_no_token");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".gemini")).expect("mkdir");
        std::fs::write(
            dir.join(".gemini").join("oauth_creds.json"),
            r#"{"refresh_token":"1//abc"}"#,
        )
        .expect("write");
        let err = read_credentials(&dir).expect_err("無 access_token 應失敗");
        assert!(err.contains("access_token"), "got: {err}");
    }

    #[test]
    fn read_credentials_invalid_json_returns_error() {
        let dir = std::env::temp_dir().join("lp_gemini_test_badjson");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".gemini")).expect("mkdir");
        std::fs::write(
            dir.join(".gemini").join("oauth_creds.json"),
            "not json at all",
        )
        .expect("write");
        let err = read_credentials(&dir).expect_err("壞 JSON 應失敗");
        assert!(err.contains("parse oauth_creds.json"), "got: {err}");
    }
}
