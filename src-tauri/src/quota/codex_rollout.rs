//! 從 `~/.codex/sessions/**/rollout-*.jsonl` 計算本地時區「今日」逐模型 token。
//!
//! Codex 的 token event type 曾變動，解析只認
//! `payload.info.total_token_usage` 的資料形狀。模型由最近一次
//! `turn_context.payload.model` 帶入；工具參數裡的 `model` 不採信。
//!
//! 整檔解析結果寫入版本化 cache。今天仍可能追加的檔案每輪都重解析；
//! 非今天的近日檔只在 cache hit 時採用。cache 損壞時 fail-open 重解析近日檔。

use crate::pricing::{PricingCatalog, TokenUsage};
use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CACHE_FORMAT: u32 = 1;
const PARSER_VERSION: &str = "codex-rollout-v1";
const RECENT_CACHE_DAYS: u64 = 2;

#[derive(Debug, Clone, Serialize)]
pub struct CodexModelUsage {
    pub name: String,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexRolloutDaily {
    pub date: String,
    pub models: Vec<CodexModelUsage>,
}

pub fn empty_today() -> CodexRolloutDaily {
    CodexRolloutDaily {
        date: Local::now().format("%Y-%m-%d").to_string(),
        models: Vec::new(),
    }
}

#[derive(Debug, Clone, Copy)]
struct RawCounters {
    input_tokens: u64,
    cached_input_tokens: u64,
    cache_write_input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

impl RawCounters {
    fn parse(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let input_tokens = object.get("input_tokens")?.as_u64()?;
        let cached_input_tokens = object.get("cached_input_tokens")?.as_u64()?;
        let output_tokens = object.get("output_tokens")?.as_u64()?;

        // 現機 rollout 使用 cache_write_input_tokens；舊版 agentacct carrier
        // 曾使用 cache_write_tokens，因此保留這個唯讀相容別名。
        let cache_write_input_tokens =
            if let Some(value) = object.get("cache_write_input_tokens") {
                value.as_u64()?
            } else if let Some(value) = object.get("cache_write_tokens") {
                value.as_u64()?
            } else {
                0
            };

        let total_tokens = if let Some(value) = object.get("total_tokens") {
            value.as_u64()?
        } else {
            input_tokens.checked_add(output_tokens)?
        };

        Some(Self {
            input_tokens,
            cached_input_tokens,
            cache_write_input_tokens,
            output_tokens,
            total_tokens,
        })
    }

    fn exceeds(self, other: Self) -> bool {
        self.input_tokens > other.input_tokens
            || self.cached_input_tokens > other.cached_input_tokens
            || self.cache_write_input_tokens > other.cache_write_input_tokens
            || self.output_tokens > other.output_tokens
            || self.total_tokens > other.total_tokens
    }

    fn checked_delta(self, previous: Self) -> Option<Self> {
        Some(Self {
            input_tokens: self.input_tokens.checked_sub(previous.input_tokens)?,
            cached_input_tokens: self
                .cached_input_tokens
                .checked_sub(previous.cached_input_tokens)?,
            cache_write_input_tokens: self
                .cache_write_input_tokens
                .checked_sub(previous.cache_write_input_tokens)?,
            output_tokens: self.output_tokens.checked_sub(previous.output_tokens)?,
            total_tokens: self.total_tokens.checked_sub(previous.total_tokens)?,
        })
    }

    fn normalized(self) -> UsageBuckets {
        // Codex input_tokens 包含 cache read/write；拆開後才能正確套不同價格。
        let cache_total = self
            .cached_input_tokens
            .saturating_add(self.cache_write_input_tokens);

        UsageBuckets {
            input_tokens: self.input_tokens.saturating_sub(cache_total),
            cached_input_tokens: self.cached_input_tokens,
            cache_write_input_tokens: self.cache_write_input_tokens,
            output_tokens: self.output_tokens,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
struct UsageBuckets {
    input_tokens: u64,
    cached_input_tokens: u64,
    cache_write_input_tokens: u64,
    output_tokens: u64,
}

impl UsageBuckets {
    fn add(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(other.cached_input_tokens);
        self.cache_write_input_tokens = self
            .cache_write_input_tokens
            .saturating_add(other.cache_write_input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
    }

    fn total_tokens(self) -> u64 {
        self.input_tokens
            .saturating_add(self.cached_input_tokens)
            .saturating_add(self.cache_write_input_tokens)
            .saturating_add(self.output_tokens)
    }

    fn pricing_usage(self) -> TokenUsage {
        TokenUsage {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_write_unknown: self.cache_write_input_tokens,
            cache_read: self.cached_input_tokens,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ParsedRollout {
    days: BTreeMap<String, BTreeMap<String, UsageBuckets>>,
    drift_segments: u64,
}

impl ParsedRollout {
    fn add(&mut self, date: String, model: String, usage: UsageBuckets) {
        self.days
            .entry(date)
            .or_default()
            .entry(model)
            .or_default()
            .add(usage);
    }
}

fn local_date(timestamp: &str) -> Option<String> {
    let when = DateTime::parse_from_rfc3339(timestamp).ok()?;
    Some(
        when
            .with_timezone(&Local)
            .format("%Y-%m-%d")
            .to_string(),
    )
}

fn parse_rollout_reader<R: BufRead>(reader: R) -> ParsedRollout {
    let mut parsed = ParsedRollout::default();
    let mut current_model: Option<String> = None;
    let mut previous_total: Option<RawCounters> = None;

    for line in reader.lines().map_while(Result::ok) {
        // 多數 rollout 行是訊息／工具輸出；先做便宜 byte 等價篩選。
        if !line.contains("\"model\"") && !line.contains("\"total_token_usage\"") {
            continue;
        }

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(payload) = value.get("payload").and_then(|v| v.as_object()) else {
            continue;
        };

        if value.get("type").and_then(|v| v.as_str()) == Some("turn_context") {
            if let Some(model) = payload
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
            {
                current_model = Some(model.to_string());
            }
        }

        // 防坑一：不認 event type 字串，只認 info.total_token_usage carrier。
        let Some(info) = payload.get("info").and_then(|v| v.as_object()) else {
            continue;
        };
        let Some(total_value) = info.get("total_token_usage") else {
            continue;
        };
        let Some(total) = RawCounters::parse(total_value) else {
            parsed.drift_segments = parsed.drift_segments.saturating_add(1);
            continue;
        };

        let last = match info.get("last_token_usage") {
            None => None,
            Some(value) => match RawCounters::parse(value) {
                Some(last) => Some(last),
                None => {
                    parsed.drift_segments = parsed.drift_segments.saturating_add(1);
                    previous_total = Some(total);
                    continue;
                }
            },
        };

        // 防坑三：last 不可能大於 cumulative total；累計本身倒退也視為
        // epoch/schema drift。本段不計，並以當前 total 重設下一段基準。
        if last.is_some_and(|last| last.exceeds(total))
            || previous_total.is_some_and(|previous| previous.exceeds(total))
        {
            parsed.drift_segments = parsed.drift_segments.saturating_add(1);
            previous_total = Some(total);
            continue;
        }

        let delta = if let Some(last) = last {
            last
        } else if let Some(previous) = previous_total {
            // 上方已排除倒退，checked_delta 只剩型別防線。
            let Some(delta) = total.checked_delta(previous) else {
                parsed.drift_segments = parsed.drift_segments.saturating_add(1);
                previous_total = Some(total);
                continue;
            };
            delta
        } else {
            total
        };
        previous_total = Some(total);

        let Some(date) = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(local_date)
        else {
            continue;
        };

        let model = payload
            .get("model")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .or_else(|| current_model.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // 防坑二在 normalized()：inclusive input 扣 cached read/write。
        parsed.add(date, model, delta.normalized());
    }

    parsed
}

fn parse_rollout_file(path: &Path) -> Option<ParsedRollout> {
    let file = std::fs::File::open(path).ok()?;
    Some(parse_rollout_reader(BufReader::new(file)))
}

#[derive(Debug, Serialize, Deserialize)]
struct ParseCache {
    format: u32,
    parser_version: String,
    entries: BTreeMap<String, ParsedRollout>,
}

fn empty_cache() -> ParseCache {
    ParseCache {
        format: CACHE_FORMAT,
        parser_version: PARSER_VERSION.to_string(),
        entries: BTreeMap::new(),
    }
}

/// 第二個回傳值代表 cache 是否健康；壞檔／讀取錯誤不出聲，讓 caller 重解析。
fn load_cache(path: &Path) -> (ParseCache, bool) {
    match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (empty_cache(), true),
        Err(_) => (empty_cache(), false),
        Ok(data) => match serde_json::from_slice::<ParseCache>(&data) {
            Ok(cache)
                if cache.format == CACHE_FORMAT && cache.parser_version == PARSER_VERSION =>
            {
                (cache, true)
            }
            _ => (empty_cache(), false),
        },
    }
}

fn save_cache(path: &Path, cache: &ParseCache) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(data) = serde_json::to_vec(cache) else {
        return;
    };

    // 共用 helper 在 Windows 使用唯一暫存檔＋ReplaceFileW／rename retry。
    let _ = crate::write_file_atomic_with_retry(path, &data);
}

fn cache_key(path: &Path, modified: SystemTime, size: u64) -> String {
    let mtime_ns = modified
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    format!(
        "{PARSER_VERSION}|{}|{mtime_ns}|{size}",
        path.to_string_lossy()
    )
}

struct RolloutFile {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
}

fn collect_rollout_files(dir: &Path, depth: u32, out: &mut Vec<RolloutFile>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();

        if file_type.is_dir() {
            collect_rollout_files(&path, depth + 1, out);
            continue;
        }
        if !file_type.is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("jsonl")
            || !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with("rollout-"))
        {
            continue;
        }

        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };

        out.push(RolloutFile {
            path,
            modified,
            size: metadata.len(),
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilePlan {
    Parse,
    Cached,
    Skip,
}

fn file_plan(
    modified: SystemTime,
    today_start: SystemTime,
    recent_floor: SystemTime,
    cache_hit: bool,
    cache_healthy: bool,
) -> FilePlan {
    if modified >= today_start {
        FilePlan::Parse
    } else if modified < recent_floor {
        FilePlan::Skip
    } else if cache_hit {
        FilePlan::Cached
    } else if !cache_healthy {
        // Cache 讀取／版本／解碼故障：fail-open，不讓 cache 破壞正確性。
        FilePlan::Parse
    } else {
        FilePlan::Skip
    }
}

fn local_day_start(now: DateTime<Local>) -> SystemTime {
    let seconds = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| Local.from_local_datetime(&naive).earliest())
        .map(|start| start.timestamp())
        .unwrap_or_else(|| now.timestamp());

    UNIX_EPOCH + Duration::from_secs(seconds.max(0) as u64)
}

fn merge_today(
    aggregate: &mut BTreeMap<String, UsageBuckets>,
    parsed: &ParsedRollout,
    today: &str,
) {
    let Some(models) = parsed.days.get(today) else {
        return;
    };

    for (model, usage) in models {
        aggregate.entry(model.clone()).or_default().add(*usage);
    }
}

pub fn collect_today(home: &Path) -> CodexRolloutDaily {
    let started = Instant::now();
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let today_start = local_day_start(now);
    let recent_floor = today_start
        .checked_sub(Duration::from_secs(RECENT_CACHE_DAYS * 86_400))
        .unwrap_or(UNIX_EPOCH);

    let sessions_root = home.join(".codex").join("sessions");
    if !sessions_root.is_dir() {
        log::debug!(
            "[codex_rollout] sessions 目錄不存在；耗時 {} ms",
            started.elapsed().as_millis()
        );
        return CodexRolloutDaily {
            date: today,
            models: Vec::new(),
        };
    }

    let cache_path = home
        .join(".lobsterpulse")
        .join("codex-rollout-cache.json");
    let (mut cache, cache_healthy) = load_cache(&cache_path);

    let mut files = Vec::new();
    collect_rollout_files(&sessions_root, 0, &mut files);
    files.sort_by(|left, right| left.path.cmp(&right.path));

    let mut aggregate = BTreeMap::new();
    let mut keep_keys = BTreeSet::new();
    let mut cache_dirty = !cache_healthy;
    let mut parsed_files = 0_u64;
    let mut cache_hits = 0_u64;
    let mut drift_segments = 0_u64;

    for file in files {
        let key = cache_key(&file.path, file.modified, file.size);
        if file.modified >= recent_floor {
            keep_keys.insert(key.clone());
        }

        let plan = file_plan(
            file.modified,
            today_start,
            recent_floor,
            cache.entries.contains_key(&key),
            cache_healthy,
        );

        let parsed = match plan {
            FilePlan::Cached => {
                cache_hits = cache_hits.saturating_add(1);
                cache.entries.get(&key).cloned()
            }
            FilePlan::Parse => {
                let result = parse_rollout_file(&file.path);
                if let Some(result) = result.as_ref() {
                    parsed_files = parsed_files.saturating_add(1);
                    if cache.entries.get(&key) != Some(result) {
                        cache.entries.insert(key, result.clone());
                        cache_dirty = true;
                    }
                }
                result
            }
            FilePlan::Skip => None,
        };

        if let Some(parsed) = parsed {
            drift_segments = drift_segments.saturating_add(parsed.drift_segments);
            merge_today(&mut aggregate, &parsed, &today);
        }
    }

    let before_prune = cache.entries.len();
    cache.entries.retain(|key, _| keep_keys.contains(key));
    cache_dirty |= cache.entries.len() != before_prune;

    if cache_dirty {
        save_cache(&cache_path, &cache);
    }

    let catalog = PricingCatalog::load(home);
    let mut models = aggregate
        .into_iter()
        .filter_map(|(name, usage)| {
            let total_tokens = usage.total_tokens();
            (total_tokens > 0).then(|| CodexModelUsage {
                name: name.clone(),
                input_tokens: usage.input_tokens,
                cached_input_tokens: usage.cached_input_tokens,
                cache_write_input_tokens: usage.cache_write_input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens,
                estimated_cost_usd: catalog.estimate_usd(&name, usage.pricing_usage()),
            })
        })
        .collect::<Vec<_>>();

    models.sort_by(|left, right| {
        right
            .total_tokens
            .cmp(&left.total_tokens)
            .then_with(|| left.name.cmp(&right.name))
    });

    log::debug!(
        "[codex_rollout] 今日逐模型掃描：解析 {} 檔、cache hit {} 檔、丟棄 {} drift 段、{} 模型；耗時 {} ms",
        parsed_files,
        cache_hits,
        drift_segments,
        models.len(),
        started.elapsed().as_millis()
    );

    CodexRolloutDaily {
        date: today,
        models,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse_fixture(fixture: &str) -> ParsedRollout {
        parse_rollout_reader(Cursor::new(fixture.as_bytes()))
    }

    fn fixture_day(timestamp: &str) -> String {
        local_date(timestamp).expect("fixture timestamp")
    }

    #[test]
    fn recognizes_total_usage_shape_instead_of_event_type() {
        let timestamp = "2026-07-30T12:00:00+08:00";
        let fixture = format!(
            r#"{{"timestamp":"{timestamp}","type":"turn_context","payload":{{"model":"gpt-5.6-sol"}}}}
{{"timestamp":"{timestamp}","type":"future_event_name","payload":{{"type":"not_token_count","info":{{"total_token_usage":{{"input_tokens":10,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":2,"total_tokens":12}},"last_token_usage":{{"input_tokens":10,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":2,"total_tokens":12}}}}}}}}
{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":"wrong shape"}}}}}}
"#
        );

        let parsed = parse_fixture(&fixture);
        let day = parsed.days.get(&fixture_day(timestamp)).expect("day");
        assert_eq!(day["gpt-5.6-sol"].total_tokens(), 12);
        assert_eq!(parsed.drift_segments, 1);
    }

    #[test]
    fn inclusive_input_subtracts_cached_and_cache_write_tokens() {
        let timestamp = "2026-07-30T12:00:00+08:00";
        let fixture = format!(
            r#"{{"timestamp":"{timestamp}","type":"turn_context","payload":{{"model":"gpt-5.6-terra"}}}}
{{"timestamp":"{timestamp}","type":"anything","payload":{{"info":{{"total_token_usage":{{"input_tokens":100,"cached_input_tokens":40,"cache_write_input_tokens":10,"output_tokens":20,"total_tokens":120}},"last_token_usage":{{"input_tokens":100,"cached_input_tokens":40,"cache_write_input_tokens":10,"output_tokens":20,"total_tokens":120}}}}}}}}
"#
        );

        let parsed = parse_fixture(&fixture);
        let usage = parsed.days[&fixture_day(timestamp)]["gpt-5.6-terra"];
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.cached_input_tokens, 40);
        assert_eq!(usage.cache_write_input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);
        assert_eq!(usage.total_tokens(), 120);
    }

    #[test]
    fn counter_drift_discards_only_the_bad_segments_and_rebaselines() {
        let timestamp = "2026-07-30T12:00:00+08:00";
        let fixture = format!(
            r#"{{"timestamp":"{timestamp}","type":"turn_context","payload":{{"model":"gpt-5.4"}}}}
{{"timestamp":"{timestamp}","type":"x","payload":{{"info":{{"total_token_usage":{{"input_tokens":100,"cached_input_tokens":0,"output_tokens":0,"total_tokens":100}},"last_token_usage":{{"input_tokens":100,"cached_input_tokens":0,"output_tokens":0,"total_tokens":100}}}}}}}}
{{"timestamp":"{timestamp}","type":"x","payload":{{"info":{{"total_token_usage":{{"input_tokens":150,"cached_input_tokens":0,"output_tokens":0,"total_tokens":150}},"last_token_usage":{{"input_tokens":160,"cached_input_tokens":0,"output_tokens":0,"total_tokens":160}}}}}}}}
{{"timestamp":"{timestamp}","type":"x","payload":{{"info":{{"total_token_usage":{{"input_tokens":120,"cached_input_tokens":0,"output_tokens":0,"total_tokens":120}},"last_token_usage":{{"input_tokens":20,"cached_input_tokens":0,"output_tokens":0,"total_tokens":20}}}}}}}}
{{"timestamp":"{timestamp}","type":"x","payload":{{"info":{{"total_token_usage":{{"input_tokens":130,"cached_input_tokens":0,"output_tokens":0,"total_tokens":130}},"last_token_usage":{{"input_tokens":10,"cached_input_tokens":0,"output_tokens":0,"total_tokens":10}}}}}}}}
"#
        );

        let parsed = parse_fixture(&fixture);
        let usage = parsed.days[&fixture_day(timestamp)]["gpt-5.4"];
        assert_eq!(usage.total_tokens(), 110);
        assert_eq!(parsed.drift_segments, 2);
    }

    #[test]
    fn cache_key_contains_path_mtime_size_and_parser_version() {
        let path = Path::new(r"C:\Users\tester\.codex\sessions\rollout-a.jsonl");
        let first = cache_key(path, UNIX_EPOCH + Duration::from_secs(10), 20);

        assert!(first.contains(PARSER_VERSION));
        assert!(first.contains(path.to_string_lossy().as_ref()));
        assert_ne!(
            first,
            cache_key(path, UNIX_EPOCH + Duration::from_secs(11), 20)
        );
        assert_ne!(
            first,
            cache_key(path, UNIX_EPOCH + Duration::from_secs(10), 21)
        );
        assert_ne!(
            first,
            cache_key(
                Path::new(r"C:\other\rollout-a.jsonl"),
                UNIX_EPOCH + Duration::from_secs(10),
                20
            )
        );
    }

    #[test]
    fn broken_cache_fails_open_and_recent_file_is_reparsed() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lp-codex-rollout-cache-{}-{nonce}.json",
            std::process::id()
        ));

        std::fs::write(&path, b"{broken").expect("write fixture");
        let (cache, healthy) = load_cache(&path);
        assert!(!healthy);
        assert!(cache.entries.is_empty());

        let today_start = UNIX_EPOCH + Duration::from_secs(500_000);
        let recent_floor = today_start - Duration::from_secs(172_800);
        assert_eq!(
            file_plan(
                today_start - Duration::from_secs(60),
                today_start,
                recent_floor,
                false,
                healthy
            ),
            FilePlan::Parse
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn today_filter_parses_fresh_files_and_only_uses_recent_cache_hits() {
        let today_start = UNIX_EPOCH + Duration::from_secs(500_000);
        let recent_floor = today_start - Duration::from_secs(172_800);

        assert_eq!(
            file_plan(today_start, today_start, recent_floor, true, true),
            FilePlan::Parse,
            "今日進行中的檔案即使 cache hit 也要重解析"
        );
        assert_eq!(
            file_plan(
                today_start - Duration::from_secs(60),
                today_start,
                recent_floor,
                true,
                true
            ),
            FilePlan::Cached
        );
        assert_eq!(
            file_plan(
                today_start - Duration::from_secs(60),
                today_start,
                recent_floor,
                false,
                true
            ),
            FilePlan::Skip
        );
        assert_eq!(
            file_plan(
                recent_floor - Duration::from_secs(1),
                today_start,
                recent_floor,
                true,
                true
            ),
            FilePlan::Skip
        );
    }
}
