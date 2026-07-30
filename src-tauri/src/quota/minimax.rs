//! MiniMax Coding Plan live quota fetch：讀 `MINIMAX_API_KEY` env（sk-cp- 訂閱 key），
//! 打 `https://api.minimax.io/v1/token_plan/remains` 拿 5h/週剩餘 %。
//! 2026-07-17 落地。端點查證：MiniMax-AI/cli endpoints.ts 同款 + 本機真 key 實測
//! （general 列 5h 71% / 週 43%，頂層平鋪、數字型別）。
//!
//! 對齊 copilot.rs 既有 contract：env 讀 credentials 早返 (ok=false) 不打 API，
//! fmt_* helper 複製不抽共用。
//!
//! 邊界（多 repo 交叉驗證 + 實測）：
//! - `model_remains` 可能平鋪頂層或包在 `data.` 下 → 兩層都試
//! - percent 欄位 live 可能回字串數字 → 防禦解析
//! - `current_*_status == 3` 是「無此權益」佔位列（如本帳號的 video 列）→ 抑制
//! - `end_time` / `weekly_end_time` 是 epoch **毫秒**

use super::RunnerQuota;

const MINIMAX_REMAINS_API: &str = "https://api.minimax.io/v1/token_plan/remains";

/// 從 env 拿 MiniMax key。優先序：MINIMAX_API_KEY > MINIMAX_DIRECT_KEY。
/// 0 bytes / whitespace-only 視同「未設定」。
fn read_credentials() -> Result<String, String> {
    for key in ["MINIMAX_API_KEY", "MINIMAX_DIRECT_KEY"] {
        if let Ok(v) = std::env::var(key) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }
    Err("no MiniMax key in env (set MINIMAX_API_KEY)".to_string())
}

/// 防禦解析 percent：數字或字串數字都收，範圍外回 None（不腦補 0）
fn parse_pct(v: Option<&serde_json::Value>) -> Option<i64> {
    let f = match v? {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if (0.0..=100.0).contains(&f) {
        Some(f.round() as i64)
    } else {
        None
    }
}

/// epoch 毫秒 → 「XhYm」剩餘時間；過期/缺欄回 None
fn fmt_countdown_ms(end_ms: Option<&serde_json::Value>, now_ms: u64) -> Option<String> {
    let end = end_ms?.as_u64()?;
    if end <= now_ms {
        return None;
    }
    let delta_min = (end - now_ms) / 60_000;
    let h = delta_min / 60;
    let m = delta_min % 60;
    Some(format!("{h}h{m}m"))
}

/// 從 remains 回應抽 general 列的 (5h %, 5h reset, 週 %, 週 reset)。
/// 找不到 general 列回 None。
type Windows = (Option<i64>, Option<String>, Option<i64>, Option<String>);
fn parse_remains(body: &serde_json::Value, now_ms: u64) -> Option<Windows> {
    let rows = body
        .get("data")
        .and_then(|d| d.get("model_remains"))
        .or_else(|| body.get("model_remains"))?
        .as_array()?;
    let g = rows
        .iter()
        .find(|r| r.get("model_name").and_then(|v| v.as_str()) == Some("general"))?;
    let status = |key: &str| g.get(key).and_then(|v| v.as_i64());
    // status 3 = 無此權益佔位列，該窗不顯示
    let (mut s_pct, mut s_reset) = (None, None);
    if status("current_interval_status") != Some(3) {
        s_pct = parse_pct(g.get("current_interval_remaining_percent"));
        s_reset = fmt_countdown_ms(g.get("end_time"), now_ms);
    }
    let (mut w_pct, mut w_reset) = (None, None);
    if status("current_weekly_status") != Some(3) {
        w_pct = parse_pct(g.get("current_weekly_remaining_percent"));
        w_reset = fmt_countdown_ms(g.get("weekly_end_time"), now_ms);
    }
    Some((s_pct, s_reset, w_pct, w_reset))
}

/// 呼叫 token_plan/remains 拿 Coding Plan 5h/週額度。
pub async fn fetch() -> RunnerQuota {
    let label = "MiniMax".to_string();
    let color = "#ec4899".to_string();
    let name = "minimax".to_string();

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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let resp = match client
        .get(MINIMAX_REMAINS_API)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .send()
        .await
    {
        Ok(r) => r,
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
    let status_code = resp.status().as_u16();
    if !resp.status().is_success() {
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: format!("⚠ token rejected ({status_code})"),
            raw: None,
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
                text: format!("⚠ parse remains: {e}"),
                raw: None,
            };
        }
    };

    // MiniMax 慣例：HTTP 200 + base_resp.status_code 表達錯誤（1004=認證失敗）
    let base_code = body
        .get("base_resp")
        .and_then(|b| b.get("status_code"))
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    if base_code != 0 {
        let msg = body
            .get("base_resp")
            .and_then(|b| b.get("status_msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: format!("⚠ MiniMax API {base_code}: {msg}"),
            raw: None,
        };
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let (s_pct, s_reset, w_pct, w_reset) = parse_remains(&body, now_ms).unwrap_or_default();

    let fmt = |p: &Option<i64>| p.map(|v| format!("{v}%")).unwrap_or_else(|| "--".to_string());
    let text = format!("⏱ 5h {} · 7d {}\nCoding Plan", fmt(&s_pct), fmt(&w_pct));

    let raw = serde_json::json!({
        "ok": true,
        "basis": "provider_api",
        "session_5h_remaining": s_pct,
        "session_5h_reset": s_reset,
        "week_7d_remaining": w_pct,
        "week_7d_reset": w_reset,
        "tier": "Coding Plan",
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

    /// 2026-07-17 本機真 key 實測形狀（general + video 兩列，video 是 status 3 佔位）
    fn real_shape() -> serde_json::Value {
        serde_json::json!({
            "model_remains": [
                { "model_name": "general",
                  "current_interval_remaining_percent": 71,
                  "current_weekly_remaining_percent": 43,
                  "current_interval_status": 1, "current_weekly_status": 1,
                  "end_time": 1784264400000u64, "weekly_end_time": 1784505600000u64 },
                { "model_name": "video",
                  "current_interval_remaining_percent": 100,
                  "current_weekly_remaining_percent": 100,
                  "current_interval_status": 3, "current_weekly_status": 3,
                  "end_time": 1784332800000u64, "weekly_end_time": 1784505600000u64 }
            ],
            "base_resp": { "status_code": 0, "status_msg": "success" }
        })
    }

    #[test]
    fn parse_remains_real_shape_extracts_general_row() {
        let now_ms = 1784260000000u64; // end_time 前 ~73 分鐘
        let (s, sr, w, wr) = parse_remains(&real_shape(), now_ms).expect("general 列應存在");
        assert_eq!(s, Some(71));
        assert_eq!(w, Some(43));
        assert_eq!(sr.as_deref(), Some("1h13m"));
        assert!(wr.is_some());
    }

    #[test]
    fn parse_remains_nested_under_data() {
        // token-monitor 實證：live 回應可能包在 data. 下
        let body = serde_json::json!({ "data": real_shape() });
        let (s, _, w, _) = parse_remains(&body, 0).expect("data. 巢狀也要解得開");
        assert_eq!(s, Some(71));
        assert_eq!(w, Some(43));
    }

    #[test]
    fn parse_remains_string_percent_defensive() {
        // token-monitor 註解實證：percent 可能以字串回傳
        let body = serde_json::json!({
            "model_remains": [
                { "model_name": "general",
                  "current_interval_remaining_percent": "98",
                  "current_weekly_remaining_percent": "67",
                  "current_interval_status": 1, "current_weekly_status": 1 }
            ]
        });
        let (s, sr, w, _) = parse_remains(&body, 0).expect("general 列應存在");
        assert_eq!(s, Some(98));
        assert_eq!(w, Some(67));
        assert!(sr.is_none(), "缺 end_time 時 reset 應為 None");
    }

    #[test]
    fn parse_remains_status_three_suppresses_window() {
        let body = serde_json::json!({
            "model_remains": [
                { "model_name": "general",
                  "current_interval_remaining_percent": 100,
                  "current_weekly_remaining_percent": 50,
                  "current_interval_status": 3, "current_weekly_status": 1 }
            ]
        });
        let (s, _, w, _) = parse_remains(&body, 0).expect("general 列應存在");
        assert!(s.is_none(), "status 3 的 5h 窗應抑制");
        assert_eq!(w, Some(50));
    }

    #[test]
    fn parse_remains_missing_general_returns_none() {
        let body = serde_json::json!({ "model_remains": [ { "model_name": "video" } ] });
        assert!(parse_remains(&body, 0).is_none());
    }

    #[test]
    fn parse_pct_rejects_out_of_range_and_garbage() {
        assert_eq!(parse_pct(Some(&serde_json::json!(101))), None);
        assert_eq!(parse_pct(Some(&serde_json::json!(-1))), None);
        assert_eq!(parse_pct(Some(&serde_json::json!("abc"))), None);
        assert_eq!(parse_pct(Some(&serde_json::json!(null))), None);
        assert_eq!(parse_pct(None), None);
        assert_eq!(parse_pct(Some(&serde_json::json!("55.4"))), Some(55));
    }

    #[test]
    fn fmt_countdown_ms_past_returns_none() {
        assert!(fmt_countdown_ms(Some(&serde_json::json!(1000u64)), 2000).is_none());
        assert!(fmt_countdown_ms(None, 0).is_none());
    }

    #[test]
    fn read_credentials_prefers_api_key_env() {
        let _env_guard = crate::quota::copilot::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        std::env::set_var("MINIMAX_API_KEY", "sk-cp-test-key");
        let t = read_credentials().expect("MINIMAX_API_KEY 應成功");
        assert_eq!(t, "sk-cp-test-key");
        std::env::remove_var("MINIMAX_API_KEY");
    }

    #[test]
    fn read_credentials_whitespace_only_treated_as_missing() {
        let _env_guard = crate::quota::copilot::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        std::env::set_var("MINIMAX_API_KEY", "  \t ");
        std::env::remove_var("MINIMAX_DIRECT_KEY");
        assert!(read_credentials().is_err());
        std::env::remove_var("MINIMAX_API_KEY");
    }
}

#[cfg(test)]
mod live_probe {
    // 手動診斷用：cargo test --release live_minimax -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_minimax_fetch_prints_result() {
        let r = super::fetch().await;
        println!("ok={} text={:?} raw={:?}", r.ok, r.text, r.raw.map(|v| v.to_string()));
    }
}
