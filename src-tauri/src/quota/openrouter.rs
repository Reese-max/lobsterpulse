//! OpenRouter credits live quota fetch：讀 env `OPENROUTER_API_KEY*`（本機為
//! _A.._E 五把獨立帳號 key），逐把打 `https://openrouter.ai/api/v1/credits`
//! 聚合成一張卡。2026-07-17 落地，端點為官方文件公開 API，本機五把 key 實測
//! （A 610/321.7、E 10/10.28 透支）。
//!
//! 卡片形狀：h5 slot = 全部帳號加總剩餘 %（label "Credits"），wk slot = 最低
//! 帳號剩餘 %（label "Low X"）；副標 "N keys · $X left"。credits 是預付制
//! 無重置時間，reset 欄一律 None。
//!
//! 邊界：
//! - 單把 key 失敗 → 跳過該把用其餘聚合；全部失敗 → ⚠ API error（觸發 retry_net）
//! - total_usage 可能超過 total_credits（透支，如本機 E 帳號）→ 剩餘 clamp 0
//! - total_credits <= 0 的帳號不參與 %（防除零），仍計入 key 數

use super::RunnerQuota;

const OPENROUTER_CREDITS_API: &str = "https://openrouter.ai/api/v1/credits";
/// 單把 key 的花費上限（limit／limit_remaining）。/credits 只回「帳號」餘額，
/// 2026-10-19 實測 A 帳號剩 $269.67 但 key 上限 $100 只剩 $58.61、B 帳號剩 $9.18
/// 但 key 上限 $5 只剩 $4.53——只看帳號餘額會高估這把 key 實際能花的錢。
const OPENROUTER_KEY_API: &str = "https://openrouter.ai/api/v1/key";

/// 一把 key 的實際可用額度。
#[derive(Debug, Clone, PartialEq)]
struct KeyBalance {
    label: String,
    /// 實際可花：min(帳號剩餘, key 上限剩餘)
    left: f64,
    /// 分母：有 key 上限時用上限，否則用帳號總額
    total: f64,
    account_left: f64,
    account_total: f64,
    key_limit: Option<f64>,
    key_limit_remaining: Option<f64>,
}

/// 帳號餘額＋/key 回應 → 實際可用額度。/key 失敗或無上限（limit: null）時退回帳號值。
fn effective_balance(label: String, account_left: f64, account_total: f64, key_body: Option<&serde_json::Value>) -> KeyBalance {
    let d = key_body.and_then(|b| b.get("data"));
    let key_limit = d.and_then(|d| d.get("limit")).and_then(|v| v.as_f64());
    let key_limit_remaining = d
        .and_then(|d| d.get("limit_remaining"))
        .and_then(|v| v.as_f64())
        .map(|v| v.max(0.0));
    // limit=0 是「這把 key 不能花錢」，不是「沒設上限」——只有 null 才退回帳號值
    let (left, total) = match (key_limit, key_limit_remaining) {
        (Some(limit), Some(rem)) => (account_left.min(rem), limit.max(0.0)),
        _ => (account_left, account_total),
    };
    KeyBalance { label, left, total, account_left, account_total, key_limit, key_limit_remaining }
}

/// 從環境變數收集所有 `OPENROUTER_API_KEY*`（即時讀，見 `api_keys::live_entries`），
/// 回 (顯示標籤, key)，按 var 名排序。`OPENROUTER_API_KEY_A` → 標籤 "A"；無後綴 → "main"。
fn read_credentials() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = crate::api_keys::live_vars()
        .into_iter()
        .filter(|(k, v)| k.starts_with("OPENROUTER_API_KEY") && !v.trim().is_empty())
        .map(|(k, v)| {
            let suffix = k
                .trim_start_matches("OPENROUTER_API_KEY")
                .trim_start_matches('_');
            let label = if suffix.is_empty() { "main".to_string() } else { suffix.to_string() };
            (label, v.trim().to_string())
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// 單帳號 credits：(剩餘美元, 總額美元)。剩餘 clamp 0（透支帳號回 0）。
fn parse_credits(body: &serde_json::Value) -> Option<(f64, f64)> {
    let d = body.get("data")?;
    let credits = d.get("total_credits")?.as_f64()?;
    let usage = d.get("total_usage")?.as_f64()?;
    Some(((credits - usage).max(0.0), credits))
}

async fn fetch_one(
    client: reqwest::Client,
    label: String,
    key: String,
) -> Result<KeyBalance, String> {
    let resp = client
        .get(OPENROUTER_CREDITS_API)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|e| format!("API error: {e}"))?;
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        return Err(format!("key {label} rejected ({status})"));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse: {e}"))?;
    let (left, total) = parse_credits(&body).ok_or_else(|| format!("key {label}: 缺 credits 欄位"))?;
    // key 上限查不到不擋整把 key：退回帳號餘額（舊行為）
    let key_body = match client.get(OPENROUTER_KEY_API).bearer_auth(&key).send().await {
        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.ok(),
        _ => None,
    };
    Ok(effective_balance(label, left, total, key_body.as_ref()))
}

/// 打全部帳號的 credits 端點並聚合成一張卡。
pub async fn fetch() -> RunnerQuota {
    let label = "OpenRouter".to_string();
    let color = "#6366f1".to_string();
    let name = "openrouter".to_string();

    let keys = read_credentials();
    if keys.is_empty() {
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: "⚠ no OpenRouter key in env (set OPENROUTER_API_KEY*)".to_string(),
            raw: None,
        };
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let mut set = tokio::task::JoinSet::new();
    for (l, k) in keys.iter().cloned() {
        set.spawn(fetch_one(client.clone(), l, k));
    }
    let mut rows: Vec<KeyBalance> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok(row)) => rows.push(row),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("join: {e}")),
        }
    }
    rows.sort_by(|a, b| a.label.cmp(&b.label));

    if rows.is_empty() {
        // 全滅：優先回帶「API error」的訊息讓 retry_net 接手網路型失敗
        let msg = errors.first().cloned().unwrap_or_else(|| "unknown".to_string());
        return RunnerQuota {
            name,
            label,
            color,
            ok: false,
            text: format!("⚠ {msg}"),
            raw: None,
        };
    }

    // 2026-07-17 使用者反饋：跨帳號加總 % 與「最低帳號」都是無效資訊（帳號額度
    // 大小差 60 倍），卡片改為 per-key 明細（前端據 raw.accounts 一把 key 一條
    // bar，右側顯示 $剩餘/$總額），這裡只留總剩餘美元當副標摘要。
    let dollars_left: f64 = rows.iter().map(|r| r.left).sum();
    let plan = format!(
        "{} key{} · ${dollars_left:.0} left",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    );
    let mut text = format!("⏱ {plan}");
    if !errors.is_empty() {
        text.push_str(&format!("\n⚠ {} key 失敗", errors.len()));
    }

    let raw = serde_json::json!({
        "ok": true,
        "basis": "provider_api",
        "plan": plan,
        // left_usd／total_usd 改為「這把 key 實際可用」（route quota.py 依賴這兩個欄位名）；
        // 帳號層與 key 上限另列，供前端註記
        "accounts": rows.iter().map(|r| serde_json::json!({
            "key": r.label, "left_usd": r.left, "total_usd": r.total,
            "account_left_usd": r.account_left, "account_total_usd": r.account_total,
            "key_limit_usd": r.key_limit, "key_limit_remaining_usd": r.key_limit_remaining,
        })).collect::<Vec<_>>(),
        "failed_keys": errors.len(),
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
    fn parse_credits_real_shape() {
        // 2026-07-17 本機 A 帳號實測形狀
        let body = serde_json::json!({"data":{"total_credits":610,"total_usage":321.725553332}});
        let (left, total) = parse_credits(&body).expect("應解析成功");
        assert!((left - 288.274446668).abs() < 1e-6);
        assert_eq!(total, 610.0);
    }

    #[test]
    fn parse_credits_overdraft_clamps_zero() {
        // 本機 E 帳號實測：usage 超過 credits（透支）
        let body = serde_json::json!({"data":{"total_credits":10,"total_usage":10.28470904}});
        let (left, _) = parse_credits(&body).expect("應解析成功");
        assert_eq!(left, 0.0, "透支帳號剩餘應 clamp 0");
    }

    #[test]
    fn effective_balance_caps_by_key_limit() {
        // 2026-10-19 本機 A 帳號實測：帳號剩 269.67，key 上限 100 剩 58.61
        let key = serde_json::json!({"data":{"limit":100,"limit_remaining":58.60975963,"usage":41.39}});
        let b = effective_balance("A".into(), 269.666, 610.0, Some(&key));
        assert!((b.left - 58.60975963).abs() < 1e-9, "受 key 上限限制");
        assert_eq!(b.total, 100.0, "分母改用 key 上限");
        assert_eq!(b.account_left, 269.666);
        assert_eq!(b.key_limit, Some(100.0));
    }

    #[test]
    fn effective_balance_account_binding_when_lower_than_key_limit() {
        // key 上限還有很多，但帳號快沒錢：以帳號餘額為準
        let key = serde_json::json!({"data":{"limit":100,"limit_remaining":90.0}});
        let b = effective_balance("X".into(), 5.0, 30.0, Some(&key));
        assert_eq!(b.left, 5.0);
        assert_eq!(b.total, 100.0);
    }

    #[test]
    fn effective_balance_zero_limit_means_unusable_key() {
        // limit=0 是「這把 key 不能花錢」，不可退回帳號餘額誤報為有額度（Codex 審查抓到）
        let key = serde_json::json!({"data":{"limit":0,"limit_remaining":0}});
        let b = effective_balance("Z".into(), 269.0, 610.0, Some(&key));
        assert_eq!((b.left, b.total), (0.0, 0.0));
    }

    #[test]
    fn effective_balance_falls_back_to_account_without_limit() {
        // C/D/E 實測：limit 為 null；/key 失敗也一樣退回帳號值
        let key = serde_json::json!({"data":{"limit":null,"limit_remaining":null,"usage":1.0}});
        let b = effective_balance("C".into(), 10.92, 30.0, Some(&key));
        assert_eq!((b.left, b.total), (10.92, 30.0));
        assert_eq!(b.key_limit, None);
        let b = effective_balance("D".into(), 9.69, 10.0, None);
        assert_eq!((b.left, b.total), (9.69, 10.0));
    }

    #[test]
    fn parse_credits_missing_fields_returns_none() {
        assert!(parse_credits(&serde_json::json!({"data":{}})).is_none());
        assert!(parse_credits(&serde_json::json!({"error":"x"})).is_none());
    }

    #[test]
    fn read_credentials_collects_suffixed_keys_sorted() {
        let _env_guard = crate::quota::copilot::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        // 用不會撞真實設定的測試專用後綴
        std::env::set_var("OPENROUTER_API_KEY_ZTEST2", "sk-or-test-2");
        std::env::set_var("OPENROUTER_API_KEY_ZTEST1", "sk-or-test-1");
        let keys = read_credentials();
        let zs: Vec<_> = keys.iter().filter(|(l, _)| l.starts_with("ZTEST")).collect();
        assert_eq!(zs.len(), 2);
        assert_eq!(zs[0].0, "ZTEST1", "應按名稱排序");
        assert_eq!(zs[1].0, "ZTEST2");
        std::env::remove_var("OPENROUTER_API_KEY_ZTEST1");
        std::env::remove_var("OPENROUTER_API_KEY_ZTEST2");
    }
}

#[cfg(test)]
mod live_probe {
    // 手動診斷用：cargo test --release live_openrouter -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_openrouter_fetch_prints_result() {
        let r = super::fetch().await;
        println!("ok={} text={:?} raw={:?}", r.ok, r.text, r.raw.map(|v| v.to_string()));
    }
}
