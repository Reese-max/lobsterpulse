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

/// 兩個端點都查、群組依名稱合併（正式端點優先）。2026-10-19 實測 daily 端點只回
/// 「Claude and GPT models」群組，正式端點才有「Gemini Models」——舊版 daily 成功就
/// 停，agy 卡片因此完全看不到 Gemini 額度。
const QUOTA_ENDPOINTS: [&str; 2] = [
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
];
/// 方案名（paidTier／currentTier）來源；失敗不擋額度顯示
const LOAD_CODE_ASSIST: &str = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
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

fn write_cached_token(home: &Path, token: &str, refresh_token: Option<&str>, expiry_epoch: u64) {
    let mut obj = serde_json::json!({
        "access_token": token,
        "expiry_epoch": expiry_epoch,
    });
    if let Some(rt) = refresh_token {
        obj["refresh_token"] = serde_json::Value::String(rt.to_string());
    }
    let path = cache_path(home);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, obj.to_string());
}

fn read_cached_refresh_token(home: &Path) -> Option<String> {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cache_path(home)).ok()?).ok()?;
    v.get("refresh_token").and_then(|t| t.as_str()).map(String::from)
}

/// 讀本機 Windows .gemini 目錄中的 token 檔
fn read_local_file_token(home: &Path) -> Option<serde_json::Value> {
    let p = home.join(".gemini").join("antigravity-cli").join("antigravity-oauth-token");
    let content = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&content).ok()
}

/// WSL2 Ubuntu 內部 token 檔 fallback（本機 agy 跑在 WSL 時可用）
fn read_wsl_token() -> Option<serde_json::Value> {
    let mut cmd = std::process::Command::new("wsl.exe");
    cmd.args(["-d", "Ubuntu", "-u", "root", "--", "cat", "/root/.gemini/antigravity-cli/antigravity-oauth-token"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    // 原本 output() 沒有逾時，WSL 卡住時整個 agy 查詢跟著卡死；冷啟動 WSL 要幾秒，給 15 秒
    let out = super::output_with_timeout(cmd, std::time::Duration::from_secs(15))?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// 窗長（秒）：優先 bucket 的 `window` 欄位（"5h"／"weekly"），退回 bucketId 後綴。
fn window_secs(b: &serde_json::Value, bucket_id: &str) -> Option<u64> {
    let w = b.get("window").and_then(|v| v.as_str()).unwrap_or(bucket_id);
    if w.ends_with("5h") {
        Some(5 * 3600)
    } else if w.ends_with("weekly") {
        Some(7 * 86400)
    } else {
        None
    }
}

/// 從單一 QuotaSummary group 的 buckets[] 抽指定 bucketId 的 (remaining_pct, reset_text, idle)。
/// 只認 exact bucketId；缺 remainingFraction → None，不腦補 0/100。
///
/// idle＝尚未開始計時：額度全滿且 resetTime ≈ 現在＋窗長。API 對沒用過的窗回
/// 「從現在起算」的時間，每次查都往後推，不是真正的重置點（2026-10-19 實測
/// 3p-5h／3p-weekly／gemini-5h 皆如此）→ 不給倒數，前端顯示未開始。
fn find_bucket(group: &serde_json::Value, bucket_id: &str) -> Option<(i64, Option<String>, bool)> {
    for b in group.get("buckets")?.as_array()? {
        if b.get("bucketId").and_then(|v| v.as_str()) != Some(bucket_id) {
            continue;
        }
        let frac = b.get("remainingFraction").and_then(|v| v.as_f64())?;
        let pct = (frac * 100.0).round().clamp(0.0, 100.0) as i64;
        let reset_epoch = b
            .get("resetTime")
            .and_then(|v| v.as_str())
            .and_then(iso_to_epoch);
        let idle = frac >= 1.0
            && match (reset_epoch, window_secs(b, bucket_id)) {
                (Some(r), Some(w)) => {
                    let left = r.saturating_sub(now_secs());
                    left + 300 >= w && left <= w + 300
                }
                _ => false,
            };
        let reset = if idle { None } else { reset_epoch.map(fmt_countdown) };
        return Some((pct, reset, idle));
    }
    None
}

/// 合併多個端點的 groups：依 displayName（缺則用 bucketId 組合）去重，先到先贏。
fn merge_groups(into: &mut Vec<serde_json::Value>, body: &serde_json::Value) {
    let key_of = |g: &serde_json::Value| {
        g.get("displayName")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                g.get("buckets")
                    .and_then(|v| v.as_array())
                    .map(|bs| {
                        bs.iter()
                            .filter_map(|b| b.get("bucketId").and_then(|v| v.as_str()))
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default()
            })
    };
    for g in body.get("groups").and_then(|v| v.as_array()).into_iter().flatten() {
        let k = key_of(g);
        if !into.iter().any(|e| key_of(e) == k) {
            into.push(g.clone());
        }
    }
}

/// loadCodeAssist 回應 → 方案顯示名：付費方案（如 Google AI Pro）優先，否則目前方案。
fn parse_tier_name(body: &serde_json::Value) -> Option<String> {
    ["paidTier", "currentTier"].iter().find_map(|k| {
        body.get(*k)
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(String::from)
    })
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
                "h5_idle": session.as_ref().map(|s| s.2),
                "wk_remaining": weekly.as_ref().map(|w| w.0),
                "wk_reset": weekly.as_ref().and_then(|w| w.1.clone()),
                "wk_idle": weekly.as_ref().map(|w| w.2),
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

    // token 來源：cache（未過期）→ 多來源 probe（Credential Manager / 本機檔 / WSL / 舊 cache）→ 過期則 refresh
    let mut token = read_cached_token(home);
    if token.is_none() {
        let mut creds: Option<(Option<String>, Option<String>, Option<String>)> = None;

        // 1. Windows Credential Manager
        if let Some(blob) = read_credential_blob("gemini:antigravity") {
            if let Some(keyring) = parse_keyring_blob(&blob) {
                creds = Some(extract_tokens(&keyring));
            }
        }
        // 2. 本機 Windows .gemini 目錄 token 檔
        if creds.is_none() {
            if let Some(v) = read_local_file_token(home) {
                creds = Some(extract_tokens(&v));
            }
        }
        // 3. WSL2 Ubuntu 內部 token 檔（同步子程序，丟到 blocking 執行緒，不佔 async worker）
        if creds.is_none() {
            if let Some(v) = tokio::task::spawn_blocking(read_wsl_token).await.ok().flatten() {
                creds = Some(extract_tokens(&v));
            }
        }
        // 4. 舊 cache 的 refresh_token
        if creds.is_none() {
            if let Some(rt) = read_cached_refresh_token(home) {
                creds = Some((None, Some(rt), None));
            }
        }

        let Some((access, refresh, expiry)) = creds else {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: "⚠ 未找到 Antigravity 憑證（agy 未登入）".to_string(),
                raw: None,
            };
        };

        let expired = expiry
            .as_deref()
            .and_then(iso_to_epoch)
            .map(|e| e <= now_secs() + 60)
            .unwrap_or(true); // 沒 expiry 資訊當過期處理（Google token 效期僅 1h）

        if !expired && access.is_some() {
            token = access;
        } else if let Some(rt) = refresh.as_ref() {
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
                            write_cached_token(home, t, Some(rt.as_str()), now_secs() + ttl);
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

    // Cloud Code QuotaSummary：兩個端點都查，groups 依名稱合併（見 QUOTA_ENDPOINTS）
    let mut groups: Vec<serde_json::Value> = Vec::new();
    let mut any_ok = false;
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
                // 200 但 JSON 壞掉不算成功：兩端點都這樣時要照舊回報失敗，不可顯示空卡
                if let Ok(b) = r.json::<serde_json::Value>().await {
                    merge_groups(&mut groups, &b);
                    any_ok = true;
                }
            }
            Ok(r) => last_code = r.status().as_u16(),
            Err(_) => continue,
        }
    }
    let body = serde_json::json!({ "groups": groups });
    if !any_ok {
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: format!("⚠ QuotaSummary 失敗（{last_code}）"),
            raw: Some(serde_json::json!({
                "ok": false, "basis": "provider_api",
                "status_code": last_code, "ts": chrono::Utc::now().to_rfc3339(),
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

    // 方案名：原本寫死 "Antigravity"（那是免費層的名字，不是實際方案）
    let tier = match client
        .post(LOAD_CODE_ASSIST)
        .bearer_auth(&token)
        .header("User-Agent", "antigravity")
        .json(&serde_json::json!({ "metadata": {
            "ideType": "ANTIGRAVITY", "platform": "PLATFORM_UNSPECIFIED", "pluginType": "GEMINI",
        }}))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| parse_tier_name(&v)),
        _ => None,
    };

    let fmt = |w: &Option<(i64, Option<String>, bool)>| {
        w.as_ref().map(|(p, _, _)| format!("{p}%")).unwrap_or_else(|| "--".to_string())
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
        "basis": "provider_api",
        "h5_remaining": session.as_ref().map(|s| s.0),
        "h5_reset": session.as_ref().and_then(|s| s.1.clone()),
        "h5_idle": session.as_ref().map(|s| s.2),
        "wk_remaining": weekly.as_ref().map(|w| w.0),
        "wk_reset": weekly.as_ref().and_then(|w| w.1.clone()),
        "wk_idle": weekly.as_ref().map(|w| w.2),
        "models": models,
        "plan": tier,
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

    fn iso_in(secs: u64) -> String {
        chrono::DateTime::<chrono::Utc>::from_timestamp((now_secs() + secs) as i64, 0)
            .unwrap()
            .to_rfc3339()
    }

    #[test]
    fn find_bucket_marks_unstarted_window_idle() {
        // 2026-10-19 實測形狀：沒用過的窗 remainingFraction=1、resetTime=現在+窗長
        let group = serde_json::json!({ "buckets": [
            { "bucketId": "3p-5h", "window": "5h", "remainingFraction": 1, "resetTime": iso_in(5 * 3600) },
            { "bucketId": "3p-weekly", "window": "weekly", "remainingFraction": 1, "resetTime": iso_in(7 * 86400) },
            // 用過一點的窗：真正的重置點，必須照常倒數
            { "bucketId": "gemini-weekly", "window": "weekly", "remainingFraction": 0.9567, "resetTime": iso_in(75 * 3600) },
            // 全滿但重置點明顯早於窗長（窗已開始、用量捨入成 100%）：不算 idle
            { "bucketId": "gemini-5h", "window": "5h", "remainingFraction": 1, "resetTime": iso_in(2 * 3600) }
        ]});
        let (p, r, idle) = find_bucket(&group, "3p-5h").unwrap();
        assert_eq!((p, r, idle), (100, None, true), "未開始的 5h 窗不給滾動倒數");
        assert!(find_bucket(&group, "3p-weekly").unwrap().2, "未開始的週窗同理");
        let (p, r, idle) = find_bucket(&group, "gemini-weekly").unwrap();
        assert_eq!(p, 96);
        assert!(!idle && r.is_some(), "已開始的窗照常顯示倒數");
        let (_, r, idle) = find_bucket(&group, "gemini-5h").unwrap();
        assert!(!idle && r.is_some());
    }

    #[test]
    fn merge_groups_unions_endpoints_by_name_first_wins() {
        // 正式端點：Gemini + 3p；daily 端點：只有 3p（數值不同，不得覆蓋正式端點）
        let prod = serde_json::json!({ "groups": [
            { "displayName": "Gemini Models", "buckets": [{ "bucketId": "gemini-weekly", "remainingFraction": 0.95 }] },
            { "displayName": "Claude and GPT models", "buckets": [{ "bucketId": "3p-weekly", "remainingFraction": 1 }] }
        ]});
        let daily = serde_json::json!({ "groups": [
            { "displayName": "Claude and GPT models", "buckets": [{ "bucketId": "3p-weekly", "remainingFraction": 0.5 }] }
        ]});
        let mut groups = Vec::new();
        merge_groups(&mut groups, &prod);
        merge_groups(&mut groups, &daily);
        merge_groups(&mut groups, &serde_json::Value::Null); // 壞回應不 panic
        assert_eq!(groups.len(), 2);
        let body = serde_json::json!({ "groups": groups });
        let models = collect_models(&body);
        assert_eq!(models[0]["name"], "Gemini Models", "Gemini 群組不可再被漏掉");
        assert_eq!(models[1]["wk_remaining"], 100, "同名群組先到先贏");
    }

    #[test]
    fn parse_tier_name_prefers_paid_tier() {
        let body = serde_json::json!({
            "currentTier": { "id": "free-tier", "name": "Antigravity" },
            "paidTier": { "id": "g1-pro-tier", "name": "Google AI Pro" }
        });
        assert_eq!(parse_tier_name(&body).as_deref(), Some("Google AI Pro"));
        let free = serde_json::json!({ "currentTier": { "id": "free-tier", "name": "Antigravity" } });
        assert_eq!(parse_tier_name(&free).as_deref(), Some("Antigravity"));
        assert!(parse_tier_name(&serde_json::json!({})).is_none());
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

    #[test]
    fn extract_tokens_from_agy_oauth_file() {
        let json_str = r#"{
            "token": {
                "access_token": "ya29.test",
                "token_type": "Bearer",
                "refresh_token": "1//test_refresh",
                "expiry": "2099-01-01T00:00:00Z"
            },
            "auth_method": "consumer"
        }"#;
        let v: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let (a, r, e) = extract_tokens(&v);
        assert_eq!(a.as_deref(), Some("ya29.test"));
        assert_eq!(r.as_deref(), Some("1//test_refresh"));
        assert_eq!(e.as_deref(), Some("2099-01-01T00:00:00Z"));
    }

    #[test]
    fn write_and_read_cached_token_preserves_refresh_token() {
        let dir = std::env::temp_dir().join(format!("lp_agy_test_{}", now_secs()));
        let _ = std::fs::create_dir_all(&dir);
        write_cached_token(&dir, "ya29.cache_test", Some("1//rf_test"), now_secs() + 3600);

        assert_eq!(read_cached_token(&dir).as_deref(), Some("ya29.cache_test"));
        assert_eq!(read_cached_refresh_token(&dir).as_deref(), Some("1//rf_test"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
