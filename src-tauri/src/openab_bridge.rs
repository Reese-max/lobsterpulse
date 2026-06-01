//! OpenAB event bridge consumer —— 讀 `~/openab/logs/cctest-events.jsonl` 的新事件，
//! 轉成 Discord embed 讓 LPBOT 對外統一發聲（LP 作為 singular speaker）。
//!
//! 上游：`bots-watchdog.sh` 以 `LP_SPEAKER=1` 模式跑時，只寫 jsonl 不發 Discord。
//! 下游：LP tick 每 15s 呼叫 `consume_new_events`，以 file offset 避免重複發。

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const OFFSET_FILE_NAME: &str = ".openab-bridge-offset";

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
pub fn tail_new_events() -> Vec<serde_json::Value> {
    let Some(path) = events_path() else {
        return vec![];
    };
    if !path.exists() {
        return vec![];
    }
    let Ok(meta) = std::fs::metadata(&path) else {
        return vec![];
    };
    let file_size = meta.len();
    let offset = read_offset();

    // 首次啟動（offset=0）或檔案被 rotate（size 變小）→ 跳到 EOF 不吃歷史
    if offset == 0 || file_size < offset {
        write_offset(file_size);
        return vec![];
    }
    if file_size == offset {
        return vec![];
    }

    // 讀從 offset 到 EOF（避免每輪把整個檔案讀進記憶體）
    let Ok(mut file) = std::fs::File::open(&path) else {
        return vec![];
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return vec![];
    }
    let mut chunk = Vec::new();
    if file.read_to_end(&mut chunk).is_err() {
        return vec![];
    }
    // 只處理「完整行」：若最後一行尚未寫完（無 '\n'），保留到下輪，避免掉事件。
    let Some(last_nl) = chunk.iter().rposition(|b| *b == b'\n') else {
        return vec![];
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
    write_offset(new_offset);
    out
}

/// 把 event 轉成 Discord embed 發出去。目前支援 source="bots-watchdog" event="state_change"。
pub fn dispatch_event(
    event: &serde_json::Value,
    discord_token: &str,
    discord_channel: &str,
) -> Result<String, String> {
    let source = event.get("source").and_then(|x| x.as_str()).unwrap_or("");
    let kind = event.get("event").and_then(|x| x.as_str()).unwrap_or("");
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
