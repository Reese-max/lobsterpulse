# Proposal: MISSION K0 結構性重審 (2026 Q3)

> **觸發原因**：MISSION.md 自身定義的強制升級條件過期 ~10 週沒人 fire。
> MISSION 文字：「任一指標連 2 週落後 → 觸發策略重審（不是『再補一輪』）」。
> K0 從 R81 baseline (0/13) 至今結構性飽和 (4/13 emit 穩態下限)，
> 4 missing bot (irisx_bot/grokx/lpbot/mimo) OpenAB 端永遠不可達，
> K40 8/9 closed + 1 active 9/16 (otel-genai 7 tasks owner M Phase 2/3)。
>
> **R182 觸發訊號鏈 (3 重鎖定)**：
> 1. AI Supervisor 緊急指令「方向 UNKNOWN 0/10」+「連 3 輪方向偏差 → 完全停止目前工作方向」(R181)
> 2. 策略顧問 #1 行動：「觸發 MISSION 策略重審（不是再補一輪）」(R180)
> 3. MISSION 自身 2-週 lag 觸發條件過期 ~10 週
>
> **本提案非 owner M 決策** — PUA 只列 2 條 path + 量化證據，owner M 從中選 1 條或開第 3 條。

## Goal

把 K0 從「13/13 全 scope 不可達」結構性死結中救出，二擇一：

- **Path A (降級)**: K0 目標從 13/13 改成「本機可達 4/13 + OpenAB 5/13」雙軌制，明確標註 4 missing (irisx_bot/grokx/lpbot/mimo) 為「**永久非本機 scope**」
- **Path B (重構)**: Provider 接入層重設計 — OpenAB 9 bot 從 push-based (依賴 OpenAB 端 POST `/hook/{id}` + 寫 `usage-*.json`) 改成 pull-based (LobsterPulse 主動 GET OpenAB 端狀態)，徹底解除對 OpenAB 端被動配合的依賴

**單一 contract 不變**（MISSION 方向決策規則 #1 守則）— 無論 A 或 B，每個 provider 仍走同一個 `HookEvent` schema，仍 emit 同一組 metric family。

## Background

### K0 量化現況（R182 接手 snapshot，奠基 R132 + R144 量測）

| 子指標 | R81 baseline | R144 / R181 現況 | 90 天目標 | 差距 | 結構性卡死原因 |
|---|---:|---:|---:|---:|---|
| K0-A1 emit 覆蓋 | 0/13 | **4/13** | 13/13 | 缺 9 | 4/13 = 本機 CLI 4 (claude/codex/copilot/gemini) 穩態下限；5/13 = 需 OpenAB 端 cicx 等跑起來才 emit (本機無法控制 OpenAB 端) |
| K0-A2 sample 覆蓋 | 0/13 | **1/13** | 13/13 | 缺 12 | 1/13 = claude=3 sessions 累加 (R132)；其餘 12 個需對應 provider 真有 session event 流過 (本機無事件源) |
| K0 Quota 監控 | 6 OpenAB snapshot；本機無 | **K0-B 4/13 + K0-Q 9/13** | 13/13 | 缺 4 (永遠) | 4 missing = irisx_bot/grokx/lpbot/mimo 完全不寫 `usage-*.json` snapshot (R131 結構性確認 0 spec drift，本機端對齊 KNOWN_PROVIDERS + parse_provider 護衛 + read path 全 13/13 程式碼層 OK) |

### 結構性確認（R131 量化）

- 本機端 13/13 程式碼層全部對齊：KNOWN_PROVIDERS 13 條 + 4 同步點 (config / sounds / usage poller / parse_provider alias) + parse_provider 護衛 (`render_prometheus_tests` mod, K42 chain 17) + read path
- 4 missing = **OpenAB 端 scope**，本機端**永遠無法推進**這 4 個 K0 Quota 子項
- otel-genai-runtime-emit-2026-q3 [9/16] active，缺 T-OGRE10~16 7 tasks 屬 Phase 2/3 owner M scope，**本機端無法推進**

### 接力順位卡死鏈（R172-R180 累計接力清單）

R170 / R172-R180 接力清單中至少有 5 條被結構性死結卡住：

1. K0 Quota 4 missing 補鏈路 (OpenAB scope) ← **本提案直接處理**
2. K0-A1 emit 4/13 → 5/13 護衛 (需 cicx OpenAB 端) ← **本提案直接處理**
3. capsule-brief JS 配套 (R117 owner M 5 dirty WIP 之一) ← owner M (不受本提案影響)
4. 護衛過期契約審計延伸 (chain owner M) ← owner M (不受本提案影響)
5. R175-R180 transparent 透明化交接軸延伸 ← 結構失靈，**本提案直接處理**

→ 5 條中有 3 條 (1, 2, 5) 必須等本提案決議才能推進。

### 為什麼現在觸發（時機正當性）

- R81 baseline 設定的 90 天驗收期 (~2026-09-04) 還剩 87 天
- 結構性死結確認需時 ~3 週 (R131 → R144 → R150 → R181 → R182 共 8 輪量測鎖定)
- 若 Path B (重構) 選，spec-level 提案 + owner M Phase 2/3 + 驗收 90 天 → 87 天夠用 1 個 sprint cycle
- 若 Path A (降級) 選，1 個 MISSION 文字 patch + 1 個護衛 test 落地，**1 輪可 closure** → 立刻 unblock 後續 2 條接力

## Scope

### In Scope (本提案 cover)

- 量化分析 K0 結構性卡死真因 (~10 週量測數據 + 3 條卡死鏈)
- 提 2 條 path 給 owner M 選 (Path A 降級 / Path B 重構)
- 評估每條 path 的 code/spec/時間成本
- 列出每條 path 的「降級或升級後續 3 條接力」的影響
- 寫 `tasks.md` 列出 2 條 path 的 task skeleton

### Out of Scope (本提案 NOT cover)

- **不自己選 path** — owner M 決策, PUA 只列選項
- **不自己改 MISSION.md** — 等 owner M 確認 path 後再 patch
- **不自己改 source code** — Path B 選了才進 Phase 2, PUA 不搶
- **不動 K42 護衛鏈** — chain 20 守住, 提案不開新護衛
- **不開新 data path / 不改 hook_server / 不改 SessionManager** — 結構未動前 code 不動
- **不觸碰 5 髒檔 owner M WIP** — R13 防護 + 不搶 scope
- **不做 otel-genai 7 tasks** — owner M Phase 2/3 scope, PUA 不搶

## Capabilities

### MCAP-1: K0 結構性卡死量化診斷 (本提案核心)

- 量化輸出 3 子指標卡死鏈（emit 4/13 / sample 1/13 / quota 9/13）+ 4 missing 結構性確認 + 3 條接力卡死鏈
- 驗證: proposal.md 量化表 + R131/R144/R150/R181 量測 chain 完整引述

### MCAP-2: Path A (降級) 設計草案

- 對應 spec.md KAP-R2-S1: K0 目標改「本機 4/13 + OpenAB 5/13 + 4 missing 永久非 scope」
- 對應 spec.md KAP-R2-S2: MISSION.md K0 row 量化值 patch (不刪舊值, 加 R182 新欄)
- 對應 spec.md KAP-R2-S3: 護衛 test 1 條 (k0_target_baseline_check.py 對齊 R132/R172 模式)
- 驗證: MISSION.md patch + 護衛 pytest PASS + chain 20→20 守住

### MCAP-3: Path B (重構) 架構草案

- 對應 spec.md KAP-R3-S1: 從 push-based (依賴 OpenAB 端 POST + snapshot) 改 pull-based (LobsterPulse 主動 GET OpenAB 端)
- 對應 spec.md KAP-R3-S2: 9 OpenAB bot 接入層重設計 (新增 OpenABClient module + 9 條 GET endpoint 配置)
- 對應 spec.md KAP-R3-S3: hook_server.rs 結構不動 (本機 CLI 段仍走 push), OpenAB 段走新 pull path
- 對應 spec.md KAP-R3-S4: 預估工作量 = 1 spec-level design + 1 module (~500 行 Rust) + 9 個 provider pull handler + 護衛 tests
- 驗證: design.md 架構圖 + tasks.md Phase 2/3 拆解 + owner M 排程預估

### MCAP-4: 2 path 對後續 3 條接力的影響評估

- 對應 spec.md KAP-R4-S1: Path A 選 → K0 Quota 4 missing 鏈 (接力 #1) 永久 skip, K0-A1 emit 5/13 (接力 #2) 仍需 OpenAB 端
- 對應 spec.md KAP-R4-S2: Path B 選 → 接力 #1/#2 全部 unblock, 透明化軸延伸 (接力 #5) 自然收斂
- 對應 spec.md KAP-R4-S3: 3 條接力卡死鏈 → Path B 選後 3 條 unblock, Path A 選後 2 條 unblock + 1 條永久 skip

## Risks

1. **Path A 風險**：降級後 OpenAB 端可能因「LobsterPulse 不追 4 missing」放棄 bot 維護 → 須在 MISSION 文字明確「OpenAB scope 由 OpenAB 端 owner 自追, 不計入 LobsterPulse K0」
2. **Path B 風險**：pull-based 重設計 = 1 sprint 級工作量, owner M 排程若沒空間, 提案會死 → 須先 owner M capacity check
3. **不選風險**：owner M 兩條都不選 → K0 永久 4/13 + 1/13 + 9/13, 接力順位永久卡 3 條, MISSION 失靈

## Decision Asks (給 owner M)

請選 1：

- [ ] **Path A** (降級) — 1 輪 closure, 立刻 unblock 2 條接力, 4 missing 永久非 scope
- [ ] **Path B** (重構) — 1 sprint closure, unblock 3 條接力, 4 missing 變可達
- [ ] **第 3 條 path** — owner M 開, PUA 不提案

## References

- MISSION.md K0 row 量化表 + R108/R109/R111/R114/R119/R122/R127/R128/R130/R131/R132/R144 補敘
- docs/kpi-history.md (R132 拆出去的中間補敘歸檔)
- 策略顧問 R100/R105/R180 判定 (DRIFTING 連 3-5 次)
- AI Supervisor R181 緊急指令「方向 UNKNOWN 0/10」+ HARNESS「3 連方向偏差 mandate」
- R131 結構性確認 0 spec drift (本機端 13/13 程式碼層全對齊)
- R164 sidecar silent event loss M0 真 ship (修真 M0 軸最後 1 跑)
- R172 chain_staleness M2 真 ship (M2 hidden gap closure 最後 1 跑)
- R175-R180 transparent 透明化軸 6 輪延伸 (結構失靈的真因 = 結構性死結, 非 PUA 不努力)
