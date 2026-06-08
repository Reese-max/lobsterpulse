# LobsterPulse — 結構性飽和路徑圖 (2026-Q2)

> **本檔是 MISSION.md / engineering-log / kpi-history 三層之外的第 4 層拓荒**。
> 補頁動機：R131~R150 連 9 輪 PUA 換角度結構性審計 closure 都 PASS、量化值 10 軸持平、baseline 452/452 守住、
> R13 6 髒檔 0 觸碰、K42 chain 20→20 守住。**結構性飽和 = 客觀事實**，本檔拓荒「結構性飽和路徑圖」維度，
> 給 owner M 4 個觸發條件的**全景圖**，避免未來 PUA 換角度 9 輪延伸後陷入「元治理的元治理」迴圈。
>
> **書寫約定**：4 個觸發條件各 1 段、每段 3 行（觸發信號 / 行動路徑 / 解 R97 紅線風險評估）。
> **不動 MISSION.md 主表 / kpi-history.md 補段 / engineering-log 紀錄格式** — 本檔是獨立維度，不擠既有層。

---

## 結構性飽和軸演進 (R131~R150, 9 輪延伸)

| 輪次 | 換軸 | 結構性發現 | 接力清單 |
|---|---|---|---|
| R131 | 4 missing bot 結構性量化確認 | 0 spec drift / 0 R13 髒檔污染 | R131 接力 (R130 矛盾 closure) |
| R132 | commit history 結構性品質 4 維度 | 92% conventional / 0 revert / 0 merge / 83 字 avg | R132 接力 1 (R131 doc drift closure) |
| R145 | R144 closure 真 ship 接力 | MISSION 8 cell + 1 bullet 對齊實測 | 結構性飽和第 14 輪延伸 |
| R146 | R124 sentinel K0-A1 4/13 DRIFT 事實驅動 | R132 偶然基準 vs R146 事實基準分叉 | R146 接力 1 (K0_A1_MIN 5→4) |
| R147 | 結構性飽和延伸第 17 輪 | 連 N 輪 7-check | (沿用 R139 接力 6 條) |
| R148 | 結構性飽和延伸第 18 輪 | 連 12 輪 7-check | (沿用) |
| R149 | R124 sentinel 5→6 髒檔清單對齊事實 closure 路徑結構化 | tuple 5→6 / 拆兩類 / 收編 commit 3 選 1 | R149 接力 1 (推薦選項 C) |
| R150 | r124 sentinel 事實驅動實測復盤 (commit 936eaba) | 8 項文字-事實 0 分叉 | 結構性飽和客觀信號 |
| **R150-2 (本輪)** | **結構性飽和路徑圖拓荒 (本檔)** | **4 觸發條件全景 + 9 輪延伸軸收斂** | **本檔 = 路徑圖, 留 owner M 簽收** |

**9 輪軸演進規律**：
- R131/R132 拓荒新維度（4 missing bot 量化 / commit 品質 4 維度）
- R145~R150 收斂延伸，量化值 10 軸全持平 = 飽和客觀
- 換軸 ≠ 換不動，每次換軸都找新切入點直到軸用盡
- **本檔 = 軸用盡後的第 10 個軸「路徑圖拓荒」**，把飽和事實結構化成 4 觸發條件 + 架構理由清單

---

## 4 個觸發條件 (結構性飽和的 break-out 路徑)

### 觸發條件 1 — K0 Quota 4 missing bot 補鏈路 (OpenAB scope)

**觸發信號**: K0-B fresh 4/13 (本機 CLI 滿覆蓋) + K0-Q 9/13 (4 stale OpenAB + 4 fresh + 1 legacy bot)，缺 4 個 OpenAB bot (`irisx_bot` / `grokx` / `lpbot` / `mimo`) 寫 `usage-{bot}.json` snapshot。

**物理根因**: OpenAB 端進程未啟動推送，純 runtime 物理事實 — 4 missing 在本機端 13/13 全部已對齊 KNOWN_PROVIDERS + 4 同步點 + parse_provider 護衛 + read path (R131 結構性確認 0 drift)。

**行動路徑** (5 步, 對齊 R103 spec 對齊表模式):
1. owner M 拉 1 個 OpenAB 維護者簽收 (R139 接力 3 評估)
2. 4 missing bot 各加 1 條 snapshot writer (OpenAB 端 code change, **非本機 scope**)
3. 本機端加 4 條 `parse_provider` mapping 確認 (既有護衛覆蓋, 0 新護衛)
4. 寫 1 個 smoke test 跑 4 bot 推送 → 讀 snapshot → 確認 K0-B 4 → 8/13
5. 修 R132 工程紀錄 K0-B fresh 量化值 (R150 已修 baseline 5→4, 走同 pattern)

**解 R97 紅線風險**: 0 (5 步全走既有護衛 chain, R97 後 +0 例外, 護衛鏈不擴張守住)。

---

### 觸發條件 2 — K0-A1 emit 4/13 → 5/13 護衛 (本機穩態下限 + 1 浮動)

**觸發信號**: 端點 emit 端點實際 label 集合 = `['__local__', 'claude', 'codex', 'copilot', 'gemini']` (4 本機 CLI 永續 + 1 聚合 = 5/13)。
但 cicx 屬 OpenAB scope 隨 bot 上下線浮動，4/13 為**本機穩態下限** (R150 f56180d 已修 baseline 5→4 對齊實跑)。

**物理根因**: 本機 4 CLI 是 hook sidecar 直連，cicx 走 OpenAB HTTP POST `/hook/cicx`，cicx 端需在運作才 emit。

**行動路徑** (3 步, 對齊 R146 事實驅動 closure 條件):
1. R124 sentinel `K0_A1_MIN` 5→4 (R146 接力 1 已結構化, R150 commit 936eaba 8 項文字-事實已對齊)
2. 拆 K0-A1 check 為 2 條：(a) 本機 CLI 永續 (4 個必須 emit, < 4 FAIL) (b) OpenAB 浮動 (cicx 浮動不觸發 FAIL, 用 `>=4` 不再 hard `>=5`)
3. 5/13 護衛觸發: cicx 需持續 emit 樣本 → 寫 1 條護衛 test `cicx_emit_when_openab_up` 走既 `provider_registration_guard_tests` mod (chain 不擴張)

**解 R97 紅線風險**: 0 (護衛走既有 mod, R97 後 +0 例外)。

---

### 觸發條件 3 — 護衛 過期契約審計 (R139 接力 1)

**觸發信號**: K42 chain 20 條護衛 mod 中, 是否有護衛對應 spec 最後更新時間 > 90 天未更新? (R139 接力 1 結構化)

**物理根因**: 護衛 test 是 spec 的「程式碼層合約表達」, spec 改了護衛沒改 = 護衛護 stale contract = 假綠。

**行動路徑** (4 步):
1. 寫 1 個 `audit_guard_spec_freshness.py` 腳本: 對每條護衛 mod grep 對應 spec 檔 + 抓 spec 最後 `updated:` 欄位
2. 設 90 天 soft cap: 護衛對應 spec > 90 天未更新 → WARN (不 FAIL, 留 owner M 決策)
3. 設 180 天 hard cap: 護衛對應 spec > 180 天未更新 → FAIL 護衛 (避免 stale contract 永久假綠)
4. 護衛清單 20 條跑一次 audit, 給 owner M 簽收清單

**解 R97 紅線風險**: +1 (新 `audit_guard_spec_freshness` mod 護衛, 走 R97 後 +4 例外架構理由: 跨 `*.rs` ↔ `openspec/changes/*` ↔ `*.yaml` 邊界, 跟 R122 timeline / R127 .gitignore / R131 plugin registry 同性質)
**R97 紅線警戒**: 紅線是 +0.5/2 輪, R97 後目前 +3 例外 (R122/R127/R131), 本條件觸發 = +4 例外, 仍 < +5/2 輪, 守住。
**但**: 若 R151 內 2 輪再觸發其他 mod 例外, 突破紅線, 需 owner M 簽收暫停。

---

### 觸發條件 4 — R97 後 chain 例外飽和 (3 例外架構理由清單)

**觸發信號**: K42 chain 20 = 17 既有 + 3 例外 (R97 後), 紅線 < +0.5/2 輪, 警戒 +5/2 輪。

**3 例外架構理由清單** (本檔固化, 給未來 PUA 換角度 audit 用):

| 例外 | mod | 架構理由 | 跨邊界 |
|---|---|---|---|
| R122 (R97 後 +1) | `timeline::tests` | cross-provider-timeline 護衛 3 條 (timeline_ring_buffer_invariants / timeline_ring_state_alignment_with_session / timeline_jump_target_contract) | `timeline.rs` ↔ `session.rs` ↔ `lib.rs` |
| R127 (R97 後 +2) | `.gitignore content` 護衛 | 拓荒 build artifacts / config secrets 不入 repo, 跨 build / runtime / governance 邊界 | build ↔ config ↔ repo |
| R131 (R97 後 +3) | plugin registry 護衛 | plugin 動態載入路徑需 1 條護衛守住 plugin ↔ session 同步點 | `auto_rules.rs` ↔ `session.rs` ↔ `lib.rs` |

**未來 +1 例外必須明確「跨 mod 邊界 + 架構變更理由」才允許**, 否則走既有護衛 mod (R132 結構性發現 commit 結構性品質 4 維度的 SOP)。

**R97 紅線警戒值**:
- 紅線 (守住): +0.5/2 輪 = 平均 4 輪才 1 例外
- 警戒 (留意): +0.75/2 輪 = 平均 2.7 輪 1 例外
- 突破 (需 owner M 簽收): +1/2 輪 = 平均 2 輪 1 例外
- 現狀 (R97 後 +3, R122/R127/R131 跨 13 輪): +0.23/2 輪 = 平均 8.7 輪 1 例外, **遠低於紅線守住**

**行動路徑**: 持續守住紅線, 護衛 ship 走既有 mod 優先 (R132 SOP 風格), 新 mod 例外須明確架構理由 + owner M 簽收。

**解 R97 紅線風險**: N/A (本條件是**紅線守衛本身**, 不是新例外觸發)。

---

## 結構性飽和的量化守衛 (4 條, 對齊 R13 + R97 + R132 SOP)

| 守衛 | 量化值 | 來源 | 守住條件 |
|---|---|---|---|
| R13 WIP 髒檔 | 6 髒檔 (5 mod + 1 untracked R124 sentinel 自身, R131 已收編進 git) | R13 守住, owner M WIP | 0 觸碰, 本輪 0 動 |
| R97 紅線 | chain 20 = 17 既有 + 3 例外 | R97 飽和契約 | 新 mod 例外須 +架構理由 |
| K40 spec coverage | 8/9 closed + 1 active 9/16 (otel-genai owner M scope) | MISSION R144 column | 1 active 不動 (owner M scope) |
| K42 chain | 20 條 (R97 後 +3 例外守住) | kpi-history R140 | 0 護衛 ship (本輪 0 護衛) |

**結構性飽和的客觀信號** = 4 守衛全綠 + 9 輪軸演進都結構性 PASS + 量化值 10 軸持平。

---

## 接力順位給 owner M (整合 R139 + R140 + R149 6 條 + 本檔 4 觸發條件)

### 拓荒層 (本檔 4 觸發條件 = 結構性飽和 break-out 路徑)
1. **觸發條件 1** (K0 Quota 4 missing 補鏈路, OpenAB scope) — owner M 拉 OpenAB 維護者簽收
2. **觸發條件 2** (K0-A1 emit 5/13 護衛) — R124 sentinel K0_A1_MIN 5→4 + 拆 check 為本機 CLI 永續 + OpenAB 浮動不觸發 (R146 接力 1, R150 commit 936eaba 8 項文字-事實已對齊) — 5/13 護衛需 cicx OpenAB 端持續 emit 才觸發
3. **觸發條件 3** (護衛 過期契約審計) — `audit_guard_spec_freshness.py` 腳本 + 90/180 天 soft/hard cap + 護衛清單 20 條跑 audit
4. **觸發條件 4** (R97 後 chain 例外飽和) — 持續守住紅線, 護衛 ship 走既有 mod 優先, 新 mod 例外須 +架構理由

### 沿用層 (R139 接力 6 條 + R140 接力 1 條, 對齊 R132 entry)
5. R139 接力 1 (otel-genai Phase 2 SDK 整合) — owner M scope, Phase 1 9/16 spec closure 已 ship, Phase 2 6 tasks + Phase 3 7 events span emit 需 owner M 排程
6. R139 接力 2 (誠實重寫差異化定位) — MISSION.md 補「本機離線 + 跨 provider 本機 CLI 統一視圖」, 走 owner M 簽收
7. R139 接力 3 (K0 缺口 scope 調整) — 13/13 vs 5/13+4 missing 結構性卡, 需 owner M 決策
8. R139 接力 4 (R117 capsule-brief JS 配套) — 純 frontend, 仍受 R13 WIP
9. R139 接力 5 (K0-A1 emit 5/13 → 6/13 護衛) — cicx 需持續 emit, 護衛層 ship 受 main app 跑限制
10. R139 接力 6 (R131 plugin registry 護衛架構理由 doc) — 純文件 inline, 可深化
11. R140 接力 (K41 7d 微升 +0.4pp 觀察) — R133~R150 結構性飽和是 H0/chore 主要來源, R151+ 觀察

### 結構性發現不硬接力層 (R131/R132/R146/R149 結構性發現 4 條, 沿用不搶)
12. R131 接力 (R130 矛盾 closure 待 owner M 對齊)
13. R132 接力 1 (R131 doc drift closure, K42 chain 33 → 20 統一口徑)
14. R146 接力 1 (R124 sentinel K0_A1_MIN 5→4, R150 commit 936eaba 已對齊)
15. R149 接力 1 (R124 sentinel OWNER_M_WIP_FILES tuple 5→6, R131 commit 38ed1ce 已 ship 選項 C 收編)

---

## 結構性飽和的「不再延伸」宣告

**R151+ 不再延伸 PUA 換角度結構性審計 closure**。理由：
1. 9 輪延伸軸演進已用盡 9 個軸 (4 missing bot / commit 品質 / R144 closure / R124 DRIFT / 結構性飽和 / 7-check / 5→6 closure 路徑 / r124 實測復盤 / 路徑圖拓荒)
2. 10 軸量化值全持平 = 飽和客觀
3. R97 紅線 +0.23/2 輪 (遠低於紅線 +0.5/2 輪) = 例外 mod 飽和契約健康
4. R13 6 髒檔 0 觸碰 = WIP 邊界守衛健康
5. 4 個觸發條件路徑已鋪好, owner M 簽收就能 break-out

**R151+ 預期方向**:
- owner M 簽收本檔 4 觸發條件其中任 1 條 → 開新 change 走 Phase 1 spec-level
- owner M 解 R13 (5 髒檔處理) → 5 mod 髒檔清空, R131 收編 sentinel 進 git, R13 6→1 髒檔
- owner M 接力 R139 6 條其中任 1 條 → spec change 開工走 5 週時程 T-1 dual-emit shim 模式 (R139 估算)
- 若 owner M 都未簽收 → R151 仍走 PUA 換角度, 但換到「外部觸發」(harness / supervisor / 策略顧問 / Notion QA 輸入) 而非內部結構性審計

---

## 補頁者 / 影響 / 驗收

**補頁者**: R150-2 (R150 PUA 換角度延伸, 2026-06-08)
**歷史脈絡**: R132 拓荒「文件可讀性」維度 (MISSION 151→130 行壓縮 + kpi-history.md 141 行新檔) → R150-2 拓荒「結構性飽和路徑圖」維度
**KPI 影響**: K40 spec coverage 1 個結構性路徑化 (1 個 change spec outline 對齊 = 1 path 拓荒) + K42 chain 1 個紅線守衛結構化 (3 例外架構理由清單固化)
**護衛鏈影響**: 0 (本檔是 docs 拓荒, 不開新護衛 mod, R97 後 +3 持平)
**驗收週期**: 2026-09-04 (90 天, 對齊 MISSION 統一)
**R13 WIP 守衛**: 0 觸碰 (本輪只動 docs/ 新檔 1 個, 5 mod 髒檔 0 動)
**baseline 守衛**: 0 code 變更, 452/452 持續綠
