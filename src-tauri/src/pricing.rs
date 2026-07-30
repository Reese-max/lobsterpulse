//! LiteLLM 價目快照與 token 成本估算。
//!
//! 快照只保留本機會查詢的 Claude、GPT-5、Gemini 價格列，避免每次掃描解析
//! LiteLLM 完整大表。快照壞掉或不存在時退回內建價；過期快照仍可繼續使用。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SOURCE_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
const SNAPSHOT_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const REFRESH_BACKOFF: Duration = Duration::from_secs(60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

/// 本機修正價不得被上游同名列覆蓋。
pub const PROTECTED_KEYS: &[&str] = &["claude-opus-4-8"];

static LAST_REFRESH_ATTEMPT: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_5m_per_million: Option<f64>,
    pub cache_write_1h_per_million: Option<f64>,
    pub cache_read_per_million: Option<f64>,
}

impl ModelPrice {
    const fn new(
        input: f64,
        output: f64,
        cache_write_5m: Option<f64>,
        cache_write_1h: Option<f64>,
        cache_read: Option<f64>,
    ) -> Self {
        Self {
            input_per_million: input,
            output_per_million: output,
            cache_write_5m_per_million: cache_write_5m,
            cache_write_1h_per_million: cache_write_1h,
            cache_read_per_million: cache_read,
        }
    }
}

/// JSONL 中可可靠分離的 token 類別。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    /// 有 cache creation 總量，但 JSONL 沒有可靠標出 5m／1h 的部分。
    pub cache_write_unknown: u64,
    pub cache_read: u64,
}

impl TokenUsage {
    pub fn total(self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_write_5m)
            .saturating_add(self.cache_write_1h)
            .saturating_add(self.cache_write_unknown)
            .saturating_add(self.cache_read)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    fetched_at: u64,
    source: String,
    models: BTreeMap<String, ModelPrice>,
}

#[derive(Debug, Clone)]
pub struct PricingCatalog {
    prices: BTreeMap<String, ModelPrice>,
}

impl PricingCatalog {
    /// 壞檔、空檔或不存在一律退回內建價；過期但可解析的快照仍照常合併。
    pub fn load(home: &Path) -> Self {
        let path = snapshot_path(home);
        let upstream = match read_snapshot(&path) {
            Ok(snapshot) => snapshot.models,
            Err(error) => {
                if path.exists() {
                    log::warn!(
                        "[pricing] {} 無法讀取，改用內建價：{error}",
                        path.display()
                    );
                }
                BTreeMap::new()
            }
        };

        Self {
            prices: merge_catalog(fallback_prices(), upstream),
        }
    }

    /// 大小寫、provider 前綴與日期後綴皆容錯；完整模型名優先，最後才退家族價。
    pub fn get(&self, model: &str) -> Option<ModelPrice> {
        let key = normalize_model_name(model);
        self.prices
            .get(&key)
            .copied()
            .or_else(|| family_fallback_key(&key).and_then(|k| self.prices.get(k).copied()))
    }

    /// 任一實際使用的 token 類別沒有可靠單價時回 None，不輸出可能低估的部分小計。
    pub fn estimate_usd(&self, model: &str, usage: TokenUsage) -> Option<f64> {
        if usage.cache_write_unknown > 0 {
            return None;
        }

        let price = self.get(model)?;
        let optional_cost = |tokens: u64, rate: Option<f64>| {
            if tokens == 0 {
                Some(0.0)
            } else {
                rate.map(|r| tokens as f64 * r / 1_000_000.0)
            }
        };

        Some(
            usage.input as f64 * price.input_per_million / 1_000_000.0
                + usage.output as f64 * price.output_per_million / 1_000_000.0
                + optional_cost(
                    usage.cache_write_5m,
                    price.cache_write_5m_per_million,
                )?
                + optional_cost(
                    usage.cache_write_1h,
                    price.cache_write_1h_per_million,
                )?
                + optional_cost(usage.cache_read, price.cache_read_per_million)?,
        )
    }
}

fn fallback_prices() -> BTreeMap<String, ModelPrice> {
    [
        (
            "claude-sonnet",
            ModelPrice::new(3.00, 15.00, Some(3.75), Some(6.00), Some(0.30)),
        ),
        (
            "claude-opus",
            ModelPrice::new(5.00, 25.00, Some(6.25), Some(10.00), Some(0.50)),
        ),
        (
            "claude-opus-4-8",
            ModelPrice::new(5.00, 25.00, Some(6.25), Some(10.00), Some(0.50)),
        ),
        (
            "claude-haiku",
            ModelPrice::new(1.00, 5.00, Some(1.25), Some(2.00), Some(0.10)),
        ),
        (
            "gpt-5",
            ModelPrice::new(1.25, 10.00, None, None, Some(0.125)),
        ),
        (
            "gpt-5-mini",
            ModelPrice::new(0.25, 2.00, None, None, Some(0.025)),
        ),
        (
            "gpt-5-nano",
            ModelPrice::new(0.05, 0.40, None, None, Some(0.005)),
        ),
        (
            "gpt-5.4-mini",
            ModelPrice::new(0.75, 4.50, None, None, Some(0.075)),
        ),
        (
            "gpt-5.5",
            ModelPrice::new(5.00, 30.00, None, None, Some(0.50)),
        ),
        (
            "gemini-pro",
            ModelPrice::new(1.25, 10.00, None, None, Some(0.125)),
        ),
        (
            "gemini-flash",
            ModelPrice::new(0.30, 2.50, None, None, Some(0.03)),
        ),
        (
            "gemini-flash-lite",
            ModelPrice::new(0.10, 0.40, None, None, Some(0.01)),
        ),
    ]
    .into_iter()
    .map(|(key, price)| (key.to_string(), price))
    .collect()
}

fn merge_catalog(
    mut builtin: BTreeMap<String, ModelPrice>,
    upstream: BTreeMap<String, ModelPrice>,
) -> BTreeMap<String, ModelPrice> {
    for (key, price) in upstream {
        let key = normalize_model_name(&key);
        if !key.is_empty() && !PROTECTED_KEYS.contains(&key.as_str()) {
            builtin.insert(key, price);
        }
    }
    builtin
}

fn family_fallback_key(key: &str) -> Option<&'static str> {
    if key.starts_with("claude-") {
        if key.contains("sonnet") {
            return Some("claude-sonnet");
        }
        if key.contains("opus") {
            return Some("claude-opus");
        }
        if key.contains("haiku") {
            return Some("claude-haiku");
        }
    }

    if key.starts_with("gpt-5") {
        if key.contains("nano") {
            return Some("gpt-5-nano");
        }
        if key.contains("mini") {
            return Some("gpt-5-mini");
        }
        return Some("gpt-5");
    }

    if key.starts_with("gemini-") {
        if key.contains("flash-lite") {
            return Some("gemini-flash-lite");
        }
        if key.contains("flash") {
            return Some("gemini-flash");
        }
        if key.contains("pro") {
            return Some("gemini-pro");
        }
    }

    None
}

/// 將 `provider/model`、`provider:model`、日期尾碼與 Claude 點號版本收斂成同一 key。
pub fn normalize_model_name(model: &str) -> String {
    let mut key = model.trim().to_ascii_lowercase().replace('_', "-");

    if let Some(at) = key.find('@') {
        key.truncate(at);
    }

    let start = ["claude-", "gpt-5", "gemini-"]
        .into_iter()
        .filter_map(|prefix| key.find(prefix))
        .min();

    if let Some(start) = start {
        key = key[start..].to_string();
    } else if let Some(last) = key.rsplit('/').next() {
        key = last.to_string();
    }

    if let Some(base) = key.strip_suffix("-v1:0") {
        key = base.to_string();
    } else if let Some(base) = key.strip_suffix(":0") {
        key = base.to_string();
    }

    if key.starts_with("claude-") {
        key = key.replace('.', "-");
    }

    strip_date_suffix(key)
}

fn strip_date_suffix(mut key: String) -> String {
    if let Some(last) = key.rsplit('-').next() {
        if last.len() == 8
            && last.starts_with("20")
            && last.bytes().all(|b| b.is_ascii_digit())
        {
            key.truncate(key.len() - last.len() - 1);
            return key;
        }
    }

    let mut parts = key.rsplitn(4, '-');
    let day = parts.next().unwrap_or("");
    let month = parts.next().unwrap_or("");
    let year = parts.next().unwrap_or("");
    if day.len() == 2
        && month.len() == 2
        && year.len() == 4
        && year.starts_with("20")
        && day.bytes().all(|b| b.is_ascii_digit())
        && month.bytes().all(|b| b.is_ascii_digit())
        && year.bytes().all(|b| b.is_ascii_digit())
    {
        let suffix = format!("-{year}-{month}-{day}");
        key.truncate(key.len() - suffix.len());
    }

    key
}

fn snapshot_path(home: &Path) -> PathBuf {
    home.join(".lobsterpulse").join("pricing-catalog.json")
}

fn read_snapshot(path: &Path) -> Result<Snapshot, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let snapshot: Snapshot = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if snapshot.models.is_empty() {
        return Err("價目快照沒有可用模型".to_string());
    }
    Ok(snapshot)
}

fn is_fresh(fetched_at: u64, now: u64) -> bool {
    // 小幅時鐘飄移可接受；明顯來自未來的時間不可永久凍結刷新。
    fetched_at <= now.saturating_add(300)
        && now.saturating_sub(fetched_at) < SNAPSHOT_TTL.as_secs()
}

fn refresh_backed_off() -> bool {
    let Ok(mut last) = LAST_REFRESH_ATTEMPT.lock() else {
        return false;
    };
    if last
        .as_ref()
        .is_some_and(|at| at.elapsed() < REFRESH_BACKOFF)
    {
        true
    } else {
        *last = Some(Instant::now());
        false
    }
}

/// 由既有 live quota 刷新路徑呼叫；新鮮快照只做本機檢查，不打網路。
pub async fn refresh_if_needed(home: Option<&Path>) {
    let Some(home) = home else {
        return;
    };

    let path = snapshot_path(home);
    let now = unix_now();
    if read_snapshot(&path)
        .map(|snapshot| is_fresh(snapshot.fetched_at, now))
        .unwrap_or(false)
    {
        return;
    }
    if refresh_backed_off() {
        return;
    }

    match fetch_snapshot(now).await {
        Ok(snapshot) => {
            let Some(parent) = path.parent() else {
                return;
            };
            if let Err(error) = std::fs::create_dir_all(parent)
                .and_then(|_| {
                    serde_json::to_vec_pretty(&snapshot)
                        .map_err(std::io::Error::other)
                        .and_then(|data| crate::write_file_atomic_with_retry(&path, &data))
                })
            {
                log::warn!(
                    "[pricing] 寫入 {} 失敗，保留舊快照：{error}",
                    path.display()
                );
            } else {
                log::info!(
                    "[pricing] LiteLLM 價目快照已刷新，{} 個模型",
                    snapshot.models.len()
                );
            }
        }
        Err(error) => {
            // 不動舊檔；同一 process 一小時內不再嘗試。
            log::warn!("[pricing] LiteLLM 價目刷新失敗，保留舊快照：{error}");
        }
    }
}

async fn fetch_snapshot(fetched_at: u64) -> Result<Snapshot, String> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent("LobsterPulse pricing snapshot")
        .build()
        .map_err(|e| e.to_string())?;

    let payload = client
        .get(SOURCE_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;

    let models = parse_litellm(&payload)?;
    Ok(Snapshot {
        fetched_at,
        source: SOURCE_URL.to_string(),
        models,
    })
}

fn parse_litellm(value: &serde_json::Value) -> Result<BTreeMap<String, ModelPrice>, String> {
    let rows = value
        .as_object()
        .ok_or_else(|| "LiteLLM 回應不是 JSON object".to_string())?;
    let mut chosen: BTreeMap<String, (u8, ModelPrice)> = BTreeMap::new();

    for (raw_key, details) in rows {
        let Some(details) = details.as_object() else {
            continue;
        };
        let key = normalize_model_name(raw_key);
        if !is_supported_model(&key) {
            continue;
        }

        let Some(input) = first_cost(
            details,
            &["input_cost_per_token", "prompt_cost_per_token"],
        ) else {
            continue;
        };
        let Some(output) = first_cost(
            details,
            &["output_cost_per_token", "completion_cost_per_token"],
        ) else {
            continue;
        };

        let price = ModelPrice {
            input_per_million: input * 1_000_000.0,
            output_per_million: output * 1_000_000.0,
            cache_write_5m_per_million: first_cost(
                details,
                &[
                    "cache_creation_input_token_cost",
                    "cache_creation_input_token_cost_5m",
                    "cache_creation_5m_input_token_cost",
                ],
            )
            .map(|v| v * 1_000_000.0),
            cache_write_1h_per_million: first_cost(
                details,
                &[
                    "cache_creation_input_token_cost_above_1hr",
                    "cache_creation_input_token_cost_1h",
                    "cache_creation_1h_input_token_cost",
                ],
            )
            .map(|v| v * 1_000_000.0),
            cache_read_per_million: first_cost(
                details,
                &[
                    "cache_read_input_token_cost",
                    "cache_read_cost_per_token",
                    "input_cache_read_cost_per_token",
                ],
            )
            .map(|v| v * 1_000_000.0),
        };
        let priority = source_priority(raw_key, details);

        if chosen
            .get(&key)
            .is_none_or(|(current, _)| priority >= *current)
        {
            chosen.insert(key, (priority, price));
        }
    }

    let models = chosen
        .into_iter()
        .map(|(key, (_, price))| (key, price))
        .collect::<BTreeMap<_, _>>();
    if models.is_empty() {
        return Err("LiteLLM 回應沒有可解析的相關價格".to_string());
    }
    Ok(models)
}

fn is_supported_model(key: &str) -> bool {
    key.starts_with("claude-") || key.starts_with("gpt-5") || key.starts_with("gemini-")
}

fn source_priority(raw_key: &str, details: &serde_json::Map<String, serde_json::Value>) -> u8 {
    let provider = details
        .get("litellm_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if matches!(provider.as_str(), "anthropic" | "openai" | "gemini") {
        4
    } else if provider.starts_with("vertex_ai") {
        3
    } else if raw_key.to_ascii_lowercase().starts_with("global.") {
        2
    } else {
        1
    }
}

fn first_cost(
    details: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<f64> {
    keys.iter()
        .find_map(|key| details.get(*key).and_then(|v| v.as_f64()))
        .filter(|v| v.is_finite() && *v >= 0.0)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_protected_builtin_and_overrides_plain_builtin() {
        let builtin = fallback_prices();
        let protected = builtin["claude-opus-4-8"];
        let upstream_price = ModelPrice::new(999.0, 999.0, None, None, None);
        let mut upstream = BTreeMap::new();
        upstream.insert("claude-opus-4-8".to_string(), upstream_price);
        upstream.insert("claude-sonnet".to_string(), upstream_price);

        let merged = merge_catalog(builtin, upstream);
        assert_eq!(merged["claude-opus-4-8"], protected);
        assert_eq!(merged["claude-sonnet"], upstream_price);
    }

    #[test]
    fn ttl_boundary_and_future_clock_are_handled() {
        let now = 2_000_000;
        assert!(is_fresh(now - SNAPSHOT_TTL.as_secs() + 1, now));
        assert!(!is_fresh(now - SNAPSHOT_TTL.as_secs(), now));
        assert!(is_fresh(now + 60, now), "小幅時鐘飄移可接受");
        assert!(!is_fresh(now + 600, now), "明顯未來時間不可凍結刷新");
    }

    #[test]
    fn model_name_normalization_tolerates_prefix_case_and_dates() {
        assert_eq!(
            normalize_model_name("ANTHROPIC/Claude-Sonnet-4-20250514"),
            "claude-sonnet-4"
        );
        assert_eq!(
            normalize_model_name("openai/gpt-5.5-2026-04-23"),
            "gpt-5.5"
        );
        assert_eq!(
            normalize_model_name("vertex_ai/claude-opus-4.8@20260401"),
            "claude-opus-4-8"
        );
        assert_eq!(
            normalize_model_name("google/gemini-2.5-pro"),
            "gemini-2.5-pro"
        );
    }

    #[test]
    fn broken_snapshot_falls_back_to_builtin() {
        let home = std::env::temp_dir().join(format!(
            "lp-pricing-bad-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let dir = home.join(".lobsterpulse");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pricing-catalog.json"), b"{broken").unwrap();

        let catalog = PricingCatalog::load(&home);
        assert_eq!(
            catalog.get("anthropic/claude-sonnet-4-20250514"),
            Some(ModelPrice::new(
                3.00,
                15.00,
                Some(3.75),
                Some(6.00),
                Some(0.30)
            ))
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn estimate_uses_input_output_and_both_cache_tiers() {
        let catalog = PricingCatalog {
            prices: fallback_prices(),
        };
        let usage = TokenUsage {
            input: 1_000_000,
            output: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write_1h: 1_000_000,
            cache_read: 1_000_000,
            cache_write_unknown: 0,
        };

        let cost = catalog
            .estimate_usd("claude-sonnet-4", usage)
            .expect("完整價目應可估算");
        assert!((cost - 28.05).abs() < 1e-9);

        assert!(
            catalog
                .estimate_usd(
                    "claude-sonnet-4",
                    TokenUsage {
                        cache_write_unknown: 1,
                        ..Default::default()
                    }
                )
                .is_none(),
            "未知 cache tier 不可硬套混合價"
        );
    }
}
