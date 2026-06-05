# Spec: Prometheus Counter Naming Convention (`_total` suffix)

> Delta spec for change `prometheus-counter-convention`. Source of truth
> for the 6 `lobsterpulse_*` counter-typed metrics that currently lack
> the `_total` suffix required by Prometheus naming convention
> (https://prometheus.io/docs/practices/naming/).
>
> 對應 change: `prometheus-counter-convention` (見 `openspec/changes/prometheus-counter-convention/`)
> 對應 design 對照表: `design.md`「Counter rename 對照表（6 條 / 2026-06-05 盤點）」
> 對應 code-level 護衛: `src-tauri/src/lib.rs` `lp_metrics_contract_tests` mod
> 新 1 條 `counter_metrics_must_have_total_suffix` test
> 對應 LP_METRICS const: `src-tauri/src/lib.rs:85-111` (lib.rs module-level)
> 護衛 chain: 17 (R102/R103 chain 之外並存, 不擴張 chain)
>
> 對齊 R100 策略顧問 (2026-06-04) 風險 #1 + R105 接力清單首位。
> 本 change 是 spec 對齊契約階段，**不實際 rename code**（破既有 Prometheus
> 抓取 + alert + dashboard = 1 輪 1 件不可承受 scope，留 R107+ owner follow-up）。

## ADDED Requirements

### Requirement: R-1 — 6 條 counter-typed metric 必須以 `_total` 結尾

LobsterPulse 的 `LP_METRICS` const 內 TYPE=counter 的 6 條 metric
(`lobsterpulse_tokens_input` / `lobsterpulse_tokens_output` /
`lobsterpulse_provider_tokens_input` / `lobsterpulse_provider_tokens_output` /
`lobsterpulse_provider_failure_count` / `lobsterpulse_provider_session_count`)
MUST 在 spec 對齊契約層以 `_total` 結尾呈現（即 R107+ 真正 rename 落地後的目標名）。

#### Scenario: 6 條 counter 都有對應 `_total` 結尾的目標名

design.md「Counter rename 對照表」必須有 6 row 完整對照（`現名 → 目標名` 一一對應），
任一 row 缺漏都 fail 此 scenario。對照表來源：2026-06-05 盤點 LP_METRICS const
(lib.rs:85-111) + render_prometheus_body emit site (lib.rs:2213-2255) +
test assertion (lib.rs:4336-4569) 三層一致。

#### Scenario: 35 個 test assertion 全部對應到 6 條 metric

`render_prometheus_body` 對 6 條 counter 的 emit 行為有 **35 個** test assertion
覆蓋（`assert!(body.contains("..."))` 計），分布在 lib.rs:4336-4569。
若實際盤點數 ≠ 35（漏算或多算），fail 此 scenario 並報
`[test assertion count] counter metric 6 條應有 35 個 test assertion 覆蓋, 觀察 = N`。

### Requirement: R-2 — 護衛 test 守住 counter convention 不漂移

`src-tauri/src/lib.rs` `lp_metrics_contract_tests` mod 內 MUST 有 1 條護衛 test
`counter_metrics_must_have_total_suffix`，斷言 `LP_METRICS` 內 TYPE=counter
的 6 條 metric 全部以 `_total` 結尾。改任一現名為不合規名（例如把
`lobsterpulse_tokens_input` 改成 `lobsterpulse_token_input`）MUST 觸發護衛 test
fail 報明確訊息。

#### Scenario: 護衛 test 斷言 6 條 counter 全部 ends_with("_total")

對 LP_METRICS 內 6 條 TYPE=counter metric 逐一檢查 `name.ends_with("_total")`，
任一 fail 報 `[counter convention] "{name}" TYPE=counter MUST 結尾 _total,
違反 Prometheus naming convention` + 列出全部違規 metric 清單。

#### Scenario: 護衛 test 斷言 LP_METRICS 內 counter 集合大小 = 6

斷言 `LP_METRICS.iter().filter(is_counter_typed_metric).count() == 6`，
防未來偷加新 counter-typed metric 但漏列進 design.md 對照表（spec drift 對齊契約）。
改對照表新增第 7 條 counter 但護衛 test 內 `is_counter_typed_metric` 沒同步加 →
fail 此 scenario 並報 `[counter count] 對照表 N 條與 LP_METRICS 內 TYPE=counter
M 條不一致`。

### Requirement: R-3 — 廣播計劃對齊 4 層面 + 5 週時程

R107+ owner 真正 rename 6 條 metric 之前 MUST 走 4 層面廣播：
(1) 抓取端（scrape config + recording rule + alert rule） /
(2) Grafana dashboard（panel query + title + legend）/
(3) 文檔（CHANGELOG / README / CONTRIBUTING / 公告）/
(4) 監控窗口（dual-emit 期間 + post-mortem）。
時程 5 週：T-0 公告 → T-1 dual-emit shim → T-2 廣播 → T-3 監控窗口 → T-4 切換
→ T-5 post-mortem。

#### Scenario: design.md 必含 4 層面 impact 段

design.md MUST 有 4 個明確段標：`### 1. Code-level` / `### 2. Prometheus 抓取端` /
`### 3. Grafana dashboard` / `### 4. 文檔 / 公告`，每段至少 1 row 描述 rename 影響面。
任一段缺漏 fail 此 scenario。

#### Scenario: design.md 必含 5 週時程段

design.md MUST 有「廣播計劃時程」段含 T-0 到 T-5 共 6 個時點，每時點至少 1 個動作描述。
任一時點缺漏 fail 此 scenario 並報
`[broadcast schedule] T-N 缺動作描述`。

### Requirement: R-4 — 不改反向違規 gauge `lobsterpulse_sessions_total`

LobsterPulse 內 `lobsterpulse_sessions_total` 是 TYPE=gauge 但用 `_total` 結尾
（反向違規：gauge 不該 `_total`），跟本 change 6 條 counter 缺 `_total` 是**不同方向**
的 spec drift。本 change MUST 不動 `lobsterpulse_sessions_total` 任何 code 路徑
（既不 rename 也不 TYPE 改 counter），留 R106+ follow-up。

#### Scenario: 護衛 test 不守 gauge 反向違規

本 change 新護衛 test `counter_metrics_must_have_total_suffix` MUST 不含
`lobsterpulse_sessions_total`（它是 gauge，結尾 `_total` 是反向違規但屬另案）。
若護衛 test 對 `lobsterpulse_sessions_total` 報「缺 `_total`」（false positive）
或報「TYPE=gauge 不該 `_total`」（越界 — 屬 R106+ 範疇），都 fail 此 scenario。

## 護衛 test 觸發模式

| 改動 | 護衛 fail 訊息 | 修正路徑 |
|---|---|---|
| design.md 對照表漏 1 條 counter | `[counter count] 對照表 N 條與 LP_METRICS 內 TYPE=counter M 條不一致` | design.md 補 row |
| R107+ rename 漏 1 條現名 → 目標名 | `cargo test` 35 個 test assertion 報「expected ... but got ...」 | 補 rename |
| R107+ dual-emit shim 沒對齊 4 層面 | `[broadcast schedule] T-N 缺動作描述` | design.md 補時程 |
| 護衛 test 內 `is_counter_typed_metric` 漏列第 7 條 | `[counter count] 對照表 N 條與 LP_METRICS 內 TYPE=counter M 條不一致` | 同步加 `is_counter_typed_metric` |
| 對 `lobsterpulse_sessions_total` 觸發 | `[scope] "lobsterpulse_sessions_total" gauge 反向違規不屬本 change` | 護衛 test 排除該 metric |

## 跟 otel-provider-metrics-contract 護衛 chain 互補

- R102/R103 護衛：`LP_METRICS` 集合大小 41 + emit ⊆ LP_METRICS（防 spec drift）
- **R105（本 change）護衛**：`LP_METRICS` 內 TYPE=counter 6 條 MUST 結尾
  `_total`（防 counter convention drift）
- 兩條護衛 chain 並存，不互相覆蓋

## 跟 R100 策略顧問風險 #1 對齊

> K0 Quota 先補滿、但 metrics schema 沒和 OpenTelemetry／Prometheus 對齊，
> 之後 13 provider 全部要重接一次。

R102 開 R103 收 otel-provider-metrics-contract 已對齊 OTel semconv；
本 change 對齊 Prometheus naming convention（counter `_total` 結尾）。
兩條護衛 chain 收齊後，K0 Provider 健康度的 metrics schema 對齊 OTel +
Prometheus 兩條業界 convention = R100 風險 #1 收尾。

## 不在本 change scope（列為 follow-up）

- ❌ **實際 rename 6 條 counter**（LP_METRICS const + emit site + 35 test
  assertion）— 破既有 Prometheus 抓取 + alert + dashboard，1 輪 1 件不可承受
  scope，留 R107+ owner follow-up
- ❌ **dual-emit shim 實作**（`render_prometheus_body` 同時 emit 舊名 + 新名
  過渡期）— 列 R107+ owner follow-up
- ❌ **改 `lobsterpulse_sessions_total` (gauge 卻用 `_total` 反向違規)** —
  不同 spec drift 類型，留 R106+ follow-up
- ❌ **接 OTel SDK** — R103+ follow-up
- ❌ **改 `provider` label 為 OTel `gen_ai.provider.name`** — R103+ follow-up
