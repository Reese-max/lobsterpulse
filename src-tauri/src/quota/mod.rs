//! K0 Quota 監控即時性 contract：聚合本機 CLI runner 的 live API fetch 結果。
//! R82 開工，R85/R86 補 anthropic、codex 兩個 fetch 實作，R89 Tauri command
//! 接入 (`get_live_quota_snapshot` → `collect_live_quota_snapshot_with_home`),
//! R108 加 gemini 對齊 4 本機 CLI 中第 3 個 (K0 Quota 8/13 → 9/13)，
//! R109 加 copilot 對齊 4 本機 CLI 中第 4 個 (K0 Quota 9/13 → 10/13)。

pub mod anthropic;
pub mod codex;
pub mod copilot;
pub mod gemini;

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
