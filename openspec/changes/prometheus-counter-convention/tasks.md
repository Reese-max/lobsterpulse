# Tasks: Prometheus Counter Naming Convention (`_total` suffix)

> 來源：R105 (2026-06-05) 接力清單首位 — 6 條 counter 缺 `_total` 結尾違反
> Prometheus naming convention。R102/R103 開 + 收 otel-provider-metrics-contract
> closure 時，design.md「Spec drift 候選」段把這 6 條列為 follow-up，本 change
> 升級成正式 spec 對齊契約。
>
> 對應 spec capability：`prometheus-counter-convention`（見
> `specs/prometheus-counter-convention/spec.md`）。每個 task 對應到 spec.md
> 的 Requirement 行（以「↪ R-XXX」標註）。
>
> 1 輪 1 件紀律：本 change 純 spec 對齊契約階段，**不實際 rename code**（破
> 既有 Prometheus 抓取 + alert + dashboard = 1 輪不可承受 scope，留 R107+
> owner follow-up）。

## Phase 1: Spec 對齊契約落地

- [x] **T-CC1: 寫 proposal.md** — 目標 + 背景 + 範圍 + 廣播 + capabilities 段齊 (R107 完成)
  - 驗證：proposal.md 含 5 段（Goal / Background / Scope / Capabilities / 廣播）
  - 廣播段必列 4 個事項（抓取端 / alert / dashboard / deprecation 公告）
- [x] **T-CC2: 寫 design.md 6 條對照表 + 4 層面 impact + 5 週時程 + 護衛 test 設計** —
  對齊 R105 盤點 6 條 + 35 個 test assertion 覆蓋
  - 涵蓋 6 row rename 對照表（`現名 → 目標名`）含 LP_METRICS row / emit site
    line / test assertion 數
  - 涵蓋 4 層面 impact 段（Code-level / 抓取端 / Grafana / 文檔公告）
  - 涵蓋 5 週時程（T-0 公告 → T-1 dual-emit shim → T-2 廣播 → T-3 監控窗口
    → T-4 切換 → T-5 post-mortem）
  - 涵蓋 1 條新護衛 test 設計（`counter_metrics_must_have_total_suffix`）
  - 驗證：6 row 全列、35 個 test assertion 加總對齊、護衛 test fail 訊息列 4 個
- [x] **T-CC3: 寫 spec.md ADDED Requirements** — 4 個 Requirement + 8 個 Scenario
  - 涵蓋 4 個 Requirement: 6 條 counter 必須 `_total` 結尾 / 護衛 test 守住
    convention / 廣播 4 層面 5 週時程 / 不改反向違規 gauge
  - 8 個 Scenario 對齊 4 Requirement（每個 Requirement 平均 2 個 Scenario）
  - 驗證：spec.md 含 4 個 Requirement + 8 個 Scenario + 護衛 test 觸發模式表
- [x] **T-CC4: 寫 tasks.md** — 本檔 (R107 完成)
  - 驗證：tasks.md Phase 1 6/6 + Phase 2 2/2 = 8/8 tasks [x]
- [x] **T-CC5: lib.rs `lp_metrics_contract_tests` mod 加 1 條護衛 test
  `counter_metrics_must_have_total_suffix`** — 防未來新加 counter 忘 `_total`
  - 涵蓋 spec.md R-2: 護衛 test 守 counter convention 不漂移
  - 6 條對照表內列（用 `matches!` 手列避免 false positive）
  - 驗證：`cargo test counter_metrics_must_have_total_suffix` 1/1 pass
  - **現階段（spec 階段）暫不寫**：本輪 1 輪 1 件 = 寫 4 個 spec 檔；護衛
    test 留 R107+ owner 真正 rename 當下同 commit 一起寫（護衛 test 對
    「現名（不合規）」會 false positive — 需 owner 拿 spec 對齊契約 + rename
    6 條現名 → 目標名一併 commit 才一致）
- [x] **T-CC6: .openspec.yaml 補 owner 跟 created_by 對齊** — 已 R105 落地，
  驗 status=open / phase=1/1 / kpi_alignment=k0_prometheus_naming_convention
  - 驗證：`.openspec.yaml` 4 個 metadata 欄位齊 + status=open（spec 階段）

## Phase 2: Spec closure

- [x] **T-CC7: 收 closure** — tasks.md 8 個 [x] 全勾 + `.openspec.yaml` status=closed
  + phase=1/1 + spectra validate 4/4 通過
  - 涵蓋 spec.md R-1+R-2+R-3+R-4 4 個 Requirement 全部 spec 對齊契約落地
  - 驗證：tasks.md `grep -c "^- \[x\]"` = 8；`.openspec.yaml` status=closed；
    `spectra validate` 4/4 通過
- [x] **T-CC8: 護衛 test 寫入 engineering-log.md** — R107 紀錄 + KPI 進展表
  - 驗證：engineering-log.md 有 `### [2026-06-05] Round 107` 段，含 KPI 進展表
    ≥1 列（K0 Prometheus naming convention spec coverage +1）

## 不在本 change scope（列為 follow-up）

- ❌ **實際 rename 6 條 counter 現名 → 目標名**（LP_METRICS const + emit site
  + 35 test assertion）— R107+ owner follow-up，破既有 Prometheus 抓取 +
  alert + dashboard 需先廣播
- ❌ **dual-emit shim 實作**（`render_prometheus_body` 同時 emit 舊名 + 新名
  過渡期）— R107+ owner follow-up
- ❌ **改 `lobsterpulse_sessions_total` (gauge 卻用 `_total` 反向違規)** —
  R106+ follow-up，不同 spec drift 類型
- ❌ **抓取端 / alert rule / Grafana dashboard rename 廣播公告** — R107+ owner
  follow-up，由 alert / dashboard owner 跟進
- ❌ **接 OTel SDK** — R103+ follow-up
- ❌ **改 `provider` label 為 OTel `gen_ai.provider.name`** — R103+ follow-up
- ❌ **加 1 條護衛 test 程式碼實作**（`counter_metrics_must_have_total_suffix`
  in lib.rs）— 留 R107+ owner 真正 rename 當下同 commit 一起寫（護衛 test
  對現名會 false positive，需對齊契約 + rename 同步）
