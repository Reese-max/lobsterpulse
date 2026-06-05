# Tasks: OpenTelemetry / Prometheus Provider Metrics Contract

> 來源：R100 策略顧問 (2026-06-04) 行動 #1：開 `openspec/changes/otel-provider-metrics-contract/`
> 對齊 OTel／Prometheus metric 名稱，不先寫 UI。R101 沒動 spec 區，R102 補此 spec。
>
> 對應 spec capability：`otel-provider-metrics-contract`（見 `specs/otel-provider-metrics-contract/spec.md`）。
> 每個 task 對應到 spec.md 的 Requirement 行（以「↪ R-XXX」標註）。

## Phase 1: Spec + 護欄 test 落地

- [x] **T-MET1: 寫 proposal.md** — 目標 + 背景 + 範圍 + capabilities 段齊 (R102 完成)
  - 驗證：proposal.md 含 4 段 (Goal/Background/Scope/Capabilities)
- [x] **T-MET2: 寫 design.md 對照表** — 41 條 LP metric 對應 OTel semconv + Prometheus convention (R103 對齊: 26→41, 6→7 段)
  - 涵蓋 design.md 7 段：Session 數量 (4 條) / Token accounting (4 條) / Failure & health (3 條) / Idle & freshness (7 條) / Session count & duration aggregates (13 條) / Quota signal (1 條) / Event & process accounting (9 條)
  - 驗證：41 條 metric 全列、每條有 Type + OTel 對應 + convention 檢查
- [x] **T-MET3: 寫 spec.md ADDED Requirements** — 3 個 Requirement + 7 個 Scenario (R102 開 2+5, R103 收齊 3+7: 加 #3 spec drift is CI-visible failure + Scenario const edit + design.md edit land in the same commit / removing metric from const without removing emit fails the guard / empty state still produces a valid contract subset), 對齊 41 / 7-section / 3 條 test 名稱
  - 涵蓋 spec.md 3 個 Requirement: LP_METRICS const is the single source of truth for emitted metric names / render_prometheus_body output is a subset of the LP_METRICS contract / spec drift in active change is a CI-visible failure
  - 驗證：spec.md 含 3 個 Requirement（spec 對齊契約 + render 護欄 + drift 可見性）+ 7 個 Scenario（≥5 達標）
- [x] **T-MET4: 寫 .openspec.yaml metadata** — schema/id/created/status/phase (R102 完成)
  - 驗證：`.openspec.yaml` 4 個 metadata 欄位齊 + status=open
- [x] **T-MET5: 寫 tasks.md** — 本檔 (R102 完成, R103 補 26→41 / 6→7 標記)
- [x] **T-MET6: lib.rs module-level 加 LP_METRICS const** — 41 條 metric 名稱收斂 (R102+R103 對齊)
  - 涵蓋 spec.md Requirement: LP_METRICS const is the single source of truth for emitted metric names（41 條收斂 + 7 段分組對齊 design.md）
  - 驗證：`const LP_METRICS: &[&str]` 41 條，order 對齊 design.md 7 段分組 (4+4+3+7+13+1+9=41)
- [x] **T-MET7: 加護欄 test `lp_metrics_contract_size_is_41_matching_emit_paths` + 2 條 render_prometheus_body_* test** — 防 spec drift (R102 寫 3 條)
  - 涵蓋 spec.md Requirement: `render_prometheus_body output is a subset of the LP_METRICS contract`（3 條 test: 大小 41 + 無重複 + empty/full state emit ⊆ LP_METRICS）
  - 驗證：cargo test 407/407 pass；改 `LP_METRICS` 移除 1 條 → 該 test fail 報「未列名 metric: ...」

## Phase 2: Spec closure

- [x] **T-MET8: 收 closure** — tasks.md 9 個 [x] 全勾 + .openspec.yaml status=closed + phase=1/1 (R104 收)
  - 涵蓋 spec.md Requirement: `spec drift in active change is a CI-visible failure`（3 條護欄 test 守住 + 同一 commit 同步更新 const/design.md/emit + 移除 const 缺 emit 必 fail guard）
  - 驗證：tasks.md `grep -c "^- \[x\]"` = 9；`.openspec.yaml` status=closed
- [x] **T-MET9: 護欄 test 寫入 engineering-log.md** — R103 紀錄 + KPI 進展表 (R103 已寫, R104 收 closure 時補 R104 紀錄)
  - 驗證：engineering-log.md 有 `### [2026-06-05] Round 103` 段 + `### [2026-06-05] Round 104` 段, 兩段皆含 KPI 進展表 ≥1 列

## 不在本 change scope（列為 follow-up）

- ❌ **重命名 6 條 counter 違反 convention 的 metric**（`sessions_total` / `tokens_input` /
  `tokens_output` / `provider_tokens_input` / `provider_tokens_output` /
  `failure_count` / `session_count`）— 改 metric 名稱 = 破既有 Prometheus 抓取 +
  alert 規則 + Grafana dashboard = 1 輪不可承受的 scope。列 R103+ owner follow-up。
- ❌ **接 OTel SDK**（`opentelemetry` / `opentelemetry-otlp` crate 整合）— 純 spec
  對齊不混 SDK 整合，列 R103+ owner follow-up。
- ❌ **改 `provider` label 為 OTel `gen_ai.provider.name` 命名空間** — 純 spec 對照
  預留 attribute 命名空間，不動現有 label，列 R103+ follow-up。
