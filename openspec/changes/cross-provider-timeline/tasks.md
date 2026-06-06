# Tasks: Cross-Provider Timeline View

> 對應 change: `cross-provider-timeline`
> 護衛 chain: 不擴張 (R67/R82 飽和契約 R113.1/R114 守住), 走 3 條獨立 test
> (M1 階段, 本 change M0 不加)
>
> **1 輪 1 件紀律**: 本 change 純 M0 spec 提案, **不寫 code**, 對齊 R108/R109/R114
> M0 closure 接力模式。R118+ owner follow-up 走實際 M1 實作。

## Phase 1: M0 spec 文件 (R117)

- [x] **T-CPT1: 寫 proposal.md** — Goal + Background (5 rounds 死循環 + R100 策略
  顧問 #3 closure source) + Scope (In/Out 11 條) + Capabilities 段齊
  - 驗證: proposal.md 含 4 段 (Goal/Background/Scope/Capabilities), 5 rounds 死循
    環根因分析段齊
- [x] **T-CPT2: 寫 design.md** — 24h strip 視覺 mockup + ring buffer 資料模型
  (18,720 cell / 18.3KB) + 整合點 (session.rs / lib.rs / main.js) + 護衛 (chain
  17 不擴張) + 5 條開放問題
  - 涵蓋 6 段: 視覺模型 / 資料模型 / 整合點 / 護衛 / 開放問題 / 對齊文件
  - 驗證: 6 段全列, 18,720 cell memory budget 對齊 K41 紅線, 護衛段明確「M0 不
    加 test, M1 加 1 條」
- [x] **T-CPT3: 寫 spec.md ADDED Requirements** — 4 個 Requirement + 8 個 Scenario
  - 涵蓋 4 個 Requirement: Timeline 24h × 13 provider ring buffer is the single
    source of truth for historical activity distribution / Timeline 視圖是第 6
    視圖, 不取代 5 views, 對齊 MISSION 北極星 3 條 / Timeline 護衛不破 K42 chain
    17 條飽和契約 (M0 不加 test, M1 加 1 條獨立護衛) / Timeline 對齊 K0 既有
    metric (不開新 OTel 維度, 不開新 data path)
  - 涵蓋 8 個 Scenario: 24h 解析度 / 7d 切換 / 13 provider 同框 / click-to-jump
    跨視圖 / 4 state 4 色 / hover detail / play/pause / process 重啟空 strip
  - 驗證: spec.md 4 個 Requirement + 8 個 Scenario 對齊
- [x] **T-CPT4: 寫 .openspec.yaml metadata** — schema/id/created/status/phase
  - 驗證: 4 metadata 欄位齊 + status=closed (M0 spec-only, 收 closure 同步 R108
    M0 模式) + phase=m0
- [x] **T-CPT5: 寫 tasks.md** — 本檔
- [x] **T-CPT6: engineering-log.md 加 R117 entry** — 紀錄 32 條佇列消化結果
  (5 大類: owner M WIP / 非本機 scope / owner-only 接力 / chain 17 飽和 / M0
  spec 開新) + cross-provider-timeline 提案理由 + 對齊 K40 7→8 + K42 17 守住
  - 驗證: engineering-log.md 結尾 R117 entry 落地 + K40 7→8 cell 對齊

## Phase 2: M1 接力範疇 (R118+, 不在本 change)

- [ ] **T-CPT7: session.rs 加 TimelineRing struct** — R118+ owner follow-up
  - 涵蓋 record_event / snapshot_24h 兩個 method + 18,720 cell 固定大小
  - 對齊 KNOWN_PROVIDERS SSoT (R114 pub const, hook_server.rs:39)
- [ ] **T-CPT8: session.rs handle_event 結尾串接 record_event** — 既有
  task-completed/waiting emit 之後
- [ ] **T-CPT9: lib.rs 註冊 3 個 Tauri command** — timeline_snapshot_24h /
  timeline_toggle_resolution / timeline_jump_to_event
- [ ] **T-CPT10: main.js 加第 6 視圖 view='timeline'** + HTML `#timeline-view` 區塊
  + CSS 沿用 theme token (--working-color 等)
- [ ] **T-CPT11: 加 1 條獨立護衛 test `timeline_ring_buffer_invariants`** — M1
  收 closure 同步 K42 chain 17→18, 需架構理由 doc (R114 R111+ chain 18 提案接
  力位置)
- [ ] **T-CPT12: 跑 cargo test --lib 確認 baseline 守住** — chain 17→18 後 baseline
  +1, K0 量化值不動
- [ ] **T-CPT13: 跑 python scripts/k0_measure.py** — K0 Quota 9/13 持平 (Timeline
  用既有 snapshot, 不開新 data path), K0-A1/A2 不動 (需事件流過, 非本機
  scope)
- [ ] **T-CPT14: engineering-log R118 R-CPT closure entry** — 收 closure + 接力
  R119+ (Timeline 編輯 / cost heatmap 提案 etc)
