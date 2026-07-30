//! Grok CLI (xAI) live quota fetch：讀 `~/.grok/auth.json`（map，key 形如
//! `https://auth.x.ai::<client_id>`），打 Grok CLI 自己 billing.rs 用的同一支
//! `cli-chat-proxy.grok.com/v1/billing?format=credits` 拿 weekly 用量 %。
//! 方法出自 OpenUsage（robinebers/openusage）GrokUsageClient.swift。
//!
//! proto3-JSON 陷阱：零值欄位整個省略——缺 `creditUsagePercent` 代表 0%，
//! 不是 schema 變了，serde 全欄位 Option。

use super::RunnerQuota;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use std::path::{Path, PathBuf};

const GROK_BILLING_API: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const GROK_SETTINGS_API: &str = "https://cli-chat-proxy.grok.com/v1/settings";
const GROK_DEFAULT_ISSUER: &str = "https://auth.x.ai";
const TOKEN_REFRESH_SKEW_SECONDS: i64 = 60;

#[derive(Debug, Clone)]
struct GrokCredentials {
    scope: String,
    token: String,
    refresh_token: Option<String>,
    expires_at: Option<String>,
    issuer: String,
    client_id: String,
}

/// 讀 auth.json：map 裡找 auth.x.ai 開頭的 entry。
fn read_credentials(home: &Path) -> Result<GrokCredentials, String> {
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
        let refresh_token = entry
            .get("refresh_token")
            .and_then(|t| t.as_str())
            .filter(|t| !t.is_empty())
            .map(String::from);
        let expires_at = entry
            .get("expires_at")
            .and_then(|e| e.as_str())
            .map(String::from);
        let issuer = entry
            .get("oidc_issuer")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .unwrap_or(GROK_DEFAULT_ISSUER)
            .to_string();
        let client_id = entry
            .get("oidc_client_id")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .or_else(|| k.split_once("::").map(|(_, client_id)| client_id))
            .ok_or("missing oidc client id")?
            .to_string();
        return Ok(GrokCredentials {
            scope: k.to_string(),
            token: token.to_string(),
            refresh_token,
            expires_at,
            issuer,
            client_id,
        });
    }
    Err("no auth.x.ai entry in auth.json (grok CLI not logged in)".to_string())
}

fn token_needs_refresh(expires_at: Option<&str>) -> bool {
    let Some(expires_at) = expires_at else {
        return false;
    };
    let Ok(expires_at) = DateTime::parse_from_rfc3339(expires_at) else {
        return false;
    };
    expires_at.timestamp() <= Utc::now().timestamp() + TOKEN_REFRESH_SKEW_SECONDS
}

fn expires_at_from_response(body: &serde_json::Value) -> Option<String> {
    if let Some(value) = body.get("expires_at").and_then(|v| v.as_str()) {
        return Some(value.to_string());
    }
    let seconds = body
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            body.get("expires_in")
                .and_then(|v| v.as_str())
                .and_then(|v| v.parse::<i64>().ok())
        })
        .filter(|seconds| *seconds > 0)?;
    Some((Utc::now() + Duration::seconds(seconds)).to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn apply_refreshed_auth(
    root: &mut serde_json::Value,
    scope: &str,
    expected_token: &str,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<&str>,
) -> Result<(), String> {
    let entry = root
        .get_mut(scope)
        .and_then(|v| v.as_object_mut())
        .ok_or("auth scope disappeared while refreshing")?;
    let current_token = entry
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or("auth scope has no access token")?;
    if current_token != expected_token {
        return Err("auth.json changed while refreshing; leaving Grok auth untouched".into());
    }
    entry.insert(
        "key".to_string(),
        serde_json::Value::String(access_token.to_string()),
    );
    if let Some(refresh_token) = refresh_token.filter(|v| !v.is_empty()) {
        entry.insert(
            "refresh_token".to_string(),
            serde_json::Value::String(refresh_token.to_string()),
        );
    }
    if let Some(expires_at) = expires_at {
        entry.insert(
            "expires_at".to_string(),
            serde_json::Value::String(expires_at.to_string()),
        );
    }
    Ok(())
}

fn persist_refreshed_auth(
    home: &Path,
    current: &GrokCredentials,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<&str>,
) -> Result<GrokCredentials, String> {
    let path = home.join(".grok").join("auth.json");
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read auth.json: {e}"))?;
    let mut root: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("parse auth.json: {e}"))?;
    apply_refreshed_auth(
        &mut root,
        &current.scope,
        &current.token,
        access_token,
        refresh_token,
        expires_at,
    )?;
    let updated =
        serde_json::to_vec_pretty(&root).map_err(|e| format!("serialize auth.json: {e}"))?;
    backup_auth_once(&path)?;
    write_auth_atomically(&path, &updated)?;
    Ok(GrokCredentials {
        scope: current.scope.clone(),
        token: access_token.to_string(),
        refresh_token: refresh_token
            .filter(|v| !v.is_empty())
            .map(String::from)
            .or_else(|| current.refresh_token.clone()),
        expires_at: expires_at.map(String::from),
        issuer: current.issuer.clone(),
        client_id: current.client_id.clone(),
    })
}

fn auth_backup_path(path: &Path) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("auth.json");
    let day = chrono::Local::now().format("%Y%m%d");
    path.with_file_name(format!("{filename}.bak-{day}"))
}

fn backup_auth_once(path: &Path) -> Result<(), String> {
    let backup = auth_backup_path(path);
    if !backup.exists() {
        std::fs::copy(path, backup).map_err(|e| format!("backup Grok auth: {e}"))?;
    }
    Ok(())
}

#[cfg(windows)]
fn replace_auth_file(path: &Path, replacement: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let path_wide = wide(path);
    let replacement_wide = wide(replacement);
    if unsafe {
        ReplaceFileW(
            path_wide.as_ptr(),
            replacement_wide.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_auth_file(path: &Path, replacement: &Path) -> std::io::Result<()> {
    std::fs::rename(replacement, path)
}

fn write_auth_atomically(path: &Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;

    let parent = path.parent().ok_or("auth path has no parent")?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("auth.json");
    let tmp = parent.join(format!(
        ".{filename}.oauth-refresh-{}-{}",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("write refreshed Grok auth: {e}"));
    }

    let mut last_err = None;
    for attempt in 0..3 {
        match replace_auth_file(path, &tmp) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(30 * (attempt + 1)));
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(format!(
        "atomically replace Grok auth: {}",
        last_err.unwrap_or_else(|| std::io::Error::other("replace failed"))
    ))
}

async fn refresh_access_token(
    client: &reqwest::Client,
    home: &Path,
    current: &GrokCredentials,
) -> Result<GrokCredentials, String> {
    let refresh_token = current
        .refresh_token
        .as_deref()
        .ok_or("Grok auth.json has no refresh_token; run grok login")?;
    let endpoint = format!("{}/oauth2/token", current.issuer.trim_end_matches('/'));
    let response = client
        .post(endpoint)
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", current.client_id.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("Grok OAuth refresh API error: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("read Grok OAuth refresh response: {e}"))?;
    if !status.is_success() {
        return Err(format!("Grok OAuth refresh API {}", status.as_u16()));
    }
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("parse Grok OAuth refresh response: {e}"))?;
    let access_token = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .ok_or("Grok OAuth refresh response has no access_token")?;
    let next_refresh_token = value
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty());
    let expires_at = expires_at_from_response(&value)
        .ok_or("Grok OAuth refresh response has no usable expiry")?;
    persist_refreshed_auth(
        home,
        current,
        access_token,
        next_refresh_token,
        Some(&expires_at),
    )
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

    let mut credentials = match read_credentials(home) {
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

    let mut refreshed = false;
    if token_needs_refresh(credentials.expires_at.as_deref()) && credentials.refresh_token.is_some()
    {
        match refresh_access_token(&client, home, &credentials).await {
            Ok(next) => {
                credentials = next;
                refreshed = true;
            }
            Err(e) => log::warn!("[quota::grok] preflight token refresh failed: {e}"),
        }
    }

    let mut resp = client
        .get(GROK_BILLING_API)
        .bearer_auth(&credentials.token)
        .header("X-XAI-Token-Auth", "xai-grok-cli")
        .header("Accept", "application/json")
        .send()
        .await;
    if matches!(&resp, Ok(response) if response.status().as_u16() == 401)
        && !refreshed
        && credentials.refresh_token.is_some()
    {
        match refresh_access_token(&client, home, &credentials).await {
            Ok(next) => {
                credentials = next;
                resp = client
                    .get(GROK_BILLING_API)
                    .bearer_auth(&credentials.token)
                    .header("X-XAI-Token-Auth", "xai-grok-cli")
                    .header("Accept", "application/json")
                    .send()
                    .await;
            }
            Err(e) => {
                return RunnerQuota {
                    name,
                    label,
                    color,
                    ok: false,
                    text: format!("⚠ billing API 401，Grok token refresh failed: {e}"),
                    raw: Some(serde_json::json!({
                        "ok": false, "status_code": 401, "ts": Utc::now().to_rfc3339(),
                    })),
                };
            }
        }
    }
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
        .bearer_auth(&credentials.token)
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
        let credentials = read_credentials(&dir).expect("應找到 entry");
        assert_eq!(credentials.token, "xai-token");
        assert_eq!(credentials.refresh_token.as_deref(), Some("r"));
        assert_eq!(credentials.client_id, "client-uuid");
        assert!(credentials.expires_at.is_some());
    }

    #[test]
    fn read_credentials_missing_entry_errors() {
        let dir = std::env::temp_dir().join("lp_grok_test_noentry");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".grok")).expect("mkdir");
        std::fs::write(dir.join(".grok").join("auth.json"), r#"{"other": {}}"#).expect("write");
        assert!(read_credentials(&dir).is_err());
    }

    #[test]
    fn expired_token_is_selected_for_refresh() {
        assert!(token_needs_refresh(Some("2020-01-01T00:00:00Z")));
        assert!(!token_needs_refresh(Some("2099-01-01T00:00:00Z")));
        assert!(!token_needs_refresh(None));
    }

    #[test]
    fn refreshed_auth_updates_tokens_and_preserves_unknown_fields() {
        let mut root = serde_json::json!({
            "scope": {
                "key": "old-token",
                "refresh_token": "old-refresh",
                "expires_at": "old-expiry",
                "unknown_field": true
            }
        });
        apply_refreshed_auth(
            &mut root,
            "scope",
            "old-token",
            "new-token",
            Some("new-refresh"),
            Some("new-expiry"),
        )
        .expect("auth refresh should apply");
        assert_eq!(root["scope"]["key"], "new-token");
        assert_eq!(root["scope"]["refresh_token"], "new-refresh");
        assert_eq!(root["scope"]["expires_at"], "new-expiry");
        assert_eq!(root["scope"]["unknown_field"], true);
    }
}
