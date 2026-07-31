//! R122 Cross-Provider Timeline ring buffer (M1 階段 T-CPT7)
//!
//! 第 6 視圖 Timeline 的單一 in-memory 資料源。對齊 R117
//! `cross-provider-timeline` M0 spec closure + R-CPT-1 4 state 4 色 +
//! R-CPT-4 不開新 OTel 維度、不開新 data path。
//!
//! ## Memory budget
//!
//! - 13 provider × 1440 minute-cells (24h) × 1 byte = 18,720 bytes (18.3 KB)
//! - 13 provider × 10080 minute-cells (7d) × 1 byte = 131,040 bytes (128 KB)
//! - 加總 149,760 bytes (146.3 KB) per process, 守 K41 紅線 < 150KB
//!   (安全 margin 3.7 KB, 對齊 chore_treadmill 紅線 < 30%)
//!
//! ## 兩條固定 buffer (R131 M1.1 對齊 design.md §5 開放問題 #1)
//!
//! - 24h ring: 1 min 解析度, 1440 cells (process 重啟後空 strip 對齊 R-CPT-1)
//! - 7d ring: 1 min 解析度, 10080 cells, 7d history 給 design §5 開放問題 #1
//!   「兩條固定 buffer 提案」護衛 (24h 1min + 7d 1min 對齊 K41 < 150KB)
//!
//! 同一個 `record_event(provider, state, minute)` 同步寫兩條 buffer,
//! 各 buffer 獨立 wrap 對齊自身長度。
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
//! engineering-log.md R122 entry。R131 M1.1 加 7d buffer 護衛 走
//! timeline::tests 既有 mod (chain 19 內延伸, 對齊 R70 補完模式)。
//!
//! R211 surgical fix: 移除模組級 `#![allow(dead_code)]`。R122 ship
//! T-CPT7 (b1b3ed3) + R131 ship 7d buffer 護衛後, `state_to_u8` 被
//! session.rs:3 引用、`TimelineRing` 被 session.rs:3 引用、
//! `TimelineJumpTarget` 被 lib.rs:259,260 引用 — 模組內所有 pub item
//! 都有 active consumer, 模組級 allow(dead_code) 已過期。

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

/// R131 M1.1: 7d × 60min × 24h = 10080 cells per provider。
/// 對齊 `cross-provider-timeline/design.md` §5 開放問題 #1 兩條固定 buffer
/// 提案 (24h 1min × 18.3KB + 7d 1min × 128KB, 加總 < 150KB 守 K41 紅線)。
pub const CELLS_PER_PROVIDER_7D: usize = 10080;

/// 13 provider ring buffer,內含兩條固定 buffer (24h + 7d)。
/// Tauri 進程共用單一 TimelineRing 實例,由 Tauri state 管理 (R-CPT-2
/// 第 6 視圖單一 source of truth)。
pub struct TimelineRing {
    /// 24h buffer: 18,720 byte 連續緩衝。索引 = provider_index * 1440 + (minute % 1440)
    cells_24h: Vec<u8>,
    /// R131 M1.1 7d buffer: 131,040 byte 連續緩衝。
    /// 索引 = provider_index * 10080 + (minute % 10080)
    cells_7d: Vec<u8>,
    /// provider 名 → row index, 對齊 KNOWN_PROVIDERS SSoT
    provider_index: HashMap<String, usize>,
    /// 最近一次 record_event 收到的 minute 計數 (用於 stale detection)
    current_minute: u32,
    /// 已成功寫入 timeline 的事件數。用來區分「初始化 Idle 填充」與「真實觀測到 Idle」。
    recorded_event_count: u64,
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
    /// R131 M1.1: 兩條 buffer (24h 18.3KB + 7d 128KB) 同步初始化。
    pub fn new() -> Self {
        let mut provider_index = HashMap::with_capacity(KNOWN_PROVIDERS.len());
        for (idx, name) in KNOWN_PROVIDERS.iter().enumerate() {
            provider_index.insert((*name).to_string(), idx);
        }
        let n = KNOWN_PROVIDERS.len();
        Self {
            cells_24h: vec![STATE_IDLE; n * CELLS_PER_PROVIDER_24H],
            cells_7d: vec![STATE_IDLE; n * CELLS_PER_PROVIDER_7D],
            provider_index,
            current_minute: 0,
            recorded_event_count: 0,
        }
    }

    /// 註冊 1 個 event,**同步寫入 24h + 7d 兩條 buffer** (R131 M1.1 兩條固定
    /// buffer 提案對齊 design.md §5 開放問題 #1)。
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
        // 24h buffer: 索引 = row * 1440 + (minute % 1440)
        let col_24h = (minute as usize) % CELLS_PER_PROVIDER_24H;
        let idx_24h = row * CELLS_PER_PROVIDER_24H + col_24h;
        self.cells_24h[idx_24h] = state;
        // 7d buffer: 索引 = row * 10080 + (minute % 10080)
        let col_7d = (minute as usize) % CELLS_PER_PROVIDER_7D;
        let idx_7d = row * CELLS_PER_PROVIDER_7D + col_7d;
        self.cells_7d[idx_7d] = state;
        self.current_minute = minute;
        self.recorded_event_count = self.recorded_event_count.saturating_add(1);
    }

    /// 13 row × 1440 cell snapshot。對齊 R-CPT-1 Scenario "24h 解析度 toggle
    /// 預設開啟"。
    pub fn snapshot_24h(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::with_capacity(KNOWN_PROVIDERS.len());
        for row in 0..KNOWN_PROVIDERS.len() {
            let start = row * CELLS_PER_PROVIDER_24H;
            let end = start + CELLS_PER_PROVIDER_24H;
            out.push(self.cells_24h[start..end].to_vec());
        }
        out
    }

    /// R131 M1.1: 13 row × 10080 cell 7d snapshot。對齊 design.md §5 開放問題
    /// #1 兩條固定 buffer 提案。給前端 Timeline view 切 7d 解析度時讀。
    pub fn snapshot_7d(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::with_capacity(KNOWN_PROVIDERS.len());
        for row in 0..KNOWN_PROVIDERS.len() {
            let start = row * CELLS_PER_PROVIDER_7D;
            let end = start + CELLS_PER_PROVIDER_7D;
            out.push(self.cells_7d[start..end].to_vec());
        }
        out
    }

    /// 護衛測試用:provider 數 = `KNOWN_PROVIDERS.len()` (對齊 R114 SSoT)。
    #[cfg(test)]
    pub fn provider_count(&self) -> usize {
        self.provider_index.len()
    }

    /// 護衛測試用:current_minute 計數。
    #[cfg(test)]
    pub fn current_minute(&self) -> u32 {
        self.current_minute
    }

    /// 成功寫入 timeline 的事件數。0 代表沒有任何觀測資料，前端不可把初始 Idle cell 當真。
    pub fn recorded_event_count(&self) -> u64 {
        self.recorded_event_count
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
    fn timeline_ring_tracks_recorded_event_count() {
        let mut ring = TimelineRing::new();
        assert_eq!(
            ring.recorded_event_count(),
            0,
            "new timeline ring has no observed data; idle-filled cells must not be treated as real events"
        );

        ring.record_event("claude", STATE_IDLE, 0);
        assert_eq!(
            ring.recorded_event_count(),
            1,
            "an explicit idle event is observed data and must be distinguishable from no data"
        );

        ring.record_event("ghost_provider_xyz", STATE_WORKING, 1);
        assert_eq!(
            ring.recorded_event_count(),
            1,
            "unknown providers are dropped and must not increase observed timeline data"
        );

        ring.record_event("claude", STATE_WORKING + 10, 2);
        assert_eq!(
            ring.recorded_event_count(),
            1,
            "invalid state bytes are dropped and must not increase observed timeline data"
        );
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

    #[test]
    fn timeline_7d_ring_buffer_invariants() {
        // R131 M1.1: 7d ring buffer 護衛。對齊
        // `cross-provider-timeline/design.md` §5 開放問題 #1 兩條固定 buffer
        // 提案 (24h 1min × 18.3KB + 7d 1min × 128KB, 加總 < 150KB 守 K41)。
        //
        // 架構理由 (走 timeline::tests 既有 mod, chain 19 內延伸, 對齊 R70
        // 補完模式 — lib.rs:1077 既有 chain 16 對稱面延伸先例): 7d buffer
        // 是 T-CPT11 ring_buffer_invariants 護衛的對稱延伸 (24h → 7d 解析度),
        // 同一個 mod 內同主題, 不破 K42 chain 19 條飽和契約。
        //
        // 護衛 5 條不變量:
        // 1. 容量 = 13 provider × 10080 cell = 131,040 cell (對齊 K41 128KB)
        // 2. record_event 同步寫 24h + 7d 兩條 buffer (雙 buffer 一致)
        // 3. state u8 ∈ {0,1,2,3} — 污染值 silently drop, 兩條 buffer 同步
        // 4. snapshot_7d 維度 = 13 row × 10080 cell (對齊 R-CPT-1 §5 #1)
        // 5. 7d wrap: minute=10080 自動 wrap 回 0 (對齊 24h wrap 語意)

        let ring = TimelineRing::new();

        // (1) 容量 = 13 × 10080 = 131,040
        let total_cells_7d: usize = ring.snapshot_7d().iter().map(|r| r.len()).sum();
        assert_eq!(
            total_cells_7d, 131_040,
            "13×10080 = 131,040 cells, 對齊 K41 memory budget 128KB (7d)"
        );

        // (2) record_event 同步寫 24h + 7d 兩條 buffer
        let mut r = TimelineRing::new();
        r.record_event("claude", STATE_WORKING, 1000);
        assert_eq!(
            r.snapshot_24h()[0][1000 % CELLS_PER_PROVIDER_24H],
            STATE_WORKING,
            "24h buffer 寫入"
        );
        assert_eq!(
            r.snapshot_7d()[0][1000 % CELLS_PER_PROVIDER_7D],
            STATE_WORKING,
            "7d buffer 同步寫入 (對齊 7d 解析度)"
        );

        // (3) state 污染值 silently drop, 兩條 buffer 同步保留 Idle
        for state_byte in [4u8, 99, 255] {
            let mut r = TimelineRing::new();
            r.record_event("claude", state_byte, 500);
            assert_eq!(
                r.snapshot_24h()[0][500],
                STATE_IDLE,
                "24h: 污染值 {state_byte} silently drop"
            );
            assert_eq!(
                r.snapshot_7d()[0][500],
                STATE_IDLE,
                "7d: 污染值 {state_byte} silently drop"
            );
        }

        // (4) snapshot_7d 維度
        let snap_7d = ring.snapshot_7d();
        assert_eq!(snap_7d.len(), 13, "13 row (對齊 KNOWN_PROVIDERS)");
        assert!(
            snap_7d.iter().all(|r| r.len() == CELLS_PER_PROVIDER_7D),
            "13 × 10080 cells"
        );

        // (5) 7d wrap: minute=10080 → col 0 (覆寫回 Idle)
        let mut r = TimelineRing::new();
        r.record_event("claude", STATE_WORKING, 0);
        assert_eq!(r.snapshot_7d()[0][0], STATE_WORKING);
        r.record_event("claude", STATE_IDLE, CELLS_PER_PROVIDER_7D as u32);
        assert_eq!(
            r.snapshot_7d()[0][0],
            STATE_IDLE,
            "7d wrap: minute=10080 → col 0 覆寫"
        );
    }
}
