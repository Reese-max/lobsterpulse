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
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let yesterday = (now - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    // mtime cutoff 放寬到 2 天前：跨時區/時鐘偏移下仍抓得到昨天尾巴的檔
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 86400);

    let mut files = Vec::new();
    recent_jsonl(&home.join(".claude").join("projects"), cutoff, 0, &mut files);

    let mut acc = std::collections::HashMap::new();
    for p in &files {
        scan_tail(p, &yesterday, &mut acc);
    }
    (
        acc.get(&today).copied().unwrap_or(0),
        acc.get(&yesterday).copied().unwrap_or(0),
        today,
    )
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
