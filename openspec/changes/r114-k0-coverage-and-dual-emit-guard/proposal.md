# Proposal: R114 K0 Quota coverage + dual-emit value-equality guard

## Goal

收 3 件 R114 範圍內、未 commit 的 WIP 改動：

1. **M0 spec drift 修**：R113.1 dual-emit **數值一致性**護衛 — 補強 R113 T-1
   dual-emit shim 既有的「string contains」護衛，防止「兩個名字都 emit 但 value
   分叉」的 silent contract drift。對齊 R106 design.md 對照表 6 條
   `_total` 雙名 pair。
2. **M1 K0 KPI 推進**：`scripts/k0_measure.py` 加 K0-Q Quota coverage 指標
   （fresh+stale 都有 data path 算覆蓋，**對齊 MISSION.md K0 Quota 監控即時性
   13/13 目標**）+ 修 openx legacy alias（`usage-bot.json` → openx bucket），
   解決「openx 永遠漏算」的 K0 報表語意錯誤。
3. **M0 SSoT 預備**：`hook_server::KNOWN_PROVIDERS` `const` → `pub const`，
   為將來 `lib.rs` `get_provider_coverage_report` 引用鋪路（避免 13 provider
   列表在 lib.rs 跟 hook_server.rs 兩處漂移）。

3 件 WIP 是同一個 R114 切片：1 個 M0 spec drift 修 + 1 個 M1 K0 推進 + 1 個
refactor SSoT 預備。對齊 R100 策略顧問 #2 行動「單一 contract」+ MISSION
K0 13/13 目標。

## Background

R113 (2026-06-05) 走 T-1 dual-emit shim 實作（5 週時程第 1 週），6 條 counter
雙名 emit 完成。既有護衛 chain 2 條 test：
- `lp_metrics_contract_size_is_47` — const size 守衛
- `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract` — 6 條
  dual-emit pair 兩 sample line 都出現（**string contains** assertion）

**漏洞**：string contains 只斷言「兩個名字都在 body」，若其中一條 emit path
改了 source 忘記同步另一條，**兩個名字都還在 body 但 value 已分叉**（silent
contract drift）。R113 ship gate 沒蓋到這種 case。

R110 (2026-06-05) M0 修 `k0_measure.py` R83 殘留 candidates 死碼 + 修誤導註解。
後續發現 `openx` provider 的 legacy alias 沒處理 — OpenAB `BackendType::Other`
寫 `usage-bot.json`（legacy），而 `hook_server.rs` 把 `POST /hook/bot` rewrite
到 `openx` bucket。本腳本只看 `usage-openx.json*` 永遠漏算 openx，**就算
OpenAB 正常運作 K0 報表也計不到**。R105 supervisor 警告 K0 Quota 停在 10/13，
實際是 9/13（含 openx 漏算），本 R114 修這條對齊 MISSION 13/13 目標。

R106 design.md 已 closure 6 條對照表 + 5 週時程，T-2 ~ T-5 後續 owner follow-up
仍待 R114+ 接力。R114 切的是「**補強 T-1 護衛** + **K0 量測語意修**」這兩條
沒被 R113 / R110 蓋到的洞。

對應到 MISSION.md K0：
> **K0 Quota 監控即時性**：13/13 provider 都有
> 量測方式：`usage-*.json` 或等價 metric 是否被讀到

新增 K0-Q 維度（fresh+stale 都有 data path）對齊 MISSION 的「等價 metric」
口徑（不只是 fresh）。

## Scope

### In Scope

- 開新 `openspec/changes/r114-k0-coverage-and-dual-emit-guard/` change 資料夾
- 寫 4 個 spec 檔：`proposal.md` / `design.md` / `specs/.../spec.md` / `tasks.md`
- `src-tauri/src/lib.rs` `render_prometheus_tests` mod 加 R113.1 護衛 test：
  `render_prometheus_body_dual_emit_values_match_per_provider` — 對 6 條
  dual-emit pair 解析 `(labels, value)` HashMap，斷言 legacy 名 = total 名
- `src-tauri/src/hook_server.rs` `KNOWN_PROVIDERS` 改 `pub const`（5 行改動）
- `scripts/k0_measure.py`：
  - `scan_quota_snapshots` openx 加 `usage-bot` 第二個 base name
  - `main` 加 `k0q_quota_coverage` 維度（fresh+stale）JSON output + console 印
- 護衛 test / KPI 量測延伸

### Out of Scope（1 輪 1 件紀律）

- ❌ **R113 T-2 ~ T-5 抓取端 / alert / dashboard 廣播** — R115+ owner follow-up
- ❌ **R113 T-4 切換日移除舊名** — R116+ owner follow-up
- ❌ **新 provider** — 不在 R114 scope
- ❌ **接 OTel SDK** — R103+ follow-up
- ❌ **改 6 條現名 → 目標名** — R106 已 closure，本 R114 不動 emit 行為
- ❌ **不**動 main.js（owner R90 WIP 留工作區）
- ❌ **不**動其他 6 untracked + 5 closed openspec/changes/

## Capabilities

- `r114-k0-coverage-and-dual-emit-guard` — 1 M0 spec drift 修 + 1 M1 K0 推進
  + 1 refactor SSoT 預備（同一 R114 切片）

## 與上游 spec 對齊契約

- R106 spec `prometheus-counter-convention` 已 closure：6 條現名 → 目標名對照表
  為 source of truth，本 R114 不動對照表
- R113 spec `prometheus-counter-rename-2026-q3` 已 closure：T-1 dual-emit shim
  為 source of truth，本 R114 加 R113.1 護衛 test 補強
- R110 spec 不存在（是 scripts/ 治理 fix），R114 延伸 R110 修的 k0_measure.py

## 與 MISSION 對齊

- K0 Quota 監控即時性：新增 K0-Q 維度（fresh+stale），修 openx legacy alias
  後 9/13 → 10/13（K0-B 4/13 + K0-Q 6 stale = 10/13），仍距 13/13 目標 3 個
  （grokx/lpbot/mimo = 3 個 missing snapshot）
- K0-A1 / K0-A2 Provider emit：R114 不動 emit 行為
- K40 spec coverage：1/1 active open（沿用 R113 closure，本 R114 開新 change）
- K41 chore_treadmill：本輪 fix + feat 守住 M0/M1 紀律
- K42 chain 17 條飽和：R113.1 護衛 test 寫進 R113 既有的 `render_prometheus_tests`
  mod（**不開新 mod**，不擴張 chain 17 → 18）
