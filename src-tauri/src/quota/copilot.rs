//! GitHub Copilot CLI (本機) live quota fetch：讀 `GH_TOKEN` env var 拿
//! GitHub OAuth token、呼叫 `https://api.github.com/user` 探 token 有效性、
//! 從 response 拿 login 顯示。R109 落地（K0 Quota 推進：9/13 → 10/13）。
//!
//! 對齊 anthropic.rs / codex.rs / gemini.rs 既有 contract：RunnerQuota 結構、
//! read_credentials 早返 (ok=false) 不打 API 防 timeout、fmt_* helper 複製不抽共用。
//!
//! 邊界：
//! - `GH_TOKEN` / `GITHUB_TOKEN` / `COPILOT_TOKEN` 都不存在 → read_credentials 早返 ⚠
//! - 0 bytes / whitespace-only token 視同「未登入」→ 早返 ⚠ 友善提示
//! - API 401 / 網路 timeout → 末尾 match 早返 ⚠，不打後續
//! - API 200 → ok=true + 顯示 GitHub login + token 末 4 碼

use super::RunnerQuota;
use std::path::Path;

const GITHUB_USER_API: &str = "https://api.github.com/user";

/// 從 env var 拿 GitHub token。優先序：GH_TOKEN > GITHUB_TOKEN > COPILOT_TOKEN。
/// Copilot CLI 內部走 `gh auth token` 取 token，三個 env var 都是
/// `gh auth token` 會採用的同源（見 gh CLI 文件）。
/// 0 bytes / whitespace-only 視同「未登入」,早返 ⚠ 友善提示。
fn read_credentials() -> Result<String, String> {
    for key in ["GH_TOKEN", "GITHUB_TOKEN", "COPILOT_TOKEN"] {
        if let Ok(v) = std::env::var(key) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }
    Err(
        "no GitHub token in env (set GH_TOKEN / GITHUB_TOKEN / COPILOT_TOKEN, or run `gh auth login`)"
            .to_string(),
    )
}

/// 取 token 末 4 碼當遮罩（不暴露完整 secret）。長度 < 4 時回 "****"。
fn token_preview(token: &str) -> String {
    if token.len() < 4 {
        return "****".to_string();
    }
    let tail = &token[token.len() - 4..];
    format!("****{tail}")
}

/// 寬鬆從 GitHub /user response body 抓 `login` 欄位。
/// 為何不用 serde_json::from_str 直接 parse 整個 GitHubUser struct?
/// fetch 路徑要容忍 GitHub 回其他形狀 (rate limit / 401 error body),
/// 寬鬆從 JSON 抓 `login` 即可,不要 strict struct 把 200 + 沒 login 欄位
/// 判成 fatal error。
fn parse_user_login(json_body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json_body).ok()?;
    v.get("login")
        .and_then(|l| l.as_str())
        .map(|s| s.to_string())
}

/// 呼叫 GitHub /user API 探 token 有效性（200 = ok，401 = invalid）。
/// 解 login 欄位供前端顯示,其他欄位丟棄(避免暴露個資)。
pub async fn fetch(home: &Path) -> RunnerQuota {
    let label = "💻 Copilot CLI（本機）".to_string();
    let color = "#6e40c9".to_string();
    let name = "copilot".to_string();
    // `home` 參數保留是對齊 anthropic / codex / gemini contract
    // (未來若改讀 ~/.copilot/ 檔案,沿用同簽名免破 wire),本輪用 env var 暫忽略。
    let _ = home;

    let token = match read_credentials() {
        Ok(t) => t,
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

    let preview = token_preview(&token);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("lobsterpulse-quota-check")
        .build()
        .unwrap_or_default();

    let resp = client
        .get(GITHUB_USER_API)
        .bearer_auth(&token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await;

    let (api_ok, status_code, login) = match resp {
        Ok(r) => {
            let code = r.status().as_u16();
            let ok = r.status().is_success();
            let body = if ok {
                r.text().await.unwrap_or_default()
            } else {
                String::new()
            };
            (ok, code, parse_user_login(&body))
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

    let text = if api_ok {
        let user = login.as_deref().unwrap_or("unknown");
        format!("✓ Copilot CLI · {user} · token {preview}")
    } else {
        format!("⚠ token rejected ({status_code})\ntoken {preview}")
    };

    let raw = serde_json::json!({
        "ok": api_ok,
        "status_code": status_code,
        "login": login,
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
    fn token_preview_full_length_shows_last_four() {
        assert_eq!(token_preview("ghp_abcdefghijklmnop1234"), "****1234");
    }

    #[test]
    fn token_preview_short_token_masks_completely() {
        assert_eq!(token_preview("abc"), "****");
        assert_eq!(token_preview(""), "****");
    }

    #[test]
    fn parse_user_login_extracts_login_field() {
        let body = r#"{"login":"Reese-max","id":12345,"name":"Reese"}"#;
        assert_eq!(parse_user_login(body), Some("Reese-max".to_string()));
    }

    #[test]
    fn parse_user_login_missing_field_returns_none() {
        let body = r#"{"id":12345}"#;
        assert_eq!(parse_user_login(body), None);
    }

    #[test]
    fn parse_user_login_invalid_json_returns_none() {
        assert_eq!(parse_user_login("not json"), None);
        assert_eq!(parse_user_login(""), None);
    }

    #[test]
    fn parse_user_login_handles_error_body() {
        // GitHub 401 / rate limit 回 {"message":"Bad credentials",...}
        // 沒有 login 欄位 → None,不 panic
        let body = r#"{"message":"Bad credentials","documentation_url":"..."}"#;
        assert_eq!(parse_user_login(body), None);
    }

    /// 透過 env 注入 token 測 read_credentials 優先序。
    /// 風險:env 是 process-global,平行 cargo test 會互相干擾 → 用
    /// `serial_test` crate 標記? 為了零新增 dep,改用 std::env::set_var
    /// 在每個測試頭尾嚴格隔離,接受小機率 flake。
    /// 真實驗證:測試單獨跑過 (`cargo test copilot`) 0 flake。
    #[test]
    fn read_credentials_prefers_gh_token() {
        // 先清空所有候選,確保優先序可重現
        std::env::remove_var("COPILOT_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::set_var("GH_TOKEN", "ghp_gh_token_value");
        let t = read_credentials().expect("GH_TOKEN 應成功");
        assert_eq!(t, "ghp_gh_token_value");
        std::env::remove_var("GH_TOKEN");
    }

    #[test]
    fn read_credentials_falls_back_to_github_token() {
        std::env::remove_var("COPILOT_TOKEN");
        std::env::remove_var("GH_TOKEN");
        std::env::set_var("GITHUB_TOKEN", "ghp_github_token_value");
        let t = read_credentials().expect("GITHUB_TOKEN 應成功");
        assert_eq!(t, "ghp_github_token_value");
        std::env::remove_var("GITHUB_TOKEN");
    }

    #[test]
    fn read_credentials_falls_back_to_copilot_token() {
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::set_var("COPILOT_TOKEN", "ghp_copilot_token_value");
        let t = read_credentials().expect("COPILOT_TOKEN 應成功");
        assert_eq!(t, "ghp_copilot_token_value");
        std::env::remove_var("COPILOT_TOKEN");
    }

    #[test]
    fn read_credentials_whitespace_only_treated_as_missing() {
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("COPILOT_TOKEN");
        std::env::set_var("GH_TOKEN", "   \n\t  ");
        let err = read_credentials().expect_err("全空白 token 應失敗");
        assert!(err.contains("no GitHub token"), "got: {err}");
        std::env::remove_var("GH_TOKEN");
    }

    #[test]
    fn read_credentials_no_env_vars_returns_friendly_error() {
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("COPILOT_TOKEN");
        let err = read_credentials().expect_err("無 env 應失敗");
        assert!(err.contains("no GitHub token"), "got: {err}");
        assert!(err.contains("GH_TOKEN"), "got: {err}");
        assert!(err.contains("gh auth login"), "got: {err}");
    }
}
