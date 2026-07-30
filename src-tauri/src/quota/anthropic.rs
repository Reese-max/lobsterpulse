//! Anthropic (Claude CLI) live quota fetch：讀 `~/.claude/.credentials.json`
//! 拿 access_token、解析 session token 到期日、抓 stats cache 顯示本週/今日用量。
//! R82 開工，R85 落地，R89 經 Tauri command 接入 (`quota::anthropic::fetch`)。

use super::RunnerQuota;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const ANTHROPIC_API: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_OAUTH_TOKEN_API: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_CODE_OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_OAUTH_REFRESH_LEEWAY_MS: u64 = 10 * 60 * 1000;
static CLAUDE_OAUTH_REFRESH_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct ClaudeCredentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOAuth>,
}

#[derive(Debug, Deserialize)]
struct ClaudeOAuth {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "refreshToken")]
    refresh_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at_ms: Option<u64>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

#[derive(Debug)]
struct ClaudeAuthCredentials {
    access_token: String,
    refresh_token: Option<String>,
    expires_at_ms: Option<u64>,
    tier: String,
}

#[derive(Debug, Deserialize)]
struct OAuthRefresh {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
    #[serde(default)]
    refresh_token_expires_in: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct StatsCache {
    #[serde(rename = "totalSessions")]
    total_sessions: Option<u64>,
    #[serde(rename = "totalMessages")]
    total_messages: Option<u64>,
    #[serde(rename = "dailyActivity")]
    daily_activity: Option<Vec<DailyActivity>>,
    #[serde(rename = "dailyModelTokens")]
    daily_model_tokens: Option<Vec<DailyModelTokens>>,
    #[serde(rename = "modelUsage")]
    model_usage: Option<std::collections::HashMap<String, ModelUsageEntry>>,
    #[serde(rename = "lastComputedDate")]
    last_computed_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DailyModelTokens {
    date: String,
    #[serde(rename = "tokensByModel")]
    tokens_by_model: Option<std::collections::HashMap<String, u64>>,
}

#[derive(Debug, Deserialize)]
struct ModelUsageEntry {
    #[serde(rename = "costUSD")]
    cost_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct DailyActivity {
    date: String,
    #[serde(rename = "messageCount")]
    message_count: Option<u64>,
    #[serde(rename = "sessionCount")]
    session_count: Option<u64>,
}

fn fmt_countdown(epoch_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if epoch_secs <= now {
        return "resetting...".to_string();
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

fn fmt_tokens(n: u64) -> String {
    // 用 round() 強制 half-up (away from zero) — Rust 預設 `{:.1f}` 是
    // round-half-to-even (banker's), 7.25B 會給 7.2B 不是 7.3B。
    let v = n as f64;
    if v >= 1e9 {
        format!("{:.1}B", (v / 1e9 * 10.0).round() / 10.0)
    } else if v >= 1e6 {
        format!("{:.1}M", (v / 1e6 * 10.0).round() / 10.0)
    } else if v >= 1e3 {
        format!("{:.1}K", (v / 1e3 * 10.0).round() / 10.0)
    } else {
        format!("{n}")
    }
}

fn claude_oauth_authorization_value(token: &str) -> Result<HeaderValue, String> {
    HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|e| format!("build authorization header: {e}"))
}

fn oauth_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn oauth_refresh_due(expires_at_ms: Option<u64>, now_ms: u64) -> bool {
    expires_at_ms
        .is_some_and(|expires| expires <= now_ms.saturating_add(CLAUDE_OAUTH_REFRESH_LEEWAY_MS))
}

fn credentials_path(home: &Path) -> PathBuf {
    home.join(".claude").join(".credentials.json")
}

fn parse_credentials(data: &str) -> Result<ClaudeAuthCredentials, String> {
    let creds: ClaudeCredentials =
        serde_json::from_str(&data).map_err(|e| format!("parse credentials: {e}"))?;
    let oauth = creds.claude_ai_oauth.ok_or("missing claudeAiOauth")?;
    Ok(ClaudeAuthCredentials {
        access_token: oauth.access_token.ok_or("missing accessToken")?,
        refresh_token: oauth.refresh_token,
        expires_at_ms: oauth.expires_at_ms,
        tier: oauth.subscription_type.unwrap_or_else(|| "pro".to_string()),
    })
}

/// 讀 ~/.claude/.credentials.json 取 OAuth token。
fn read_credentials(home: &Path) -> Result<ClaudeAuthCredentials, String> {
    let path = credentials_path(home);
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read credentials: {e}"))?;
    parse_credentials(&data)
}

fn oauth_expiry_ms(now_ms: u64, expires_in_secs: u64) -> Result<u64, String> {
    now_ms
        .checked_add(
            expires_in_secs
                .checked_mul(1000)
                .ok_or("OAuth expiry overflow")?,
        )
        .ok_or("OAuth expiry overflow".to_string())
}

fn apply_oauth_refresh(
    credentials: &mut serde_json::Value,
    refreshed: &OAuthRefresh,
    now_ms: u64,
) -> Result<(), String> {
    let oauth = credentials
        .get_mut("claudeAiOauth")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("missing claudeAiOauth")?;
    oauth.insert(
        "accessToken".into(),
        serde_json::Value::String(refreshed.access_token.clone()),
    );
    if let Some(refresh_token) = &refreshed.refresh_token {
        oauth.insert(
            "refreshToken".into(),
            serde_json::Value::String(refresh_token.clone()),
        );
    }
    oauth.insert(
        "expiresAt".into(),
        serde_json::Value::from(oauth_expiry_ms(now_ms, refreshed.expires_in)?),
    );
    if let Some(expires_in) = refreshed.refresh_token_expires_in {
        oauth.insert(
            "refreshTokenExpiresAt".into(),
            serde_json::Value::from(oauth_expiry_ms(now_ms, expires_in)?),
        );
    }
    Ok(())
}

fn credentials_backup_path(path: &Path) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("credentials.json");
    let day = chrono::Local::now().format("%Y%m%d");
    path.with_file_name(format!("{filename}.bak-{day}"))
}

fn backup_credentials_once(path: &Path) -> Result<(), String> {
    let backup = credentials_backup_path(path);
    if !backup.exists() {
        std::fs::copy(path, &backup).map_err(|e| format!("backup credentials: {e}"))?;
    }
    Ok(())
}

#[cfg(windows)]
fn replace_credentials_file(path: &Path, replacement: &Path) -> std::io::Result<()> {
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
fn replace_credentials_file(path: &Path, replacement: &Path) -> std::io::Result<()> {
    std::fs::rename(replacement, path)
}

fn write_credentials_atomically(path: &Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;

    let parent = path.parent().ok_or("credentials path has no parent")?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("credentials.json");
    let tmp = parent.join(format!(
        ".{filename}.oauth-refresh-{}-{}",
        std::process::id(),
        oauth_now_ms()
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
        return Err(format!("write refreshed credentials: {e}"));
    }

    let mut last_err = None;
    for attempt in 0..3 {
        match replace_credentials_file(path, &tmp) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(30 * (attempt + 1)));
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(format!(
        "atomically replace credentials: {}",
        last_err.unwrap_or_else(|| std::io::Error::other("replace failed"))
    ))
}

fn persist_oauth_refresh(
    path: &Path,
    requested_refresh_token: &str,
    refreshed: &OAuthRefresh,
    now_ms: u64,
) -> Result<ClaudeAuthCredentials, String> {
    let data = std::fs::read_to_string(path).map_err(|e| format!("read credentials: {e}"))?;
    let current = parse_credentials(&data)?;
    if current.refresh_token.as_deref() != Some(requested_refresh_token) {
        return Err(
            "credentials changed while renewing; leaving Claude Code auth untouched".into(),
        );
    }
    let mut value: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("parse credentials: {e}"))?;
    apply_oauth_refresh(&mut value, refreshed, now_ms)?;
    let updated =
        serde_json::to_vec_pretty(&value).map_err(|e| format!("serialize credentials: {e}"))?;
    backup_credentials_once(path)?;
    write_credentials_atomically(path, &updated)?;
    parse_credentials(
        std::str::from_utf8(&updated).map_err(|e| format!("read refreshed credentials: {e}"))?,
    )
}

async fn refresh_access_token(
    client: &reqwest::Client,
    refresh_token: &str,
) -> Result<OAuthRefresh, String> {
    let response = client
        .post(ANTHROPIC_OAUTH_TOKEN_API)
        .header("anthropic-beta", "oauth-2025-04-20")
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLAUDE_CODE_OAUTH_CLIENT_ID,
        }))
        .send()
        .await
        .map_err(|e| format!("OAuth refresh API error: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("OAuth refresh HTTP {}", response.status()));
    }
    let refreshed: OAuthRefresh = response
        .json()
        .await
        .map_err(|e| format!("parse OAuth refresh response: {e}"))?;
    if refreshed.access_token.is_empty() {
        return Err("OAuth refresh response missing access token".into());
    }
    Ok(refreshed)
}

async fn refresh_credentials_if_due(
    home: &Path,
    client: &reqwest::Client,
    credentials: ClaudeAuthCredentials,
) -> Result<ClaudeAuthCredentials, String> {
    if !oauth_refresh_due(credentials.expires_at_ms, oauth_now_ms()) {
        return Ok(credentials);
    }

    let _guard = CLAUDE_OAUTH_REFRESH_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let latest = read_credentials(home)?;
    if !oauth_refresh_due(latest.expires_at_ms, oauth_now_ms()) {
        return Ok(latest);
    }
    let refresh_token = latest
        .refresh_token
        .as_deref()
        .ok_or("Claude OAuth access token expires soon and has no refresh token")?;
    let refreshed = refresh_access_token(client, refresh_token).await?;
    persist_oauth_refresh(
        &credentials_path(home),
        refresh_token,
        &refreshed,
        oauth_now_ms(),
    )
}

/// 背景 local runner 的 Claude 腳本只會讀 access token；在 spawn 前由這裡
/// 共用同一套安全續約與原子寫入流程，避免 UI live fetch 成為續約的隱性前提。
pub(crate) async fn ensure_fresh_credentials(home: &Path) -> Result<(), String> {
    let credentials = read_credentials(home)?;
    if !oauth_refresh_due(credentials.expires_at_ms, oauth_now_ms()) {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("build OAuth client: {e}"))?;
    refresh_credentials_if_due(home, &client, credentials)
        .await
        .map(|_| ())
}

/// 讀 ~/.claude/stats-cache.json 取本地統計
fn read_stats(home: &Path) -> StatsCache {
    let path = home.join(".claude").join("stats-cache.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn sum_tokens_for(dmt: &[DailyModelTokens], pred: impl Fn(&str) -> bool) -> u64 {
    dmt.iter()
        .filter(|d| pred(&d.date))
        .filter_map(|d| d.tokens_by_model.as_ref())
        .flat_map(|m| m.values())
        .sum()
}

/// 聚合 stats-cache 每日 token：today/yesterday/近 30 天 + 累計 cost。
/// 日期是 YYYY-MM-DD 字串，字典序 == 時間序，直接比較。
fn aggregate_daily_stats(
    stats: &StatsCache,
    today: &str,
    yesterday: &str,
    cutoff_30d: &str,
) -> serde_json::Value {
    let dmt = stats.daily_model_tokens.as_deref().unwrap_or(&[]);
    let total_cost_usd = stats
        .model_usage
        .as_ref()
        .map(|m| m.values().filter_map(|e| e.cost_usd).sum::<f64>());
    serde_json::json!({
        // 這兩個值來自已凍結的 stats-cache（今日恆 0）——消費端
        // get_claude_daily_stats（lib.rs）靠 LIVE_DAILY 覆寫兜底才正確。
        // 拆掉那層覆寫前，這裡必須先改吃 LIVE_DAILY（同 fetch() 的做法）。
        "today_tokens": sum_tokens_for(dmt, |d| d == today),
        "yesterday_tokens": sum_tokens_for(dmt, |d| d == yesterday),
        "tokens_30d": sum_tokens_for(dmt, |d| d >= cutoff_30d),
        "total_cost_usd": total_cost_usd,
        "computed_date": stats.last_computed_date,
    })
}

/// Usage 面板資料源：讀 stats-cache.json 聚合每日 token 統計。
/// 檔案缺 / 壞 JSON → None（前端顯示 No data，不誤報 0）。
pub fn daily_stats(home: &Path) -> Option<serde_json::Value> {
    let path = home.join(".claude").join("stats-cache.json");
    let s = std::fs::read_to_string(&path).ok()?;
    let stats: StatsCache = serde_json::from_str(&s).ok()?;
    let now = chrono::Local::now();
    let fmt = |d: chrono::DateTime<chrono::Local>| d.format("%Y-%m-%d").to_string();
    Some(aggregate_daily_stats(
        &stats,
        &fmt(now),
        &fmt(now - chrono::Duration::days(1)),
        &fmt(now - chrono::Duration::days(30)),
    ))
}

/// 呼叫 Anthropic API 取 rate limit headers
pub async fn fetch(home: &Path) -> RunnerQuota {
    let label = "🤖 Claude Code".to_string();
    let color = "#d97757".to_string();
    let name = "claude".to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let credentials = match read_credentials(home) {
        Ok(credentials) => match refresh_credentials_if_due(home, &client, credentials).await {
            Ok(credentials) => credentials,
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
        },
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
    let token = credentials.access_token;
    let tier = credentials.tier;

    let stats = read_stats(home);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // dailyModelTokens 在 stats-cache 裡是另一個結構，先用 dailyActivity 簡化
    let today_activity = stats
        .daily_activity
        .as_ref()
        .and_then(|v| v.iter().find(|d| d.date == today));

    // 打 API 取 rate limit headers
    let body = serde_json::json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "1"}]
    });

    let resp = client
        .post(ANTHROPIC_API)
        .header(
            AUTHORIZATION,
            match claude_oauth_authorization_value(&token) {
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
            },
        )
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await;

    match resp {
        Ok(r) => {
            let headers = r.headers();
            let h5u = headers
                .get("anthropic-ratelimit-unified-5h-utilization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<f64>().ok());
            let h5r = headers
                .get("anthropic-ratelimit-unified-5h-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let d7u = headers
                .get("anthropic-ratelimit-unified-7d-utilization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<f64>().ok());
            let d7r = headers
                .get("anthropic-ratelimit-unified-7d-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            let h5_remaining = h5u.map(|v| ((1.0 - v) * 100.0).round() as i64);
            let h5_used = h5u.map(|v| (v * 100.0).round() as i64);
            let d7_remaining = d7u.map(|v| ((1.0 - v) * 100.0).round() as i64);
            let d7_used = d7u.map(|v| (v * 100.0).round() as i64);

            let h5_str = h5_remaining
                .map(|v| format!("{v}%"))
                .unwrap_or_else(|| "--".to_string());
            let d7_str = d7_remaining
                .map(|v| format!("{v}%"))
                .unwrap_or_else(|| "--".to_string());
            let h5_reset_str = h5r.map(fmt_countdown).unwrap_or_else(|| "N/A".to_string());
            let d7_reset_str = d7r.map(fmt_countdown).unwrap_or_else(|| "N/A".to_string());

            // stats-cache 已停更數月（實測凍結在 2026-05-24），dailyModelTokens
            // 對今天永遠是 0——優先吃 LIVE_DAILY（即時 JSONL 掃描，與膠囊同源），
            // 日期吻合才用；掃描未完成（剛啟動）或跨日空窗則退回 stats-cache 值。
            let live_today = crate::LIVE_DAILY
                .lock()
                .unwrap()
                .as_ref()
                .filter(|d| d.date == today)
                .map(|d| d.today);
            let today_tokens = fmt_tokens(live_today.unwrap_or_else(|| {
                stats
                    .daily_model_tokens
                    .as_deref()
                    .map(|dmt| sum_tokens_for(dmt, |d| d == today))
                    .unwrap_or(0)
            }));
            let today_msgs = today_activity.and_then(|a| a.message_count).unwrap_or(0);
            let total_sessions = stats.total_sessions.unwrap_or(0);
            let total_messages = stats.total_messages.unwrap_or(0);

            let text =
                format!("⏱ 5h {h5_str} · 7d {d7_str}\n🔥 {today_tokens} · msgs {today_msgs}");

            let raw = serde_json::json!({
                "ok": true,
                "basis": "provider_api",
                "rate_headers_found": h5u.is_some(),
                // 視窗名寫實際長度：不給的話面板會顯示泛稱的 Session/Weekly，
                // 跟膠囊的 5h/7d 對不起來（面板走 live API 這條，跟膠囊不同源）
                "session_label": "5h",
                "weekly_label": "7d",
                "session_5h_remaining": h5_remaining,
                "session_5h_used": h5_used,
                "session_5h_reset": h5_reset_str,
                "week_7d_remaining": d7_remaining,
                "week_7d_used": d7_used,
                "week_7d_reset": d7_reset_str,
                "today_messages": today_msgs,
                "today_sessions": today_activity.and_then(|a| a.session_count).unwrap_or(0),
                "today_tokens": today_tokens,
                "total_sessions": total_sessions.to_string(),
                "total_messages": total_messages.to_string(),
                "tier": if tier == "max" { "Claude Max" } else { "Claude Pro" },
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
        Err(e) => RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: format!("⚠ API error: {e}"),
            raw: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_tokens_below_thousand_keeps_raw() {
        assert_eq!(fmt_tokens(0), "0");
        assert_eq!(fmt_tokens(1), "1");
        assert_eq!(fmt_tokens(999), "999");
    }

    #[test]
    fn fmt_tokens_thousands_uses_k() {
        assert_eq!(fmt_tokens(1_000), "1.0K");
        assert_eq!(fmt_tokens(1_234), "1.2K");
        assert_eq!(fmt_tokens(999_499), "999.5K");
    }

    #[test]
    fn fmt_tokens_millions_uses_m() {
        assert_eq!(fmt_tokens(1_000_000), "1.0M");
        assert_eq!(fmt_tokens(2_500_000), "2.5M");
    }

    #[test]
    fn fmt_tokens_billions_uses_b() {
        assert_eq!(fmt_tokens(1_000_000_000), "1.0B");
        assert_eq!(fmt_tokens(7_250_000_000), "7.3B");
    }

    #[test]
    fn claude_oauth_authorization_value_uses_bearer_not_api_key() {
        let value = claude_oauth_authorization_value("tok_123").unwrap();
        assert_eq!(value.to_str().unwrap(), "Bearer tok_123");
    }

    #[test]
    fn fmt_countdown_past_epoch_returns_resetting() {
        assert_eq!(fmt_countdown(0), "resetting...");
    }

    #[test]
    fn fmt_countdown_under_one_hour_renders_minutes() {
        // ~30 分鐘後
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let future = now + 30 * 60;
        let s = fmt_countdown(future);
        assert!(s.ends_with('m'), "expected trailing 'm', got {s:?}");
        assert!(s.contains("30m"), "expected '30m' in {s:?}");
    }

    #[test]
    fn fmt_countdown_over_one_hour_renders_hm() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let future = now + 2 * 3600 + 15 * 60; // 2h15m
        let s = fmt_countdown(future);
        assert!(s.starts_with("2h"), "expected '2h' prefix, got {s:?}");
        assert!(s.contains("15m"), "expected '15m' in {s:?}");
    }

    #[test]
    fn aggregate_daily_stats_sums_today_yesterday_30d() {
        let stats: StatsCache = serde_json::from_str(
            r#"{
            "lastComputedDate": "2026-07-16",
            "dailyModelTokens": [
                {"date": "2026-05-01", "tokensByModel": {"a": 100}},
                {"date": "2026-07-15", "tokensByModel": {"a": 10, "b": 5}},
                {"date": "2026-07-16", "tokensByModel": {"a": 7}}
            ],
            "modelUsage": {
                "a": {"costUSD": 1.5},
                "b": {"costUSD": 0.5}
            }
        }"#,
        )
        .unwrap();
        let v = aggregate_daily_stats(&stats, "2026-07-16", "2026-07-15", "2026-06-16");
        assert_eq!(v["today_tokens"], 7);
        assert_eq!(v["yesterday_tokens"], 15);
        assert_eq!(v["tokens_30d"], 22); // 05-01 在 cutoff 之前，不計
        assert_eq!(v["total_cost_usd"], 2.0);
        assert_eq!(v["computed_date"], "2026-07-16");
    }

    #[test]
    fn aggregate_daily_stats_empty_cache_yields_zeros_and_nulls() {
        let stats = StatsCache::default();
        let v = aggregate_daily_stats(&stats, "2026-07-16", "2026-07-15", "2026-06-16");
        assert_eq!(v["today_tokens"], 0);
        assert_eq!(v["tokens_30d"], 0);
        assert!(v["total_cost_usd"].is_null());
        assert!(v["computed_date"].is_null());
    }

    #[test]
    fn oauth_refresh_is_due_ten_minutes_before_expiry() {
        let now_ms = 1_000_000;
        assert!(!oauth_refresh_due(
            Some(now_ms + 10 * 60 * 1000 + 1),
            now_ms
        ));
        assert!(oauth_refresh_due(Some(now_ms + 10 * 60 * 1000), now_ms));
        assert!(oauth_refresh_due(Some(now_ms - 1), now_ms));
        assert!(!oauth_refresh_due(None, now_ms));
    }

    #[test]
    fn oauth_refresh_contract_matches_claude_code() {
        assert_eq!(
            ANTHROPIC_OAUTH_TOKEN_API,
            "https://platform.claude.com/v1/oauth/token"
        );
        assert_eq!(
            CLAUDE_CODE_OAUTH_CLIENT_ID,
            "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
        );
    }

    #[test]
    fn apply_oauth_refresh_rotates_tokens_without_dropping_credentials_fields() {
        let mut credentials = serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "old-access",
                "refreshToken": "old-refresh",
                "expiresAt": 1,
                "refreshTokenExpiresAt": 2,
                "subscriptionType": "max",
                "scopes": ["user:inference"]
            },
            "otherProvider": {"enabled": true}
        });
        let refreshed = OAuthRefresh {
            access_token: "new-access".into(),
            refresh_token: Some("new-refresh".into()),
            expires_in: 3600,
            refresh_token_expires_in: Some(7200),
        };

        apply_oauth_refresh(&mut credentials, &refreshed, 1_000).expect("refresh applies");

        assert_eq!(credentials["claudeAiOauth"]["accessToken"], "new-access");
        assert_eq!(credentials["claudeAiOauth"]["refreshToken"], "new-refresh");
        assert_eq!(credentials["claudeAiOauth"]["expiresAt"], 3_601_000);
        assert_eq!(
            credentials["claudeAiOauth"]["refreshTokenExpiresAt"],
            7_201_000
        );
        assert_eq!(credentials["claudeAiOauth"]["subscriptionType"], "max");
        assert_eq!(credentials["otherProvider"]["enabled"], true);
    }

    #[test]
    fn persist_oauth_refresh_replaces_credentials_and_keeps_daily_backup() {
        let dir = std::env::temp_dir().join(format!(
            "lobsterpulse-anthropic-refresh-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join(".credentials.json");
        std::fs::write(
            &path,
            r#"{"claudeAiOauth":{"accessToken":"old-access","refreshToken":"old-refresh","expiresAt":1,"subscriptionType":"max","scopes":["user:inference"]},"otherProvider":{"enabled":true}}"#,
        )
        .expect("write credentials");
        let refreshed = OAuthRefresh {
            access_token: "new-access".into(),
            refresh_token: Some("new-refresh".into()),
            expires_in: 3600,
            refresh_token_expires_in: None,
        };

        let updated =
            persist_oauth_refresh(&path, "old-refresh", &refreshed, 1_000).expect("persist");
        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read updated"))
                .expect("parse updated");
        let backup: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(credentials_backup_path(&path)).expect("read backup"),
        )
        .expect("parse backup");

        assert_eq!(updated.access_token, "new-access");
        assert_eq!(on_disk["claudeAiOauth"]["refreshToken"], "new-refresh");
        assert_eq!(on_disk["otherProvider"]["enabled"], true);
        assert_eq!(backup["claudeAiOauth"]["accessToken"], "old-access");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
