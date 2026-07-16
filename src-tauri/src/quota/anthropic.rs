//! Anthropic (Claude CLI) live quota fetch：讀 `~/.claude/.credentials.json`
//! 拿 access_token、解析 session token 到期日、抓 stats cache 顯示本週/今日用量。
//! R82 開工，R85 落地，R89 經 Tauri command 接入 (`quota::anthropic::fetch`)。

use super::RunnerQuota;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use serde::Deserialize;
use std::path::Path;

const ANTHROPIC_API: &str = "https://api.anthropic.com/v1/messages";

#[derive(Debug, Deserialize)]
struct ClaudeCredentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOAuth>,
}

#[derive(Debug, Deserialize)]
struct ClaudeOAuth {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
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

/// 讀 ~/.claude/.credentials.json 取 OAuth token
fn read_credentials(home: &Path) -> Result<(String, String), String> {
    let path = home.join(".claude").join(".credentials.json");
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read credentials: {e}"))?;
    let creds: ClaudeCredentials =
        serde_json::from_str(&data).map_err(|e| format!("parse credentials: {e}"))?;
    let oauth = creds.claude_ai_oauth.ok_or("missing claudeAiOauth")?;
    let token = oauth.access_token.ok_or("missing accessToken")?;
    let tier = oauth.subscription_type.unwrap_or_else(|| "pro".to_string());
    Ok((token, tier))
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

    let (token, tier) = match read_credentials(home) {
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

    let stats = read_stats(home);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // dailyModelTokens 在 stats-cache 裡是另一個結構，先用 dailyActivity 簡化
    let today_activity = stats
        .daily_activity
        .as_ref()
        .and_then(|v| v.iter().find(|d| d.date == today));

    // 打 API 取 rate limit headers
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

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

            // R124 前這裡硬編 fmt_tokens(0)；改讀 dailyModelTokens 實值
            let today_tokens = fmt_tokens(
                stats
                    .daily_model_tokens
                    .as_deref()
                    .map(|dmt| sum_tokens_for(dmt, |d| d == today))
                    .unwrap_or(0),
            );
            let today_msgs = today_activity.and_then(|a| a.message_count).unwrap_or(0);
            let total_sessions = stats.total_sessions.unwrap_or(0);
            let total_messages = stats.total_messages.unwrap_or(0);

            let text =
                format!("⏱ 5h {h5_str} · 7d {d7_str}\n🔥 {today_tokens} · msgs {today_msgs}");

            let raw = serde_json::json!({
                "ok": true,
                "rate_headers_found": h5u.is_some(),
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
}
