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

- [x] **T-OGRE1: 寫 proposal.md** — 目標 + 背景 + 範圍 + capabilities 段齊
      ↪ 對應 R100 策略顧問 #1 行動 closure 路徑
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

## Phase 2: OTel SDK init (owner M M1 接力, 不在 R126 scope)

- [ ] **T-OGRE10: Cargo.toml 加 3 個 OTel crate** —
      `opentelemetry` (RUNTIME trait) +
      `opentelemetry-otlp` (exporter) +
      `opentelemetry-semantic-conventions` (attribute key 常數)
      驗證: `cargo build` 通過, binary 大小 +2-5MB, build time +10-30s

- [ ] **T-OGRE11: 開新 `src-tauri/src/telemetry.rs` mod** —
      OTel SDK init + tracer provider + OTLP exporter
      驗證: `cargo build` 通過, 0 panic 0 silent fallback

- [ ] **T-OGRE12: Tauri command `start_otlp_exporter`** —
      接 `OTEL_EXPORTER_OTLP_ENDPOINT` env var, 預設
      `http://localhost:4317` gRPC
      驗證: command 在 lib.rs 註冊, 啟動時呼叫 telemetry::init_otel_sdk()

## Phase 3: Runtime emit (owner M M1 接力, 不在 R126 scope)

- [ ] **T-OGRE13: SessionManager 4 個事件點 emit span** —
      SessionStart / UserPromptSubmit / PostToolUseFailure / SessionEnd
      各 emit 1 個 OTel span
      驗證: cargo test --lib 既有護衛 event flow 測試全綠 +
      新增護衛 test `telemetry::tests::session_start_emits_span_*`

- [ ] **T-OGRE14: provider → OTel `gen_ai.provider.name` mapping lookup** —
      13 條靜態 lookup table (4 本機 CLI + 9 OpenAB bot)
      驗證: 新增護衛 test `telemetry::tests::provider_mapping_size_is_13_*`

- [ ] **T-OGRE15: `telemetry::tests` 護衛 mod** —
      走 R97 後 +4 例外架構理由, 守住 emit 路徑 + mapping 表大小
      驗證: K42 chain 20 → 21, 護衛 mod 大小 < 800 行

- [ ] **T-OGRE16: .gitignore 護衛 +1 (OTel config 不入 repo)** —
      走 `r127_daemon_exclusion_gitignore_tests` 既有 mod, chain 不擴張
      驗證: K42 chain 不變, OTel config token 不入 git

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
