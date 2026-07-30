//! Antigravity（agy CLI）live quota fetch：token 在 Windows Credential Manager
//! （go-keyring 寫入，generic credential `gemini:antigravity`），打 Cloud Code
//! `retrieveUserQuotaSummary` 拿 gemini-5h / gemini-weekly buckets。
//! 方法出自 OpenUsage AntigravityUsageClient.swift（Cloud Code 遠端路徑，
//! app 沒開也能查；本機 language server 路徑刻意不做——省掉自簽憑證處理）。
//!
//! Token refresh 後只寫進自己的 cache 檔（~/.lobsterpulse/antigravity-token.json），
//! 不回寫 Credential Manager——那是 Antigravity 自己的狀態。

use super::RunnerQuota;
use std::path::Path;

const QUOTA_ENDPOINTS: [&str; 2] = [
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
];
/// Antigravity 安裝包內建的 installed-app OAuth client（OpenUsage 同值）
const AG_OAUTH_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const AG_OAUTH_CLIENT_SECRET: &str = "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";

/// 讀 Windows Credential Manager generic credential 的 blob
#[cfg(windows)]
fn read_credential_blob(target: &str) -> Option<Vec<u8>> {
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };
    let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let mut pcred: *mut CREDENTIALW = std::ptr::null_mut();
    let ok = unsafe { CredReadW(wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut pcred) };
    if ok == 0 || pcred.is_null() {
        return None;
    }
    let blob = unsafe {
        let cred = &*pcred;
        std::slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize).to_vec()
    };
    unsafe { CredFree(pcred as *mut core::ffi::c_void) };
    Some(blob)
}

#[cfg(not(windows))]
fn read_credential_blob(_target: &str) -> Option<Vec<u8>> {
    None // 非 Windows：Antigravity token 在 OS keychain，本專案只跑 Windows
}

/// blob → JSON：處理 go-keyring 的 `go-keyring-base64:` 前綴
fn parse_keyring_blob(blob: &[u8]) -> Option<serde_json::Value> {
    let s = String::from_utf8_lossy(blob);
    let s = s.trim_matches('\0').trim();
    let decoded;
    let json_str = if let Some(b64) = s.strip_prefix("go-keyring-base64:") {
        decoded = super::codex::base64_decode(b64).ok()?;
        String::from_utf8_lossy(&decoded).to_string()
    } else {
        s.to_string()
    };
    serde_json::from_str(&json_str).ok()
}

/// 從 keyring JSON 抽 (access_token, refresh_token, expiry_rfc3339)。
/// 形狀容錯：{"token":{...}} 或平鋪 {...}
fn extract_tokens(v: &serde_json::Value) -> (Option<String>, Option<String>, Option<String>) {
    let t = v.get("token").unwrap_or(v);
    let s = |key: &str| t.get(key).and_then(|x| x.as_str()).map(String::from);
    (s("access_token"), s("refresh_token"), s("expiry"))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn iso_to_epoch(s: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp().max(0) as u64)
}

fn fmt_countdown(epoch_secs: u64) -> String {
    let now = now_secs();
    if epoch_secs <= now {
        return "expired".to_string();
    }
    let delta = epoch_secs - now;
    format!("{}h{}m", delta / 3600, (delta % 3600) / 60)
}

/// refresh 後 token 的本地 cache（不回寫 Credential Manager）
fn cache_path(home: &Path) -> std::path::PathBuf {
    home.join(".lobsterpulse").join("antigravity-token.json")
}

fn read_cached_token(home: &Path) -> Option<String> {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cache_path(home)).ok()?).ok()?;
    let exp = v.get("expiry_epoch").and_then(|e| e.as_u64())?;
    if exp <= now_secs() + 60 {
        return None;
    }
    v.get("access_token").and_then(|t| t.as_str()).map(String::from)
}

fn write_cached_token(home: &Path, token: &str, expiry_epoch: u64) {
    let v = serde_json::json!({ "access_token": token, "expiry_epoch": expiry_epoch });
    let path = cache_path(home);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, v.to_string());
}

/// 從單一 QuotaSummary group 的 buckets[] 抽指定 bucketId 的 (remaining_pct, reset_text)。
/// 只認 exact bucketId；缺 remainingFraction → None，不腦補 0/100。
fn find_bucket(group: &serde_json::Value, bucket_id: &str) -> Option<(i64, Option<String>)> {
    for b in group.get("buckets")?.as_array()? {
        if b.get("bucketId").and_then(|v| v.as_str()) != Some(bucket_id) {
            continue;
        }
        let frac = b.get("remainingFraction").and_then(|v| v.as_f64())?;
        let pct = (frac * 100.0).round().clamp(0.0, 100.0) as i64;
        let reset = b
            .get("resetTime")
            .and_then(|v| v.as_str())
            .and_then(iso_to_epoch)
            .map(fmt_countdown);
        return Some((pct, reset));
    }
    None
}

/// 逐 group 收集模型額度：每個 group（如 Opus 4.7、Flash）帶自己的 displayName 與 buckets。
fn collect_models(body: &serde_json::Value) -> Vec<serde_json::Value> {
    body.get("groups")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|group| {
            let session = find_bucket(group, "gemini-5h").or_else(|| find_bucket(group, "3p-5h"));
            let weekly = find_bucket(group, "gemini-weekly").or_else(|| find_bucket(group, "3p-weekly"));
            serde_json::json!({
                "name": group.get("displayName").and_then(|v| v.as_str()).unwrap_or("Antigravity"),
                "h5_remaining": session.as_ref().map(|s| s.0),
                "h5_reset": session.as_ref().and_then(|s| s.1.clone()),
                "wk_remaining": weekly.as_ref().map(|w| w.0),
                "wk_reset": weekly.as_ref().and_then(|w| w.1.clone()),
            })
        })
        .collect()
}

pub async fn fetch(home: &Path) -> RunnerQuota {
    let label = "Antigravity".to_string();
    let color = "#f59e0b".to_string();
    let name = "agy".to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    // token 來源：cache（未過期）→ Credential Manager → 過期則 refresh
    let mut token = read_cached_token(home);
    if token.is_none() {
        let Some(blob) = read_credential_blob("gemini:antigravity") else {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: "⚠ Credential Manager 無 gemini:antigravity（agy 未登入）".to_string(),
                raw: None,
            };
        };
        let Some(keyring) = parse_keyring_blob(&blob) else {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: "⚠ keyring blob 解析失敗".to_string(),
                raw: None,
            };
        };
        let (access, refresh, expiry) = extract_tokens(&keyring);
        let expired = expiry
            .as_deref()
            .and_then(iso_to_epoch)
            .map(|e| e <= now_secs() + 60)
            .unwrap_or(true); // 沒 expiry 資訊當過期處理（Google token 效期僅 1h）
        if !expired {
            token = access;
        } else if let Some(rt) = refresh {
            // refresh 並 cache（Google 回 expires_in 秒）
            let resp = client
                .post("https://oauth2.googleapis.com/token")
                .form(&[
                    ("client_id", AG_OAUTH_CLIENT_ID),
                    ("client_secret", AG_OAUTH_CLIENT_SECRET),
                    ("refresh_token", rt.as_str()),
                    ("grant_type", "refresh_token"),
                ])
                .send()
                .await;
            if let Ok(r) = resp {
                if r.status().is_success() {
                    if let Ok(v) = r.json::<serde_json::Value>().await {
                        if let Some(t) = v.get("access_token").and_then(|t| t.as_str()) {
                            let ttl = v.get("expires_in").and_then(|e| e.as_u64()).unwrap_or(3600);
                            write_cached_token(home, t, now_secs() + ttl);
                            token = Some(t.to_string());
                        }
                    }
                }
            }
        }
    }
    let Some(token) = token else {
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: "⚠ token 過期且 refresh 失敗（開一次 Antigravity 可重登）".to_string(),
            raw: None,
        };
    };

    // Cloud Code QuotaSummary：daily endpoint 失敗換正式 endpoint
    let mut body = serde_json::Value::Null;
    let mut last_code = 0u16;
    for url in QUOTA_ENDPOINTS {
        let resp = client
            .post(url)
            .bearer_auth(&token)
            .header("User-Agent", "antigravity")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({}))
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => {
                body = r.json().await.unwrap_or(serde_json::Value::Null);
                break;
            }
            Ok(r) => last_code = r.status().as_u16(),
            Err(_) => continue,
        }
    }
    if body.is_null() {
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: format!("⚠ QuotaSummary 失敗（{last_code}）"),
            raw: Some(serde_json::json!({
                "ok": false, "status_code": last_code, "ts": chrono::Utc::now().to_rfc3339(),
            })),
        };
    }

    let find_first = |bucket_id: &str| {
        body.get("groups")
            .and_then(|v| v.as_array())
            .and_then(|groups| groups.iter().find_map(|group| find_bucket(group, bucket_id)))
    };
    let session = find_first("gemini-5h");
    let weekly = find_first("gemini-weekly");
    let claude_5h = find_first("3p-5h");
    let claude_weekly = find_first("3p-weekly");
    let models = collect_models(&body);

    let fmt = |w: &Option<(i64, Option<String>)>| {
        w.as_ref().map(|(p, _)| format!("{p}%")).unwrap_or_else(|| "--".to_string())
    };
    let text = format!(
        "⏱ 5h {} · 7d {}\n3p 5h {} · 7d {}",
        fmt(&session),
        fmt(&weekly),
        fmt(&claude_5h),
        fmt(&claude_weekly)
    );

    let raw = serde_json::json!({
        "ok": true,
        "h5_remaining": session.as_ref().map(|s| s.0),
        "h5_reset": session.as_ref().and_then(|s| s.1.clone()),
        "wk_remaining": weekly.as_ref().map(|w| w.0),
        "wk_reset": weekly.as_ref().and_then(|w| w.1.clone()),
        "models": models,
        "plan": "Antigravity",
        "claude_5h_remaining": claude_5h.as_ref().map(|c| c.0),
        "claude_weekly_remaining": claude_weekly.as_ref().map(|c| c.0),
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
    fn parse_keyring_blob_plain_json() {
        let v = parse_keyring_blob(br#"{"token":{"access_token":"ya29.x","refresh_token":"1//r","expiry":"2099-01-01T00:00:00Z"}}"#)
            .expect("plain json");
        let (a, r, e) = extract_tokens(&v);
        assert_eq!(a.as_deref(), Some("ya29.x"));
        assert_eq!(r.as_deref(), Some("1//r"));
        assert!(e.is_some());
    }

    #[test]
    fn parse_keyring_blob_base64_prefixed() {
        // {"access_token":"t"} 的 base64
        let b64 = "eyJhY2Nlc3NfdG9rZW4iOiJ0In0=";
        let blob = format!("go-keyring-base64:{b64}");
        let v = parse_keyring_blob(blob.as_bytes()).expect("b64 json");
        let (a, _, _) = extract_tokens(&v);
        assert_eq!(a.as_deref(), Some("t"));
    }

    #[test]
    fn find_bucket_exact_id_only() {
        let group = serde_json::json!({ "buckets": [
            { "bucketId": "gemini-5h", "remainingFraction": 0.87, "resetTime": "2099-01-01T00:00:00Z" },
            { "bucketId": "gemini-weekly", "remainingFraction": 0.5 },
            { "bucketId": "3p-5h" }
        ]});
        assert_eq!(find_bucket(&group, "gemini-5h").map(|b| b.0), Some(87));
        assert_eq!(find_bucket(&group, "gemini-weekly").map(|b| b.0), Some(50));
        assert!(find_bucket(&group, "3p-5h").is_none(), "缺 remainingFraction 不腦補");
        assert!(find_bucket(&group, "nonexistent").is_none());
    }

    #[test]
    fn collect_models_keeps_duplicate_bucket_ids_in_each_group() {
        let body = serde_json::json!({ "groups": [
            { "displayName": "Opus 4.7", "buckets": [
                { "bucketId": "3p-5h", "remainingFraction": 0.8 },
                { "bucketId": "3p-weekly", "remainingFraction": 0.6 }
            ]},
            { "displayName": "Flash", "buckets": [
                { "bucketId": "3p-5h", "remainingFraction": 0.3 },
                { "bucketId": "3p-weekly", "remainingFraction": 0.1 }
            ]}
        ]});

        let models = collect_models(&body);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["name"], "Opus 4.7");
        assert_eq!(models[0]["h5_remaining"], 80);
        assert_eq!(models[1]["name"], "Flash");
        assert_eq!(models[1]["wk_remaining"], 10);
    }
}
