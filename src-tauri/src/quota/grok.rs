//! Grok CLI (xAI) live quota fetch：讀 `~/.grok/auth.json`（map，key 形如
//! `https://auth.x.ai::<client_id>`），打 Grok CLI 自己 billing.rs 用的同一支
//! `cli-chat-proxy.grok.com/v1/billing?format=credits` 拿 weekly 用量 %。
//! 方法出自 OpenUsage（robinebers/openusage）GrokUsageClient.swift。
//!
//! proto3-JSON 陷阱：零值欄位整個省略——缺 `creditUsagePercent` 代表 0%，
//! 不是 schema 變了，serde 全欄位 Option。

use super::RunnerQuota;
use std::path::Path;

const GROK_BILLING_API: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const GROK_SETTINGS_API: &str = "https://cli-chat-proxy.grok.com/v1/settings";

/// 讀 auth.json：map 裡找 auth.x.ai 開頭的 entry，回 (access_token, expires_at)
fn read_credentials(home: &Path) -> Result<(String, Option<String>), String> {
    let path = home.join(".grok").join("auth.json");
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read auth.json: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("parse auth.json: {e}"))?;
    let obj = v.as_object().ok_or("auth.json is not an object")?;
    for (k, entry) in obj {
        if !k.starts_with("https://auth.x.ai") {
            continue;
        }
        let token = entry
            .get("key")
            .and_then(|t| t.as_str())
            .ok_or("missing key (access token)")?;
        let expires = entry
            .get("expires_at")
            .and_then(|e| e.as_str())
            .map(String::from);
        return Ok((token.to_string(), expires));
    }
    Err("no auth.x.ai entry in auth.json (grok CLI not logged in)".to_string())
}

/// ISO8601 → 「XhYm」倒數；過期/壞格式 → None
fn countdown_from_iso(s: &str) -> Option<String> {
    let dt = chrono::DateTime::parse_from_rfc3339(s).ok()?;
    let delta = (dt.timestamp() - chrono::Utc::now().timestamp()).max(0);
    if delta == 0 {
        return None;
    }
    Some(format!("{}h{}m", delta / 3600, (delta % 3600) / 60))
}

/// 從 billing 回應抽 weekly (remaining_pct, reset_text)。
/// 只認 WEEKLY period；缺 creditUsagePercent = 0% used（proto3 省略零值）。
fn parse_billing(body: &serde_json::Value) -> (Option<i64>, Option<String>) {
    let Some(config) = body.get("config") else {
        return (None, None);
    };
    let period_type = config
        .get("currentPeriod")
        .and_then(|p| p.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    if period_type != "USAGE_PERIOD_TYPE_WEEKLY" {
        return (None, None);
    }
    let used = config
        .get("creditUsagePercent")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0); // proto3-JSON：缺欄位 = 0
    let remaining = (100.0 - used).round().clamp(0.0, 100.0) as i64;
    let reset = config
        .get("currentPeriod")
        .and_then(|p| p.get("end"))
        .and_then(|e| e.as_str())
        .and_then(countdown_from_iso);
    (Some(remaining), reset)
}

pub async fn fetch(home: &Path) -> RunnerQuota {
    let label = "Grok CLI".to_string();
    let color = "#9aa0a6".to_string();
    let name = "grok".to_string();

    let (token, _expires) = match read_credentials(home) {
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

    let resp = client
        .get(GROK_BILLING_API)
        .bearer_auth(&token)
        .header("X-XAI-Token-Auth", "xai-grok-cli")
        .header("Accept", "application/json")
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
                text: format!("⚠ billing API {code}（token 可能過期，跑一次 grok 可刷新）"),
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

    let (weekly_pct, weekly_reset) = parse_billing(&body);

    // 方案名（可選，失敗不擋）
    let tier = match client
        .get(GROK_SETTINGS_API)
        .bearer_auth(&token)
        .header("X-XAI-Token-Auth", "xai-grok-cli")
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| {
                v.get("subscription_tier_display")
                    .and_then(|t| t.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| "Grok".to_string()),
        _ => "Grok".to_string(),
    };

    let text = match weekly_pct {
        Some(p) => format!("⏱ 7d {p}%\n{tier}"),
        None => format!("✓ {tier}（非週期帳務，無 % 資料）"),
    };

    let raw = serde_json::json!({
        "ok": true,
        "wk_remaining": weekly_pct,
        "wk_reset": weekly_reset,
        "plan": tier,
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
    fn parse_billing_weekly_with_usage() {
        let body = serde_json::json!({ "config": {
            "creditUsagePercent": 99.0,
            "currentPeriod": { "type": "USAGE_PERIOD_TYPE_WEEKLY",
                               "start": "2026-07-03T04:01:09+00:00",
                               "end": "2099-07-10T04:01:09+00:00" },
            "isUnifiedBillingUser": true
        }});
        let (pct, reset) = parse_billing(&body);
        assert_eq!(pct, Some(1));
        assert!(reset.is_some());
    }

    #[test]
    fn parse_billing_missing_percent_means_zero_used() {
        // proto3-JSON 省略零值：缺 creditUsagePercent = 0% used = 100% left
        let body = serde_json::json!({ "config": {
            "currentPeriod": { "type": "USAGE_PERIOD_TYPE_WEEKLY", "end": "2099-01-01T00:00:00Z" }
        }});
        let (pct, _) = parse_billing(&body);
        assert_eq!(pct, Some(100));
    }

    #[test]
    fn parse_billing_non_weekly_returns_none() {
        let body = serde_json::json!({ "config": {
            "creditUsagePercent": 50.0,
            "currentPeriod": { "type": "USAGE_PERIOD_TYPE_MONTHLY" }
        }});
        let (pct, _) = parse_billing(&body);
        assert!(pct.is_none());
    }

    #[test]
    fn read_credentials_finds_auth_xai_entry() {
        let dir = std::env::temp_dir().join("lp_grok_test_auth");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".grok")).expect("mkdir");
        std::fs::write(
            dir.join(".grok").join("auth.json"),
            r#"{"https://auth.x.ai::client-uuid": {"key": "xai-token", "refresh_token": "r", "expires_at": "2099-01-01T00:00:00Z"}}"#,
        )
        .expect("write");
        let (token, exp) = read_credentials(&dir).expect("應找到 entry");
        assert_eq!(token, "xai-token");
        assert!(exp.is_some());
    }

    #[test]
    fn read_credentials_missing_entry_errors() {
        let dir = std::env::temp_dir().join("lp_grok_test_noentry");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".grok")).expect("mkdir");
        std::fs::write(dir.join(".grok").join("auth.json"), r#"{"other": {}}"#).expect("write");
        assert!(read_credentials(&dir).is_err());
    }
}
