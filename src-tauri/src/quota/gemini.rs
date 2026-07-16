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

/// Cloud Code（gemini CLI 後端）：quota 用兩步 loadCodeAssist → retrieveUserQuota
const CODE_ASSIST_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";
/// gemini-cli 內建的公開 installed-app OAuth client（bundle 內同值）
const GEMINI_OAUTH_CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const GEMINI_OAUTH_CLIENT_SECRET: &str = "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";

#[derive(Debug, Deserialize)]
struct GeminiCredentials {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expiry: Option<String>,
    /// gemini CLI 實際寫的是毫秒 epoch `expiry_date`
    expiry_date: Option<u64>,
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

/// 讀 `~/.gemini/oauth_creds.json`。
/// 0 bytes 視同「未登入」(gemini CLI 登出後會清空檔案),早返 ⚠ 友善提示。
fn read_credentials(home: &Path) -> Result<GeminiCredentials, String> {
    let path = home.join(".gemini").join("oauth_creds.json");
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read oauth_creds.json: {e}"))?;
    if data.trim().is_empty() {
        return Err("oauth_creds.json is empty (gemini CLI not logged in)".to_string());
    }
    let creds: GeminiCredentials =
        serde_json::from_str(&data).map_err(|e| format!("parse oauth_creds.json: {e}"))?;
    if creds.access_token.is_none() {
        return Err("missing access_token".to_string());
    }
    Ok(creds)
}

/// access_token 過期（expiry_date 毫秒 epoch 或 expiry RFC3339）→ true
fn token_expired(creds: &GeminiCredentials, now_secs: u64) -> bool {
    if let Some(ms) = creds.expiry_date {
        return ms / 1000 <= now_secs + 60;
    }
    if let Some(exp) = creds.expiry.as_deref().and_then(parse_expiry) {
        return exp <= now_secs + 60;
    }
    false // 沒有到期資訊 → 直接試打，401 再說
}

/// 用 refresh_token 換新 access_token（公開 installed-app client 憑證）
async fn refresh_access_token(client: &reqwest::Client, refresh_token: &str) -> Option<String> {
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", GEMINI_OAUTH_CLIENT_ID),
            ("client_secret", GEMINI_OAUTH_CLIENT_SECRET),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    v.get("access_token").and_then(|t| t.as_str()).map(String::from)
}

/// 從 retrieveUserQuota 的 buckets 取「最緊」bucket。
/// remainingFraction 是 0-1 比例（不是百分比）。回 (remaining_pct, reset_text, buckets_raw)。
fn tightest_bucket(body: &serde_json::Value) -> (Option<i64>, Option<String>, serde_json::Value) {
    let buckets = body.get("buckets").and_then(|b| b.as_array());
    let Some(buckets) = buckets else {
        return (None, None, serde_json::Value::Null);
    };
    let mut best: Option<(f64, Option<String>)> = None;
    for b in buckets {
        let Some(frac) = b.get("remainingFraction").and_then(|v| v.as_f64()) else {
            continue; // 缺 remainingFraction 不腦補 0/100
        };
        let reset = b.get("resetTime").and_then(|v| v.as_str()).map(String::from);
        if best.as_ref().map(|(f, _)| frac < *f).unwrap_or(true) {
            best = Some((frac, reset));
        }
    }
    match best {
        Some((frac, reset)) => {
            let pct = (frac * 100.0).round().clamp(0.0, 100.0) as i64;
            let reset_text = reset
                .as_deref()
                .and_then(parse_expiry)
                .map(fmt_countdown);
            (Some(pct), reset_text, body.get("buckets").cloned().unwrap_or(serde_json::Value::Null))
        }
        None => (None, None, serde_json::Value::Null),
    }
}

/// 兩步 Cloud Code API：loadCodeAssist（拿 project）→ retrieveUserQuota（拿 buckets）
pub async fn fetch(home: &Path) -> RunnerQuota {
    let label = "💻 Gemini CLI（本機）".to_string();
    let color = "#4285f4".to_string();
    let name = "gemini".to_string();

    let creds = match read_credentials(home) {
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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    // token 過期 → 先 refresh（不寫回 oauth_creds.json，CLI 自己管自己的檔）
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut token = creds.access_token.clone().unwrap_or_default();
    if token_expired(&creds, now) {
        match creds.refresh_token.as_deref() {
            Some(rt) => match refresh_access_token(&client, rt).await {
                Some(t) => token = t,
                None => {
                    return RunnerQuota {
                        name,
                        label,
                        color,
                        ok: false,
                        text: "⚠ token 過期且 refresh 失敗（跑一次 gemini 可重登）".to_string(),
                        raw: None,
                    };
                }
            },
            None => {
                return RunnerQuota {
                    name,
                    label,
                    color,
                    ok: false,
                    text: "⚠ token 過期且無 refresh_token".to_string(),
                    raw: None,
                };
            }
        }
    }

    // Step 1: loadCodeAssist → cloudaicompanionProject（免費 tier 可能沒有，容許空）
    let load_body = serde_json::json!({
        "metadata": { "ideType": "GEMINI_CLI", "pluginType": "GEMINI" }
    });
    let project = match client
        .post(format!("{CODE_ASSIST_BASE}:loadCodeAssist"))
        .bearer_auth(&token)
        .json(&load_body)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            let v: serde_json::Value = r.json().await.unwrap_or(serde_json::Value::Null);
            let p = v.get("cloudaicompanionProject");
            p.and_then(|p| {
                p.as_str().map(String::from).or_else(|| {
                    p.get("id")
                        .or_else(|| p.get("projectId"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
            })
        }
        Ok(r) => {
            let code = r.status().as_u16();
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: format!("⚠ loadCodeAssist {code}"),
                raw: Some(serde_json::json!({
                    "ok": false, "status_code": code, "ts": chrono::Utc::now().to_rfc3339(),
                })),
            };
        }
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

    // Step 2: retrieveUserQuota → buckets（按 model 分桶；取最緊）
    let quota_body = match project.as_deref() {
        Some(p) => serde_json::json!({ "project": p }),
        None => serde_json::json!({}),
    };
    let resp = client
        .post(format!("{CODE_ASSIST_BASE}:retrieveUserQuota"))
        .bearer_auth(&token)
        .json(&quota_body)
        .send()
        .await;
    let body: serde_json::Value = match resp {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or(serde_json::Value::Null),
        Ok(r) => {
            let code = r.status().as_u16();
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: format!("⚠ retrieveUserQuota {code}"),
                raw: Some(serde_json::json!({
                    "ok": false, "status_code": code, "ts": chrono::Utc::now().to_rfc3339(),
                })),
            };
        }
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

    let (pct, reset, buckets) = tightest_bucket(&body);
    let text = match pct {
        Some(p) => format!("⏱ Daily {p}%"),
        None => "✓ Gemini CLI（無額度資料）".to_string(),
    };

    let raw = serde_json::json!({
        "ok": true,
        // 按 model 分桶的日額度：借 h5 slot + Daily label
        "h5_remaining": pct,
        "h5_reset": reset,
        "session_label": "Daily",
        "plan": "Code Assist",
        "buckets": buckets,
        "ts": chrono::Utc::now().to_rfc3339(),
    });

    RunnerQuota {
        name,
        label,
        color,
        ok: true,
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
        let creds = read_credentials(&dir).expect("valid 應成功");
        assert_eq!(creds.access_token.as_deref(), Some("ya29.test"));
        assert_eq!(creds.expiry.as_deref(), Some("2026-12-31T23:59:59Z"));
    }

    #[test]
    fn token_expired_uses_expiry_date_ms() {
        let creds: GeminiCredentials = serde_json::from_str(
            r#"{"access_token":"t","expiry_date":1700000000000}"#,
        )
        .unwrap();
        assert!(token_expired(&creds, 1700000001)); // 已過
        assert!(!token_expired(&creds, 1600000000)); // 未過
    }

    #[test]
    fn tightest_bucket_picks_min_fraction() {
        let body = serde_json::json!({ "buckets": [
            { "modelId": "gemini-2.5-pro", "remainingFraction": 0.83, "resetTime": "2099-01-01T00:00:00Z" },
            { "modelId": "gemini-2.5-flash", "remainingFraction": 0.97 },
            { "modelId": "broken-no-fraction" }
        ]});
        let (pct, reset, buckets) = tightest_bucket(&body);
        assert_eq!(pct, Some(83));
        assert!(reset.is_some());
        assert_eq!(buckets.as_array().unwrap().len(), 3);
    }

    #[test]
    fn tightest_bucket_empty_returns_none() {
        let (pct, reset, _) = tightest_bucket(&serde_json::json!({}));
        assert!(pct.is_none() && reset.is_none());
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
