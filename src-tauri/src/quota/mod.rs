//! K0 Quota 監控即時性 contract：聚合本機 CLI runner 的 live API fetch 結果。
//! R82 開工，R85/R86 補 anthropic、codex 兩個 fetch 實作，R89 Tauri command
//! 接入 (`get_live_quota_snapshot` → `collect_live_quota_snapshot_with_home`),
//! R108 加 gemini 對齊 4 本機 CLI 中第 3 個 (K0 Quota 8/13 → 9/13)，
//! R109 加 copilot 對齊 4 本機 CLI 中第 4 個 (K0 Quota 9/13 → 10/13)。

pub mod anthropic;
pub mod antigravity;
pub mod claude_logs;
pub mod provider_daily;
pub mod codex;
pub mod codex_rollout;
pub mod copilot;
pub mod devin;
pub mod grok;
pub mod minimax;
pub mod openrouter;

use serde::{Deserialize, Serialize};

/// 跑一個短命子程序（`gh auth token`、`wsl.exe cat ...`）拿輸出，超過時限就殺掉並回收。
/// std 的 `output()` 沒有逾時；外層包 tokio timeout 只是放棄等待，卡住的子程序和佔著的
/// blocking 執行緒會一直留著（2026-10-20 稽核）。
/// 逾時時不等讀取執行緒：被殺的程序若有孫程序還握著管線，等下去又會卡住，交給它自己收尾。
pub(crate) fn output_with_timeout(
    mut cmd: std::process::Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    // 另開執行緒讀管線：輸出若塞滿管線緩衝，子程序會卡在寫入、永遠不結束
    let out_reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b);
        b
    });
    let err_reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b);
        b
    });
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Some(std::process::Output {
                    status,
                    stdout: out_reader.join().unwrap_or_default(),
                    stderr: err_reader.join().unwrap_or_default(),
                });
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// 單一 runner 的即時額度資料（對應前端 `runners[]` 結構）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerQuota {
    pub name: String,
    pub label: String,
    pub color: String,
    pub ok: bool,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

/// 所有 runner 的聚合結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveQuotaSnapshot {
    pub runners: Vec<RunnerQuota>,
    pub source: String,
    pub updated_at: u64,
}

/// Usage 面板卡片清單的資料源：本機實際安裝的 AI CLI。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledCli {
    pub id: String,
    pub label: String,
    pub color: String,
}

/// 偵測本機安裝了哪些 AI CLI（設定目錄 / 已知安裝路徑存在即視為已安裝）。
/// 面板卡片以此清單為準——沒安裝的不顯示，不寫死。
/// `extra_roots`: (APPDATA, LOCALAPPDATA)，None 的 probe 直接跳過。
/// `has_minimax` / `has_openrouter`: 這兩家無設定目錄，以「env 有對應 API key」
/// 為安裝訊號，caller（Tauri command 殼）查 env 傳入，本函式保持純路徑可測。
pub fn detect_installed_clis_with_roots(
    home: Option<&std::path::Path>,
    appdata: Option<&std::path::Path>,
    localappdata: Option<&std::path::Path>,
    has_minimax: bool,
    has_openrouter: bool,
) -> Vec<InstalledCli> {
    let h = |rel: &str| home.map(|p| p.join(rel));
    let _ = appdata; // 2026-07-17 使用者裁掉 opencode 卡後暫無 APPDATA probe，參數保留簽名穩定
    let l = |rel: &str| localappdata.map(|p| p.join(rel));
    // (id, label, color, 任一存在即算安裝)
    // 2026-07-17 使用者指示移除沒在用/抓不到額度的卡：gemini、qwen、opencode、hermes
    let table: Vec<(&str, &str, &str, Vec<Option<std::path::PathBuf>>)> = vec![
        ("claude", "Claude Code", "#d97757", vec![h(".claude")]),
        ("codex", "Codex CLI", "#10a37f", vec![h(".codex")]),
        ("copilot", "Copilot CLI", "#8957e5", vec![h(".copilot")]),
        ("grok", "Grok CLI", "#9aa0a6", vec![h(".grok")]),
        ("agy", "Antigravity CLI", "#f59e0b", vec![h("bin/agy.ps1")]),
        ("devin", "Devin CLI", "#2ea3ff", vec![l("devin")]),
    ];
    let mut out: Vec<InstalledCli> = table
        .into_iter()
        .filter(|(_, _, _, probes)| probes.iter().flatten().any(|p| p.exists()))
        .map(|(id, label, color, _)| InstalledCli {
            id: id.to_string(),
            label: label.to_string(),
            color: color.to_string(),
        })
        .collect();
    if has_minimax {
        out.push(InstalledCli {
            id: "minimax".to_string(),
            label: "MiniMax".to_string(),
            color: "#ec4899".to_string(),
        });
    }
    if has_openrouter {
        out.push(InstalledCli {
            id: "openrouter".to_string(),
            label: "OpenRouter".to_string(),
            color: "#6366f1".to_string(),
        });
    }
    out
}

#[cfg(test)]
mod detect_tests {
    use super::*;

    #[test]
    fn detect_uses_config_dir_presence() {
        let tmp = std::env::temp_dir().join(format!("lp-detect-test-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join(".claude")).unwrap();
        std::fs::create_dir_all(tmp.join(".grok")).unwrap();
        let got = detect_installed_clis_with_roots(Some(&tmp), None, None, false, false);
        let ids: Vec<_> = got.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"claude"), "expected claude in {ids:?}");
        assert!(ids.contains(&"grok"), "expected grok in {ids:?}");
        assert!(!ids.contains(&"codex"), "codex 不該被偵測到: {ids:?}");
        assert!(!ids.contains(&"minimax"), "has_minimax=false 不該出 minimax: {ids:?}");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn detect_empty_home_yields_empty() {
        let tmp = std::env::temp_dir().join(format!("lp-detect-empty-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(detect_installed_clis_with_roots(Some(&tmp), None, None, false, false).is_empty());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn detect_minimax_via_env_flag() {
        let tmp = std::env::temp_dir().join(format!("lp-detect-mm-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let got = detect_installed_clis_with_roots(Some(&tmp), None, None, true, false);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "minimax");
        assert_eq!(got[0].label, "MiniMax");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn detect_openrouter_via_env_flag() {
        let tmp = std::env::temp_dir().join(format!("lp-detect-or-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let got = detect_installed_clis_with_roots(Some(&tmp), None, None, false, true);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "openrouter");
        assert_eq!(got[0].label, "OpenRouter");
        std::fs::remove_dir_all(&tmp).ok();
    }
}

#[cfg(all(test, windows))]
mod output_with_timeout_tests {
    use super::output_with_timeout;
    use std::time::{Duration, Instant};

    fn cmd(args: &[&str]) -> std::process::Command {
        let mut c = std::process::Command::new("cmd.exe");
        c.args(args);
        c
    }

    #[test]
    fn returns_output_when_child_finishes_in_time() {
        let out = output_with_timeout(cmd(&["/c", "echo lp-ok"]), Duration::from_secs(5)).expect("應該拿到輸出");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "lp-ok");
    }

    #[test]
    fn kills_and_returns_none_when_child_hangs() {
        let started = Instant::now();
        // ping 約 10 秒；300ms 就該放棄，而且不能被還握著管線的 ping 拖住
        let out = output_with_timeout(cmd(&["/c", "ping -n 11 127.0.0.1 >nul"]), Duration::from_millis(300));
        assert!(out.is_none());
        assert!(started.elapsed() < Duration::from_secs(3), "逾時後要立刻返回，實際 {:?}", started.elapsed());
    }
}
