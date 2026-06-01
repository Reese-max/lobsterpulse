//! OpenAB event bridge consumer —— 讀 `~/openab/logs/cctest-events.jsonl` 的新事件，
//! 轉成 Discord embed 讓 LPBOT 對外統一發聲（LP 作為 singular speaker）。
//!
//! 上游：`bots-watchdog.sh` 以 `LP_SPEAKER=1` 模式跑時，只寫 jsonl 不發 Discord。
//! 下游：LP tick 每 15s 呼叫 `consume_new_events`，以 file offset 避免重複發。

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const OFFSET_FILE_NAME: &str = ".openab-bridge-offset";

/// R13：`tail_new_events` 內 fs 操作的錯誤分類。caller 端靠 `Display` 落 log，
/// 4 個 variant 各對應一條原 silent fail 路徑，方便 log filter 一次定位。
#[derive(Debug)]
enum ReadEventsError {
    Metadata(std::io::Error),
    Open(std::io::Error),
    Seek(std::io::Error),
    Read(std::io::Error),
}

impl std::fmt::Display for ReadEventsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metadata(e) => write!(f, "metadata failed: {e}"),
            Self::Open(e) => write!(f, "file open failed: {e}"),
            Self::Seek(e) => write!(f, "seek to offset failed: {e}"),
            Self::Read(e) => write!(f, "read_to_end failed: {e}"),
        }
    }
}

fn events_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join("openab").join("logs").join("cctest-events.jsonl"))
}

fn offset_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".lobsterpulse").join(OFFSET_FILE_NAME))
}

fn read_offset() -> u64 {
    offset_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// 寫入 offset 到指定路徑（採用「先 tmp 後 rename」原子寫；rename 失敗 fallback 直接寫）。
/// 失敗回 `io::Error` — caller 必須 log warning，因 offset 追蹤壞掉會導致下次重複發同批
/// OpenAB event，operator 無 log 就分不清「無新事件」 vs 「offset 寫失敗」。
///
/// **不** surface 的一條：`remove_file(&tmp)` 清理是 best-effort，下輪 tmp 名稱帶 PID 會換新，
/// 留著 stale tmp 不會擋寫入。
fn write_offset_at(path: &std::path::Path, pos: u64) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = pos.to_string();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("offset");
    let tmp = path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
    if std::fs::write(&tmp, data.as_bytes()).is_ok() {
        if std::fs::rename(&tmp, path).is_err() {
            // rename 失敗（Windows target 被 hold / 跨 device）→ best-effort 清理 tmp 後直接寫
            let _ = std::fs::remove_file(&tmp);
            std::fs::write(path, data.as_bytes())?;
        }
    } else {
        // tmp 寫失敗（磁碟滿 / 權限）→ 直接寫 final
        std::fs::write(path, data.as_bytes())?;
    }
    Ok(())
}

fn write_offset(pos: u64) {
    let Some(p) = offset_path() else {
        return;
    };
    if let Err(e) = write_offset_at(&p, pos) {
        // 對齊 R6/R8 surface pattern：offset tracking 失敗要可觀察，否則 next tick
        // 會重複發同批 event，user 端分不清「OpenAB 沒新事件」vs「我們自己 offset 寫失敗」。
        log::warn!(
            "[openab_bridge] write_offset({}) failed: {} — offset tracking broken, \
             may reprocess events next tick",
            pos,
            e
        );
    }
}

/// 讀 jsonl 從 offset 到 EOF，回傳 (events, new_offset)。
/// 新安裝（offset=0 + file 很大）會一口氣吃掉所有歷史 event —— 避免此情況，
/// 第一次碰到的狀況直接跳到 EOF，標記成「從現在開始追」。
///
/// R13：把 fs + parse 抽到 `read_events_since` 為 pure fn，讓 caller 端
/// `tail_new_events` 用 `log::warn!` surfaced 5 條原 silent fail 路徑
/// （metadata / file open / seek / read_to_end，events_path()→None）。原先每條
/// 都用 `let Ok(...) = ... else { return vec![]; }` 沉默吞 error，operator
/// 看到的現象是「OpenAB 橋接停了」但 log 完全沒線索區分「OpenAB 沒新事件」
/// vs「我們 fs 讀失敗」。`!path.exists()` 視為 first-run expected 走 log::debug。
pub fn tail_new_events() -> Vec<serde_json::Value> {
    let Some(path) = events_path() else {
        log::warn!(
            "[openab_bridge] tail_new_events: events_path() returned None \
             (home_dir missing?) — OpenAB 橋接整輪停擺"
        );
        return vec![];
    };
    if !path.exists() {
        log::debug!(
            "[openab_bridge] tail_new_events: events file not found at {} \
             (first-run or OpenAB not installed yet)",
            path.display()
        );
        return vec![];
    }
    let offset = read_offset();
    match read_events_since(&path, offset) {
        Ok((events, new_offset)) => {
            if new_offset != offset {
                write_offset(new_offset);
            }
            events
        }
        Err(e) => {
            log::warn!(
                "[openab_bridge] tail_new_events failed at offset={}: {} — \
                 next tick will re-read from same offset, may reprocess events",
                offset,
                e
            );
            vec![]
        }
    }
}

/// Pure fn：給定 `events_path` + `offset`，回 (events, new_offset)。
/// - `offset=0` 或 `file_size < offset`（rotate）→ 跳到 EOF，回 (vec![], file_size)
/// - `file_size == offset` → 沒新事件，回 (vec![], offset)
/// - 讀到 `chunk` 但無 newline（partial line）→ 保留到下輪，回 (vec![], offset)
/// - 任何 fs 操作失敗 → 對應 variant 的 `ReadEventsError`
///
/// 抽這條出來是為了 unit test 鎖行為（happy / rotate / partial line / metadata fail），
/// 並把 4 條原 silent fail 集中到一個 enum，caller 端 1 個 `match` 統一 log。
fn read_events_since(
    events_path: &std::path::Path,
    offset: u64,
) -> Result<(Vec<serde_json::Value>, u64), ReadEventsError> {
    let meta = std::fs::metadata(events_path).map_err(ReadEventsError::Metadata)?;
    let file_size = meta.len();

    // 首次啟動（offset=0）或檔案被 rotate（size 變小）→ 跳到 EOF 不吃歷史
    if offset == 0 || file_size < offset {
        return Ok((vec![], file_size));
    }
    if file_size == offset {
        return Ok((vec![], offset));
    }

    let mut file = std::fs::File::open(events_path).map_err(ReadEventsError::Open)?;
    file.seek(SeekFrom::Start(offset))
        .map_err(ReadEventsError::Seek)?;

    let mut chunk = Vec::new();
    file.read_to_end(&mut chunk)
        .map_err(ReadEventsError::Read)?;

    // 只處理「完整行」：若最後一行尚未寫完（無 '\n'），保留到下輪，避免掉事件。
    let Some(last_nl) = chunk.iter().rposition(|b| *b == b'\n') else {
        return Ok((vec![], offset));
    };
    let complete = &chunk[..=last_nl];
    let text = String::from_utf8_lossy(complete);
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
            out.push(v);
        }
    }
    let new_offset = offset.saturating_add((last_nl + 1) as u64);
    Ok((out, new_offset))
}

/// 把 event 轉成 Discord embed 發出去。目前支援 source="bots-watchdog" event="state_change"。
pub fn dispatch_event(
    event: &serde_json::Value,
    discord_token: &str,
    discord_channel: &str,
) -> Result<String, String> {
    let source = event.get("source").and_then(|x| x.as_str()).unwrap_or("?");
    let kind = event.get("event").and_then(|x| x.as_str()).unwrap_or("?");
    match (source, kind) {
        ("bots-watchdog", "state_change") => {
            let changes = event.get("changes").and_then(|x| x.as_str()).unwrap_or("?");
            let summary = event.get("summary").and_then(|x| x.as_str()).unwrap_or("");
            let title = "OpenAB · 狀態變化".to_string();
            let desc = format!("**變化** {changes}\n**現狀** {summary}");
            crate::discord::send_embed(discord_token, discord_channel, &title, &desc, 0x5865F2)
        }
        _ => Err(format!("unknown event {source}/{kind}")),
    }
}

#[cfg(test)]
mod write_offset_at_tests {
    //! R12 regression：`write_offset` 之前 3 條 `let _ =` 沉默吞 fs error（create_dir_all、
    //! rename fallback write、tmp fallback write），offset 追蹤壞掉時 operator 完全無
    //! log 可查。改 `write_offset_at(path, pos) -> io::Result<()>` 後 caller 端 `if let Err`
    //! 統一 log warning。本 module 鎖「寫入值正確」「parent dir 自動建立」「invalid path
    //! 回 Err」三條契約。

    use super::*;

    /// 為每個 test 製造獨立 tmp 路徑（避免 parallel test 互踩 / 污染 home dir）。
    /// 用 nanos 當 nonce，比 R4 的 SystemTime-based nonce 更不會撞（同進程內連呼叫）。
    fn tmp_path(tag: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("lp-offset-{tag}-{nonce}-{}", std::process::id()));
        p
    }

    #[test]
    fn write_offset_at_writes_value_atomically() {
        let path = tmp_path("happy");
        write_offset_at(&path, 12345).expect("happy path should succeed");
        let raw = std::fs::read_to_string(&path).expect("file should exist after write");
        assert_eq!(raw, "12345", "offset 應原樣寫入檔案");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_offset_at_creates_parent_dir_on_demand() {
        let mut path = tmp_path("parent");
        path.pop(); // 拿掉檔名留下 dir parent
        let nested = path.join("sub").join("nested").join("offset");
        // 故意不先建 dir → 測 create_dir_all 自動建立
        write_offset_at(&nested, 99).expect("nested write should auto-create parents");
        let raw = std::fs::read_to_string(&nested).expect("file should exist after write");
        assert_eq!(raw, "99");
        // cleanup：nested + 中間空目錄
        let _ = std::fs::remove_file(&nested);
        let _ = std::fs::remove_dir(nested.parent().unwrap());
    }

    #[test]
    fn write_offset_at_returns_err_on_invalid_path() {
        // Windows / Unix 都會拒絕 NUL 裝置或不可寫的 path
        // 用 control char（U+0001）做檔名 → 大多 fs 拒絕
        let bad = std::path::PathBuf::from("\x01invalid\x02");
        let result = write_offset_at(&bad, 1);
        assert!(result.is_err(), "invalid path 應回 Err 而不是 silent fail");
    }

    #[test]
    fn write_offset_overwrites_existing_value() {
        let path = tmp_path("overwrite");
        write_offset_at(&path, 100).expect("first write should succeed");
        write_offset_at(&path, 200).expect("second write should succeed");
        let raw = std::fs::read_to_string(&path).expect("file should exist after overwrite");
        assert_eq!(raw, "200", "第二次寫入應覆蓋而非 append");
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod read_events_since_tests {
    //! R13 regression：`tail_new_events` 之前 5 條 `let Ok(...) = ... else { return vec![]; }`
    //! 沉默吞 fs error（events_path()→None、metadata、file open、seek、read_to_end），
    //! OpenAB 橋接 fs 路徑失敗時 operator 完全無 log 可查。改 `read_events_since(path, offset)
    //! -> Result<(events, new_offset), ReadEventsError>` 為 pure fn，4 個 variant 對應 4 條
    //! fs 失敗點，caller 端 `tail_new_events` 用 `match` 統一 log warning。
    //!
    //! 本 module 鎖 7 條契約：
    //! 1. happy path：offset 設在 line 1 之後，回剩餘 event + 正確 new_offset
    //! 2. offset=0 → first-launch 跳到 EOF
    //! 3. offset==file_size → 沒新事件
    //! 4. offset>file_size → rotate 跳到 EOF
    //! 5. partial line 保留到下輪
    //! 6. 不存在路徑 → ReadEventsError::Metadata
    //! 7. 4 個 variant 的 Display 都帶 fs error reason（log grep 用）

    use super::*;
    use std::io::Write;

    /// 為每個 test 製造獨立 tmp jsonl，避免 parallel test 互踩。
    fn tmp_jsonl(tag: &str, body: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("lp-events-{tag}-{nonce}-{}", std::process::id()));
        let mut f = std::fs::File::create(&p).expect("tmp file create");
        f.write_all(body.as_bytes()).expect("tmp file write");
        p
    }

    #[test]
    fn read_events_since_happy_path_returns_events_and_new_offset() {
        // 3 條 newline-terminated jsonl + offset 設在 line 1 結尾後 → 預期讀到剩 2 條
        let body = "{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n";
        let path = tmp_jsonl("happy", body);
        let line1_end = body.find('\n').unwrap() as u64 + 1; // 第一條 newline 之後
        let (events, new_offset) = read_events_since(&path, line1_end).expect("happy path 應成功");
        assert_eq!(events.len(), 2, "offset 在 line 1 後應讀到剩 2 條");
        assert_eq!(events[0]["b"], 2);
        assert_eq!(events[1]["c"], 3);
        assert_eq!(new_offset, body.len() as u64, "new_offset 應指到 EOF");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_events_since_offset_zero_jumps_to_eof() {
        let body = "{\"x\":1}\n{\"y\":2}\n";
        let path = tmp_jsonl("zero", body);
        let (events, new_offset) = read_events_since(&path, 0).expect("first-launch 應回 Ok");
        assert!(events.is_empty(), "offset=0 不應吃歷史");
        assert_eq!(new_offset, body.len() as u64, "應跳到 EOF");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_events_since_offset_equals_size_returns_empty() {
        let body = "{\"k\":1}\n";
        let path = tmp_jsonl("eqsize", body);
        let (events, new_offset) =
            read_events_since(&path, body.len() as u64).expect("無新事件應回 Ok");
        assert!(events.is_empty());
        assert_eq!(new_offset, body.len() as u64);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_events_since_offset_greater_than_size_jumps_to_eof() {
        // 模擬 rotate：原本 offset 500，現 file size 變 50
        let body = "{\"r\":1}\n";
        let path = tmp_jsonl("rotate", body);
        let (events, new_offset) = read_events_since(&path, 500).expect("rotate 應回 Ok 不報錯");
        assert!(events.is_empty(), "rotate 不應吃舊資料");
        assert_eq!(
            new_offset,
            body.len() as u64,
            "rotate 應把 offset 推到新 EOF"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_events_since_partial_line_dropped_for_next_tick() {
        // 1 完整行 + 1 個 partial（沒 newline）。offset 設在 line 1 結尾後
        // 走真正 read loop：partial line 不算完整行 → events 為空，offset 不動
        // 留給下輪 tail 抓。
        let body = "{\"p\":1}\n{\"q\":2"; // 第二條無 \n
        let path = tmp_jsonl("partial", body);
        let line1_end = body.find('\n').unwrap() as u64 + 1;
        let (events, new_offset) = read_events_since(&path, line1_end).expect("partial 應回 Ok");
        assert!(events.is_empty(), "partial line 不算完整行");
        assert_eq!(new_offset, line1_end, "partial 時 offset 不動，留給下輪");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_events_since_metadata_error_for_missing_file() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("lp-events-missing-{nonce}-{}", std::process::id()));
        // 不建立檔案
        let err = read_events_since(&p, 0).expect_err("不存在路徑應回 Err");
        assert!(
            matches!(err, ReadEventsError::Metadata(_)),
            "不存在的 path 應觸發 Metadata variant，實際：{err:?}"
        );
        // Display 應帶 "metadata failed" prefix 給 log filter grep
        let msg = err.to_string();
        assert!(
            msg.starts_with("metadata failed:"),
            "Display 應含 'metadata failed:' prefix，實際：{msg}"
        );
    }

    #[test]
    fn read_events_error_display_covers_all_four_variants() {
        // 鎖 4 個 variant 的 Display prefix 穩定，方便 log filter
        // io::Error 沒 Copy/Clone，每個 case 各自構造一次
        let make_err = || -> std::io::Error {
            std::fs::File::open("/nonexistent/lp-test-does-not-exist")
                .expect_err("non-existent path 應有 io::Error")
        };
        let cases: Vec<(ReadEventsError, &str)> = vec![
            (ReadEventsError::Metadata(make_err()), "metadata failed:"),
            (ReadEventsError::Open(make_err()), "file open failed:"),
            (ReadEventsError::Seek(make_err()), "seek to offset failed:"),
            (ReadEventsError::Read(make_err()), "read_to_end failed:"),
        ];
        for (err, expected_prefix) in cases {
            let msg = err.to_string();
            assert!(
                msg.starts_with(expected_prefix),
                "Display prefix 應為 {expected_prefix:?}，實際：{msg}"
            );
        }
    }
}

#[cfg(test)]
mod dispatch_event_tests {
    //! R12 regression：`dispatch_event` 之前只被 `lib.rs:1393` 以 `let _ =` 呼叫吞 error，
    //! R12 改成 `if let Err(e) = ... { log::warn!(source=…, event=…, e) }` 後，
    //! unknown source/kind 必須回 Err（這樣 caller 才會走 log 警告分支而不是 silent skip）。
    //!
    //! 本 module 鎖 3 條契約：
    //! 1. known pair (bots-watchdog, state_change) 不在單元測試範圍（會觸發真 Discord HTTP call）
    //! 2. unknown source / kind → 回 Err 且訊息含 source + kind（給 log 端 grep）
    //! 3. missing source / kind 欄位 → 走 "?" fallback，回 Err 同樣含 fallback 標記

    use super::*;

    #[test]
    fn dispatch_event_returns_err_for_unknown_source_kind() {
        let ev = serde_json::json!({
            "source": "totally-bogus",
            "event": "nonsense",
        });
        let result = dispatch_event(&ev, "fake-token", "fake-channel");
        assert!(
            result.is_err(),
            "unknown source/kind 必須回 Err 給 caller log"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("totally-bogus") && msg.contains("nonsense"),
            "Err 訊息應含 source + kind 供 log 端 grep 定位，實際：{msg}"
        );
    }

    #[test]
    fn dispatch_event_falls_back_to_question_mark_for_missing_fields() {
        // 沒有 source / event 欄位 → 用 "?" fallback → 仍走 unknown branch 回 Err
        let ev = serde_json::json!({});
        let result = dispatch_event(&ev, "fake-token", "fake-channel");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("?"),
            "missing 欄位 fallback 應為 '?'，實際：{msg}"
        );
    }

    #[test]
    fn dispatch_event_handles_known_kind_with_bogus_inner_shape() {
        // source/kind 是 known pair，但 inner 缺欄位 → 走 known branch 呼叫 discord::send_embed
        // 會因 token 假而回 Err（curl 401 格式）。本測試只驗「不會 panic 且有 Err 路徑」，
        // 不驗 Discord API 行為。
        let ev = serde_json::json!({
            "source": "bots-watchdog",
            "event": "state_change",
            "changes": "x→y",
            "summary": "test",
        });
        let result = dispatch_event(&ev, "definitely-not-a-real-token", "0");
        // 不論 Discord 是 401 / 4xx / network fail，反正 Err 就對
        assert!(
            result.is_err(),
            "fake token 應觸發 Discord 失敗、caller 可 log"
        );
    }
}
