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

fn write_offset(pos: u64) {
    if let Some(p) = offset_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let data = pos.to_string();
        let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("offset");
        let tmp = p.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
        if std::fs::write(&tmp, data.as_bytes()).is_ok() {
            if std::fs::rename(&tmp, &p).is_err() {
                let _ = std::fs::remove_file(&tmp);
                let _ = std::fs::write(&p, data.as_bytes());
            }
        } else {
            let _ = std::fs::write(&p, data.as_bytes());
        }
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
