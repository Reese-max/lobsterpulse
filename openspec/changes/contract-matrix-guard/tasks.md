# Tasks: Provider Contract Matrix Guard (13×3)

> 來源: R100 策略顧問 (2026-06-04) 行動 #2 follow-up — 開新 change 補 13 provider × 3
> attribute matrix 護衛。R67 (2026-06-03) 護衛 chain 16 (config.rs:909) 只護 keys
> 對稱, R106 護 value 對齊, 兩條並存互補。
>
> 對應 spec capability: `contract-matrix-guard` (見 `specs/contract-matrix-guard/spec.md`)。
> 每個 task 對應到 spec.md 的 Requirement 行 (以「↪ R-XXX」標註)。

## Phase 1: Spec + 護衛 test 落地

- [x] **T-MTX1: 寫 proposal.md** — 目標 + 背景 + 範圍 + capabilities 段齊 (R106 完成)
  - 驗證: proposal.md 含 4 段 (Goal/Background/Scope/Capabilities)
- [x] **T-MTX2: 寫 design.md 13×3 期望值表** — 13 row 完整表 + 3 attribute 設計理由
  - 涵蓋 4 本機 CLI + 9 OpenAB bot = 13 row, 每 row 標 name prefix / enabled_default /
    sound_file / waiting_sound_file / OPENAB_BOT_IDS member
  - 驗證: 13 row 全列、特殊情況 (mimo disabled / codex-bot 共用 sound / 3 CLI 無 sound) 標註
- [x] **T-MTX3: 寫 spec.md ADDED Requirements** — 4 個 Requirement + 6 個 Scenario
  - 涵蓋 4 個 Requirement: CONTRACT const is the single source of truth for 13×3 matrix /
    name prefix / enabled_default / sound file mapping
  - 6 個 Scenario 對齊 13 / 3 attribute / 4 sync points
- [x] **T-MTX4: 寫 .openspec.yaml metadata** — schema/id/created/status/phase
  - 驗證: `.openspec.yaml` 4 個 metadata 欄位齊 + status=open
- [x] **T-MTX5: 寫 tasks.md** — 本檔
- [x] **T-MTX6: config.rs module-level 加 provider_contract_matrix_tests + 1 條護衛 test**
  - 涵蓋 spec.md Requirement: 13×3 matrix 對齊 4 同步點
  - 驗證: `cargo test provider_contract` 1/1 pass

## Phase 2: Spec closure

- [ ] **T-MTX7: 收 closure** — tasks.md 6 個 [x] 全勾 + .openspec.yaml status=closed + phase=1/1
  - 涵蓋 spec.md Requirement: 護衛 1 條 test 守住 + 同一 commit 同步更新 CONTRACT + 4 同步點
  - 驗證: tasks.md `grep -c "^- \[x\]"` = 6; `.openspec.yaml` status=closed
- [ ] **T-MTX8: 護衛 test 寫入 engineering-log.md** — R106 紀錄 + KPI 進展表
  - 驗證: engineering-log.md 有 `### [2026-06-05] Round 106` 段, 含 KPI 進展表 ≥1 列

## 不在本 change scope（列為 follow-up）

- ❌ 改既有 R67 護欄 (config.rs:909) — R67 護 keys 對稱, R106 護 value 對齊, 兩條並存
- ❌ 動 4 同步點本體 — R106 只驗對齊, 不修對齊源
- ❌ 加新 provider — 走 MISSION.md 納入標準 review
- ❌ 接 OTel SDK / 6 條 counter 命名 — 留 R103+ follow-up
