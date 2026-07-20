//! 「切到終端機」支援：hook 事件進來時（TCP 連線還開著、hook 進程還活著）
//! 用 peer port 反查 hook 進程 PID → 走父進程鏈找出終端機宿主
//! （WindowsTerminal / conhost / Code 等），存進 session；使用者點
//! 完成紀錄的按鈕時 SetForegroundWindow 切過去。
//!
//! 不改 lobster-pulse-hook.exe（其他 session 鎖著 binary，不能重建）——
//! 全部在 server 端解決。整個模組 Windows-only（lib.rs cfg 掛載）。

use std::collections::HashMap;

/// 父鏈走到這些名字就停（它們擁有桌面/系統視窗，聚焦到會誤傷）。
const STOP_NAMES: [&str; 6] = [
    "explorer.exe",
    "services.exe",
    "wininit.exe",
    "winlogon.exe",
    "svchost.exe",
    "system",
];

/// hook 事件入口：peer port → hook PID → 終端機候選 PID 鏈。
/// 任何一步失敗回空 vec（監控功能不能反噬事件主路徑）。
pub fn peer_terminal_pids(peer_port: u16, server_port: u16) -> Vec<u32> {
    let Some(pid) = pid_of_loopback_peer(peer_port, server_port) else {
        return Vec::new();
    };
    walk_chain(&process_map(), pid)
}

/// 從 TCP table 找「local port = peer 的 ephemeral port、remote port = 本
/// server port」的連線 owner PID（= lobster-pulse-hook.exe，socket 還開著所以必在）。
fn pid_of_loopback_peer(peer_port: u16, server_port: u16) -> Option<u32> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, TCP_TABLE_OWNER_PID_ALL,
    };
    const AF_INET: u32 = 2;
    // 兩段式呼叫（先問大小再讀）之間連線數可能增加 → 第二段 buffer 不足回非 0；
    // 重新量測再試一次，仍失敗才放棄（功能性 no-op，不影響事件主路徑）
    for _ in 0..2 {
        let mut size: u32 = 0;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if size == 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            let rc = GetExtendedTcpTable(
                buf.as_mut_ptr() as *mut _,
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if rc != 0 {
                continue;
            }
            // layout: dwNumEntries: u32, 接著 N 筆 MIB_TCPROW_OWNER_PID（6 × u32）
            let n = u32::from_ne_bytes(buf[0..4].try_into().ok()?) as usize;
            for i in 0..n {
                let off = 4 + i * 24;
                if off + 24 > buf.len() {
                    break;
                }
                let f = |j: usize| {
                    u32::from_ne_bytes(buf[off + j * 4..off + j * 4 + 4].try_into().unwrap())
                };
                let local_port = u16::from_be((f(2) & 0xFFFF) as u16);
                let remote_port = u16::from_be((f(4) & 0xFFFF) as u16);
                if local_port == peer_port && remote_port == server_port {
                    return Some(f(5));
                }
            }
            return None; // table 讀成功但沒找到 → 不用重試
        }
    }
    None
}

/// UserPromptSubmit 當下抓「使用者正在打字的分頁標題」：前景視窗若屬於
/// 該 session 的終端機 PID 鏈，其標題就是該分頁的真實標題（作用中分頁）。
/// 之後跳轉用這個標題選分頁——比「猜專案名有沒有出現在標題」確定得多。
/// 前景不屬於鏈上（bot 程式化送 prompt、使用者已切走）回 None 不誤抓。
pub fn foreground_title_if_owned(pids: &[u32]) -> Option<String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if !pids.contains(&pid) {
            return None;
        }
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
        if n <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// 去掉標題開頭的狀態符號（WT/Claude 的 ✳/⠐/⠂ 會隨狀態變）取穩定核心。
pub fn title_core(s: &str) -> String {
    s.trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim()
        .to_string()
}

/// 系統開機時刻（UTC）。重開機前紀錄的 PID 必已被 OS 回收，
/// focus 前用來擋「聚焦到 PID 重用後的無關視窗」。
pub fn boot_time_utc() -> chrono::DateTime<chrono::Utc> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount64;
    let up_ms = unsafe { GetTickCount64() };
    chrono::Utc::now() - chrono::Duration::milliseconds(up_ms as i64)
}

/// Toolhelp 全進程快照：pid → (ppid, exe 名小寫)。
fn process_map() -> HashMap<u32, (u32, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut map = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return map;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(260);
                let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
                map.insert(e.th32ProcessID, (e.th32ParentProcessID, name));
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    map
}

/// 純邏輯：從 hook PID 走父鏈（不含 hook 自己），到 STOP_NAMES / 未知 PID /
/// 深度 8 / 循環為止；再把鏈上進程的 conhost/OpenConsole 子進程附在尾端
/// （傳統 console 視窗的 owner 是 conhost，不在父鏈裡）。
fn walk_chain(map: &HashMap<u32, (u32, String)>, start: u32) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(start);
    let mut cur = match map.get(&start) {
        Some(&(ppid, _)) => ppid,
        None => return chain,
    };
    while chain.len() < 8 {
        let Some((ppid, name)) = map.get(&cur) else { break };
        if STOP_NAMES.contains(&name.as_str()) || !seen.insert(cur) {
            break;
        }
        chain.push(cur);
        cur = *ppid;
    }
    let mut out: Vec<u32> = map
        .iter()
        .filter(|(_, (ppid, name))| {
            chain.contains(ppid) && (name == "conhost.exe" || name == "openconsole.exe")
        })
        .map(|(&pid, _)| pid)
        .collect();
    // 順序（focus 端會反向迭代）：conhost 墊底、父鏈由近到遠接後 →
    // 反向後「終端機宿主最優先、conhost 最後」。實測教訓：conhost 排前面時，
    // 鏈上工具進程的雜項 console 視窗（快顯主機）會搶在 WT 之前被聚焦。
    out.extend(chain);
    out
}

/// 聚焦候選 PID 擁有的可見有標題視窗。
///
/// Windows Terminal 是單一進程管多個視窗——光靠 PID 會抓到同進程的別的
/// 視窗。所以先用 `title_hint`（專案資料夾名；Claude 的分頁標題帶專案名）
/// 在候選裡挑標題命中的；沒命中再退回鏈序由遠到近（終端機宿主優先）。
/// 聚焦後若有 hint 再補一發 UIA 分頁切換（select_tab_async）處理 WT 多分頁。
/// ponytail: 多個 WT 視窗且目標分頁藏在「沒被選中的那個視窗」時仍會選錯視窗，
/// 要跨視窗掃分頁需同步 UIA 查詢再挑視窗，等真的踩到再說。
pub fn focus_window_for_pids(pids: &[u32], hints: &[String]) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows;
    unsafe extern "system" fn cb(
        hwnd: windows_sys::Win32::Foundation::HWND,
        lparam: windows_sys::Win32::Foundation::LPARAM,
    ) -> i32 {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        };
        let out = unsafe { &mut *(lparam as *mut Vec<(isize, u32, String)>) };
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
            if IsWindowVisible(hwnd) != 0 {
                let mut buf = [0u16; 256];
                let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
                if n > 0 {
                    let title = String::from_utf16_lossy(&buf[..n as usize]);
                    out.push((hwnd as isize, pid, title));
                }
            }
        }
        1
    }
    let mut wins: Vec<(isize, u32, String)> = Vec::new();
    unsafe {
        EnumWindows(Some(cb), &mut wins as *mut _ as isize);
    }
    let pid_set: std::collections::HashSet<u32> = pids.iter().copied().collect();
    let candidates: Vec<&(isize, u32, String)> =
        wins.iter().filter(|(_, p, _)| pid_set.contains(p)).collect();

    let hints: Vec<&String> = hints.iter().filter(|h| !h.is_empty()).collect();
    // 1) 標題命中 hint 的候選優先（hint 依序：實抓分頁標題核心 → 專案名；大小寫不敏感）
    for h in &hints {
        let hl = h.to_lowercase();
        if let Some(&&(hwnd, _, _)) = candidates
            .iter()
            .find(|(_, _, t)| t.to_lowercase().contains(&hl))
        {
            if try_focus(hwnd) {
                select_tab_async(&[hwnd], &hints);
                return Ok(());
            }
        }
    }
    // 2) 退回鏈序由遠到近（終端機宿主優先）。同 PID 可能有多個視窗
    //    （WT 單進程多視窗＋雜訊 console 視窗），先聚焦第一個聚得起來的求快，
    //    再把該 PID 全部視窗交給 UIA 腳本——找到含目標分頁的視窗會改聚焦它。
    for &pid in pids.iter().rev() {
        let same_pid: Vec<isize> = candidates
            .iter()
            .filter(|(_, p, _)| *p == pid)
            .map(|&&(h, _, _)| h)
            .collect();
        for &hwnd in &same_pid {
            if try_focus(hwnd) {
                if !hints.is_empty() {
                    select_tab_async(&same_pid, &hints);
                }
                return Ok(());
            }
        }
    }
    Err("找不到可聚焦的終端機視窗（可能已關閉）".to_string())
}

/// WT 多分頁：視窗聚焦後用 UI Automation 把標題含 hint 的分頁切成作用中
/// （WT 視窗標題只反映作用中分頁，目標分頁在背景時光聚焦視窗不夠）。
/// 可傳多個候選視窗（WT 單進程多視窗）：腳本逐一掃描，找到含目標分頁的
/// 視窗就聚焦「那個視窗」並選中分頁——同 PID 選錯視窗時自我修正。
/// 走 PowerShell 的 System.Windows.Automation——Rust 直接摸 COM/UIA 太重。
/// fire-and-forget＋隱窗（踩雷§25 不能閃黑窗）；沒有 TabItem 的普通視窗
/// 找不到就默默結束，失敗退化成「只聚焦視窗」。
/// ponytail: spawn 無逾時——目標視窗卡死時 UIA 會卡住該 powershell 常駐；
/// 手動點擊觸發、頻率極低，觀察到堆積再加 timeout/kill。
fn select_tab_async(hwnds: &[isize], hints: &[&String]) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if hwnds.is_empty() || hints.is_empty() {
        return;
    }
    // 純 ASCII inline script；hwnds/hints 走 env var 免跳脫（硬規則7）。
    // hints 以 | 分隔依序嘗試（實抓分頁標題核心優先、專案名次之）。
    const SCRIPT: &str = "Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes; \
Add-Type -Namespace LP -Name W -MemberDefinition '[DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport(\"user32.dll\")] public static extern bool ShowWindow(IntPtr h, int n); [DllImport(\"user32.dll\")] public static extern bool IsIconic(IntPtr h);'; \
$c=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::TabItem); \
foreach($hint in $env:LP_TAB_HINT.ToLower().Split('|')){ \
if(-not $hint){continue}; \
foreach($hs in $env:LP_TAB_HWND.Split(',')){ \
$h=[IntPtr][long]$hs; \
try{$root=[System.Windows.Automation.AutomationElement]::FromHandle($h)}catch{continue}; \
foreach($t in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$c)){ \
if($t.Current.Name.ToLower().Contains($hint)){ \
if([LP.W]::IsIconic($h)){[void][LP.W]::ShowWindow($h,9)}; \
[void][LP.W]::SetForegroundWindow($h); \
$t.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); \
exit } } } }";
    let joined = hwnds
        .iter()
        .map(|h| h.to_string())
        .collect::<Vec<_>>()
        .join(",");
    // hint 內若含分隔符 | 會被切壞——換成空白（標題比對本來就是 contains，影響極小）
    let hint_env = hints
        .iter()
        .map(|h| h.replace('|', " "))
        .collect::<Vec<_>>()
        .join("|");
    // 前景鎖：SetForegroundWindow 只有前景進程叫得動。腳本跑在另一個
    // powershell 進程裡，不授權的話它只能切分頁、視窗浮不上來（實測踩到）。
    // ASFW_ANY = 任何進程；本行由「使用者剛點過的」LP 進程呼叫才有效。
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX);
    }
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("LP_TAB_HWND", joined)
        .env("LP_TAB_HINT", hint_env)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

fn try_focus(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::walk_chain;
    use std::collections::HashMap;

    fn m(entries: &[(u32, u32, &str)]) -> HashMap<u32, (u32, String)> {
        entries
            .iter()
            .map(|&(pid, ppid, name)| (pid, (ppid, name.to_string())))
            .collect()
    }

    #[test]
    fn chain_walks_up_stops_at_explorer_and_adds_conhost_kids() {
        let map = m(&[
            (100, 90, "lobster-pulse-hook.exe"),
            (90, 80, "cmd.exe"),
            (80, 70, "node.exe"),
            (70, 60, "pwsh.exe"),
            (60, 50, "windowsterminal.exe"),
            (50, 1, "explorer.exe"),
            (95, 80, "conhost.exe"), // node 的 console 視窗宿主
        ]);
        let chain = walk_chain(&map, 100);
        assert_eq!(
            chain,
            vec![95, 90, 80, 70, 60],
            "conhost 墊底在前、父鏈由近到遠在後（focus 端反向迭代 → 宿主優先）"
        );
        assert!(!chain.contains(&50), "explorer 不得入鏈");
        assert!(!chain.contains(&100), "hook 自己不得入鏈");
    }

    #[test]
    fn title_core_strips_status_glyphs() {
        assert_eq!(super::title_core("✳ 監控系統配置"), "監控系統配置");
        assert_eq!(super::title_core("⠐ autodev-ng 專案開發"), "autodev-ng 專案開發");
        assert_eq!(super::title_core("PowerShell"), "PowerShell");
        assert_eq!(super::title_core("✳ "), "");
    }

    #[test]
    fn chain_survives_ppid_cycle_and_unknown_parent() {
        // ppid 循環（pid 重用造成）不得無窮迴圈
        let map = m(&[(100, 90, "hook"), (90, 80, "a.exe"), (80, 90, "b.exe")]);
        let chain = walk_chain(&map, 100);
        assert_eq!(chain, vec![90, 80]);
        // 未知起點 → 空
        assert!(walk_chain(&map, 999).is_empty());
    }
}
