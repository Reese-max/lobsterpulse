//! 用「累計計數器的當日差值」算各家 CLI 的今日用量。
//!
//! Claude 有 JSONL 可以逐筆加總（見 `claude_logs`），其他 CLI 沒有——它們的 API
//! 只回一個生涯累計數（例如 codex 的 `total_tokens_raw`）。把每天第一次與最近一次
//! 看到的累計值記下來，相減就是今日用量。
//!
//! 誠實邊界：app 沒開的期間不會被記到，所以這是**下界**（會低估、不會灌水）。
//! 呼叫端要照實標「app 記錄到的用量」，不可講成「今日總用量」。

use std::collections::BTreeMap;
use std::path::Path;

/// date → provider → (當日第一次看到的累計值, 最近一次看到的累計值)
type Daily = BTreeMap<String, BTreeMap<String, (u64, u64)>>;

/// 記錄一次觀測，回傳今日到目前為止的用量（last - first）。
/// 計數器倒退（換機器／對方重置）時把基準重設為新值並回 0，不產生天文數字。
pub fn record_cumulative(path: &Path, date: &str, provider: &str, value: u64) -> u64 {
    let mut map: Daily = match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            let bad = path.with_extension("json.bad");
            log::warn!(
                "[provider_daily] {} 解析失敗（{e}），保留為 {}，重新開始",
                path.display(),
                bad.display()
            );
            let _ = std::fs::rename(path, &bad);
            Default::default()
        }),
        Err(_) => Default::default(),
    };

    let entry = map
        .entry(date.to_string())
        .or_default()
        .entry(provider.to_string())
        .or_insert((value, value));
    if value < entry.0 {
        *entry = (value, value); // 計數器倒退 → 重設基準
    } else {
        entry.1 = value;
    }
    let used = entry.1 - entry.0;

    // 只留 30 天，檔案不會無限長大
    while map.len() > 30 {
        let oldest = map.keys().next().cloned().unwrap();
        map.remove(&oldest);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string(&map) {
        Ok(s) => {
            let tmp = path.with_extension("json.tmp");
            if let Err(e) = std::fs::write(&tmp, s) {
                log::warn!("[provider_daily] 寫入 {} 失敗: {e}", tmp.display());
            } else if let Err(e) = std::fs::rename(&tmp, path) {
                log::warn!("[provider_daily] rename 到 {} 失敗: {e}", path.display());
            }
        }
        Err(e) => log::warn!("[provider_daily] 序列化失敗: {e}"),
    }
    used
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lp-pd-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("provider-daily.json")
    }

    #[test]
    fn delta_is_from_first_observation_of_the_day() {
        let p = tmp_path("delta");
        let _ = std::fs::remove_file(&p);
        assert_eq!(record_cumulative(&p, "2026-07-21", "codex", 1000), 0, "第一次沒有基準，用量 0");
        assert_eq!(record_cumulative(&p, "2026-07-21", "codex", 1500), 500);
        assert_eq!(record_cumulative(&p, "2026-07-21", "codex", 1800), 800);
        // 新的一天重新起算，不受昨天基準影響
        assert_eq!(record_cumulative(&p, "2026-07-22", "codex", 1900), 0);
        assert_eq!(record_cumulative(&p, "2026-07-22", "codex", 2000), 100);
        // 不同 provider 各自獨立
        assert_eq!(record_cumulative(&p, "2026-07-22", "gemini", 50), 0);
        assert_eq!(record_cumulative(&p, "2026-07-22", "codex", 2100), 200);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn counter_reset_does_not_produce_garbage() {
        let p = tmp_path("reset");
        let _ = std::fs::remove_file(&p);
        record_cumulative(&p, "2026-07-21", "codex", 9000);
        record_cumulative(&p, "2026-07-21", "codex", 9500);
        // 對方把累計歸零（換帳號／重灌）→ 基準跟著重設，不可回報 -x 或天文數字
        assert_eq!(record_cumulative(&p, "2026-07-21", "codex", 10), 0);
        assert_eq!(record_cumulative(&p, "2026-07-21", "codex", 60), 50);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn prunes_to_thirty_days_and_survives_bad_file() {
        let p = tmp_path("prune");
        let _ = std::fs::remove_file(&p);
        for d in 1..=35 {
            record_cumulative(&p, &format!("2026-06-{d:02}"), "codex", d as u64 * 10);
        }
        let raw = std::fs::read_to_string(&p).unwrap();
        let map: Daily = serde_json::from_str(&raw).unwrap();
        assert_eq!(map.len(), 30, "只保留 30 天");
        assert!(!map.contains_key("2026-06-01"), "最舊的要被清掉");

        std::fs::write(&p, "{壞掉").unwrap();
        assert_eq!(record_cumulative(&p, "2026-07-21", "codex", 5), 0);
        assert!(p.with_extension("json.bad").exists(), "壞檔要保留成 .bad");
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }
}
