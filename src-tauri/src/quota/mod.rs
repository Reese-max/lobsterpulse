// R82 半成品: K0 Quota 監控即時性 contract。
// RunnerQuota / LiveQuotaSnapshot 為聚合 14 provider 的設計，R86+ Tauri command 接入。
// R86: 加 `codex` 模組,本機 OpenAI 體系 CLI quota fetch 實作。
#![allow(dead_code)]

pub mod anthropic;
pub mod codex;

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
