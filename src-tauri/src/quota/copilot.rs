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

/// Copilot 額度（quota_snapshots）：必須帶 IDE 模擬 headers 才會回額度欄位
const COPILOT_USER_API: &str = "https://api.github.com/copilot_internal/user";

/// 從 env var 拿 GitHub token。優先序：GH_TOKEN > GITHUB_TOKEN > COPILOT_TOKEN
/// > `gh auth token` subprocess（本機 gh CLI 把 token 存 Windows Credential
/// Manager，hosts.yml 只有 user 名，subprocess 是最穩取法）。
/// 0 bytes / whitespace-only 視同「未登入」,早返 ⚠ 友善提示。
fn read_credentials() -> Result<String, String> {
    if let Some(t) = env_token() {
        return Ok(t);
    }
    if let Some(t) = gh_auth_token() {
        return Ok(t);
    }
    Err(
        "no GitHub token in env (set GH_TOKEN / GITHUB_TOKEN / COPILOT_TOKEN, or run `gh auth login`)"
            .to_string(),
    )
}

/// env 層單獨拆出：測試不受本機 gh 登入狀態影響
fn env_token() -> Option<String> {
    for key in ["GH_TOKEN", "GITHUB_TOKEN", "COPILOT_TOKEN"] {
        if let Ok(v) = std::env::var(key) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// shell out `gh auth token`。GUI app 下必帶 CREATE_NO_WINDOW（0x08000000）
/// 否則每次刷新閃一個黑色 console 窗（踩雷 §25）。
fn gh_auth_token() -> Option<String> {
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["auth", "token"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

/// 從 copilot_internal/user 回應抽額度。回 (plan, premium_pct, reset_text, snapshots_raw)。
/// `unlimited: true` 視為 100% 剩餘；欄位缺 → None（前端 No data），不腦補 0。
fn parse_copilot_quota(
    body: &serde_json::Value,
) -> (Option<String>, Option<i64>, Option<String>, serde_json::Value) {
    let plan = body
        .get("copilot_plan")
        .and_then(|v| v.as_str())
        .map(String::from);
    let premium = body
        .get("quota_snapshots")
        .and_then(|q| q.get("premium_interactions"));
    let pct = premium.and_then(|p| {
        if p.get("unlimited").and_then(|v| v.as_bool()) == Some(true) {
            return Some(100);
        }
        p.get("percent_remaining")
            .and_then(|v| v.as_f64())
            .map(|v| v.round().clamp(0.0, 100.0) as i64)
    });
    // quota_reset_date "YYYY-MM-DD"（月重置）→ 距今小時數 "XhYm" 供前端天數換算
    let reset = body
        .get("quota_reset_date")
        .and_then(|v| v.as_str())
        .and_then(|d| {
            let date = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
            let reset_dt = date.and_hms_opt(0, 0, 0)?.and_utc();
            let delta = (reset_dt - chrono::Utc::now()).num_minutes();
            if delta <= 0 {
                return None;
            }
            Some(format!("{}h{}m", delta / 60, delta % 60))
        });
    let snaps = body
        .get("quota_snapshots")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    (plan, pct, reset, snaps)
}

/// 取 token 末 4 碼當遮罩（不暴露完整 secret）。長度 < 4 時回 "****"。
fn token_preview(token: &str) -> String {
    if token.len() < 4 {
        return "****".to_string();
    }
    let tail = &token[token.len() - 4..];
    format!("****{tail}")
}

/// 呼叫 copilot_internal/user 拿 Copilot 額度（premium requests % + 月重置日）。
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

    // copilot_internal/user：IDE 模擬 headers 是硬需求（缺了 quota_snapshots 不出現）。
    // auth scheme 先 `token`，401/403 再試 `Bearer`（PAT 與 OAuth token 偏好不同）。
    let mut api_ok = false;
    let mut status_code = 0u16;
    let mut body = serde_json::Value::Null;
    for scheme in ["token", "Bearer"] {
        let resp = client
            .get(COPILOT_USER_API)
            .header("Authorization", format!("{scheme} {token}"))
            .header("Editor-Version", "vscode/1.107.0")
            .header("Editor-Plugin-Version", "copilot-chat/0.35.0")
            .header("User-Agent", "GitHubCopilotChat/0.35.0")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await;
        match resp {
            Ok(r) => {
                status_code = r.status().as_u16();
                if r.status().is_success() {
                    body = r.json().await.unwrap_or(serde_json::Value::Null);
                    api_ok = true;
                    break;
                }
                if status_code != 401 && status_code != 403 {
                    break; // 非認證錯不用換 scheme
                }
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
        }
    }

    if !api_ok {
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: format!("⚠ token rejected ({status_code})\ntoken {preview}"),
            raw: Some(serde_json::json!({
                "ok": false, "status_code": status_code,
                "ts": chrono::Utc::now().to_rfc3339(),
            })),
        };
    }

    let (plan, premium_pct, reset, snaps) = parse_copilot_quota(&body);
    let plan_disp = plan.clone().unwrap_or_else(|| "Copilot".to_string());
    let text = match premium_pct {
        Some(p) => format!("⏱ Premium {p}%\n{plan_disp}"),
        None => format!("✓ {plan_disp} · token {preview}"),
    };

    let raw = serde_json::json!({
        "ok": true,
        "status_code": status_code,
        // Premium requests 是月配額：借 h5 slot + 自訂 label 顯示
        "h5_remaining": premium_pct,
        "h5_reset": reset,
        "session_label": "Premium",
        "plan": plan_disp,
        "quota_snapshots": snaps,
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

/// process-global Mutex 序列化所有讀寫 `GH_TOKEN` / `GITHUB_TOKEN` / `COPILOT_TOKEN`
/// 的測試。env var 是 process-global 狀態,cargo test parallel 跑時多個
/// test 同時 set_var / remove_var 會互相覆寫 → 偶發 1 fail (R111 紀錄
/// 「parallel 預設跑 quota::copilot 會因 env-var race 偶發 1 fail」)。
/// 為零新增 dep 守 K42 chain 不擴張,用 std::sync::Mutex 全局序列化。
/// `pub(crate)` 讓 lib.rs 內的 quota 測試也共用同一把鎖,避免 cross-mod 污染。
/// 用 `unwrap_or_else(|p| p.into_inner())` 防 panic 導致 poison 後續 test 連鎖死。
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    fn parse_copilot_quota_extracts_premium_percent() {
        let body = serde_json::json!({
            "copilot_plan": "individual_pro",
            "quota_reset_date": "2099-08-01",
            "quota_snapshots": {
                "premium_interactions": { "percent_remaining": 87.5, "unlimited": false,
                                           "entitlement": 300, "remaining": 262 },
                "chat": { "unlimited": true },
                "completions": { "unlimited": true }
            }
        });
        let (plan, pct, reset, snaps) = parse_copilot_quota(&body);
        assert_eq!(plan.as_deref(), Some("individual_pro"));
        assert_eq!(pct, Some(88));
        assert!(reset.is_some(), "未來的 reset date 應有 countdown");
        assert!(snaps.get("premium_interactions").is_some());
    }

    #[test]
    fn parse_copilot_quota_unlimited_is_full() {
        let body = serde_json::json!({
            "quota_snapshots": { "premium_interactions": { "unlimited": true } }
        });
        let (_, pct, _, _) = parse_copilot_quota(&body);
        assert_eq!(pct, Some(100));
    }

    #[test]
    fn parse_copilot_quota_missing_snapshots_yields_none() {
        let (plan, pct, reset, _) = parse_copilot_quota(&serde_json::json!({"message":"Bad credentials"}));
        assert!(plan.is_none() && pct.is_none() && reset.is_none());
    }

    /// 透過 env 注入 token 測 read_credentials 優先序。
    /// race 防護:env 是 process-global,cargo test parallel 跑多個 test
    /// 同時 set_var / remove_var 會互相覆寫。用 `ENV_LOCK` (process-global
    /// std::sync::Mutex) 序列化所有 env 寫入的 test,跨 mod 共用同一把鎖
    /// 避免 copilot.rs::tests 與 lib.rs::tests 互相污染。
    /// 零新增 dep 守 K42 chain 不擴張。`unwrap_or_else(|p| p.into_inner())`
    /// 防 panic 導致 poison 後續 test 連鎖死。
    #[test]
    fn read_credentials_prefers_gh_token() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // 先清空所有候選,確保優先序可重現
        std::env::remove_var("COPILOT_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::set_var("GH_TOKEN", "ghp_gh_token_value");
        let t = env_token().expect("GH_TOKEN 應成功");
        assert_eq!(t, "ghp_gh_token_value");
        std::env::remove_var("GH_TOKEN");
    }

    #[test]
    fn read_credentials_falls_back_to_github_token() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::remove_var("COPILOT_TOKEN");
        std::env::remove_var("GH_TOKEN");
        std::env::set_var("GITHUB_TOKEN", "ghp_github_token_value");
        let t = env_token().expect("GITHUB_TOKEN 應成功");
        assert_eq!(t, "ghp_github_token_value");
        std::env::remove_var("GITHUB_TOKEN");
    }

    #[test]
    fn read_credentials_falls_back_to_copilot_token() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::set_var("COPILOT_TOKEN", "ghp_copilot_token_value");
        let t = env_token().expect("COPILOT_TOKEN 應成功");
        assert_eq!(t, "ghp_copilot_token_value");
        std::env::remove_var("COPILOT_TOKEN");
    }

    #[test]
    fn read_credentials_whitespace_only_treated_as_missing() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("COPILOT_TOKEN");
        assert!(env_token().is_none(), "無 env 應回 None（gh fallback 另計）");
    }
}
