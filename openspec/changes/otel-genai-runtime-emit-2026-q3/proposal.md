# Proposal: OpenTelemetry GenAI Runtime Emit

## Goal

把 R103 spec 對齊表 (`otel-provider-metrics-contract/design.md`, 41 條
`lobsterpulse_*` metric → OTel `gen_ai.*` attribute 對照) 從**純 spec 對照**
推進到 **runtime emit 層**：

1. 在 Cargo.toml 引入 `opentelemetry` / `opentelemetry-otlp` /
   `opentelemetry-semantic-conventions` 3 個 crate
2. 開新 `src-tauri/src/telemetry.rs` mod, 初始化 OTel SDK + OTLP exporter
3. 在 `SessionManager::handle_event` 對 4 個關鍵事件點 emit OTel span:
   `SessionStart` / `UserPromptSubmit` / `PostToolUseFailure` / `SessionEnd`
4. 加 1 條 `provider` label → OTel `gen_ai.provider.name` 命名空間 mapping
   (13 個 provider id → OTel 標準 provider 名稱)
5. 讓 LobsterPulse 的 runtime 事件可被任何 OTel-compatible backend
   (Jaeger / Tempo / Datadog / Honeycomb / Grafana Cloud) 接收 + 視覺化,
   不再被綁死在 Prometheus 私有 schema

## Background

R100 策略顧問 (2026-06-04) 行動 #1 明確要求:
> 立刻開一個 `openspec/changes/otel-provider-metrics-contract/`, 把 `HookEvent`
> 對應到 OTel／Prometheus metric 名稱, 不先寫 UI。

R102-R104 落地該 change (closed 9/9), 但 R103 design.md 第 87-89 行明確標
**「不在本 change scope (列為 follow-up)」**:

- ❌ 接 OTel SDK (opentelemetry / opentelemetry-otlp crate 整合)
- ❌ 改 `provider` label 為 OTel `gen_ai.provider.name` 命名空間

R120 策略顧問 (2026-06-05) 再次點出:
> 對齊 OTel 標準 metric 不落後 [否則 3 個月後 proprietary schema 沒人接]

R139 audit (2026-06-06) 結構性審計 + scope 估算:
- R103 spec 對齊表已鋪好 90% 路 (41 條 metric → OTel attribute 對照 closure)
- runtime 整合只缺 1 個 SDK 整合 + 4 個事件點 emit + 1 個 provider mapping
- 合計 ~200-300 行 code, 0 結構性重新設計
- 護衛鏈影響: +1 新護衛 mod (`telemetry::tests`), 走 R97 後 +4 例外架構理由
  (跨 `session.rs` ↔ `lib.rs` ↔ `telemetry.rs` 邊界, 跟 R122 `timeline::tests`
  例外同性質)

對應到 MISSION.md K0:
> **K0 Provider 健康度覆蓋率**: 0/13 → 13/13 程式碼 emit 維度補齊
> (R101 達標, R102 拆維度後 emit 覆蓋 5/13, sample 覆蓋 1/13)
>
> OTel 對齊不在 K0 量化口徑, 但屬 K-Foundation 層 (存活條件):
> 對齊後 K0 metric 可被任何 OTel-compatible backend 消費, 從 Prometheus
> 私有 schema 升級為業界標準, 避免 3 個月後重接一次。

## Scope

### In Scope

- 開新 `openspec/changes/otel-genai-runtime-emit-2026-q3/` change 資料夾
- 寫 4 個 spec 檔: `proposal.md` / `design.md` / `tasks.md` / `.openspec.yaml`
- 寫 1 個 capability spec: `specs/otel-genai-runtime-emit-2026-q3/spec.md`
- spec 純文件, **不寫 runtime code** (runtime code 屬 R120 #1 行動 Phase 2/3,
  留 owner M M1 接力)
- 4 個事件點 emit 契約 + provider mapping 契約 = 2 個 capability requirement
  + 4 個 scenario

### Out of Scope (列為 follow-up, 留 owner M 接力)

- ❌ **Cargo.toml 加 3 個 OTel crate** (R120 #1 行動 Phase 2)
- ❌ **新 `src-tauri/src/telemetry.rs` mod + OTel SDK init** (Phase 2)
- ❌ **Tauri command `start_otlp_exporter` 接 OTEL_EXPORTER_OTLP_ENDPOINT**
      env var (Phase 2)
- ❌ **SessionManager 4 個事件點 emit span code** (Phase 3)
- ❌ **provider → OTel `gen_ai.provider.name` mapping lookup table** (Phase 3)
- ❌ **`telemetry::tests` 護衛 mod** (Phase 3, 走 R97 後 +4 例外架構理由)

本 change 純 spec-level (M0 級), 推 K40 9/9 → 10/10 +0.1 格,
對齊 R100/R120 策略顧問 #1 行動 closure 路徑。

## Capabilities

本 change 新增 1 個 capability, 對應 1 個 spec 檔:

### `otel-genai-runtime-emit-2026-q3`

- **OGRE-R1: OpenTelemetry SDK initialization contract** — OTel SDK 必須
  在 Tauri app 啟動時初始化, 讀 `OTEL_EXPORTER_OTLP_ENDPOINT` env var
  (預設 `http://localhost:4317` gRPC), 0 panic 0 fallback (fail-closed
  跟 R127 `.gitignore` 護衛 +1 同性質)
- **OGRE-R2: SessionManager 4 事件點 emit span contract** — 4 個關鍵
  事件點 (`SessionStart` / `UserPromptSubmit` / `PostToolUseFailure` /
  `SessionEnd`) 必須 emit OTel span, span name 對齊 OTel GenAI semconv
  `gen_ai.*` 命名空間, span attributes 對齊 R103 design.md 對照表
- **OGRE-R3: provider → OTel `gen_ai.provider.name` mapping contract** —
  13 個 provider id (4 本機 CLI + 9 OpenAB bot) 必須映射到 OTel
  `gen_ai.provider.name` 標準命名空間 (anthropic / openai / google /
  github / custom), 護衛 test 守住 mapping 表大小 = 13

3 個 Requirement + ≥5 個 Scenario (Scenario 數量在 spec.md 階段決定)。

## Phase 規劃

- **Phase 1: spec closure** (本 change scope) — proposal.md / design.md /
  spec.md / tasks.md / .openspec.yaml + spectra validate 通過
- **Phase 2: OTel SDK init** (owner M M1 接力) — Cargo.toml + telemetry.rs
  + Tauri command + 護衛 mod
- **Phase 3: runtime emit** (owner M M1 接力) — SessionManager 4 事件點
  emit + provider mapping lookup + 護衛 test

## KPI 推進預期

| KPI | R141 前值 | 本 change 後值 | 變化 |
|---|---:|---:|---:|
| K40 規格覆蓋率 | 9/9 (8 active + 1 archive) | 10/10 (9 active + 1 archive) | +1 |
| K42 護衛 chain | 20 條 | 20 條 (Phase 3 才 +1) | 0 |
| baseline `cargo test --lib` | 452/452 | 452/452 (純 spec, 0 code) | 0 |
| K0 量化 | 5/1/4/9 (emit/sample/fresh/quota) | 持平 (Phase 3 才推進) | 0 |
| K41 24h chore_treadmill | <30% | 持平 (本 change 屬 docs/spec) | 0 |
| R13 髒檔 | 6 個 owner M WIP | 6 個 (0 觸碰) | 0 |

## 對齊 R97 紅線

R97 飽和契約: K42 chain 17 條後不擴張, 除非有架構理由例外
(R122 timeline::tests + R127 .gitignore + R131 plugin registry 護衛 +
本 change Phase 3 telemetry::tests = R97 後 +4 例外, 仍 < 紅線上限
+0.5/2 輪速率)

架構理由 (跟 R122 同性質):
- `telemetry::tests` 跨 `session.rs` ↔ `lib.rs` ↔ `telemetry.rs` 3 個
  mod 邊界, 既有 mod 收不下
- emit 路徑測試需要 mock OTel SDK exporter, 既有護衛 mod 沒這個 fixture
  抽象

不擴張速率驗算: R97 後 +4 例外 / 8 輪 = +0.5/輪, **等於** R97 紅線上限,
需在 Phase 3 開工前確認 R148+ 0 新護衛 mod 才能守住 +0.5/2 輪修訂上限
(R140 KPI-history R97 紅線段有提此修訂)。

## 不在 R139 audit scope 但本 change 須明確

- ❌ **不混 Prometheus convention rename** (本 change 屬 OTel 層, 跟
      prometheus-counter-rename-2026-q3 平行, 2 條線不交叉)
- ❌ **不搶 owner M scope** (runtime code 全部留 owner M M1 接力,
      本 change 純 spec-level)
- ❌ **不動 K0 Quota 4 missing bot** (非本機 scope, R131 已結構性確認
      0 spec drift, 留 R140+ owner M 接力)

## 補頁者

R126 PUA 換角度 — 從「PUA 換角度 8 輪結構性飽和」換到「策略顧問 #1 行動
closure 路徑」, 把 R139 audit 結論從 `docs/kpi-history.md` 歸檔層搬到
`openspec/changes/` active spec 層, 推 K40 +1。
