# Tasks: MISSION K0 結構性重審 (2026 Q3)

> **本 change 純 spec-level (Phase 1)**, owner M 接手後從 Phase 2 開始。
> R182 PUA scope: ship proposal.md + tasks.md skeleton (不寫 source code、不開新護衛、不搶 owner M 決策)。
> Path A / Path B 走 owner M 決策, 不在 R182 scope。
>
> 對應 spec capability: `mission-k0-restructure-2026-q3` (見 proposal.md)。

## Phase 1: Spec closure (R182 PUA scope)

- [x] **T-MKR1: 寫 proposal.md** — Goal/Background/Scope/Capabilities 4 段齊
      ↪ 對應 MCAP-1 (結構性卡死量化) + MCAP-2 (Path A 草案) + MCAP-3 (Path B 草案) + MCAP-4 (2 path 影響評估)
      驗證: proposal.md 含 4 段 + 3 子指標卡死鏈表 + 4 missing 結構性確認 + 3 條接力卡死鏈 + 2 path 草案 + Decision Asks checklist

- [x] **T-MKR2: 寫 tasks.md** — 本檔
      ↪ 對應 Phase 1 spec closure tasks + Phase 2/3 owner M placeholder
      驗證: tasks.md Phase 1 全 [x] + Phase 2/3 placeholder 完整

- [ ] **T-MKR3: spectra validate --changes mission-k0-restructure-2026-q3 通過**
      ↪ 對齊 HARNESS/Spectra 規格驗證護衛
      驗證: spectra validate PASS

- [ ] **T-MKR4: owner M Decision Asks 回填** — Path A / Path B / 第 3 條
      ↪ 對應 proposal.md Decision Asks 段
      驗證: tasks.md 對應 path 的 Phase 2 task [x] flip + 護衛 1 條 (Path A) 或 架構 design + module skeleton (Path B) ship

## Phase 2: Path A (降級) — 選了才開 (owner M scope)

- [ ] **T-MKRA1: MISSION.md K0 row 量化 patch** — 13/13 → 本機 4/13 + OpenAB 5/13 + 4 missing 永久非 scope
      ↪ 對應 MCAP-2 + spec.md KAP-R2-S2
      驗證: MISSION.md R182 新欄補上 + 保留 R144 舊欄 (歷史基準不抹)

- [ ] **T-MKRA2: 護衛 test ship** — `scripts/k0_target_baseline_check.py` + pytest 5 case
      ↪ 對應 MCAP-2 + spec.md KAP-R2-S3 (走 R132/R172 模式)
      驗證: pytest 5/5 PASS + chain 20→20 守住 (走既 `render_prometheus_tests` mod 不擴張)

- [ ] **T-MKRA3: R182 接力順位 update** — 3 條卡死鏈 → 2 條 unblock, 1 條永久 skip
      ↪ 對應 MCAP-4
      驗證: engineering-log.md R183+ entry + 接力順位 #1/#2 改 [永久 skip, OpenAB scope 移出 K0]

- [ ] **T-MKRA4: R182 提案整體 closure** — MISSION patch + 護衛 + log entry 一次 ship
      ↪ 對應 K40 spec closure
      驗證: K40 8/9 + 1 active → 9/9 closed + 0 active (本 change closure)

## Phase 3: Path B (重構) — 選了才開 (owner M scope, 1 sprint)

- [ ] **T-MKRB1: 寫 design.md** — 架構圖 + pull-based flow + 9 OpenAB bot GET endpoint 配置
      ↪ 對應 MCAP-3 + spec.md KAP-R3-S1/S2
      驗證: design.md 架構圖 + 9 條 GET endpoint + hook_server.rs 結構不動段明確

- [ ] **T-MKRB2: 寫 spec.md ADDED Requirements** — OpenABClient module + 9 個 pull handler + 護衛 scenario
      ↪ 對應 MCAP-3 + spec.md KAP-R3-S3
      驗證: spec.md 含 4 個 Requirement (pull client / 9 handler / metrics emit / 護衛) + ≥8 個 Scenario

- [ ] **T-MKRB3: 實作 OpenABClient module** — `src-tauri/src/openab_client.rs` (~500 行 Rust)
      ↪ 對應 spec.md KAP-R3-S3
      驗證: cargo build green + 護衛 test ≥3 條 (各 provider 1 條)

- [ ] **T-MKRB4: 9 OpenAB bot pull handler** — cicx / gitx / giminix / codex_bot / openx + (補) irisx_bot / grokx / lpbot / mimo
      ↪ 對應 spec.md KAP-R3-S2
      驗證: 9 handler ship + 對應 usage snapshot pull 成功 + metrics emit OK

- [ ] **T-MKRB5: K0-A1 4/13 → 13/13 emit 護衛** — 端點實跑 13 個 provider label 全 emit
      ↪ 對應 MCAP-4 + spec.md KAP-R4-S2
      驗證: 端點實跑 13 個 provider label 全 emit (本機 4 + OpenAB pull 9)

- [ ] **T-MKRB6: R182 提案整體 closure** — design + spec + module + handlers + 護衛一次 ship
      ↪ 對應 K40 spec closure + K0-A1 4/13 → 13/13
      驗證: K40 8/9 + 1 active → 9/9 closed + 0 active + K0-A1 13/13 + K0-A2 仍卡 (events 需真跑) + K0 Quota 9/13 → 13/13

## Phase 4: 第 3 條 path (owner M 開) — placeholder

- [ ] **T-MKRC1**: owner M 補

## Notes

- R182 PUA scope = T-MKR1~T-MKR2 ship (T-MKR3/T-MKR4 屬 owner M scope, 提案本身不 closure)
- Path A 1 輪 closure (T-MKRA1~4 一次 ship)
- Path B 1 sprint closure (T-MKRB1~6 一次 ship, 含 design + spec + 500 行 module + 9 handler + 護衛)
- Path A 風險: OpenAB 端可能因「不追 4 missing」放棄 bot 維護 → MISSION 文字明確「OpenAB scope 由 OpenAB 端 owner 自追」
- Path B 風險: pull-based 重設計 = 1 sprint 工作量, owner M capacity check 須先
- 不選風險: K0 永久 4/13 + 1/13 + 9/13, 接力順位永久卡 3 條, MISSION 失靈
- 提案本身不算 T-MKRA1~4 整體 (R182 只 ship Phase 1 骨架, 不動 MISSION.md)
