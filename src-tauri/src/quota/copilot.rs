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

/// 從 copilot_internal/user 回應抽額度。回 (plan, buckets, reset_text, snapshots_raw)。
/// buckets 是至多 2 個 (顯示名, 剩餘%)，優先序 Premium > Chat > Completions。
/// 關鍵陷阱（2026-07-17 實測）：individual plan 的 premium_interactions 是
/// `has_quota: false`、entitlement 0——不是「用光了」而是「這個 plan 沒這種額度」，
/// 寫死 premium 會顯示誤導的 0%。`has_quota: false` 的 bucket 一律跳過。
/// `unlimited: true` 視為 100% 剩餘；欄位缺 → 不出 bucket（前端 No data），不腦補 0。
fn parse_copilot_quota(
    body: &serde_json::Value,
) -> (
    Option<String>,
    Vec<(&'static str, i64)>,
    Option<String>,
    serde_json::Value,
) {
    let plan = body
        .get("copilot_plan")
        .and_then(|v| v.as_str())
        .map(String::from);
    let snapshots = body.get("quota_snapshots");
    let mut buckets = Vec::new();
    for (key, label) in [
        ("premium_interactions", "Premium"),
        ("chat", "Chat"),
        ("completions", "Completions"),
    ] {
        if buckets.len() == 2 {
            break;
        }
        let Some(b) = snapshots.and_then(|q| q.get(key)) else {
            continue;
        };
        if b.get("has_quota").and_then(|v| v.as_bool()) == Some(false) {
            continue;
        }
        let pct = if b.get("unlimited").and_then(|v| v.as_bool()) == Some(true) {
            Some(100)
        } else {
            b.get("percent_remaining")
                .and_then(|v| v.as_f64())
                .map(|v| v.round().clamp(0.0, 100.0) as i64)
        };
        if let Some(p) = pct {
            buckets.push((label, p));
        }
    }
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
    let snaps = snapshots.cloned().unwrap_or(serde_json::Value::Null);
    (plan, buckets, reset, snaps)
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

    // read_credentials 內含 `gh auth token` 同步子進程——直接在 async fn 裡呼叫
    // 會佔住 tokio worker，gh 若卡住（credential manager 互動等）整個 snapshot
    // 永久卡死。spawn_blocking 隔離 + 10s 逾時（基線 ~0.8s，但本機 24/7 跑
    // 自動化，2026-07-17 實測編譯尖峰下 5s 會誤殺）；逾時非網路型錯誤，不觸發 retry_net。
    // ponytail: 逾時只放棄等待，卡死的 gh 子進程不會被殺——若 gh 永久掛住，每個
    // TTL 週期會漏一條 blocking 執行緒；真發生時改為顯式 kill 子進程。
    let cred = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::task::spawn_blocking(read_credentials),
    )
    .await;
    let token = match cred {
        Ok(Ok(Ok(t))) => t,
        Ok(Ok(Err(e))) => {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: format!("⚠ {e}"),
                raw: None,
            };
        }
        _ => {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: "⚠ gh auth token 逾時或執行失敗（>10s），檢查 gh CLI 狀態".to_string(),
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
                "ok": false, "basis": "provider_api",
                "status_code": status_code,
                "ts": chrono::Utc::now().to_rfc3339(),
            })),
        };
    }

    let (plan, buckets, reset, snaps) = parse_copilot_quota(&body);
    let plan_disp = plan.clone().unwrap_or_else(|| "Copilot".to_string());
    let text = if buckets.is_empty() {
        format!("✓ {plan_disp} · token {preview}")
    } else {
        let parts: Vec<String> = buckets.iter().map(|(l, p)| format!("{l} {p}%")).collect();
        format!("⏱ {}\n{plan_disp}", parts.join(" · "))
    };

    // 月配額借 h5/wk 兩個 slot + 自訂 label 顯示；reset 是同一個月重置日
    let raw = serde_json::json!({
        "ok": true,
        "basis": "provider_api",
        "status_code": status_code,
        "h5_remaining": buckets.first().map(|(_, p)| *p),
        "h5_reset": buckets.first().map(|_| reset.clone()),
        "session_label": buckets.first().map(|(l, _)| *l),
        "wk_remaining": buckets.get(1).map(|(_, p)| *p),
        "wk_reset": buckets.get(1).map(|_| reset.clone()),
        "weekly_label": buckets.get(1).map(|(l, _)| *l),
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
        let (plan, buckets, reset, snaps) = parse_copilot_quota(&body);
        assert_eq!(plan.as_deref(), Some("individual_pro"));
        assert_eq!(buckets, vec![("Premium", 88), ("Chat", 100)]);
        assert!(reset.is_some(), "未來的 reset date 應有 countdown");
        assert!(snaps.get("premium_interactions").is_some());
    }

    #[test]
    fn parse_copilot_quota_skips_has_quota_false_buckets() {
        // 2026-07-17 本機 individual plan 實測形狀：premium 是 has_quota:false /
        // entitlement 0（plan 沒這種額度，不是用光），真額度在 chat + completions。
        let body = serde_json::json!({
            "copilot_plan": "individual",
            "quota_snapshots": {
                "premium_interactions": { "percent_remaining": 0.0, "unlimited": false,
                                           "has_quota": false, "entitlement": 0, "remaining": 0 },
                "chat": { "percent_remaining": 95.3, "unlimited": false,
                          "has_quota": true, "entitlement": 200, "remaining": 190 },
                "completions": { "percent_remaining": 100.0, "unlimited": false,
                                  "has_quota": true, "entitlement": 2000, "remaining": 2000 }
            }
        });
        let (_, buckets, _, _) = parse_copilot_quota(&body);
        assert_eq!(buckets, vec![("Chat", 95), ("Completions", 100)]);
    }

    #[test]
    fn parse_copilot_quota_unlimited_is_full() {
        let body = serde_json::json!({
            "quota_snapshots": { "premium_interactions": { "unlimited": true } }
        });
        let (_, buckets, _, _) = parse_copilot_quota(&body);
        assert_eq!(buckets, vec![("Premium", 100)]);
    }

    #[test]
    fn parse_copilot_quota_missing_snapshots_yields_none() {
        let (plan, buckets, reset, _) =
            parse_copilot_quota(&serde_json::json!({"message":"Bad credentials"}));
        assert!(plan.is_none() && buckets.is_empty() && reset.is_none());
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

#[cfg(test)]
mod live_probe {
    // 手動診斷用：cargo test --release live_copilot -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_copilot_fetch_prints_result() {
        for i in 0..3 {
            let r = super::fetch(&dirs::home_dir().unwrap()).await;
            println!("[{i}] ok={} text={:?} raw={:?}", r.ok, r.text, r.raw.map(|v| v.to_string()));
        }
    }
}
