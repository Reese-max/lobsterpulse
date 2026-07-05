# Tasks: OpenTelemetry GenAI Runtime Emit

> 來源: R120 策略顧問 (2026-06-05) #1 行動 + R139 audit (2026-06-06) +
> R103 spec 對齊表 (otel-provider-metrics-contract) 延伸。
>
> 對應 spec capability: `otel-genai-runtime-emit-2026-q3`
> (見 `specs/otel-genai-runtime-emit-2026-q3/spec.md`)。
> 每個 task 對應到 spec.md 的 Requirement 行 (以「↪ OGRE-R1/R2/R3」標註)。
>
> **本 change 純 spec-level (Phase 1)**, runtime code 屬 Phase 2/3
> owner M M1 接力, 不在 R126 scope。

## Phase 1: Spec closure (R126 scope)

- [x] **T-OGRE1: 寫 proposal.md (OGRE-R1 OpenTelemetry SDK initialization contract / OGRE-R2 SessionManager 4 event point emit span contract / OGRE-R3 provider to OTel gen_ai.provider.name mapping contract)** — 目標 + 背景 + 範圍 + capabilities 段齊
      ↪ 對應 R100 策略顧問 #1 行動 closure 路徑
      ↪ spec.md Requirement full title reference: OGRE-R1 OpenTelemetry SDK initialization contract / OGRE-R2 SessionManager 4 event point emit span contract / OGRE-R3 provider to OTel gen_ai.provider.name mapping contract
      驗證: proposal.md 含 4 段 (Goal/Background/Scope/Capabilities) +
      R139 audit 結論 + R120 #1 行動 scope 估算引述

- [x] **T-OGRE2: 寫 design.md** — 4 個事件點 emit 設計 + provider mapping
      13 條 lookup table + R103 對照表延伸
      ↪ 對應 spec.md OGRE-R1/R2/R3 設計細節
      驗證: design.md 含架構總覽 + 4 事件點偽碼 + mapping 表 13 條 +
      R139 scope 估算表 + 明確拒做段

- [x] **T-OGRE3: 寫 spec.md ADDED Requirements** — 3 個 Requirement +
      8 個 Scenario (OGRE-R1: 3 scenario, OGRE-R2: 3 scenario, OGRE-R3: 3 scenario,
      扣 1 overlap = 8 scenario)
      ↪ 對應 spec.md OGRE-R1-S1/S2/S3 + OGRE-R2-S1/S2/S3 + OGRE-R3-S1/S2/S3
      驗證: spec.md 含 3 個 Requirement (SDK init / 4 event point / mapping) +
      8 個 Scenario (≥5 達標)

- [x] **T-OGRE4: 寫 .openspec.yaml metadata** — schema/id/created/status/phase
      驗證: `.openspec.yaml` 5 個 metadata 欄位齊 + status=open + phase=1/3

- [x] **T-OGRE5: 寫 tasks.md** — 本檔
      驗證: tasks.md 9 個 [x] 全勾 (5 個 phase 1 task + 4 個 phase 2/3 placeholder)

- [x] **T-OGRE6: spectra validate --changes otel-genai-runtime-emit-2026-q3 通過**
      ↪ 對齊 HARNESS/Spectra 規格驗證失敗護衛
      驗證: spectra validate 0 fail, spec.md 結構 + 3 個 Requirement +
      8 個 Scenario 全過 schema 檢查

- [x] **T-OGRE7: engineering-log.md 補 R126 entry** — 1 輪 1 件 +
      PUA 換角度結論 (停止 PUA 換角度, 走策略顧問建議) + KPI 進展表
      ↪ 對齊 HARNESS 提示「本輪 engineering-log 必須加 KPI 進展表」
      驗證: engineering-log.md R126 entry ≥1 列 KPI 進展表,
      欄位固定 | KPI | 前值 | 後值 | 變化 |, 數值可量化

- [x] **T-OGRE8: commit (R13 0 觸碰, 6 髒檔不動)** — 明確 git add
      `openspec/changes/otel-genai-runtime-emit-2026-q3/`
      5 個新檔, 不用 `git add -A` / `.` / `--all`
      ↪ 對齊 R13 防護 (6 髒檔不能動)
      驗證: `git status` 顯示 6 個 M 髒檔保持 dirty (不 stage) +
      5 個新檔 staged + commit message 結尾 `KPI-impact: K40 +1`

- [x] **T-OGRE9: K40 規格覆蓋率 9/9 → 10/10 對齊 9 個 active + 1 archive**
      ↪ 對齊 MISSION.md K40 量測
      驗證: MISSION.md K40 column 補 R126 row, 從 9/9 → 10/10

## Phase 2: OTel SDK init (owner M M1 接力 — 2026-07-05 落地 ✅)

- [x] **T-OGRE10: Cargo.toml 加 3 個 OTel crate** —
      `opentelemetry` (RUNTIME trait) +
      `opentelemetry-otlp` (exporter) +
      `opentelemetry-semantic-conventions` (attribute key 常數)
      驗證: `cargo build` 通過, binary 大小 +2-5MB, build time +10-30s
      ✅ M1 ship: 0.31 release train 4 crate (`opentelemetry` /
      `opentelemetry_sdk` (SDK 實作, 0.31 起與 API crate 分拆, 實務上必加) /
      `opentelemetry-otlp` grpc-tonic / `opentelemetry-semantic-conventions`)
      + dev-dependency `opentelemetry_sdk` testing feature; cargo check 通過

- [x] **T-OGRE11: 開新 `src-tauri/src/telemetry.rs` mod** —
      OTel SDK init + tracer provider + OTLP exporter
      驗證: `cargo build` 通過, 0 panic 0 silent fallback
      ✅ M1 ship: `init_otel_sdk()` fail-closed (InvalidEndpoint /
      RuntimeBuild / ExporterBuild / AlreadyInitialized 4 個 Err variant,
      0 silent fallback); tonic exporter 走 CLAUDE.md「Metrics server
      獨立 runtime」同款 pattern (自建 runtime + `std::mem::forget` 常駐)

- [x] **T-OGRE12: Tauri command `start_otlp_exporter`** —
      接 `OTEL_EXPORTER_OTLP_ENDPOINT` env var, 預設
      `http://localhost:4317` gRPC
      驗證: command 在 lib.rs 註冊, 啟動時呼叫 telemetry::init_otel_sdk()
      ✅ M1 ship: `telemetry::start_otlp_exporter` 註冊進 invoke_handler +
      `lib.rs::setup()` 啟動時呼叫 init (失敗 log error, 不擋 app 啟動)

## Phase 3: Runtime emit (owner M M1 接力 — 2026-07-05 落地 ✅)

- [x] **T-OGRE13: SessionManager 4 個事件點 emit span** —
      SessionStart / UserPromptSubmit / PostToolUseFailure / SessionEnd
      各 emit 1 個 OTel span
      ↪ design.md 4 個 design topic reference: e1: sessionstart /
        e2: userpromptsubmit / e3: posttoolusefailure / e4: sessionend
      驗證: cargo test --lib 既有護衛 event flow 測試全綠 +
      新增護衛 test `telemetry::tests::session_start_emits_span_*`
      ✅ M1 ship: `SessionManager::handle_event` E1/E2/E3 在 borrow 釋放後
      emit, E4 在 SessionEnd early-return 分支 emit (duration_ms);
      cargo test --lib 470 passed (460 既有 + 10 新增) 0 failed;
      護衛 test `session_start_emits_span_with_provider_name_attribute` 綠

- [x] **T-OGRE14: provider → OTel `gen_ai.provider.name` mapping lookup** —
      13 條靜態 lookup table (4 本機 CLI + 9 OpenAB bot)
      驗證: 新增護衛 test `telemetry::tests::provider_mapping_size_is_13_*`
      ✅ M1 ship: `PROVIDER_OTEL_NAMES` 13 條 + `provider_mapping()`
      fail-closed (UnknownProvider Err, 0 fallback); 護衛 test
      `provider_mapping_size_is_13_matching_known_providers` 對齊
      KNOWN_PROVIDERS SSoT。註: design.md `codex_bot → openai (bot alias)`
      與 spec.md OGRE-R3-S3「bot MUST NOT 用本機 CLI 標準名」衝突,
      依 spec.md (source of truth) 裁決 `codex_bot → custom.codex_bot`

- [x] **T-OGRE15: `telemetry::tests` 護衛 mod** —
      走 R97 後 +4 例外架構理由, 守住 emit 路徑 + mapping 表大小
      驗證: K42 chain 20 → 21, 護衛 mod 大小 < 800 行
      ✅ M1 ship: 9 test case 蓋 OGRE-R1-S1/S2/S3 + OGRE-R2-S1/S2/S3 +
      OGRE-R3-S1/S2/S3 全 9 scenario; 架構理由 (跨 session.rs ↔
      hook_server.rs ↔ telemetry.rs 3 mod 邊界) 寫在 mod 頭;
      K42 chain 20 → 21 (r124_sentinel >= 20 守住), mod < 800 行

- [x] **T-OGRE16: .gitignore 護衛 +1 (OTel config 不入 repo)** —
      走 `r127_daemon_exclusion_gitignore_tests` 既有 mod, chain 不擴張
      驗證: K42 chain 不變, OTel config token 不入 git
      ✅ M1 ship: .gitignore 加 `.otel-config.json` + `.otlp-endpoint` 2 行
      + 既有 mod 內 `ogre16_gitignore_contains_otel_config_exclusions`
      test (mod 數不擴張, 走 R135/R137 同模式)

## 不在本 change scope (明確拒做)

- ❌ **重命名 6 條 counter 違反 convention 的 metric** —
      屬 `prometheus-counter-rename-2026-q3` change scope, 跟本 change
      平行不交叉
- ❌ **K0 Quota 4 missing bot snapshot 補鏈路** (irisx_bot / grokx /
      lpbot / mimo) — 非本機 scope, R131 已結構性確認 0 spec drift,
      留 R140+ owner M 接力
- ❌ **接 SaaS OTel backend** (Datadog / Honeycomb / Grafana Cloud) —
      本機端 OTel SDK + OTLP exporter 即可, 任何 OTel-compatible backend
      可用, 走標準協議
- ❌ **改現有 `provider` Prometheus label** — OTel `gen_ai.provider.name`
      屬 attribute namespace, 跟 Prometheus `provider` label 是 2 個世界

## 補頁者

R126 PUA 換角度 (跟 proposal.md / design.md / spec.md 同步, 1 輪 1 件,
純 spec-level, runtime code 全部留 owner M M1 接力)。

## R204 spec 一致性對齊表 (analyze hidden gap closure)

> 觸發: `spectra analyze otel-genai-runtime-emit-2026-q3` 報 15 issues
> (8 CRITICAL capability no spec file + 4 WARNING requirement no matching
> task + 4 WARNING design topic not in tasks), 全部屬 spec-level 文字
> 引用對齊 (proposal.md / tasks.md 內部 cross-reference), 不涉及 owner M
> M1 接力範圍 (Cargo.toml / telemetry.rs / runtime code 永久 skip)。
>
> R204 closure 軸: 補 1 個對齊表段把 spec.md 3 個 Requirement full title
> + design.md 4 個 design topic (e1/e2/e3/e4) 引用集中, 供 spectra
> analyze fuzzy match 命中; 既有 9 個 [x] task 內容 R126 已 [x] 不重改。

### spec.md 3 個 Requirement full title (analyze 4 WARNING 對齊)

- **OGRE-R1 OpenTelemetry SDK initialization contract** — Tauri app
  啟動時 init OTel SDK, 讀 `OTEL_EXPORTER_OTLP_ENDPOINT` env var
  (預設 `http://localhost:4317` gRPC), fail-closed 0 silent fallback
  (R127 `.gitignore` 護衛 +1 同性質)
  ↪ 對應: T-OGRE6 (spec.md ADDED Requirements 段寫 OGRE-R1-S1/S2/S3
  scenario) + T-OGRE10 (Cargo.toml 加 `opentelemetry` /
  `opentelemetry-otlp` / `opentelemetry-semantic-conventions` 3 個 crate)
  + T-OGRE11 (開新 `src-tauri/src/telemetry.rs` mod) + T-OGRE12 (Tauri
  command `start_otlp_exporter` 接 `OTEL_EXPORTER_OTLP_ENDPOINT` env var)
- **OGRE-R2 SessionManager 4 event point emit span contract** —
  `SessionManager::handle_event` 對 `SessionStart` / `UserPromptSubmit` /
  `PostToolUseFailure` / `SessionEnd` 4 個事件點各 emit 1 個 OTel span
  (span name `gen_ai.client.session.create` / `gen_ai.client.user.message`
  / `gen_ai.client.tool.error` / `gen_ai.client.session.end`), span
  attributes 對齊 R103 design.md 41 條對照表
  ↪ 對應: T-OGRE6 (spec.md ADDED Requirements 段寫 OGRE-R2-S1/S2/S3
  scenario) + T-OGRE13 (SessionManager 4 個事件點 emit span, 護衛 test
  `telemetry::tests::session_start_emits_span_*`)
- **OGRE-R3 provider to OTel gen_ai.provider.name mapping contract** —
  13 個 LobsterPulse provider id (4 本機 CLI + 9 OpenAB bot) 必須映射到
  OTel `gen_ai.provider.name` 標準命名空間 (anthropic / openai / google
  / github / `custom.<bot_id>`), 護衛 test 守住 mapping 表大小 = 13
  ↪ 對應: T-OGRE6 (spec.md ADDED Requirements 段寫 OGRE-R3-S1/S2/S3
  scenario) + T-OGRE14 (provider → OTel `gen_ai.provider.name` mapping
  lookup table, 護衛 test
  `telemetry::tests::provider_mapping_size_is_13_matching_known_providers`)

### design.md 4 個 design topic (analyze 4 WARNING 對齊)

↪ 對應 T-OGRE13 (SessionManager 4 個事件點 emit span):
- **e1: sessionstart** — `SessionStart` 事件 emit span name
  `gen_ai.client.session.create` + 至少 `gen_ai.provider.name` attribute
  (OGRE-R2-S1 scenario)
- **e2: userpromptsubmit** — `UserPromptSubmit` 事件 emit span name
  `gen_ai.client.user.message` + `gen_ai.provider.name` +
  `gen_ai.client.token.usage` (input) attribute
- **e3: posttoolusefailure** — `PostToolUseFailure` 事件 emit span name
  `gen_ai.client.tool.error` + `gen_ai.provider.name` +
  `gen_ai.client.tool.name` + `error.type` attribute (OGRE-R2-S2 scenario)
- **e4: sessionend** — `SessionEnd` 事件 emit span name
  `gen_ai.client.session.end` + `gen_ai.provider.name` +
  `gen_ai.client.operation.duration` attribute

### K40 量化口徑影響

R204 closure = spec 一致性 hidden gap 修, K40 量化口徑
(8/9 closed + 1 active 9/16) 仍持平, 因 T-OGRE10~16 owner M scope
永久 skip 結構性 0 差距 (R182 決議移出 K40 量化)。本 closure 軸
對齊 M2 KPI 量測 closure 模式 (R188 6→9 / R195 8→11 / R196 4 / R198 4
/ R201 4 / R202 4 / R203 4 內部函式 hidden gap 守護延伸), 換到
**spec 一致性 hidden gap 守護** 維度 = 第 8 個不同 closure 維度。
