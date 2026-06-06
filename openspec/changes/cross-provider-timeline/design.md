# Design: Cross-Provider Timeline View (M0 spec 提案)

## 1. 視覺模型 (24h strip + zoom + toggle)

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Timeline          [24h] [7d] [all-time]    ⓘ K0 Quota 9/13  K42 17條守住 │
├─────────────────────────────────────────────────────────────────────────┤
│ 00:00      06:00      12:00      18:00      24:00  ← 時間軸 (24h 解析度 1min) │
│ cicx       ▁▁▂▃▅▆█▇▆▅▄▃▂▁▁▁▁▁▂▃▅▆▇█▇▆▄▂▁  ▁ Working=blue   (13 cells active) │
│ gitx       ▁▁▁▁▁▁▁▁▁▁▁▂▃▅▇▇▅▃▂▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells, idle) │
│ giminix    ▂▃▅▇▆▄▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (5 cells active) │
│ codex_bot  ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells) │
│ openx      ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells) │
│ irisx_bot  ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells, missing) │
│ grokx      ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells, missing) │
│ lpbot      ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells, missing) │
│ mimo       ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells, disabled) │
│ claude     ▂▃▅▆▇▆▅▃▂▁▁▁▁▂▃▅▆▇▇▆▄▂▁▁▁▁▁▂  ▁ Working=blue   (claude=11) │
│ codex      ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells) │
│ copilot    ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells) │
│ gemini     ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  ▁ Working=blue   (0 cells) │
├─────────────────────────────────────────────────────────────────────────┤
│ [⏵ play] [⏸ pause] [↻ refresh]      點 row 跳到 Bot 總覽該 provider 卡  │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3 個 layout 區塊

1. **Header bar** — `[24h] [7d] [all-time]` 解析度 toggle + K0/K42 量化錨點顯示
   (MISSION 北極星量化值釘死, 跟 R116 MISSION spec 對齊 R111 column 同邏輯)
2. **Strip 區** — 13 row × N cell matrix, 每 cell 4 state 4 色:
   - Working = `--working-color` (預設 blue)
   - WaitingForUser = `--waiting-color` (預設 amber)
   - Idle = `--idle-color` (預設 green-soft)
   - Stale = `--stale-color` (預設 red-mute)
3. **Control bar** — play/pause 即時捲動 + refresh + click-to-jump (點 row 跳 Bot
   總覽, 點 cell 跳事件診斷該 provider 該時間窗)

## 2. 資料模型 (ring buffer)

### 2.1 Ring buffer 設計

```rust
// 預計放 src-tauri/src/session.rs (R118+ M1 落地, M0 不動)
pub struct TimelineRing {
    /// 13 provider × 1440 minute-cells (24h) × 1 byte state
    /// 4 state encode 0=Idle / 1=Working / 2=WaitingForUser / 3=Stale
    /// 總計 18,720 bytes = 18.3 KB (per process)
    cells: [[[u8; 1440]; 13]; 1],
    /// 13 provider index 對應 KNOWN_PROVIDERS SSoT
    provider_index: HashMap<String, usize>,
    /// 1 分鐘解析度 tick counter (從 process start 算)
    current_minute: u32,
}

impl TimelineRing {
    /// SessionManager.handle_event 結尾呼叫 (既有 task-completed / waiting emit 之後)
    pub fn record_event(&mut self, provider: &str, state: u8, minute: u32) {
        // 1. provider 對到 index
        // 2. 寫入 cell[provider][minute % 1440] = state
    }

    /// Timeline 視圖讀 (Tauri command)
    pub fn snapshot_24h(&self) -> Vec<Vec<u8>> {
        // 13 row × 1440 cell
    }
}
```

### 2.2 Memory budget

| 解析度 | cell count / provider | 13 provider 總計 | bytes |
|---|---:|---:|---:|
| 1 min (24h) | 1,440 | 18,720 | 18.3 KB |
| 1 min (7d) | 10,080 | 131,040 | 128 KB |
| 1 sec (24h) | 86,400 | 1,123,200 | 1.1 MB |
| **預設 1 min (24h)** | **1,440** | **18,720** | **18.3 KB** |

18.3 KB / process, 跟既有 SessionManager provider_totals (HashMap) 同量級, 不
破 K41 chore_treadmill 紅線。

## 3. 整合點 (R118+ M1 範疇, M0 規劃好)

### 3.1 session.rs 整合

```rust
// 既有 handle_event 流程 (R108~R116 落地版)
pub fn handle_event(&mut self, event: RawHookEvent) -> SessionTransition {
    // ... 既有邏輯: state 累加 + transition emit

    // NEW: 寫入 Timeline ring buffer (R118+ M1)
    self.timeline_ring.record_event(
        &event.provider,
        state_to_u8(&new_state),
        current_minute_unix(),
    );

    transition
}
```

### 3.2 lib.rs 整合

```rust
// 既有 Tauri command 註冊位置
tauri::generate_handler![
    // ... 既有 9 個 command
    // NEW: Timeline 視圖支援
    timeline_snapshot_24h,   // GET Timeline 24h data
    timeline_toggle_resolution, // 切 24h/7d
    timeline_jump_to_event,    // click-to-jump 對齊 Bot 總覽 / 事件診斷
]);
```

### 3.3 main.js 整合 (前端)

既有 5 個 view switch 加第 6 個: `view === 'timeline'`。
HTML 結構新增 `#timeline-view` 區塊, CSS 沿用 `--working-color` / `--waiting-color`
等 token (R70+ theme system), 不破 theme 切換。

## 4. 護衛 (K42 chain 17 條不擴張)

M0 spec 階段**不加 test**, 走既有 K42 chain:
- `provider_registration_guard_tests` (R67 chain 16) — Timeline 加 provider 自動
  走護衛 (新 provider 漏 4 同步點會 fail)
- `render_prometheus_body_*` (R102/R103) — Timeline emit 不開新 metric, 走既有
  K0-A1/A2

R118+ M1 落地時再加 1 條獨立護衛 `timeline_ring_buffer_invariants`:
- 18,720 cell 永不變 size
- provider_index SSoT 對齊 KNOWN_PROVIDERS (R114 pub const)
- record_event 的 state u8 ∈ {0,1,2,3} (不污染既 4 state 對齊 session.rs)

**M0 守住 17, M1 加 1 → 18** — 需架構理由 doc (R114 R111+ 留的 chain 18 提案接
力位置)。

## 5. 開放問題 (R118+ M1 接力)

1. **解析度動態切換** — 24h ↔ 7d 切換是 ring buffer expand 還是兩條 buffer?
   提案: 兩條固定 buffer (24h 1min × 18.3KB + 7d 1min × 128KB), 加總 < 150KB /
   process, 仍守住 K41。
2. **持久化** — Timeline 資料在 process 重啟後是否保留? 提案: 不保留 (對齊
   SessionManager 既有行為), 開機後空 strip, 即時累加。
3. **Click-to-jump 跨視圖** — Timeline 點 row 跳 Bot 總覽, 點 cell 跳事件診斷。
   跨視圖 state 傳遞走既有 `view` global state + URL hash (R40 既有 pattern)。
4. **Hover detail 內容** — cell hover 顯示什麼? 提案: 該 cell 期間最後 1 個
   event 的 provider + state + time (從既有 SessionManager 拉), 不開新資料源。
5. **play/pause 自動捲動** — play 模式讓 strip 自動往左捲 1 cell / 100ms, 模擬
   「現在」位置移動。R118+ M1 評估對膠囊 60fps 有無影響。

## 6. 對齊文件

| 對齊對象 | 對齊方式 |
|---|---|
| MISSION.md 北極星 | 「0 切換成本」+「單一膠囊統一監控」, Timeline 補時間軸化 |
| CLAUDE.md 5 視圖章節 | 加第 6 視圖 (Timeline), 不取代既有 5 |
| CLAUDE.md 競品備忘 3 條邊界 | 不做 cloud dashboard / 不做 cost anomaly / 不做 log reader |
| engineering-log.md R1xx+ 接力清單 | R117 開新 change closure, K40 7→8 |
| CONTRIBUTING.md | 加 1 段「Timeline 視圖新增 provider 注意事項」 (R118+ M1 接力) |
