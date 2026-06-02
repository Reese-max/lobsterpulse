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
            // 對齊 R12 surface pattern：writeln 失敗要可觀察 + 不能算進 `written`。
            // 之前 `let _ = writeln!(f, ...)` 沉默吞 fs error，caller 看到 `Ok(N)`
            // 誤以為 N 列都 commit，實際磁碟滿/fd 斷時缺資料無 log。
            match write_csv_row(&mut f, now, name, pct) {
                Ok(()) => written += 1,
                Err(e) => log::warn!(
                    "[quota_history] write_csv_row failed (ts={now}, runner={name}, pct={pct}): \
                     {e} — quota-history.csv 該輪缺一筆"
                ),
            }
        }
    }
    Ok(written)
}

/// 寫單列 CSV row。抽成 pure fn 方便 unit test 鎖 format 與 IO error 傳播。
/// 失敗回 `io::Error` — caller 端決定 log policy 與計數是否要扣（snapshot_once
/// 故意不把失敗 row 算進 `written`，避免 caller 看到 `Ok(N)` 誤判）。
fn write_csv_row(f: &mut std::fs::File, ts: u64, name: &str, pct: u8) -> std::io::Result<()> {
    writeln!(f, "{ts},{name},{pct}")
}

/// 讀全部 history，回傳 (name → Vec<(ts, pct)>)。截掉超過 KEEP_DAYS 的紀錄。
pub fn load_history() -> Result<std::collections::HashMap<String, Vec<(u64, u8)>>, String> {
    let _io_guard = QUOTA_HISTORY_IO_LOCK
        .lock()
        .map_err(|_| "quota history lock poisoned".to_string())?;
    let Some(path) = csv_path() else {
        return Err("no path".into());
    };
    load_history_at(&path)
}

/// 從指定 path 取 quota-history.csv 的 `modified` mtime，給 Prometheus K21 gauge
/// `lobsterpulse_quota_history_csv_age_seconds` 用：operator 看「CSV 多久沒被
/// OpenAB 寫進來」，跟 K11 5 個 snapshot freshness 互補（K11 看「每個 bot 的
/// current usage snapshot 多舊」、K21 看「歷史聚合 CSV pipeline 多舊」）。
///
/// 對齊 R32 `load_history_at` first-run 契約 + R30 `load_local_usage_snapshot_at`
/// silent-fail surfacing 模式：
/// - NotFound（first-run 還沒建立 CSV）→ `Ok(None)`：不是 silent-fail，caller
///   端走「header only」契約、不出 sample line
/// - 其他 IO 錯（權限拒絕 / 磁碟鎖住）→ `Err(String)`：caller log warn
/// - OK → `Ok(Some(mtime))`：caller 走 `compute_quota_snapshot_age_seconds` 算 age
///
/// 抽成 pure fn 對齊 R28 `load_config_at` pattern：caller 端傳入 path（不一定
/// 是 `~/.lobsterpulse/quota-history.csv`，test 端可注入 tmpdir）。
pub fn quota_history_csv_mtime_at(path: &std::path::Path) -> Result<Option<SystemTime>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mtime = meta.modified().map_err(|e| e.to_string())?;
    Ok(Some(mtime))
}

/// 從指定 path 讀 quota-history，回傳「每個 runner 名稱的最新 (max-ts) pct」map。
/// 抽成 pure fn 對齊 K11 `compute_quota_snapshot_age_seconds` / K8 idle pattern：
///   - Prometheus `lobsterpulse_provider_quota_remaining_pct` gauge 直接吃這個 map
///   - missing entry（runner 沒 snapshot）→ caller 端不在 metric map 裡放 entry
///     → 對齊 K8 `last_event_at = None` 跳過策略：避免 Prometheus 把「沒看到」
///     當「0% quota 耗盡」誤判
///   - IO 錯 / load_history 失敗 → `Err(String)`（caller 端 log warn 對齊 R30
///     `get_quota_history` pattern，整個 metric map 留空，不部分 emit 假資料）
///   - 空檔 / 全是壞 row → `Ok(empty HashMap)`，對齊 first-run NotFound 契約
///
/// 實作直接走 `load_history_at`（不重複 IO / 解析邏輯），只負責 reduce：
/// 對每個 `(name, vec<(ts, pct)>)` 取 `vec.iter().max_by_key(|(ts, _)| ts)` 對應 pct。
pub fn latest_quota_pct_at(
    path: &std::path::Path,
) -> Result<std::collections::HashMap<String, u8>, String> {
    let history = load_history_at(path)?;
    let mut out: std::collections::HashMap<String, u8> = std::collections::HashMap::new();
    for (name, rows) in history {
        // rows 可能為空（load_history_at 已過濾 cutoff 跟壞 row，但保險檢查）
        if let Some((_ts, pct)) = rows.iter().max_by_key(|(ts, _)| *ts) {
            out.insert(name, *pct);
        }
    }
    Ok(out)
}

/// 從指定 path 讀 quota-history。抽成 pure fn 方便 unit test 鎖 contract：
/// - NotFound（檔不存在）→ `Ok(HashMap::new())`（first-run 預期，不算 silent-fail）
/// - IO 錯（權限拒絕 / 磁碟鎖住）→ `Err(String)`（caller 端要 log warn）
/// - 壞 CSV row → skip + log warn，繼續 parse 其他 row（既有 `parse_quota_history_row` 行為）
/// - 超過 `KEEP_DAYS` 30 天的 row → skip（既有的時間窗過濾）
///
/// 對齊 R28 `load_config_at` / R30 `load_local_usage_snapshot_at` pattern：
/// 公開 wrapper 只負責 path 解析 + lock，IO/parse 邏輯下沉到 `_at(path)` 純 fn
/// 讓 unit test 用 tempdir + 餵假檔案驗契約，不依賴真實 `~/.lobsterpulse/quota-history.csv`。
pub fn load_history_at(
    path: &std::path::Path,
) -> Result<std::collections::HashMap<String, Vec<(u64, u8)>>, String> {
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let cutoff = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .saturating_sub(KEEP_DAYS * 86400);
    let mut map: std::collections::HashMap<String, Vec<(u64, u8)>> =
        std::collections::HashMap::new();
    for (line_no, line) in data.lines().enumerate() {
        let parts: Vec<&str> = line.splitn(3, ',').collect();
        let Some((ts, pct)) = parse_quota_history_row(&parts, line_no + 1) else {
            continue;
        };
        if ts < cutoff {
            continue;
        }
        let name = parts[1].to_string();
        map.entry(name).or_default().push((ts, pct));
    }
    Ok(map)
}

/// 解析單列 quota-history CSV，回 `(ts, pct)` 或 `None`（skip + log warn）。
///
/// R29 silent-fail surfacing：原 `parts[0].parse().unwrap_or(0)` 跟
/// `parts[2].parse().unwrap_or(0)` 兩條鏈在 CSV 寫入半截（斷電 / OOM /
/// 手動編輯壞 row）時，會把整列靜默當成 `(ts=0, pct=0)` 推進 map：
/// - `ts=0` → 1970-01-01 變成「最舊」,可能過了 `cutoff` 過濾掉（純丟失）
///   或污染 history 圖（cutoff 寬鬆時）
/// - `pct=0` → 假的「quota 用完」資料點,後續 alert/graph 全誤判
///
/// 抽成 helper 收斂兩條路徑的 log policy（對齊 R13 `read_events_since`
/// 純函式風格），caller 端用 `?` 風格的 `Option` continue skip 該列。
/// schema 錯（`parts.len() != 3`）保持靜默 skip,跟原本 `continue` 行為一致。
fn parse_quota_history_row(parts: &[&str], line_no: usize) -> Option<(u64, u8)> {
    if parts.len() != 3 {
        return None;
    }
    let ts: u64 = match parts[0].parse() {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "[quota_history] load_history: row {line_no} ts parse failed \
                 (raw=\"{}\"): {e} — skip row",
                parts[0]
            );
            return None;
        }
    };
    let pct: u8 = match parts[2].parse() {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "[quota_history] load_history: row {line_no} pct parse failed \
                 (raw=\"{}\"): {e} — skip row",
                parts[2]
            );
            return None;
        }
    };
    Some((ts, pct))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn write_csv_row_writes_csv_line() {
        let dir = std::env::temp_dir().join(format!("lp-quota-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.csv");
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        write_csv_row(&mut f, 1_700_000_000, "cicx", 42).unwrap();
        write_csv_row(&mut f, 1_700_000_001, "openx", 7).unwrap();
        drop(f);

        let mut buf = String::new();
        File::open(&path).unwrap().read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "1700000000,cicx,42\n1700000001,openx,7\n");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn write_csv_row_returns_err_on_read_only_handle() {
        // 對齊 R20 silent-fail 改善的 contract：writeln 失敗必須傳出 Err，
        // caller 才能 log + 不算進 `written`。在 read-only handle 寫入 → io::Error。
        let dir = std::env::temp_dir().join(format!("lp-quota-history-ro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ro.csv");
        // 先建立檔案，才能以 read-only 開啟
        File::create(&path).unwrap();
        let mut ro = OpenOptions::new().read(true).open(&path).unwrap();
        let err = write_csv_row(&mut ro, 1_700_000_000, "cicx", 50).unwrap_err();
        // 不鎖特定 kind（不同 OS 回的 kind 不同：Windows BadFileDesc、Unix InvalidInput/Other），
        // 只要是 io::Error 就代表 caller 不會誤把這列算進 `written`。
        let k = err.kind();
        assert!(
            matches!(
                k,
                std::io::ErrorKind::InvalidInput
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::Other
            ),
            "read-only 檔寫入應回 io::Error，實際 kind={k:?}"
        );
        drop(ro);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn parse_quota_history_row_valid_returns_some() {
        // 對齊 R29 contract：3 段 row 合法 → Some((ts, pct))，
        // caller 端再依 cutoff 過濾。
        let parts = vec!["1700000000", "cicx", "42"];
        assert_eq!(
            parse_quota_history_row(&parts, 1),
            Some((1_700_000_000, 42))
        );
    }

    #[test]
    fn parse_quota_history_row_wrong_schema_returns_none_silently() {
        // 對齊原本 load_history 行為：parts.len() != 3 靜默 skip（first-run 預期
        // + 編輯壞 row 不需 log spam）。不測 log 是因為 schema 錯不是 silent-fail
        // surfacing 的目標——純 keep 既有行為。
        let too_few = vec!["1700000000", "cicx"];
        assert_eq!(parse_quota_history_row(&too_few, 1), None);
        let empty: Vec<&str> = vec![];
        assert_eq!(parse_quota_history_row(&empty, 1), None);
    }

    #[test]
    fn parse_quota_history_row_ts_parse_fail_returns_none() {
        // R29 silent-fail surfacing 核心：ts 壞掉必須 log warn + skip，
        // 不能 unwrap_or(0) 變成「1970-01-01 假資料」污染 map。
        let parts = vec!["not-a-number", "cicx", "42"];
        assert_eq!(parse_quota_history_row(&parts, 7), None);
    }

    #[test]
    fn parse_quota_history_row_pct_parse_fail_returns_none() {
        // pct 壞掉（不是數字）必須 log warn + skip，
        // 不能 unwrap_or(0) 變成「假的 quota 用完 0%」誤判 alert/graph。
        let parts = vec!["1700000000", "cicx", "abc"];
        assert_eq!(parse_quota_history_row(&parts, 3), None);
    }

    #[test]
    fn parse_quota_history_row_pct_overflow_returns_none() {
        // pct 超出 u8 範圍（>255）必須 skip，不能 saturate 變成 255 假資料。
        let parts = vec!["1700000000", "cicx", "999"];
        assert_eq!(parse_quota_history_row(&parts, 5), None);
    }

    #[test]
    fn parse_quota_history_row_empty_pct_field_returns_none() {
        // 空字串（寫入半截 CSV row 結尾 `"ts,name,\n"`）也視為 parse 失敗 → skip。
        // 對齊 write 半截場景：disk full / 斷電留下 `"1700000000,cicx,\n"`。
        let parts = vec!["1700000000", "cicx", ""];
        assert_eq!(parse_quota_history_row(&parts, 9), None);
    }

    /// 為 load_history_at 系列 test 製造獨立 tmp CSV 路徑（避免污染真實
    /// `~/.lobsterpulse/quota-history.csv` + parallel test 互踩）。
    fn tmp_csv(tag: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("lp-load-history-{tag}-{nonce}.csv"));
        p
    }

    /// R32 contract：first-run 情境（quota-history.csv 還沒建立）必須回 `Ok(empty)`，
    /// 讓 caller（`get_quota_history` Dashboard / `!lp trend` cmd / `token_spike` rule）
    /// 不會誤觸 silent-fail warn 路徑。
    #[test]
    fn load_history_at_not_found_returns_ok_empty() {
        let path = tmp_csv("notfound");
        // 確保檔案不存在
        let _ = std::fs::remove_file(&path);
        let r = load_history_at(&path);
        assert!(
            matches!(r, Ok(ref m) if m.is_empty()),
            "NotFound 應回 Ok(empty HashMap)，實際: {r:?}"
        );
    }

    /// R32 contract：合法 CSV（含 2 runner 各 1 row）應正確 parse，欄位對應到 (ts, pct)。
    /// ts 用 dynamic `now() - N` 而非寫死 1700000000：寫死 2023-11 會被 `KEEP_DAYS=30`
    /// cutoff 過濾掉，map 變空（測試 fail），用 dynamic ts 才能穩定在 30 天窗內。
    #[test]
    fn load_history_at_valid_csv_parses_rows() {
        let path = tmp_csv("valid");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ts1 = now - 100;
        let ts2 = now - 200;
        let body = format!("{ts1},cicx,42\n{ts2},openx,7\n");
        std::fs::write(&path, body).expect("write csv");
        let r = load_history_at(&path).expect("valid csv 應回 Ok");
        assert_eq!(r.len(), 2, "2 runner 應都進 map，實際: {r:?}");
        assert_eq!(r.get("cicx").map(|v| v.as_slice()), Some(&[(ts1, 42)][..]));
        assert_eq!(r.get("openx").map(|v| v.as_slice()), Some(&[(ts2, 7)][..]));
        let _ = std::fs::remove_file(&path);
    }

    /// R32 contract：0-byte 檔（disk full / 寫入中斷常見殘留）應回 `Ok(empty)`，
    /// 對齊 first-run NotFound 契約。
    ///
    /// `parse_quota_history_row` 對壞 row 採「log warn + skip」策略、不會讓整檔
    /// 變 Err，所以「全檔都是壞 row」也是 `Ok(empty)` 帶 log warn。真正的 IO 錯
    /// （如磁碟鎖、目錄）才會回 Err（見 `load_history_at_io_error_returns_err`）。
    #[test]
    fn load_history_at_empty_file_returns_ok_empty() {
        let path = tmp_csv("empty");
        std::fs::write(&path, b"").expect("write empty");
        let r = load_history_at(&path);
        assert!(
            matches!(r, Ok(ref m) if m.is_empty()),
            "0-byte 檔應回 Ok(empty)，實際: {r:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// R32 contract：IO 錯（檔案存在但 read 失敗）必須回 `Err`，
    /// 對齊 R30 `load_local_usage_snapshot_at_io_error_returns_err` pattern。
    /// **不能** silent 當 NotFound 處理（會讓 `get_quota_history` Dashboard sparkline
    /// 在「壞檔」時渲染空白卻無 log）。
    ///
    /// 觸發 IO 錯策略：用「path 是目錄（不是檔案）」— `std::fs::read_to_string` 對
    /// 目錄 path 在 Unix/Windows 都會回 IO 錯（Not a directory / Access denied），
    /// 而 `path.exists()` 對目錄回 true → 不會被 NotFound 短路。
    /// 比 R30 用的 NUL path 更可靠：NUL path 在 Windows 上 `path.exists()` 視為 false
    /// 走 NotFound 分支（`load_history_at` 開頭有 `if !path.exists() { return Ok(empty) }`
    /// 短路），改用目錄 path 才能確定觸發 read 階段的 IO 錯。
    #[test]
    fn load_history_at_io_error_returns_err() {
        // `std::env::temp_dir()` 一定存在 + 一定是目錄
        let dir_path = std::env::temp_dir();
        let r = load_history_at(&dir_path);
        assert!(
            r.is_err(),
            "目錄 path 應回 Err（read_to_string 對目錄 IO 失敗）讓 caller log warn，實際: {r:?}"
        );
    }

    // ===== K20 latest_quota_pct_at tests =====

    /// K20 contract：NotFound（first-run 還沒建立 quota-history.csv）必須回 `Ok(empty)`，
    /// 對齊 R32 `load_history_at_not_found_returns_ok_empty` 契約 → render 端可以
    /// 安全地「不輸出 sample line」而不是誤判「0% quota 耗盡」。
    #[test]
    fn latest_quota_pct_at_not_found_returns_ok_empty() {
        let path = tmp_csv("latest-notfound");
        let _ = std::fs::remove_file(&path);
        let r = latest_quota_pct_at(&path);
        assert!(
            matches!(r, Ok(ref m) if m.is_empty()),
            "NotFound 應回 Ok(empty HashMap)，實際: {r:?}"
        );
    }

    /// K20 contract：每個 runner 只回「max-ts 那筆的 pct」（不是平均、不是 sum、不是首筆）。
    /// 故意插入 cicx 三筆（ts=now-300/now-100/now-200）→ 預期輸出 cicx→42（now-100 那筆）。
    /// openx 一筆（ts=now-50）→ 預期 7。對齊 R30 `get_quota_history` Dashboard 給前端
    /// sparkline 用「最新一點」的語意，operator alert `quota_remaining_pct < 10`
    /// 也只關心「現在還剩多少」。
    #[test]
    fn latest_quota_pct_at_picks_max_ts_per_name() {
        let path = tmp_csv("latest-max-ts");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ts_old = now - 300;
        let ts_mid = now - 200;
        let ts_new = now - 100;
        let ts_openx = now - 50;
        // 故意非時間序：ts_old 先寫、ts_mid 第二、ts_new 第三
        let body =
            format!("{ts_old},cicx,10\n{ts_mid},cicx,20\n{ts_new},cicx,42\n{ts_openx},openx,7\n");
        std::fs::write(&path, body).expect("write csv");
        let r = latest_quota_pct_at(&path).expect("valid csv 應回 Ok");
        assert_eq!(r.len(), 2, "2 runner 應都進 map，實際: {r:?}");
        assert_eq!(r.get("cicx"), Some(&42), "cicx 應挑 max-ts 那筆 42");
        assert_eq!(r.get("openx"), Some(&7), "openx 唯一一筆 7");
        let _ = std::fs::remove_file(&path);
    }

    /// K20 contract：超 KEEP_DAYS 30 天的舊 row 必須被 cutoff 過濾掉（不走 reduce），
    /// 對齊 R32 `load_history_at` 既有的時間窗過濾 → map 留空（first-run 等價）。
    /// 對齊 R32 同一 dynamic ts 策略（寫死 2023-11 會被 cutoff 過濾掉 → 測試 fail）。
    #[test]
    fn latest_quota_pct_at_skips_rows_outside_keep_window() {
        let path = tmp_csv("latest-cutoff");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // KEEP_DAYS=30 → 31 天前 = now - 31*86400，應被過濾
        let ts_too_old = now - 31 * 86400;
        let body = format!("{ts_too_old},cicx,99\n");
        std::fs::write(&path, body).expect("write csv");
        let r = latest_quota_pct_at(&path).expect("valid csv 應回 Ok");
        assert!(
            r.is_empty(),
            "31 天前的 row 應被 cutoff 過濾，map 應為空，實際: {r:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// K20 contract：0% quota 是有效資料（runner quota 已耗盡 → operator 必須看到），
    /// 不能在 reduce 階段當作 None 跳過。對齊 R32 `load_history_at` 把 0 視為合法 u8。
    #[test]
    fn latest_quota_pct_at_zero_pct_is_emitted_not_dropped() {
        let path = tmp_csv("latest-zero");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ts = now - 100;
        let body = format!("{ts},cicx,0\n");
        std::fs::write(&path, body).expect("write csv");
        let r = latest_quota_pct_at(&path).expect("valid csv 應回 Ok");
        assert_eq!(
            r.get("cicx"),
            Some(&0),
            "0% 是有效 quota 耗盡訊號，必須保留，實際: {r:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// K20 contract：IO 錯（目錄 path → read_to_string 失敗）必須傳出 `Err`，
    /// 對齊 R32 `load_history_at_io_error_returns_err` + R30 `load_local_usage_snapshot_at_io_error_returns_err`
    /// 同一 pattern：caller 端才能 log warn，整個 metric map 留空，不部分 emit 假資料。
    #[test]
    fn latest_quota_pct_at_io_error_returns_err() {
        let dir_path = std::env::temp_dir();
        let r = latest_quota_pct_at(&dir_path);
        assert!(
            r.is_err(),
            "目錄 path 應回 Err（load_history_at 內部 read 失敗）讓 caller log warn，實際: {r:?}"
        );
    }

    // ============== K21 quota_history_csv_mtime_at helper ==============
    // 對齊 K11 freshness 視角 + K20 同一資料源（quota-history.csv）。
    // 2 個 test 覆蓋:NotFound → Ok(None) first-run 契約 / 存在 → Ok(Some(mtime))。
    // IO 錯 path (`Err` variant) 在 portable Rust 難以穩定觸發（`metadata()` 在
    // 目錄上也成功,不像 `read_to_string` 會因 EISDIR 失敗）,契約保留在 helper doc
    // 端跟 R30/R32 pattern 一致。資料源 consumer 在 `lib.rs::render_prometheus` 端。

    #[test]
    fn quota_history_csv_mtime_at_returns_none_when_file_missing() {
        // first-run 契約：CSV 還沒被 OpenAB 寫過 → caller 端不出 sample line，
        // 避免把「檔案根本不存在」誤判成「剛剛 mtime=now → age=0」。
        let dir = tempfile_dir();
        let missing = dir.join("nope-quota-history.csv");
        let r = quota_history_csv_mtime_at(&missing);
        assert!(
            matches!(r, Ok(None)),
            "missing file 應回 Ok(None)（first-run 契約，非 silent-fail），實際: {r:?}"
        );
    }

    #[test]
    fn quota_history_csv_mtime_at_returns_some_for_existing_file() {
        // 正常情況：write 完檔案 → mtime 應 Some 且 ≤ now。容忍極小 clock skew，
        // 不直接 assert 等於 now，鎖住「最近寫過」語意即可。
        let dir = tempfile_dir();
        let path = dir.join("quota-history.csv");
        std::fs::write(&path, b"1700000000,cicx,42\n").expect("write csv");
        let r = quota_history_csv_mtime_at(&path);
        match r {
            Ok(Some(mtime)) => {
                let elapsed = SystemTime::now().duration_since(mtime).unwrap_or_default();
                // mtime 應該在過去（剛寫完），不該是未來；容許 60s clock skew 防偶發
                assert!(
                    elapsed.as_secs() <= 60,
                    "剛寫的 CSV mtime 應 ≤ now 60s 容差，實際 elapsed={}s",
                    elapsed.as_secs()
                );
            }
            other => panic!("existing file 應回 Ok(Some(mtime))，實際: {other:?}"),
        }
    }

    /// K21 test 共用 helper：建一個 tmpdir 給 unit test 用，跑完 OS 自動回收。
    /// 對齊 R32 + R35 既有 tmpdir 風格（不引入 `tempfile` crate，零依賴）。
    fn tempfile_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("lobsterpulse-k21-{pid}-{n}"));
        std::fs::create_dir_all(&dir).expect("create tmpdir");
        dir
    }
}
