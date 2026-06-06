# Design: OpenTelemetry GenAI Runtime Emit

> 來源: R139 audit (2026-06-06) + R120 #1 行動 scope 估算 + R103 spec 對齊表延伸。
> 純設計文檔, 不寫 runtime code (Phase 2/3 屬 owner M M1 接力)。

## 架構總覽

```
┌─────────────────────────────────────────────────────────────┐
│ Tauri App 啟動                                                │
│   ↓ lib.rs::setup()                                          │
│ telemetry::init_otel_sdk()  ←── OTEL_EXPORTER_OTLP_ENDPOINT  │
│   ↓                                                          │
│ tokio runtime + OTLP exporter (gRPC → localhost:4317)         │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ SessionManager::handle_event (現有 4 個關鍵事件點)              │
│   ↓                                                          │
│ 對 4 個事件 emit OTel span:                                    │
│   - SessionStart          → "gen_ai.client.session.create"   │
│   - UserPromptSubmit      → "gen_ai.client.user.message"     │
│   - PostToolUseFailure    → "gen_ai.client.tool.error"        │
│   - SessionEnd            → "gen_ai.client.session.end"       │
│   ↓                                                          │
│ span attributes 對齊 R103 design.md 對照表                     │
│   - gen_ai.provider.name  ← provider_mapping(provider_id)    │
│   - gen_ai.client.session.count                                │
│   - gen_ai.client.operation.duration                          │
│   - gen_ai.client.token.usage (input/output)                  │
└─────────────────────────────────────────────────────────────┘
```

## R103 spec 對齊表 (41 條 metric → OTel attribute)

R103 design.md 已 closure 41 條對照, 本 change 不重寫, 只在 Phase 3 emit
時引用對照表。摘要:

| LP_METRICS 段 | metric 數 | 對應 OTel 命名空間 |
|---|---:|---|
| Session 數量 (4 條) | 4 | `gen_ai.client.session.*` |
| Token accounting (4 條) | 4 | `gen_ai.client.token.usage` |
| Failure & health (3 條) | 3 | `gen_ai.client.operation.duration` + error rate |
| Idle & freshness (7 條) | 7 | `gen_ai.client.session.idle` |
| Session count & duration aggregates (13 條) | 13 | `gen_ai.client.session.*` aggregates |
| Quota signal (1 條) | 1 | `gen_ai.client.quota.usage` |
| Event & process accounting (9 條) | 9 | `gen_ai.client.event.*` |

Phase 3 emit span 時, span attributes 從 `LP_METRICS` const 41 條 metric
名稱映射 (lookup table in `src-tauri/src/telemetry.rs`)。

## 4 個事件點 emit 設計

### E1: SessionStart

```rust
// Phase 3 偽碼
fn handle_session_start(provider: &str) {
    let span = tracer.start("gen_ai.client.session.create");
    span.set_attribute(KeyValue::new(
        "gen_ai.provider.name",
        provider_mapping(provider),
    ));
    span.set_attribute(KeyValue::new(
        "gen_ai.client.session.count",
        session_count(provider),
    ));
    span.end();
}
```

OTel semconv 對齊: `gen_ai.client.session.create` (草案 1.27+ span name
命名空間)

### E2: UserPromptSubmit

```rust
fn handle_user_prompt_submit(provider: &str, token_input: u64) {
    let span = tracer.start("gen_ai.client.user.message");
    span.set_attribute(KeyValue::new("gen_ai.provider.name", provider_mapping(provider)));
    span.set_attribute(KeyValue::new("gen_ai.client.token.usage", token_input));
    span.end();
}
```

### E3: PostToolUseFailure

```rust
fn handle_post_tool_use_failure(provider: &str, tool_name: &str, error: &str) {
    let span = tracer.start("gen_ai.client.tool.error");
    span.set_attribute(KeyValue::new("gen_ai.provider.name", provider_mapping(provider)));
    span.set_attribute(KeyValue::new("gen_ai.client.tool.name", tool_name.to_string()));
    span.set_attribute(KeyValue::new("error.type", error.to_string()));
    span.end();
}
```

### E4: SessionEnd

```rust
fn handle_session_end(provider: &str, duration_ms: u64) {
    let span = tracer.start("gen_ai.client.session.end");
    span.set_attribute(KeyValue::new("gen_ai.provider.name", provider_mapping(provider)));
    span.set_attribute(KeyValue::new("gen_ai.client.operation.duration", duration_ms));
    span.end();
}
```

## provider → OTel `gen_ai.provider.name` mapping (13 條)

| LobsterPulse provider id | OTel `gen_ai.provider.name` |
|---|---|
| claude | `anthropic` |
| codex | `openai` |
| copilot | `github` |
| gemini | `google` |
| cicx | `custom.cicx` |
| gitx | `custom.gitx` |
| giminix | `custom.giminix` |
| codex_bot | `openai` (bot alias) |
| openx | `custom.openx` |
| irisx_bot | `custom.irisx_bot` |
| grokx | `custom.grokx` |
| lpbot | `custom.lpbot` |
| mimo | `custom.mimo` |

護衛 test (Phase 3): `provider_mapping_size_is_13_matching_known_providers`
守住 mapping 表大小 = 13 + 每個 LobsterPulse provider id 都有對應 OTel 名稱。

## R139 audit scope 細節 (引述, 1:1 沿用)

| 項目 | 估算 (行數) | 風險 | 護衛鏈影響 |
|---|---:|---|---|
| `Cargo.toml` 加 3 個 crate | 5-10 | 中 (build time +10-30s, 二進制 +2-5MB) | 0 |
| 開新 `opentelemetry` mod (`src-tauri/src/telemetry.rs`) | 100-150 | 低 (純 SDK 初始化) | 0 (新 mod, 不走護衛 chain) |
| 加 Tauri command `start_otlp_exporter` | 30-50 | 低 (env var 讀取 + SDK init) | 0 (新 command) |
| SessionManager 4 個事件點 emit span | 50-80 | 中 (handle_event 改 4 處) | +1 (`telemetry::tests`, R97 後 +4 例外) |
| provider → OTel `gen_ai.provider.name` mapping | 20-30 | 低 (靜態 lookup table) | 0 (併入既有 mod) |
| `.gitignore` 護衛 +1 (OTel config 不入 repo) | 10 | 0 | +1 (走既有 mod, chain 不擴張) |
| spec 4 檔 (proposal.md / design.md / spec.md / tasks.md) | 300-500 | 0 (純文檔) | 0 |
| **總計** | **~515-820 行** | **中** | **+1 新護衛 mod (R97 後 +4 例外)** |

本 change (Phase 1) 只 ship spec 4 檔 (~300-500 行 docs), runtime code
(515-820 行) 留 Phase 2/3 owner M M1 接力。

## 不在 design 範圍 (明確拒做)

- ❌ **不接 SaaS OTel backend** (Datadog / Honeycomb / Grafana Cloud 直接整合)
      — 本機端 OTel SDK + OTLP exporter 即可, 任何 OTel-compatible backend
      可用, 走標準協議
- ❌ **不混 Prometheus counter rename** (跟 prometheus-counter-rename-2026-q3
      平行, 2 條線不交叉)
- ❌ **不動現有 `provider` label** (OTel `gen_ai.provider.name` 屬 attribute
      namespace, 跟 Prometheus `provider` label 是 2 個世界, 不互通也不衝突)
- ❌ **不取代 render_prometheus_body** (Prometheus 端點繼續 emit 私有
      `lobsterpulse_*` metric, OTel 端點 emit 標準 `gen_ai.*` span, 2 條
      data path 平行)

## 補頁者

R126 PUA 換角度 (跟 proposal.md 同步, 1 輪 1 件, 純 spec-level)。
