//! OpenAI (Codex CLI) live quota fetch：讀 `~/.codex/auth.json` 拿 access_token、
//! 解析 JWT exp 拿 token 到期日、打 `/v1/models` 探 token 有效性、從 `config.toml`
//! 拿 model 名、顯示 ChatGPT 訂閱類型。R86 落地，R89 經 Tauri command 接入
//! (`quota::codex::fetch`)。

use super::RunnerQuota;
use serde::Deserialize;
use std::path::Path;

const OPENAI_MODELS_API: &str = "https://api.openai.com/v1/models";
/// ChatGPT 訂閱額度（Codex CLI /status 同源；OpenUsage/CodexBar 皆用此 endpoint）
const CHATGPT_USAGE_API: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Deserialize)]
struct CodexAuth {
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
    tokens: Option<CodexTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexTokens {
    #[serde(rename = "id_token")]
    id_token: Option<String>,
    #[serde(rename = "access_token")]
    access_token: Option<String>,
    #[serde(rename = "account_id")]
    account_id: Option<String>,
}

/// 解析 JWT payload 拿 `exp` claim（不驗簽，只讀 base64 payload）
fn jwt_exp(token: &str) -> Option<u64> {
    let json = decode_jwt_payload(token)?;
    json.get("exp").and_then(|v| v.as_u64())
}

/// 解析 JWT payload 為 serde_json::Value（不驗簽，只讀 base64 payload）
fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload_b64 = token.split('.').nth(1)?;
    let mut b64 = payload_b64.replace('-', "+").replace('_', "/");
    match b64.len() % 4 {
        2 => b64.push_str("=="),
        3 => b64.push('='),
        _ => {}
    }
    let bytes = base64_decode(&b64).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 最小 base64 解碼（不引 base64 crate，避無謂 dep）
/// -1 = 非法 char；0..=63 = base64 值
fn base64_lookup(c: u8) -> i8 {
    match c {
        b'A'..=b'Z' => (c - b'A') as i8,
        b'a'..=b'z' => (c - b'a' + 26) as i8,
        b'0'..=b'9' => (c - b'0' + 52) as i8,
        b'+' => 62,
        b'/' => 63,
        _ => -1,
    }
}

pub(crate) fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let v = base64_lookup(c);
        if v < 0 {
            return Err(format!("invalid base64 char {c}"));
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// 從 `epoch_secs` 算「XhYm」剩餘時間，過期回 "expired"
fn fmt_countdown(epoch_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if epoch_secs <= now {
        return "expired".to_string();
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

/// 讀 `~/.codex/auth.json`，回 (access_token, account_id, plan_type)
fn read_credentials(home: &Path) -> Result<(String, String, String), String> {
    let path = home.join(".codex").join("auth.json");
    let data = std::fs::read_to_string(&path).map_err(|e| format!("read auth.json: {e}"))?;
    let auth: CodexAuth =
        serde_json::from_str(&data).map_err(|e| format!("parse auth.json: {e}"))?;

    // API key path（純 API 用戶）
    if let Some(key) = auth.openai_api_key.as_deref() {
        if !key.is_empty() && key != "null" {
            return Ok((key.to_string(), "—".to_string(), "API key".to_string()));
        }
    }

    // OAuth path（ChatGPT 訂閱用戶）
    let tokens = auth.tokens.ok_or("missing tokens")?;
    let access = tokens.access_token.ok_or("missing access_token")?;
    let account = tokens.account_id.unwrap_or_else(|| "—".to_string());

    // 訂閱類型從 id_token JWT 解（包在 `https://api.openai.com/auth` 物件下）
    let plan = tokens
        .id_token
        .as_deref()
        .and_then(decode_jwt_payload)
        .and_then(|v| {
            v.get("https://api.openai.com/auth")
                .and_then(|a| a.get("chatgpt_plan_type"))
                .and_then(|p| p.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| "ChatGPT".to_string());

    Ok((access, account, plan))
}

/// 從 `~/.codex/config.toml` 抓 `model = "..."`（容錯 grep，TOML 解析非必要）
///
/// 邊界：必須 `model` 開頭且後面只能是空白或 `=`，避免誤吃 `model_reasoning_effort`。
fn read_model(home: &Path) -> String {
    let path = home.join(".codex").join("config.toml");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| {
            for line in s.lines() {
                let trimmed = line.trim();
                let after_model = match trimmed.strip_prefix("model") {
                    Some(r)
                        if r.is_empty()
                            || r.starts_with(|c: char| c.is_whitespace() || c == '=') =>
                    {
                        r
                    }
                    _ => continue,
                };
                let after_eq = match after_model.find('=') {
                    Some(i) => &after_model[i + 1..],
                    None => continue,
                };
                let value = after_eq.trim().trim_matches('"');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
            None
        })
        .unwrap_or_else(|| "—".to_string())
}

/// 從 wham/usage 的 rate_limit 抽 (session, weekly) 兩窗。
/// 關鍵陷阱：不可假設 primary=5h、secondary=weekly——要用 limit_window_seconds
/// 分類（<=24h 視為 session、其餘 weekly）；任一窗可為 null。
/// 回傳 (remaining_pct, reset_countdown_text)。
fn classify_windows(
    rate_limit: &serde_json::Value,
    now: u64,
) -> (Option<(i64, String)>, Option<(i64, String)>) {
    let mut session = None;
    let mut weekly = None;
    for key in ["primary_window", "secondary_window"] {
        let Some(w) = rate_limit.get(key).filter(|v| !v.is_null()) else {
            continue;
        };
        let Some(win_secs) = w.get("limit_window_seconds").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(used) = w.get("used_percent").and_then(|v| v.as_f64()) else {
            continue;
        };
        let remaining = (100.0 - used).round().clamp(0.0, 100.0) as i64;
        let reset_epoch = w
            .get("reset_at")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                w.get("reset_after_seconds")
                    .and_then(|v| v.as_u64())
                    .map(|d| now + d)
            });
        let reset = reset_epoch.map(fmt_countdown).unwrap_or_else(|| "—".to_string());
        let slot = if win_secs <= 86400 { &mut session } else { &mut weekly };
        *slot = Some((remaining, reset));
    }
    (session, weekly)
}

/// 逐模型收集額度：主 rate_limit＝目前模型，additional_rate_limits[] 各自獨立（如 Spark）。
fn collect_models(body: &serde_json::Value, model: &str, now: u64) -> Vec<serde_json::Value> {
    let mut models = Vec::new();
    let mut push = |name: &str, rate_limit: &serde_json::Value| {
        let (session, weekly) = classify_windows(rate_limit, now);
        if session.is_none() && weekly.is_none() {
            return;
        }
        models.push(serde_json::json!({
            "name": name,
            "h5_remaining": session.as_ref().map(|s| s.0),
            "h5_reset": session.as_ref().map(|s| s.1.clone()),
            "wk_remaining": weekly.as_ref().map(|w| w.0),
            "wk_reset": weekly.as_ref().map(|w| w.1.clone()),
        }));
    };

    if let Some(rate_limit) = body.get("rate_limit") {
        push(model, rate_limit);
    }
    if let Some(additional) = body.get("additional_rate_limits").and_then(|v| v.as_array()) {
        for limit in additional {
            if let (Some(name), Some(rate_limit)) = (
                limit.get("limit_name").and_then(|v| v.as_str()),
                limit.get("rate_limit"),
            ) {
                push(name, rate_limit);
            }
        }
    }
    models
}

/// 呼叫 ChatGPT wham/usage 拿訂閱額度；API-key-only 用戶 fallback 舊探活路徑
pub async fn fetch(home: &Path) -> RunnerQuota {
    let label = "💻 Codex CLI（本機）".to_string();
    let color = "#10a37f".to_string();
    let name = "codex".to_string();

    let (token, account, plan) = match read_credentials(home) {
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

    let model = read_model(home);
    let token_exp = jwt_exp(&token)
        .map(fmt_countdown)
        .unwrap_or_else(|| "—".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    // API-key-only 用戶沒有 ChatGPT 訂閱額度，保留舊探活路徑
    if plan == "API key" {
        let resp = client.get(OPENAI_MODELS_API).bearer_auth(&token).send().await;
        let (api_ok, status_code) = match resp {
            Ok(r) => (r.status().is_success(), r.status().as_u16()),
            Err(e) => {
                return RunnerQuota {
                    name,
                    label,
                    color,
                    ok: false,
                    text: format!("⚠ API error: {e}"),
                    raw: None,
                }
            }
        };
        let text = if api_ok {
            format!("✓ API key · model {model}")
        } else {
            format!("⚠ token rejected ({status_code})\nmodel {model}")
        };
        let raw = serde_json::json!({
            "ok": api_ok, "plan": plan, "model": model,
            "token_expires_in": token_exp, "account_id": account,
            "status_code": status_code, "ts": chrono::Utc::now().to_rfc3339(),
        });
        return RunnerQuota { name, label, color, ok: api_ok, text, raw: Some(raw) };
    }

    // ChatGPT 訂閱：wham/usage 一次拿 plan + 5h/weekly 額度
    let mut req = client
        .get(CHATGPT_USAGE_API)
        .bearer_auth(&token)
        .header("Accept", "application/json");
    if account != "—" {
        req = req.header("ChatGPT-Account-Id", &account);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: format!("⚠ API error: {e}"),
                raw: None,
            }
        }
    };
    let status_code = resp.status().as_u16();
    if !resp.status().is_success() {
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: format!("⚠ usage API {status_code}（token 可能過期，跑一次 codex 可刷新）"),
            raw: Some(serde_json::json!({
                "ok": false, "plan": plan, "model": model,
                "status_code": status_code, "ts": chrono::Utc::now().to_rfc3339(),
            })),
        };
    }
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return RunnerQuota {
                name,
                label,
                color,
                ok: false,
                text: format!("⚠ parse usage: {e}"),
                raw: None,
            }
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let empty = serde_json::json!({});
    let (session, weekly) = classify_windows(body.get("rate_limit").unwrap_or(&empty), now);
    let models = collect_models(&body, &model, now);
    // plan_type 以 API 回應為準（比 id_token claim 新），首字大寫顯示
    let tier = body
        .get("plan_type")
        .and_then(|v| v.as_str())
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .unwrap_or(plan);

    let fmt_win = |w: &Option<(i64, String)>| {
        w.as_ref().map(|(p, _)| format!("{p}%")).unwrap_or_else(|| "--".to_string())
    };
    let text = format!("⏱ 5h {} · 7d {}\n{}", fmt_win(&session), fmt_win(&weekly), tier);

    let raw = serde_json::json!({
        "ok": true,
        "session_5h_remaining": session.as_ref().map(|s| s.0),
        "session_5h_reset": session.as_ref().map(|s| s.1.clone()),
        "week_7d_remaining": weekly.as_ref().map(|w| w.0),
        "week_7d_reset": weekly.as_ref().map(|w| w.1.clone()),
        "models": models,
        "tier": tier,
        "plan": tier,
        "model": model,
        "token_expires_in": token_exp,
        "account_id": account,
        "status_code": status_code,
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
    fn base64_decode_known_value() {
        // "hello" base64 = "aGVsbG8="
        let decoded = base64_decode("aGVsbG8=").expect("decode");
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn base64_decode_empty_string() {
        let decoded = base64_decode("").expect("decode");
        assert!(decoded.is_empty());
    }

    #[test]
    fn base64_decode_rejects_invalid_char() {
        assert!(base64_decode("aGVs!G8=").is_err());
    }

    #[test]
    fn base64_decode_handles_url_safe_via_caller() {
        // url-safe 轉換是 caller 的責任，這裡只測標準 base64
        let decoded = base64_decode("aGVsbG8=").expect("decode");
        assert_eq!(std::str::from_utf8(&decoded).unwrap(), "hello");
    }

    #[test]
    fn jwt_exp_returns_none_for_malformed_token() {
        assert!(jwt_exp("not-a-jwt").is_none());
        assert!(jwt_exp("only.two").is_none());
        assert!(jwt_exp("").is_none());
    }

    #[test]
    fn jwt_exp_extracts_real_exp() {
        // 真實 JWT: header={"alg":"RS256"} payload={"exp":1780459263,"sub":"user"}
        // header base64: eyJhbGciOiJSUzI1NiJ9
        // payload base64: eyJleHAiOjE3ODA0NTkyNjMsInN1YiI6InVzZXIifQ
        let token = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjE3ODA0NTkyNjMsInN1YiI6InVzZXIifQ.signature";
        assert_eq!(jwt_exp(token), Some(1780459263));
    }

    #[test]
    fn fmt_countdown_past_epoch_returns_expired() {
        assert_eq!(fmt_countdown(0), "expired");
    }

    #[test]
    fn fmt_countdown_under_one_hour_renders_minutes() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let s = fmt_countdown(now + 30 * 60);
        assert!(s.ends_with('m'));
        assert!(s.contains("30m"), "got {s:?}");
    }

    #[test]
    fn fmt_countdown_over_one_hour_renders_hm() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let s = fmt_countdown(now + 2 * 3600 + 15 * 60);
        assert!(s.starts_with("2h"));
        assert!(s.contains("15m"));
    }

    #[test]
    fn read_model_parses_standard_config() {
        let dir = std::env::temp_dir().join("lp_codex_test_model");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".codex")).expect("mkdir");
        std::fs::write(
            dir.join(".codex").join("config.toml"),
            "model = \"gpt-5.5\"\n",
        )
        .expect("write");
        assert_eq!(read_model(&dir), "gpt-5.5");
    }

    #[test]
    fn read_model_missing_file_returns_dash() {
        let dir = std::env::temp_dir().join("lp_codex_test_model_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        assert_eq!(read_model(&dir), "—");
    }

    #[test]
    fn classify_windows_by_window_seconds_not_position() {
        // weekly 在 primary、session 缺席（本機實測 2026-07-16 的真實形狀）
        let rl = serde_json::json!({
            "primary_window": { "used_percent": 9, "limit_window_seconds": 604800,
                                 "reset_after_seconds": 592387, "reset_at": 1784780145u64 },
            "secondary_window": null
        });
        let (session, weekly) = classify_windows(&rl, 1784100000);
        assert!(session.is_none());
        let (pct, _reset) = weekly.expect("weekly window");
        assert_eq!(pct, 91);
    }

    #[test]
    fn classify_windows_both_present() {
        let rl = serde_json::json!({
            "primary_window": { "used_percent": 25.4, "limit_window_seconds": 18000,
                                 "reset_after_seconds": 3600 },
            "secondary_window": { "used_percent": 3, "limit_window_seconds": 604800,
                                   "reset_after_seconds": 500000 }
        });
        let (session, weekly) = classify_windows(&rl, 1784100000);
        assert_eq!(session.expect("session").0, 75);
        assert_eq!(weekly.expect("weekly").0, 97);
    }

    #[test]
    fn classify_windows_empty_rate_limit() {
        let (session, weekly) = classify_windows(&serde_json::json!({}), 0);
        assert!(session.is_none() && weekly.is_none());
    }

    #[test]
    fn collect_models_includes_additional_rate_limits() {
        let window = |seconds: u64, used: f64| serde_json::json!({
            "limit_window_seconds": seconds,
            "used_percent": used,
            "reset_after_seconds": 60,
        });
        let body = serde_json::json!({
            "rate_limit": { "primary_window": window(18_000, 25.0) },
            "additional_rate_limits": [{
                "limit_name": "gpt-5.3-codex-spark",
                "rate_limit": {
                    "primary_window": window(18_000, 40.0),
                    "secondary_window": window(604_800, 10.0),
                }
            }]
        });

        let models = collect_models(&body, "gpt-5.4", 1_000);
        assert_eq!(models.len(), 2);
        assert_eq!(models[1]["name"], "gpt-5.3-codex-spark");
        assert_eq!(models[1]["h5_remaining"], 60);
        assert_eq!(models[1]["wk_remaining"], 90);
    }

    #[test]
    fn read_model_handles_comments_and_other_keys() {
        let dir = std::env::temp_dir().join("lp_codex_test_model_comments");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".codex")).expect("mkdir");
        std::fs::write(
            dir.join(".codex").join("config.toml"),
            "# comment\nmodel_reasoning_effort = \"medium\"\nmodel = \"gpt-5.4-mini\"\n",
        )
        .expect("write");
        assert_eq!(read_model(&dir), "gpt-5.4-mini");
    }
}
