//! runner 健康狀態持久化。
//!
//! quota／usage snapshot 回答「抓到了什麼資料」；本檔只回答 runner 是否成功、
//! 最後何時成功與失敗原因。兩者分開，失敗不得偽裝成活動資料。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

static RUNNER_HEALTH_IO_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunnerHealthState {
    pub degraded: bool,
    pub runners: BTreeMap<String, RunnerHealthEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunnerHealthEntry {
    pub last_success_ts: Option<u64>,
    pub last_error_ts: Option<u64>,
    pub last_error: Option<String>,
    pub consecutive_failures: u64,
}

fn degraded_state() -> RunnerHealthState {
    RunnerHealthState {
        degraded: true,
        runners: BTreeMap::new(),
    }
}

/// 缺檔代表尚無健康資料；壞檔或 IO 錯誤投影成 degraded，不把錯誤拋給 UI。
pub fn read_at(path: &Path) -> RunnerHealthState {
    match std::fs::read(path) {
        Ok(raw) => serde_json::from_slice(&raw).unwrap_or_else(|_| degraded_state()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => RunnerHealthState::default(),
        Err(_) => degraded_state(),
    }
}

/// 一輪結果只做一次 read-modify-write，避免 local/live 同時更新時互相覆蓋。
pub fn record_results_at(
    path: &Path,
    results: &[(String, bool, String)],
    now: u64,
) -> Result<(), String> {
    let _guard = RUNNER_HEALTH_IO_LOCK
        .lock()
        .map_err(|_| "runner health lock poisoned".to_string())?;

    let mut state = read_at(path);
    state.degraded = false;

    for (name, ok, text) in results {
        let entry = state.runners.entry(name.to_string()).or_default();
        if *ok {
            entry.last_success_ts = Some(now);
            entry.consecutive_failures = 0;
        } else {
            entry.last_error_ts = Some(now);
            entry.last_error = Some(text.chars().take(120).collect());
            entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        }
    }

    let parent = path
        .parent()
        .ok_or_else(|| "runner health path has no parent".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("create runner health dir: {e}"))?;
    let data = serde_json::to_vec_pretty(&state)
        .map_err(|e| format!("serialize runner health: {e}"))?;
    crate::write_file_atomic_with_retry(path, &data)
        .map_err(|e| format!("write runner health atomically: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "lp-runner-health-{tag}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_replaces_existing_state() {
        let dir = temp_dir("atomic");
        let path = dir.join("runner-health.json");

        record_results_at(
            &path,
            &[("live:claude".into(), true, "ok".into())],
            10,
        )
        .unwrap();
        record_results_at(
            &path,
            &[("live:claude".into(), false, "timeout".into())],
            20,
        )
        .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let state: RunnerHealthState = serde_json::from_str(&raw).unwrap();
        let entry = &state.runners["live:claude"];
        assert_eq!(entry.last_success_ts, Some(10));
        assert_eq!(entry.last_error_ts, Some(20));
        assert_eq!(entry.consecutive_failures, 1);
        assert!(
            std::fs::read_dir(&dir).unwrap().flatten().all(|item| {
                !item
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".runner-health.json.tmp-")
            }),
            "原子替換後不得殘留暫存檔"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_projects_degraded_unknown() {
        let dir = temp_dir("corrupt");
        let path = dir.join("runner-health.json");
        std::fs::write(&path, b"{broken").unwrap();

        let state = read_at(&path);
        assert!(state.degraded);
        assert!(state.runners.is_empty(), "壞檔時所有 runner 狀態應視為未知");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn consecutive_failures_increment_and_reset_on_success() {
        let dir = temp_dir("failures");
        let path = dir.join("runner-health.json");
        let long_error = "錯".repeat(130);

        record_results_at(
            &path,
            &[("local:codex".into(), false, long_error)],
            10,
        )
        .unwrap();
        let first = read_at(&path);
        assert_eq!(
            first.runners["local:codex"]
                .last_error
                .as_deref()
                .unwrap()
                .chars()
                .count(),
            120
        );

        record_results_at(
            &path,
            &[("local:codex".into(), false, "second".into())],
            20,
        )
        .unwrap();
        let failed = read_at(&path);
        assert_eq!(failed.runners["local:codex"].consecutive_failures, 2);
        assert_eq!(
            failed.runners["local:codex"].last_error.as_deref(),
            Some("second")
        );

        record_results_at(
            &path,
            &[("local:codex".into(), true, "ok".into())],
            30,
        )
        .unwrap();
        let recovered = read_at(&path);
        let entry = &recovered.runners["local:codex"];
        assert_eq!(entry.last_success_ts, Some(30));
        assert_eq!(entry.last_error_ts, Some(20));
        assert_eq!(entry.last_error.as_deref(), Some("second"));
        assert_eq!(entry.consecutive_failures, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
