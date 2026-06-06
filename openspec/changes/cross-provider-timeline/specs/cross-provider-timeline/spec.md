# Spec: Cross-Provider Timeline View

> 對應 change: `cross-provider-timeline`
> 對齊 MISSION 北極星 3 條: 單一膠囊 / 真實任務狀態 / 0 切換成本

## ADDED Requirements

### Requirement: R-CPT-1 — Timeline 24h × 13 provider ring buffer is the single source of truth for historical activity distribution

The Timeline view MUST read from a single in-memory ring buffer
(`TimelineRing`) that records every `SessionManager.handle_event` outcome.
The buffer is fixed at `13 provider × 1440 minute-cells (24h) = 18,720 cell`,
each cell 1 byte encoding state ∈ {`Idle=0`, `Working=1`, `WaitingForUser=2`,
`Stale=3`}. Memory budget: 18.3 KB per process.

#### Scenario: 24h 解析度 toggle 預設開啟

- **WHEN** Timeline 視圖開啟, 預設顯示 24h 解析度 (1 cell = 1 minute)
- **THEN** ring buffer 提供 18,720 cell snapshot
- **AND** strip 顯示 13 row (4 本機 CLI + 9 OpenAB bot)
- **AND** 4 state 用 4 種 theme color 區分 (--working-color /
  --waiting-color / --idle-color / --stale-color)

#### Scenario: 7d 解析度 toggle 切換

- **WHEN** user 點 `[7d]` toggle
- **THEN** ring buffer 切到 7d 模式 (1 cell = 1 minute, 13 provider × 10080
  cell = 131,040 cell = 128 KB)
- **AND** strip 顯示 13 row × 7d cells, 時間軸 header 顯示日期
- **AND** memory budget 仍 < 150 KB (24h 18.3KB + 7d 128KB = 146.3KB, K41 紅線
  守住)

#### Scenario: process 重啟空 strip

- **WHEN** main app 重啟, Timeline 視圖開啟
- **THEN** strip 顯示 13 row × 1440 cell 全 `Idle=0` (空狀態)
- **AND** SessionManager handle_event 第一個 event 進來後, 對應 cell 寫入
  `Working=1` 並觸發 Timeline 即時 refresh

### Requirement: R-CPT-2 — Timeline 視圖是第 6 視圖, 不取代既有 5 views, 對齊 MISSION 北極星 3 條

Timeline 視圖 MUST 跟既有 5 views (膠囊 / 展開面板 / Bot 總覽 / 事件診斷 /
設定) 並存, 不取代任何一個。對齊 MISSION.md 北極星 3 條:
- 單一膠囊統一監控 (膠囊常駐, Timeline 是展開後的第 6 view)
- 真實任務狀態 (Timeline 顯示 Working/Idle/WaitingForUser/Stale 4 state, 不顯示
  cost / token 數字)
- 0 切換成本 (1 strip 13 provider 同框, 1 眼看出 24h 活動分布)

#### Scenario: 第 6 視圖加進既有 5 views 切換

- **WHEN** user 點 Timeline 視圖 entry (e.g. tray menu / 快捷鍵 / 展開面板加按
  鈕)
- **THEN** main.js 切 `view='timeline'`, 顯示 `#timeline-view` 區塊
- **AND** 既有 5 views 完全不變 (膠囊 / 展開面板 / Bot 總覽 / 事件診斷 / 設定)
- **AND** 不取代膠囊, 膠囊 300×46 常駐屬性不破

#### Scenario: 對齊 CLAUDE.md 競品備忘 3 條邊界

- **WHEN** Timeline 設計檢視
- **THEN** 不做 cloud dashboard (Timeline 是本機 Tauri 視圖, 不開 port, 不接
  server)
- **AND** 不做 cost anomaly detection (Timeline 顯示 state distribution, 不顯
  示 token/cost 數字)
- **AND** 不做純 log reader (Timeline 用 SessionManager 即時累加, emit
  task-completed/waiting 同步觸發 hover detail)

### Requirement: R-CPT-3 — Timeline 護衛不破 K42 chain 17 條飽和契約 (M0 不加 test, M1 加 1 條獨立護衛)

Timeline spec MUST 走既有 K42 chain 17 條飽和契約, M0 階段**不加 test**。M1 階
段加 1 條獨立護衛 `timeline_ring_buffer_invariants` 收 closure 時 K42 chain
17→18, 需架構理由 doc (R114 R111+ chain 18 提案接力位置)。

#### Scenario: M0 守住 K42 chain 17 條

- **WHEN** M0 spec 落地
- **THEN** `cargo test --lib` baseline 443/443 持續綠 (M0 不動 code)
- **AND** K42 chain 17 條不變 (M0 spec-only, 0 test 新增)
- **AND** K41 chore_treadmill 24h < 30% 守住 (M0 spec-only, 0 chore commit)

#### Scenario: M1 加 1 條獨立護衛 chain 17→18

- **WHEN** R118+ M1 收 closure
- **THEN** 加 1 條 `timeline_ring_buffer_invariants` 護衛 test (走新 mod
  `timeline::tests`, 不擴既有 mod)
- **AND** K42 chain 17→18, baseline 443→444 (+1 護衛 test)
- **AND** 架構理由 doc 解 R114 後的 chain 18 (對齊 R114 R111+ 接力位置)

### Requirement: R-CPT-4 — Timeline 對齊 K0 既有 metric (不開新 OTel 維度, 不開新 data path)

Timeline 視圖 MUST 用既有 K0-A1/A2 metric (`lobsterpulse_provider_sessions` /
`lobsterpulse_provider_*{provider="X"}` 等 R102/R103 41 條 metric), 不開新
OTel 維度, 不開新 data path。對齊 R102 OTel/Prometheus contract spec closure。

#### Scenario: Timeline 不開新 OTel 維度

- **WHEN** Timeline emit 設計檢視
- **THEN** ring buffer 不 emit 新 Prometheus metric (e.g. 不加
  `lobsterpulse_timeline_*`)
- **AND** Timeline 視圖從 SessionManager 既有 provider_totals + state 累加拉資
  料, 不從 `/metrics` endpoint parse
- **AND** `LP_METRICS` const 41 條不變 (R103 對齊契約守住)

#### Scenario: Timeline 不開新 data path

- **WHEN** K0 Quota 推進檢視
- **THEN** Timeline 不開新 snapshot (e.g. 不讀 `usage-timeline-*.json`)
- **AND** K0 Quota K0-Q 9/13 持平 (Timeline 用既有 snapshot, 不開新 data path)
- **AND** K0-A1/A2 量化值不動 (Timeline 不 emit 新 sample, 需事件流過, 非本機
  scope)

#### Scenario: 4 state 對齊 session.rs SSoT

- **WHEN** Timeline 4 state 設計檢視
- **THEN** 4 state 命名對齊 session.rs `Idle / Working / WaitingForUser / Stale`
- **AND** u8 encoding 0/1/2/3 對齊 state 順序 (Idle=0, Working=1,
  WaitingForUser=2, Stale=3)
- **AND** 4 色用既有 theme token (--working-color / --waiting-color /
  --idle-color / --stale-color), 不破 theme 切換 (light/dark)
