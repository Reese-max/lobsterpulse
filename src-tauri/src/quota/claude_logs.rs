//! 從 `~/.claude/projects/**/*.jsonl` 直接算「今天／昨天」token 用量。
//!
//! 為什麼要自己算：`~/.claude/stats-cache.json` 由 Claude Code 自己維護，實測
//! 會停更數月（2026-07-20 查到停在 2026-04-10，101 天），ccusage 走 CLI 掃全部
//! 11800 檔會 timeout（>120s）。原始 JSONL 一直是活的，只是沒人彙總。
//!
//! 怎麼做到便宜：
//! 1. mtime 過濾——只有近兩天寫過的檔才可能有今天的資料（11800 → ~320 檔）
//! 2. 從檔尾往回讀 1MB 區塊，讀到「時間戳早於昨天」就停——只讀新增那段
//!    （實測實讀 180MB，Python 原型 10s，Rust 走 bytes 前置過濾快數倍）
//! 3. 沒有 `"usage"` 字樣的行不進 JSON parser（絕大多數行是 user/tool 訊息）
//!
//! 日期歸屬用**本地時區**：JSONL 時間戳是 UTC，UTC+8 每天有 8 小時會跨日，
//! 直接比字串會把凌晨的用量算到前一天。

use chrono::{DateTime, Local, Utc};
use std::path::{Path, PathBuf};

/// 單行事件抽出的 (本地日期, tokens)。行不含 usage / 壞 JSON / 無時間戳 → None。
/// 抽成純函式方便測試：這是整個掃描唯一的語意判斷點。
pub fn parse_usage_line(line: &[u8]) -> Option<(String, u64)> {
    if !contains_usage(line) {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(line).ok()?;
    let ts = v.get("timestamp")?.as_str()?;
    let when: DateTime<Utc> = ts.parse::<DateTime<Utc>>().ok()?;
    let date = when.with_timezone(&Local).format("%Y-%m-%d").to_string();
    let u = v.get("message")?.get("usage")?;
    let f = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    let total = f("input_tokens")
        + f("output_tokens")
        + f("cache_creation_input_tokens")
        + f("cache_read_input_tokens");
    if total == 0 {
        return None;
    }
    Some((date, total))
}

/// 便宜的前置過濾：memchr 級別的 byte 搜尋，避免對每行都跑 serde。
fn contains_usage(line: &[u8]) -> bool {
    line.windows(7).any(|w| w == b"\"usage\"")
}

/// 遞迴列出 `.jsonl`，只留 mtime >= cutoff 的檔（深度上限防連結成環）。
fn recent_jsonl(dir: &Path, cutoff: std::time::SystemTime, depth: u32, out: &mut Vec<PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let p = e.path();
        if ft.is_dir() {
            recent_jsonl(&p, cutoff, depth + 1, out);
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            let fresh = e
                .metadata()
                .and_then(|m| m.modified())
                .map(|m| m >= cutoff)
                .unwrap_or(false);
            if fresh {
                out.push(p);
            }
        }
    }
}

/// 從檔尾往回掃，累加 `>= stop_date` 的行；讀到更早的日期即停（append-only 假設）。
/// 回傳 (日期 → tokens) 增量，寫進 acc。
fn scan_tail(path: &Path, stop_date: &str, acc: &mut std::collections::HashMap<String, u64>) {
    use std::io::{Read, Seek, SeekFrom};
    const CHUNK: u64 = 1 << 20;
    let Ok(mut f) = std::fs::File::open(path) else {
        return;
    };
    let Ok(size) = f.metadata().map(|m| m.len()) else {
        return;
    };
    let mut pos = size;
    let mut tail: Vec<u8> = Vec::new(); // 被區塊邊界切半的第一行，留給下一輪
    while pos > 0 {
        let step = CHUNK.min(pos);
        pos -= step;
        if f.seek(SeekFrom::Start(pos)).is_err() {
            return;
        }
        let mut buf = vec![0u8; step as usize];
        if f.read_exact(&mut buf).is_err() {
            return;
        }
        buf.extend_from_slice(&tail);
        let mut parts: Vec<&[u8]> = buf.split(|&b| b == b'\n').collect();
        // 第一段可能不是完整行（除非已讀到檔頭）
        let first = if pos > 0 { parts.remove(0) } else { &[][..] };
        // 單行亂序（subagent 事件非同步落盤）不該讓整檔提早截斷 → 本區塊全部
        // 處理完，確認「這 1MB 內沒有任何一行還在範圍內」才停止本檔。
        let mut in_range = false;
        let mut saw_line = false; // 本區塊有無任何 usage 行（全是雜訊時不可判定越過 cutoff）
        for line in parts.iter().rev() {
            let Some((date, tokens)) = parse_usage_line(line) else {
                continue;
            };
            saw_line = true;
            if date.as_str() < stop_date {
                continue;
            }
            in_range = true;
            *acc.entry(date).or_insert(0) += tokens;
        }
        if saw_line && !in_range {
            return; // 整個區塊都比 cutoff 舊，本檔剩下的只會更舊
        }
        tail = first.to_vec();
    }
}

/// 掃出「今天／昨天」token 總量（本地時區）＋掃描當下所認定的今天日期。
/// 日期必須跟著資料一起回傳：跨午夜後、下一輪掃描完成前，若呼叫端自己用
/// `Local::now()` 重算，會把「昨天算出來的今天」誤標成新一天的今天（審查抓到）。
/// 呼叫端負責放到背景執行緒——實測掃描約 1~3 秒。
pub fn scan_today_yesterday(home: &Path) -> (u64, u64, String) {
    let (acc, today) = scan_recent_days(home, 2);
    let yesterday = prev_day(&today);
    (
        acc.get(&today).copied().unwrap_or(0),
        acc.get(&yesterday).copied().unwrap_or(0),
        today,
    )
}

/// 由日期字串反推前一天。**不可**改用 `Local::now() - 1day`：掃描要 1~3 秒，
/// 跨午夜時「掃描開始的今天」與「掃描結束後重算的昨天」會變成同一天，
/// 後寫的昨天值會蓋掉剛算好的今天值（審查抓到的午夜競態）。
pub fn prev_day(date: &str) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| (d - chrono::Duration::days(1)).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| date.to_string())
}

/// 掃最近 `days` 天（含今天）的每日 token，回傳 (日期 → tokens, 掃描當下的今天)。
/// `days` 越大讀得越多：2 天實讀約 340MB、30 天約 2.9GB（≈ 整個語料庫）。
pub fn scan_recent_days(
    home: &Path,
    days: i64,
) -> (std::collections::HashMap<String, u64>, String) {
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let stop_date = (now - chrono::Duration::days(days - 1))
        .format("%Y-%m-%d")
        .to_string();
    // mtime cutoff 多放一天：跨時區/時鐘偏移下仍抓得到範圍尾巴的檔
    let cutoff =
        std::time::SystemTime::now() - std::time::Duration::from_secs(days as u64 * 86400 + 86400);

    let mut files = Vec::new();
    recent_jsonl(&home.join(".claude").join("projects"), cutoff, 0, &mut files);

    let mut acc = std::collections::HashMap::new();
    for p in &files {
        scan_tail(p, &stop_date, &mut acc);
    }
    (acc, today)
}

/// 每日 token 累積檔（date → tokens）。每輪掃描把今天/昨天寫進去，天數自然累積。
///
/// 為什麼不直接掃 30 天：實測 30 天＝整個語料庫 2.9GB／10679 檔／**361 秒**，
/// 而 2 天只要 0.97 秒（`scan_30d_cost` ignored test 可複驗）。既然每輪都算出
/// 今天/昨天了，存起來比重掃便宜四個數量級。代價：只能從安裝日往後累積，
/// 前端要照實標「已記錄 N 天」，不可假裝是完整 30 天。
pub fn merge_daily(
    path: &Path,
    days: &[(String, u64)],
    keep_days: i64,
) -> std::collections::BTreeMap<String, u64> {
    // 壞 JSON 不可靜默當成空 map——後面會把空 map 寫回去，等於無聲清空累積歷史。
    // 先留痕（log + 改名保留原檔），再從空的重來，資料至少查得到。
    let mut map: std::collections::BTreeMap<String, u64> = match std::fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str(&s) {
            Ok(m) => m,
            Err(e) => {
                let bad = path.with_extension("json.bad");
                log::warn!(
                    "[claude_logs] {} 解析失敗（{e}），保留為 {}，累積重新開始。開頭: {:.80}",
                    path.display(),
                    bad.display(),
                    s
                );
                let _ = std::fs::rename(path, &bad);
                Default::default()
            }
        },
        Err(_) => Default::default(), // 檔案不存在＝第一次跑，正常
    };
    for (d, tok) in days {
        // 覆蓋而非累加：同一天會被掃很多輪，每次都是該天的最新總量
        map.insert(d.clone(), *tok);
    }
    let floor = (Local::now() - chrono::Duration::days(keep_days))
        .format("%Y-%m-%d")
        .to_string();
    map.retain(|d, _| d.as_str() >= floor.as_str());
    // 寫入失敗要出聲：目錄不在／被防毒鎖住時，功能會「看起來活著但永遠不落地」
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string(&map) {
        Ok(s) => {
            let tmp = path.with_extension("json.tmp");
            if let Err(e) = std::fs::write(&tmp, s) {
                log::warn!("[claude_logs] 寫入 {} 失敗: {e}", tmp.display());
            } else if let Err(e) = std::fs::rename(&tmp, path) {
                log::warn!("[claude_logs] rename 到 {} 失敗: {e}", path.display());
            }
        }
        Err(e) => log::warn!("[claude_logs] 序列化每日累積失敗: {e}"),
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_usage_line_sums_all_token_kinds_in_local_date() {
        // 2026-07-20T00:30:00Z 在 UTC+8 是 2026-07-20 08:30（同日）；
        // 2026-07-19T17:00:00Z 在 UTC+8 是 2026-07-20 01:00（跨到隔天）——
        // 直接比 UTC 字串會算錯日，這條護住時區換算。
        let line = br#"{"timestamp":"2026-07-19T17:00:00Z","message":{"usage":
            {"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":3,
             "cache_read_input_tokens":4}}}"#;
        let (date, tokens) = parse_usage_line(line).expect("應解析成功");
        assert_eq!(tokens, 10, "四種 token 都要加總");
        let expect = DateTime::parse_from_rfc3339("2026-07-19T17:00:00Z")
            .unwrap()
            .with_timezone(&Local)
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(date, expect, "日期須換算成本地時區");
    }

    #[test]
    fn parse_usage_line_skips_noise() {
        assert!(parse_usage_line(b"").is_none());
        assert!(parse_usage_line(br#"{"type":"user","timestamp":"2026-07-20T00:00:00Z"}"#).is_none());
        assert!(parse_usage_line(b"{not json but has \"usage\"}").is_none());
        // usage 全 0（例如空回合）不計入
        assert!(
            parse_usage_line(
                br#"{"timestamp":"2026-07-20T00:00:00Z","message":{"usage":{"input_tokens":0}}}"#
            )
            .is_none()
        );
    }

    #[test]
    fn scan_tail_stops_at_cutoff_and_sums_per_date() {
        let dir = std::env::temp_dir().join(format!("lp-claudelogs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        // 用本地時區產生跨日資料，避免測試在不同 TZ 下漂移
        let now = Local::now();
        let mk = |d: chrono::DateTime<Local>, tok: u64| {
            format!(
                r#"{{"timestamp":"{}","message":{{"usage":{{"output_tokens":{}}}}}}}"#,
                d.with_timezone(&Utc).to_rfc3339(),
                tok
            )
        };
        let body = [
            mk(now - chrono::Duration::days(5), 999), // 應被 cutoff 擋下
            mk(now - chrono::Duration::days(1), 20),
            mk(now, 30),
            mk(now, 40),
        ]
        .join("\n");
        std::fs::write(&path, body).unwrap();

        let today = now.format("%Y-%m-%d").to_string();
        let yest = (now - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
        let mut acc = std::collections::HashMap::new();
        scan_tail(&path, &yest, &mut acc);
        assert_eq!(acc.get(&today).copied().unwrap_or(0), 70, "今天兩筆相加");
        assert_eq!(acc.get(&yest).copied().unwrap_or(0), 20);
        assert!(!acc.contains_key("1970-01-01"));
        assert_eq!(acc.len(), 2, "cutoff 之前的行不得混入");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 午夜競態：掃描期間跨日時，若昨天用 `Local::now()` 重算會等於 date 本身，
    /// merge_daily 後寫的昨天值就會蓋掉剛算好的今天值。
    #[test]
    fn prev_day_is_derived_from_date_not_clock() {
        assert_eq!(prev_day("2026-07-21"), "2026-07-20");
        assert_eq!(prev_day("2026-01-01"), "2025-12-31", "跨年");
        assert_eq!(prev_day("2024-03-01"), "2024-02-29", "閏年");
        assert_ne!(prev_day("2026-07-21"), "2026-07-21", "絕不可等於自己");
        assert_eq!(prev_day("壞資料"), "壞資料", "壞輸入不 panic");
    }

    #[test]
    fn merge_daily_keeps_bad_file_instead_of_silently_wiping() {
        let dir = std::env::temp_dir().join(format!("lp-daily-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("daily.json");
        std::fs::write(&path, "{壞掉的 JSON").unwrap();

        let now = Local::now().format("%Y-%m-%d").to_string();
        let m = merge_daily(&path, &[(now.clone(), 7)], 30);
        assert_eq!(m.get(&now), Some(&7), "壞檔不該擋住新資料");
        assert!(
            path.with_extension("json.bad").exists(),
            "壞檔要保留成 .bad，不可無聲蒸發"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_daily_creates_missing_parent_dir() {
        let dir = std::env::temp_dir().join(format!("lp-daily-mkdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("daily.json");
        let now = Local::now().format("%Y-%m-%d").to_string();
        merge_daily(&path, &[(now, 3)], 30);
        assert!(path.exists(), "父目錄不存在時要自己建，不能靜默不落地");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_daily_overwrites_today_and_prunes_old() {
        let dir = std::env::temp_dir().join(format!("lp-daily-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("daily.json");
        let now = Local::now();
        let day = |n: i64| (now - chrono::Duration::days(n)).format("%Y-%m-%d").to_string();

        let m = merge_daily(&path, &[(day(0), 10), (day(1), 20)], 30);
        assert_eq!(m.get(&day(0)), Some(&10));

        // 同一天再掃一次是覆蓋，不是累加（否則每 5 分鐘就把今天翻倍）
        let m = merge_daily(&path, &[(day(0), 55)], 30);
        assert_eq!(m.get(&day(0)), Some(&55));
        assert_eq!(m.get(&day(1)), Some(&20), "上一輪的昨天要留著");

        // 超過保留天數的舊資料要清掉，檔案不會無限長大
        let m = merge_daily(&path, &[(day(90), 999)], 30);
        assert!(!m.contains_key(&day(90)));
        assert_eq!(m.len(), 2);

        // 落地後重讀應等值（跨重啟累積的前提）
        let reread = merge_daily(&path, &[], 30);
        assert_eq!(reread, m);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 量測用（非斷言）：30 天掃描實際要多久，決定背景週期。
    /// `cargo test -p lobster-pulse --lib -- --ignored --nocapture scan_30d_cost`
    #[test]
    #[ignore]
    fn scan_30d_cost() {
        let home = dirs::home_dir().expect("home");
        for days in [2i64, 30] {
            let t = std::time::Instant::now();
            let (acc, today) = scan_recent_days(&home, days);
            let sum: u64 = acc.values().sum();
            println!(
                "days={days} elapsed={:?} dates={} sum={sum} today={}",
                t.elapsed(),
                acc.len(),
                acc.get(&today).copied().unwrap_or(0)
            );
        }
    }

    /// subagent 事件非同步落盤會造成單行亂序——中間插一筆很舊的行，
    /// 不得讓掃描提早截斷、把它前面（仍在範圍內）的行整批漏算。
    #[test]
    fn scan_tail_tolerates_single_out_of_order_line() {
        let dir = std::env::temp_dir().join(format!("lp-claudelogs-ooo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let now = Local::now();
        let mk = |d: chrono::DateTime<Local>, tok: u64| {
            format!(
                r#"{{"timestamp":"{}","message":{{"usage":{{"output_tokens":{}}}}}}}"#,
                d.with_timezone(&Utc).to_rfc3339(),
                tok
            )
        };
        let body = [
            mk(now, 10),
            mk(now - chrono::Duration::days(9), 999), // 亂序的舊行夾在中間
            mk(now, 5),
        ]
        .join("\n");
        std::fs::write(&path, body).unwrap();

        let today = now.format("%Y-%m-%d").to_string();
        let yest = (now - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
        let mut acc = std::collections::HashMap::new();
        scan_tail(&path, &yest, &mut acc);
        assert_eq!(
            acc.get(&today).copied().unwrap_or(0),
            15,
            "亂序舊行之前的合法行不得被截斷漏算"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
