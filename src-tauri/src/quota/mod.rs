//! K0 Quota 監控即時性 contract：聚合本機 CLI runner 的 live API fetch 結果。
//! R82 開工，R85/R86 補 anthropic、codex 兩個 fetch 實作，R89 Tauri command
//! 接入 (`get_live_quota_snapshot` → `collect_live_quota_snapshot_with_home`),
//! R108 加 gemini 對齊 4 本機 CLI 中第 3 個 (K0 Quota 8/13 → 9/13)，
//! R109 加 copilot 對齊 4 本機 CLI 中第 4 個 (K0 Quota 9/13 → 10/13)。

pub mod anthropic;
pub mod antigravity;
pub mod codex;
pub mod copilot;
pub mod devin;
pub mod grok;
pub mod minimax;

use serde::{Deserialize, Serialize};

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
/// `has_minimax`: MiniMax 無設定目錄，以「env 有 MINIMAX_API_KEY」為安裝訊號，
/// caller（Tauri command 殼）查 env 傳入，本函式保持純路徑可測。
pub fn detect_installed_clis_with_roots(
    home: Option<&std::path::Path>,
    appdata: Option<&std::path::Path>,
    localappdata: Option<&std::path::Path>,
    has_minimax: bool,
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
        let got = detect_installed_clis_with_roots(Some(&tmp), None, None, false);
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
        assert!(detect_installed_clis_with_roots(Some(&tmp), None, None, false).is_empty());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn detect_minimax_via_env_flag() {
        let tmp = std::env::temp_dir().join(format!("lp-detect-mm-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let got = detect_installed_clis_with_roots(Some(&tmp), None, None, true);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "minimax");
        assert_eq!(got[0].label, "MiniMax");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
