# Tasks: Prometheus Counter Rename `2026-Q3` (T-1 dual-emit shim)

> 來源：R106 (2026-06-05) 收 closure 的 `prometheus-counter-convention` spec
> 對齊契約 — 6 條 counter 現名 → 目標名對照表 + 影響面盤點 + 5 週時程。
> 本 change 走 T-1 週（5 週時程第 1 週）的實作切入口，承接 R106 spec closure
> 接力清單首位。
>
> 對應 spec capability：`prometheus-counter-rename-2026-q3`（見
> `specs/prometheus-counter-rename-2026-q3/spec.md`）。每個 task 對應到 spec.md
> 的 Requirement 行（以「↪ R-XXX」標註）。
>
> 1 輪 1 件紀律：本 change 純 T-1 dual-emit 實作，**不實際 rename code**（不
> 移除舊名 emit，不改 35 個 test assertion），T-2 ~ T-5 廣播 + 切換留
> R114+ owner follow-up。

## Phase 1: T-1 dual-emit 實作

- [x] **T-PCR1: `LP_METRICS` const 加 6 條新 `_total` 名（41 → 47）** — 對齊 R-PCR1
  - 範圍：`src-tauri/src/lib.rs:85-134` LP_METRICS const 區段
  - 加 6 條新 row（緊跟對應舊名之後，segment 內排序保留）
    - segment 2 (Token accounting) 4 → 8：tokens_input_total / tokens_output_total /
      provider_tokens_input_total / provider_tokens_output_total
    - segment 3 (Failure & health) 3 → 4：failure_count_total
    - segment 5 (Session count) 13 → 14：session_count_total
  - LP_METRICS const 註解更新：「R103 補齊 41 條 → R113 T-1 dual-emit 補齊 47 條」
  - 既有 35 個 `body.contains("lobsterpulse_tokens_input ...")` 等 test
    assertion 仍 pass（舊名仍 emit）
  - 驗證：`LP_METRICS.len() == 47`（R103 chain 既有 size test 改 41 → 47）

- [x] **T-PCR2: `render_prometheus_body` 6 條 counter dual-emit block** — 對齊 R-PCR2
  - 範圍：`src-tauri/src/lib.rs:2206-2267` 6 條 counter emit 區段
  - 對每條 counter：保留舊 emit block + 加新 emit block（HELP/TYPE/sample）
  - 舊名 HELP comment 加 `# DEPRECATED: use {new_name}, scheduled removal week 4`
  - 樣本值同 source（從同 map / 變數讀，不重算）
  - 對應 emit 順序：tokens_input (L2223) → tokens_output (L2225) →
    provider_tokens_input (L2227) → provider_tokens_output (L2233) →
    failure_count (L2243) → session_count (L2262)
  - 驗證：6 條 counter body 內**同時**出現舊名 + 新名 sample line

- [x] **T-PCR3: R103 護衛 chain 既有 test 改名 + 增 assertion** — 對齊 R-PCR3
  - 範圍：`src-tauri/src/lib.rs:11134-11150` + `:11190-11272`
  - Test A 改名：`lp_metrics_contract_size_is_41_matching_emit_paths` →
    `lp_metrics_contract_size_is_47_matching_emit_paths`，size 41 → 47
  - Test A 註解更新：7 段分組對齊 design.md 4+4+3+7+13+1+9 = 41 → 4+8+4+7+14+1+9 = 47
  - Test B 增 assertion：`_full_state_all_emits_in_lp_metrics_contract` 末尾
    加 `dual_emit_pairs` 6 條 for 迴圈，斷言每條 counter body 同時含舊名 + 新名
  - 既有 35 個 `body.contains("lobsterpulse_tokens_input ...")` test assertion
    **不動**（舊名仍 emit，T-1 階段仍 pass）
  - 驗證：`cargo test lp_metrics_contract_size_is_47` 1/1 pass +
    `cargo test render_prometheus_body_full_state_all_emits_in_lp_metrics_contract` 1/1 pass
  - K42 chain 17 條**不擴張**（既有 R103 chain 延伸，0 新 chain）

- [x] **T-PCR4: spec 4 檔落地** — 對齊 R-PCR1+R-PCR2+R-PCR3+R-PCR4
  - `proposal.md` 6 段（Goal / Background / Scope / Capabilities / 與上游
    spec 對齊契約 / 與 MISSION 對齊）
  - `design.md` 6 段（Source of Truth / Code-level 變更面 / 影響面盤點 /
    廣播計劃時程 / 跟 R103 護衛 chain 互補 / 為何本輪只走 T-1）
  - `specs/prometheus-counter-rename-2026-q3/spec.md` 4 個 Requirement + 8 個 Scenario
  - `tasks.md` 本檔 Phase 1 4 個 task
  - 驗證：tasks.md `grep -c "^- \[x\]"` = 4

- [x] **T-PCR5: 落地紀錄寫入 engineering-log.md** — 對齊 K40 spec coverage
  - 範圍：`engineering-log.md` 追加 R113 段
  - 含 KPI 進展表（K0 Prometheus naming convention 0/6 spec → 6/6 dual-emit
    階段半程 +1）
  - 留 R114+ owner 接力清單（T-2 抓取端 / alert / dashboard 廣播首位）

- [x] **T-PCR6: 收 closure** — 對齊 K40 spec coverage 計數
  - tasks.md 4 個 [x] 全勾 + `.openspec.yaml` status=closed + phase=1/1
  - spectra validate 4/4 通過
  - 含 commit hash + 落地驗證（cargo test --lib 0 flake + fmt + clippy clean）
  - 驗證：`grep -c "^- \[x\]" tasks.md` = 4；`.openspec.yaml` status=closed；
    `cargo test --lib` 連 3 次 0 flake（baseline 穩定性守住）

## 不在本 change scope（列為 R114+ follow-up）

- ❌ **T-2 抓取端 / alert rule / Grafana dashboard rename 廣播公告** —
  R114+ owner follow-up，由 alert / dashboard owner 跟進
- ❌ **T-3 monitoring window** — R115+ owner follow-up
- ❌ **T-4 切換日移除舊名 emit + 拿掉 LP_METRICS 舊 row** — R116+ owner
  follow-up，本 change 不動 35 個既有 test assertion（保留 T-4 切換日才改）
- ❌ **T-5 post-mortem** — R117+ owner follow-up
- ❌ **改 `lobsterpulse_sessions_total` (gauge 卻用 `_total` 反向違規)** —
  R106+ follow-up，不同 spec drift 類型
- ❌ **改 `provider` label 為 OTel `gen_ai.provider.name`** — R103+ follow-up
- ❌ **接 OTel SDK** — R103+ follow-up
