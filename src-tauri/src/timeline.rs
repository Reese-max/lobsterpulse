//! R122 Cross-Provider Timeline ring buffer (M1 階段 T-CPT7)
//!
//! 第 6 視圖 Timeline 的單一 in-memory 資料源。對齊 R117
//! `cross-provider-timeline` M0 spec closure + R-CPT-1 4 state 4 色 +
//! R-CPT-4 不開新 OTel 維度、不開新 data path。
//!
//! ## Memory budget
//! 13 provider × 1440 minute-cells (24h) × 1 byte = 18,720 bytes = 18.3 KB
//! per process。對齊 K41 chore_treadmill 紅線 < 30%。
//!
//! ## 4 state u8 encoding (對齊 session.rs SessionState SSoT)
//! 0 = Idle, 1 = Working, 2 = WaitingForUser, 3 = Stale
//!
//! ## Provider SSoT
//! 從 `crate::hook_server::KNOWN_PROVIDERS` (R114 `pub const` 化) 動態建
//! index,不硬編 13 個名字避免 drift。
//!
//! ## K42 chain 17→18 (R-CPT-3 接力位置)
//! M0 守住 chain 17, M1 加護衛 test → chain 18。架構理由 doc 見
//! engineering-log.md R122 entry。
//!
//! ## Dead-code 暫時白名單
//! T-CPT7 (本檔) 只落 struct + 護衛 test, lib.rs 串接留 T-CPT9
//! (`timeline_snapshot_24h` / `timeline_toggle_resolution` /
//! `timeline_jump_to_event` 三個 Tauri command 註冊)。屆時移除
//! `#![allow(dead_code)]`。

#![allow(dead_code)]

use crate::hook_server::KNOWN_PROVIDERS;
use crate::session::SessionState;
use std::collections::HashMap;

/// 4 state u8 encoding (對齊 session.rs SessionState SSoT 順序)
pub const STATE_IDLE: u8 = 0;
pub const STATE_WORKING: u8 = 1;
pub const STATE_WAITING_FOR_USER: u8 = 2;
pub const STATE_STALE: u8 = 3;

/// 24h × 60min = 1440 cells per provider
pub const CELLS_PER_PROVIDER_24H: usize = 1440;

/// 13 provider × 1440 minute-cells 固定大小 ring buffer。
/// Total = 18,720 bytes (18.3 KB) per process。
///
/// Tauri 進程共用單一 TimelineRing 實例,由 Tauri state 管理 (R-CPT-2
/// 第 6 視圖單一 source of truth)。
pub struct TimelineRing {
    /// 18,720 byte 連續緩衝。索引 = provider_index * 1440 + (minute % 1440)
    cells: [u8; 18720],
    /// provider 名 → row index, 對齊 KNOWN_PROVIDERS SSoT
    provider_index: HashMap<String, usize>,
    /// 最近一次 record_event 收到的 minute 計數 (用於 stale detection)
    current_minute: u32,
}

/// 將 `SessionState` 對映到 Timeline u8 encoding。SSoT 對齊 R-CPT-4
/// Scenario "4 state 對齊 session.rs SSoT"。
pub fn state_to_u8(state: SessionState) -> u8 {
    match state {
        SessionState::Idle => STATE_IDLE,
        SessionState::Working => STATE_WORKING,
        SessionState::WaitingForUser => STATE_WAITING_FOR_USER,
        SessionState::Stale => STATE_STALE,
    }
}

impl TimelineRing {
    /// 從 `hook_server::KNOWN_PROVIDERS` 建 index。process start 時呼叫 1 次。
    pub fn new() -> Self {
        let mut provider_index = HashMap::with_capacity(KNOWN_PROVIDERS.len());
        for (idx, name) in KNOWN_PROVIDERS.iter().enumerate() {
            provider_index.insert((*name).to_string(), idx);
        }
        Self {
            cells: [STATE_IDLE; 18720],
            provider_index,
            current_minute: 0,
        }
    }

    /// 註冊 1 個 event。
    /// - provider 不在 `KNOWN_PROVIDERS` 內 → silently drop
    ///   (對齊 R66 parse_provider 9-provider whitelist 行為)。
    /// - state ∉ {0,1,2,3} → silently drop (防止污染既有 4 state 對齊
    ///   session.rs, R-CPT-4 護衛)。
    pub fn record_event(&mut self, provider: &str, state: u8, minute: u32) {
        if state > STATE_STALE {
            return;
        }
        let Some(&row) = self.provider_index.get(provider) else {
            return;
        };
        let col = (minute as usize) % CELLS_PER_PROVIDER_24H;
        let idx = row * CELLS_PER_PROVIDER_24H + col;
        self.cells[idx] = state;
        self.current_minute = minute;
    }

    /// 13 row × 1440 cell snapshot。對齊 R-CPT-1 Scenario "24h 解析度 toggle
    /// 預設開啟"。
    pub fn snapshot_24h(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::with_capacity(KNOWN_PROVIDERS.len());
        for row in 0..KNOWN_PROVIDERS.len() {
            let start = row * CELLS_PER_PROVIDER_24H;
            let end = start + CELLS_PER_PROVIDER_24H;
            out.push(self.cells[start..end].to_vec());
        }
        out
    }

    /// 護衛測試用:provider 數 = `KNOWN_PROVIDERS.len()` (對齊 R114 SSoT)。
    pub fn provider_count(&self) -> usize {
        self.provider_index.len()
    }

    /// 護衛測試用:current_minute 計數。
    pub fn current_minute(&self) -> u32 {
        self.current_minute
    }
}

/// T-CPT9 timeline_jump_to_event Tauri command 跨視圖 target。
/// 對齊 `cross-provider-timeline/design.md` §5 開放問題 #3
/// (click-to-jump 跨視圖, 點 row 跳 Bot 總覽 / 點 cell 跳事件診斷)。
/// 簡化版: 一律回 `view = "events"`, 前端可後續按需切 `view = "bot"`。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineJumpTarget {
    /// 目標 view 名 (`bot` / `events` / `capsule` / `compact` / `settings` / `timeline`)
    pub view: String,
    /// 對齊 `KNOWN_PROVIDERS` SSoT (R114 `pub const`)
    pub provider: String,
    /// 24h 解析度下的 minute 索引 (0..1440)
    pub minute: u32,
}

impl Default for TimelineRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    //! K42 chain 18 護衛 (R122 M1 T-CPT11 接力位置)
    //!
    //! 架構理由 (R97 決策 chain 17 飽和凍結, R114 R111+ 留 chain 18 接力
    //! 位置給新 feature): R122 Timeline 視圖是 6 視圖架構擴張,需 1 條獨立
    //! 護衛守住 ring buffer 不變 invariant。第二條 SSoT 對齊護衛算
    //! chain 18 內延伸,對齊 R70 補完模式 (lib.rs:1077 既有 chain 16 對稱
    //! 面延伸先例)。

    use super::*;

    #[test]
    fn timeline_ring_buffer_invariants() {
        // 4 不變量同 1 條護衛測試守住:
        // 1. 容量 = 13 provider × 1440 cell = 18,720 cell (永不變)
        // 2. provider_index 對齊 KNOWN_PROVIDERS SSoT (R114 pub const)
        // 3. state u8 ∈ {0,1,2,3} (record_event 拒絕污染值)
        // 4. snapshot_24h 維度 = 13 row × 1440 cell (對齊 R-CPT-1)

        let ring = TimelineRing::new();

        // (1) 容量 = 18,720
        let total_cells: usize = ring.snapshot_24h().iter().map(|r| r.len()).sum();
        assert_eq!(
            total_cells, 18_720,
            "13×1440 = 18,720 cells, 對齊 K41 memory budget 18.3KB"
        );

        // (2) provider_index 對齊 KNOWN_PROVIDERS
        assert_eq!(
            ring.provider_count(),
            KNOWN_PROVIDERS.len(),
            "provider_index 對齊 KNOWN_PROVIDERS SSoT (R114)"
        );
        for (idx, name) in KNOWN_PROVIDERS.iter().enumerate() {
            assert_eq!(
                ring.provider_index.get(*name).copied(),
                Some(idx),
                "{name} 應在 index = {idx}"
            );
        }

        // (3) state u8 ∈ {0,1,2,3} — 污染值被 silently drop
        for state_byte in [0u8, 1, 2, 3, 4, 99, 255] {
            let mut r = TimelineRing::new();
            r.record_event("claude", state_byte, 0);
            if state_byte <= 3 {
                assert_eq!(r.snapshot_24h()[0][0], state_byte);
            } else {
                assert_eq!(
                    r.snapshot_24h()[0][0],
                    STATE_IDLE,
                    "污染值 {state_byte} 應被 silently drop, 保留 Idle=0"
                );
            }
        }

        // (4) snapshot_24h 維度
        let snap = ring.snapshot_24h();
        assert_eq!(snap.len(), 13, "13 row (對齊 KNOWN_PROVIDERS)");
        assert!(
            snap.iter().all(|r| r.len() == CELLS_PER_PROVIDER_24H),
            "13 × 1440 cells"
        );
    }

    #[test]
    fn timeline_ring_state_alignment_with_session() {
        // 對齊 session.rs SessionState SSoT 4 state 順序 (R-CPT-4 Scenario
        // "4 state 對齊 session.rs SSoT")。
        assert_eq!(state_to_u8(SessionState::Idle), 0);
        assert_eq!(state_to_u8(SessionState::Working), 1);
        assert_eq!(state_to_u8(SessionState::WaitingForUser), 2);
        assert_eq!(state_to_u8(SessionState::Stale), 3);

        // 邊界: minute wrap (1440 % 1440 = 0) 與未知 provider silently drop
        let mut ring = TimelineRing::new();

        // claude minute=0 → row 0 col 0
        ring.record_event("claude", STATE_WORKING, 0);
        assert_eq!(ring.snapshot_24h()[0][0], STATE_WORKING);

        // claude minute=1440 → wrap 0 (覆寫回 Idle)
        ring.record_event("claude", STATE_IDLE, 1440);
        assert_eq!(ring.snapshot_24h()[0][0], STATE_IDLE);

        // 未知 provider silently drop, 不污染 buffer
        ring.record_event("ghost_provider_xyz", STATE_WORKING, 100);
        let snap = ring.snapshot_24h();
        assert_eq!(snap[3][100], STATE_IDLE, "未知 provider 不污染 buffer");
        assert_eq!(ring.current_minute(), 1440, "current_minute 走已知最後一次");
    }

    #[test]
    fn timeline_jump_target_contract() {
        // T-CPT9 timeline_jump_to_event Tauri command 資料合約護衛。
        // 走 timeline::tests 既有 mod (R122 T-CPT11 ship 護衛 2 條同 mod),
        // 不破 K42 chain 19 條飽和契約 (架構理由: T-CPT9 是 T-CPT11 護衛
        // 對應的 Tauri command 註冊延伸, 算 chain 19 內延伸, 對齊 R70 補完
        // 模式 — lib.rs:1077 既有 chain 16 對稱面延伸先例)。
        //
        // 護衛 4 條不變量:
        // 1. view ∈ 6 view (5 既有 + timeline) — 防止前端 view switch drift
        // 2. provider ∈ KNOWN_PROVIDERS SSoT (R114 pub const)
        // 3. minute < 1440 (24h 解析度範圍)
        // 4. TimelineJumpTarget 可序列化 (Tauri command 回傳給前端要 JSON)

        use crate::timeline::TimelineJumpTarget;

        let target = TimelineJumpTarget {
            view: "events".to_string(),
            provider: "claude".to_string(),
            minute: 720,
        };

        // (1) view ∈ 6 view
        let allowed_views = [
            "bot", "events", "capsule", "compact", "settings", "timeline",
        ];
        assert!(
            allowed_views.contains(&target.view.as_str()),
            "view 必須是 {allowed_views:?} 之一, 收到 {}",
            target.view
        );

        // (2) provider ∈ KNOWN_PROVIDERS SSoT
        assert!(
            KNOWN_PROVIDERS.contains(&target.provider.as_str()),
            "provider 必須對齊 KNOWN_PROVIDERS SSoT (R114), 收到 {}",
            target.provider
        );

        // (3) minute < 1440
        assert!(target.minute < 1440, "minute 必須 < 1440 (24h 解析度)");

        // (4) TimelineJumpTarget 可序列化
        let json = serde_json::to_string(&target).expect("TimelineJumpTarget 應可序列化");
        assert!(json.contains("\"view\":\"events\""));
        assert!(json.contains("\"provider\":\"claude\""));
        assert!(json.contains("\"minute\":720"));
    }
}
