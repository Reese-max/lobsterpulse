# Proposal: OpenTelemetry / Prometheus Provider Metrics Contract

## Goal

把 LobsterPulse 現有的 26 條 `lobsterpulse_*` Prometheus metric 收斂到一份
**spec 對照表**，明確標出每一條 metric：
1. 對應的 **OpenTelemetry GenAI semantic convention**（草案 1.27+）attribute
2. 對應的 **Prometheus naming convention**（`*_total` / `*_seconds` / `*_ratio` 等）
3. **provider 標籤**語意（OpenAB bot_id vs 本機 CLI provider_id）

加 1 條 Rust 護欄 test 守住「`render_prometheus_body` 每次新加 metric 必須先列入 spec
對照表」，避免後續 commit 偷偷 emit 未在 spec 出現的 metric（spec drift 防護）。

## Background

R100 策略顧問 (2026-06-04) 直接命中盲點 #2：「你們現在像是在補 quota 覆蓋，
但還沒看到『Provider 健康度 P95／成功率』被同等速度推進」+ 風險 #1：
「K0 Quota 先補滿、但 metrics schema 沒和 OpenTelemetry／Prometheus 對齊，
之後 13 provider 全部要重接一次」。

對應到 MISSION.md K0：
> **K0 Provider 健康度覆蓋率**：0/13 provider 有 P95 延遲 + 成功率指標 → 13/13。
> 量測方式：Prometheus exporter 對應 metric 是否存在且有非零樣本。

R101 (2026-06-05) 落地 `lobsterpulse_provider_success_rate` 後，K0 Provider 健康度
覆蓋率 0/13 → 13/13 metric emit 維度補齊。但 R101 沒補「metric 命名契約對齊 OTel
semantic conventions」這層 — 24 條 metric 各自命名，靠 developer review 守住風格，
無 spec-level 契約。

R100 策略顧問行動 #1 明確要求：
> 立刻開一個 `openspec/changes/otel-provider-metrics-contract/`，把 `HookEvent`
> 對應到 OTel／Prometheus metric 名稱，不先寫 UI。

## Scope

### In Scope

- 開新 `openspec/changes/otel-provider-metrics-contract/` change 資料夾
- 寫 4 個 spec 檔：`proposal.md` / `design.md` / `tasks.md` / `.openspec.yaml`
- 寫 1 個 capability spec：`specs/otel-provider-metrics-contract/spec.md`
- 在 `src-tauri/src/lib.rs` 新增 module-level const `LP_METRICS: &[&str]` 收斂
  全部 emit 的 metric 名稱（26 條，6 段分組），作為 spec 對照表的 code-level 落地
- 加 1 條護欄 test `render_prometheus_body_only_emits_metrics_in_lp_metrics_contract`：
  斷言 `render_prometheus_body` 輸出內所有非 `# HELP` / `# TYPE` / 空行 的 metric 行
  起始 token 都 ∈ `LP_METRICS` 集合，防 spec drift（未先列進 spec 就 emit）

### Out of Scope

- **不**改寫現有 26 條 metric 名稱：純對齊 spec 對照，不重命名（重命名會破既有
  Prometheus 抓取 / alert 規則 / Grafana dashboard，是 1 輪 1 件不可承受的 scope）
- **不**改 `parse_provider` / `HookEvent` 結構：本 change 純 spec 對齊契約
- **不**接 OTel SDK（`opentelemetry` / `opentelemetry-otlp` crate）：1 輪 spec 對齊
  不混 SDK 整合（owner R103+ 排程）
- **不**動 main.js（owner R90 WIP 留工作區）
- **不**動 8 supervisor untracked + 其他 openspec/changes/

## Capabilities

- `otel-provider-metrics-contract` — LP `lobsterpulse_*` metric 對齊 OTel
  GenAI semconv + Prometheus naming convention 的 spec 對照表 + 護欄 test 防
  spec drift（涵蓋 41 條 metric 對照 / Prometheus counter / time 命名慣例 /
  provider label 語意 / 未列名 metric emit 阻擋）。
