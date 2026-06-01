//! 每小時 snapshot usage-local.json → ~/.lobsterpulse/quota-history.csv
//! Schema: `timestamp_secs,runner_name,remaining_pct`
//! 保留最近 30 天，讀取時切掉超舊列。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP_DAYS: u64 = 30;
static QUOTA_HISTORY_IO_LOCK: Mutex<()> = Mutex::new(());

fn csv_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".lobsterpulse").join("quota-history.csv"))
}

/// 掃 usage-local.json 裡每個 runner 的剩餘 %（用同一個 extract_min_percent 邏輯），
/// 附加一行到 CSV。返回 Ok(N) = 寫了 N 列。
pub fn snapshot_once() -> Result<usize, String> {
    let _io_guard = QUOTA_HISTORY_IO_LOCK
        .lock()
        .map_err(|_| "quota history lock poisoned".to_string())?;
    let Some(home) = dirs::home_dir() else {
        return Err("no home dir".into());
    };
    let src = home.join(".lobsterpulse").join("usage-local.json");
    let data = std::fs::read_to_string(&src).map_err(|e| format!("read {}: {e}", src.display()))?;
    let v: serde_json::Value = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    let Some(runners) = v.get("runners").and_then(|x| x.as_array()) else {
        return Ok(0);
    };

    let Some(path) = csv_path() else {
        return Err("no path".into());
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open csv: {e}"))?;
    let mut written = 0;
    for r in runners {
        let name = r.get("name").and_then(|x| x.as_str()).unwrap_or("?");
        let ok = r.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        if !ok {
            continue;
        }
        let text = r.get("text").and_then(|x| x.as_str()).unwrap_or("");
        if let Some(pct) = extract_min_percent(text) {
            let _ = writeln!(f, "{now},{name},{pct}");
            written += 1;
        }
    }
    Ok(written)
}

/// 讀全部 history，回傳 (name → Vec<(ts, pct)>)。截掉超過 KEEP_DAYS 的紀錄。
pub fn load_history() -> Result<std::collections::HashMap<String, Vec<(u64, u8)>>, String> {
    let _io_guard = QUOTA_HISTORY_IO_LOCK
        .lock()
        .map_err(|_| "quota history lock poisoned".to_string())?;
    let Some(path) = csv_path() else {
        return Err("no path".into());
    };
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let cutoff = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .saturating_sub(KEEP_DAYS * 86400);
    let mut map: std::collections::HashMap<String, Vec<(u64, u8)>> =
        std::collections::HashMap::new();
    for line in data.lines() {
        let parts: Vec<&str> = line.splitn(3, ',').collect();
        if parts.len() != 3 {
            continue;
        }
        let ts: u64 = parts[0].parse().unwrap_or(0);
        if ts < cutoff {
            continue;
        }
        let name = parts[1].to_string();
        let pct: u8 = parts[2].parse().unwrap_or(0);
        map.entry(name).or_default().push((ts, pct));
    }
    Ok(map)
}

/// 從 text 抓所有 `N%`（含小數 48.3%）回傳最小值 u8。
fn extract_min_percent(text: &str) -> Option<u8> {
    let mut min: Option<u8> = None;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i > 0 {
            let mut j = i;
            // 往前收 digits + 最多一個 '.'（支援 48.3%）
            while j > 0 {
                let c = bytes[j - 1];
                if c.is_ascii_digit() || c == b'.' {
                    j -= 1;
                } else {
                    break;
                }
            }
            if j < i {
                if j > 0 && bytes[j - 1] == b'-' {
                    i += 1;
                    continue;
                }
                if let Ok(f) = std::str::from_utf8(&bytes[j..i])
                    .unwrap_or("0")
                    .parse::<f64>()
                {
                    if f.is_finite() && (0.0..=100.0).contains(&f) {
                        let n = f.round() as u8;
                        min = Some(min.map(|m| m.min(n)).unwrap_or(n));
                    }
                }
            }
        }
        i += 1;
    }
    min
}
