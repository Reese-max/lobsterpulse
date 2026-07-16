//! Devin CLI live quota fetch：讀 `%APPDATA%\devin\credentials.toml` 拿
//! windsurf_api_key，打 Windsurf/Codeium seat-management RPC `GetUserStatus`
//! 拿 Daily/Weekly 剩餘 %。方法出自 OpenUsage DevinUsageClient.swift。
//! 注意：API key 放 body 的 metadata，不是 header。

use super::RunnerQuota;
use std::path::Path;

const DEFAULT_API_SERVER: &str = "https://server.codeium.com";
const GET_USER_STATUS_PATH: &str =
    "/exa.seat_management_pb.SeatManagementService/GetUserStatus";

/// 簡易 TOML 行解析（key = "value"），對齊 codex.rs read_model 的容錯 grep 做法
fn toml_str(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('=') {
            continue;
        }
        let value = rest[1..].trim().trim_matches('"');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// 讀 credentials.toml，回 (api_key, api_server_url)
fn read_credentials(appdata: &Path) -> Result<(String, String), String> {
    let path = appdata.join("devin").join("credentials.toml");
    let data =
        std::fs::read_to_string(&path).map_err(|e| format!("read credentials.toml: {e}"))?;
    let key = toml_str(&data, "windsurf_api_key").ok_or("missing windsurf_api_key")?;
    let server =
        toml_str(&data, "api_server_url").unwrap_or_else(|| DEFAULT_API_SERVER.to_string());
    Ok((key, server))
}

fn fmt_countdown(epoch_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if epoch_secs <= now {
        return "expired".to_string();
    }
    let delta = epoch_secs - now;
    format!("{}h{}m", delta / 3600, (delta % 3600) / 60)
}

/// 從 GetUserStatus 回應抽 (daily, weekly, plan)。
/// 欄位缺 → None（前端 No data），不腦補。
#[allow(clippy::type_complexity)]
fn parse_user_status(
    body: &serde_json::Value,
) -> (
    Option<(i64, Option<String>)>,
    Option<(i64, Option<String>)>,
    Option<String>,
) {
    let Some(ps) = body.get("userStatus").and_then(|u| u.get("planStatus")) else {
        return (None, None, None);
    };
    let win = |pct_key: &str, reset_key: &str| {
        ps.get(pct_key).and_then(|v| v.as_f64()).map(|p| {
            let reset = ps
                .get(reset_key)
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .map(fmt_countdown);
            (p.round().clamp(0.0, 100.0) as i64, reset)
        })
    };
    let daily = win("dailyQuotaRemainingPercent", "dailyQuotaResetAtUnix");
    let weekly = win("weeklyQuotaRemainingPercent", "weeklyQuotaResetAtUnix");
    let plan = ps
        .get("planInfo")
        .and_then(|p| p.get("planName"))
        .and_then(|v| v.as_str())
        .map(String::from);
    (daily, weekly, plan)
}

pub async fn fetch(appdata: &Path) -> RunnerQuota {
    let label = "Devin CLI".to_string();
    let color = "#2ea3ff".to_string();
    let name = "devin".to_string();

    let (api_key, server) = match read_credentials(appdata) {
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

    let body = serde_json::json!({ "metadata": {
        "apiKey": api_key,
        "ideName": "devin", "ideVersion": "1.108.2",
        "extensionName": "devin", "extensionVersion": "1.108.2",
        "locale": "en"
    }});
    let resp = client
        .post(format!("{server}{GET_USER_STATUS_PATH}"))
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .json(&body)
        .send()
        .await;
    let v: serde_json::Value = match resp {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or(serde_json::Value::Null),
        Ok(r) => {
            let code = r.status().as_u16();
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: format!("⚠ GetUserStatus {code}"),
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

    let (daily, weekly, plan) = parse_user_status(&v);
    let plan_disp = plan.unwrap_or_else(|| "Devin".to_string());
    let fmt = |w: &Option<(i64, Option<String>)>| {
        w.as_ref().map(|(p, _)| format!("{p}%")).unwrap_or_else(|| "--".to_string())
    };
    let text = format!("⏱ Daily {} · 7d {}\n{}", fmt(&daily), fmt(&weekly), plan_disp);

    let raw = serde_json::json!({
        "ok": true,
        "h5_remaining": daily.as_ref().map(|d| d.0),
        "h5_reset": daily.as_ref().and_then(|d| d.1.clone()),
        "session_label": "Daily",
        "wk_remaining": weekly.as_ref().map(|w| w.0),
        "wk_reset": weekly.as_ref().and_then(|w| w.1.clone()),
        "plan": plan_disp,
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
    fn toml_str_extracts_quoted_value() {
        let content = "windsurf_api_key = \"devin-abc123\"\napi_server_url = \"https://server.codeium.com\"\n";
        assert_eq!(toml_str(content, "windsurf_api_key").as_deref(), Some("devin-abc123"));
        assert_eq!(
            toml_str(content, "api_server_url").as_deref(),
            Some("https://server.codeium.com")
        );
    }

    #[test]
    fn toml_str_missing_key_returns_none() {
        assert!(toml_str("other = \"x\"\n", "windsurf_api_key").is_none());
    }

    #[test]
    fn parse_user_status_extracts_windows() {
        let body = serde_json::json!({ "userStatus": { "planStatus": {
            "dailyQuotaRemainingPercent": 72.4,
            "weeklyQuotaRemainingPercent": 91.0,
            "dailyQuotaResetAtUnix": 4102444800u64,
            "weeklyQuotaResetAtUnix": 4102444800u64,
            "planInfo": { "planName": "Core" }
        }}});
        let (daily, weekly, plan) = parse_user_status(&body);
        assert_eq!(daily.as_ref().map(|d| d.0), Some(72));
        assert_eq!(weekly.as_ref().map(|w| w.0), Some(91));
        assert_eq!(plan.as_deref(), Some("Core"));
        assert!(daily.unwrap().1.is_some());
    }

    #[test]
    fn parse_user_status_empty_returns_nones() {
        let (d, w, p) = parse_user_status(&serde_json::json!({}));
        assert!(d.is_none() && w.is_none() && p.is_none());
    }
}
