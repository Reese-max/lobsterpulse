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
/// 時限同時涵蓋「等程序結束」與「讀完輸出」：子程序先結束、孫程序還握著管線時，讀取也不會
/// 無限等。逾時時連整棵程序樹一起砍（Windows 的 kill 只砍直接子程序），管線才會關、讀取
/// 執行緒才會結束。
pub(crate) fn output_with_timeout(
    mut cmd: std::process::Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    #[cfg(windows)]
    let job = KillOnCloseJob::attach(&child);
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    // 另開執行緒讀管線（輸出若塞滿管線緩衝，子程序會卡在寫入、永遠不結束），讀完用 channel 交回
    let (out_tx, out_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b);
        let _ = out_tx.send(b);
    });
    std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b);
        let _ = err_tx.send(b);
    });
    let deadline = Instant::now() + timeout;
    let remaining = || deadline.saturating_duration_since(Instant::now());
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => break None,
        }
    };
    let collected = status.and_then(|status| {
        let stdout = out_rx.recv_timeout(remaining()).ok()?;
        let stderr = err_rx.recv_timeout(remaining()).ok()?;
        Some(std::process::Output { status, stdout, stderr })
    });
    // 收尾：關掉 Job＝結束 job 裡還活著的所有程序（含直接子程序已退出後留下的孤兒孫程序）；
    // 放進 Job 失敗時才退回 taskkill /T（它追不到父程序已結束的孤兒）
    #[cfg(windows)]
    let tree_handled = match job {
        Some(job) => {
            drop(job);
            true
        }
        None => false,
    };
    #[cfg(not(windows))]
    let tree_handled = false;
    if collected.is_none() {
        kill_tree(&mut child, tree_handled);
    }
    collected
}

/// Windows：子程序一啟動就放進 KILL_ON_JOB_CLOSE 的 Job，之後它衍生的程序也都在 job 裡；
/// Drop 時關掉 job 就整群結束（taskkill /T 依父 PID 追，父程序先退出的孤兒追不到）。
/// spawn 與放進 job 之間有極短空窗，這期間衍生的程序不會被追蹤（gh／wsl 不會這麼快衍生）。
#[cfg(windows)]
struct KillOnCloseJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl KillOnCloseJob {
    fn attach(child: &std::process::Child) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        // SAFETY: CreateJobObjectW 參數皆為 null；成功的 handle 立刻交給 guard，由 Drop 關閉
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return None;
        }
        let guard = KillOnCloseJob(job);
        // SAFETY: 結構全是整數欄位，零值合法；指標與長度如實傳入；child 的 handle 在呼叫期間有效
        unsafe {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return None;
            }
            if AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0 {
                return None;
            }
        }
        Some(guard)
    }
}

#[cfg(windows)]
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        // SAFETY: handle 來自 CreateJobObjectW 且只關一次；KILL_ON_JOB_CLOSE 會結束 job 內所有程序
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

/// 砍掉子程序。tree_handled＝已由 Job 結束整群，只需回收；否則 Windows 再用 taskkill /T 補砍後代。
fn kill_tree(child: &mut std::process::Child, tree_handled: bool) {
    #[cfg(windows)]
    if !tree_handled {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .status();
    }
    #[cfg(not(windows))]
    let _ = tree_handled;
    let _ = child.kill();
    let _ = child.wait();
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
    fn does_not_hang_when_grandchild_keeps_the_pipe_after_parent_exits() {
        let started = Instant::now();
        // cmd 用 start /b 丟出背景的 ping（繼承 stdout）後自己馬上結束：讀取要受時限約束
        let out = output_with_timeout(cmd(&["/c", "start /b ping -n 11 127.0.0.1 & exit 0"]), Duration::from_millis(800));
        assert!(started.elapsed() < Duration::from_secs(4), "不可被握著管線的孫程序拖住，實際 {:?}", started.elapsed());
        assert!(out.is_none(), "輸出沒讀完就不算成功");
    }

    #[test]
    fn orphaned_grandchild_is_killed_not_just_abandoned() {
        use std::os::windows::process::CommandExt;
        let dir = std::env::temp_dir().join(format!("lp_job_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let script = dir.join("tick.cmd");
        let tick = dir.join("tick.txt");
        // 孫程序每秒往 tick.txt 寫一行，跑 40 秒；父程序丟出它就立刻結束
        std::fs::write(&script, "@echo off\r\nfor /l %%i in (1,1,40) do (\r\n  echo x>>\"%~dp0tick.txt\"\r\n  ping -n 2 127.0.0.1 >nul\r\n)\r\n").unwrap();
        let mut c = std::process::Command::new("cmd.exe");
        c.raw_arg(format!("/c start \"\" /b \"{}\" & exit 0", script.display()));
        let out = output_with_timeout(c, Duration::from_millis(1500));
        assert!(out.is_none(), "孫程序握著管線、輸出沒讀完，不算成功");
        std::thread::sleep(Duration::from_millis(1500));
        let size1 = std::fs::metadata(&tick).map(|m| m.len()).unwrap_or(0);
        std::thread::sleep(Duration::from_millis(2500));
        let size2 = std::fs::metadata(&tick).map(|m| m.len()).unwrap_or(0);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(size1 > 0, "孫程序應該有跑起來（不然這個測試沒驗到東西）");
        assert_eq!(size1, size2, "收尾後孫程序必須已被結束，檔案不可再長大");
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
