# Design: OpenTelemetry / Prometheus Provider Metrics Contract

## 41 條 metric 對照表（OpenTelemetry GenAI semconv + Prometheus convention）

> Source of truth：對照表本身 = spec 對照（spec.md ADDED Requirements）
> Code-level 落地：`src-tauri/src/lib.rs` module-level `const LP_METRICS: &[&str]`
> 護欄：3 條 `lp_metrics_contract_*` test 守住 `render_prometheus_body` emit 必須
> ⊆ `LP_METRICS` + 集合大小 41 + 內部無重複（spec.md Scenario 對齊）
>
> **R103 對齊筆記**：R102 開工時只盤到當時 emit 過的 26 條；R46 (event_type_total)、
> R44 (sessions_by_state)、R47 (idle_ratio / max_session_age)、
> R45 (p25/p75/p99 + interarrival_avg)、Discord 模組 3 條 (R19+) 跟 Hook 模組
> 3 條 (R46+) 後續輪次陸續加進 render_prometheus_body，但 spec 文檔沒同步補。
> R103 修本對齊：26→41 條 / 6→7 段分組。
>
> 7 段分組加總檢查：4 + 4 + 3 + 7 + 13 + 1 + 9 = 41（護欄 test
> `lp_metrics_contract_size_is_41_matching_emit_paths` 守恆等）。

### 1. Session 數量（4 條 / per-provider session activity）

| LobsterPulse metric | Type | OTel semconv 對應 | Prometheus convention 檢查 |
|---|---|---|---|
| `lobsterpulse_sessions_total` | gauge | `gen_ai.client.session.count` (draft) | ⚠️ 結尾 `_total` 暗示 counter，現行為 gauge — 列入 spec drift 候選（不修，列入 follow-up）|
| `lobsterpulse_sessions_active` | gauge | `gen_ai.client.session.active` (draft) | ✅ 非 `_total` 結尾，gauge OK |
| `lobsterpulse_provider_sessions` | gauge | `gen_ai.client.session.count` + `gen_ai.provider.name` | ✅ 命名清晰，per-provider 用 label |
| `lobsterpulse_provider_active` | gauge | `gen_ai.client.session.active` + `gen_ai.provider.name` | ✅ |

### 2. Token accounting（4 條 / lifetime aggregate）

| LobsterPulse metric | Type | OTel semconv 對應 | Prometheus convention 檢查 |
|---|---|---|---|
| `lobsterpulse_tokens_input` | counter | `gen_ai.client.token.usage` + `gen_ai.token.type=input` | ⚠️ 缺 `_total` 結尾（counter 應為 `tokens_input_total`）— 列入 spec drift 候選（不修，列入 follow-up）|
| `lobsterpulse_tokens_output` | counter | `gen_ai.client.token.usage` + `gen_ai.token.type=output` | ⚠️ 同上 |
| `lobsterpulse_provider_tokens_input` | counter | `gen_ai.client.token.usage` + `gen_ai.token.type=input` + `gen_ai.provider.name` | ⚠️ 同上 |
| `lobsterpulse_provider_tokens_output` | counter | `gen_ai.client.token.usage` + `gen_ai.token.type=output` + `gen_ai.provider.name` | ⚠️ 同上 |

### 3. Failure & health（3 條）

| LobsterPulse metric | Type | OTel semconv 對應 | Prometheus convention 檢查 |
|---|---|---|---|
| `lobsterpulse_provider_failure_count` | counter | (無 OTel 對應 — 自定 health metric) | ⚠️ counter 缺 `_total` 結尾（應為 `failure_count_total`）— 列入 spec drift 候選 |
| `lobsterpulse_provider_failure_to_completion_ratio` | gauge | (無 OTel 對應 — 自定 ratio) | ✅ `_ratio` 結尾 |
| `lobsterpulse_provider_success_rate` | gauge | (無 OTel 對應 — 自定 rate) | ✅ `_rate` 結尾 |

### 4. Idle / freshness（7 條）

| LobsterPulse metric | Type | OTel semconv 對應 | Prometheus convention 檢查 |
|---|---|---|---|
| `lobsterpulse_provider_idle_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_since_timestamp` | gauge | (無 OTel 對應) | ✅ `_timestamp` 結尾 |
| `lobsterpulse_provider_quota_snapshot_age_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |
| `lobsterpulse_quota_history_csv_age_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_last_completed_session_age_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_idle_ratio` | gauge | (無 OTel 對應 — 自定 ratio) | ✅ `_ratio` 結尾 |
| `lobsterpulse_provider_max_session_age_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |

### 5. Session count / duration aggregates（13 條 / per-provider lifetime）

| LobsterPulse metric | Type | OTel semconv 對應 | Prometheus convention 檢查 |
|---|---|---|---|
| `lobsterpulse_provider_session_count` | counter | `gen_ai.client.session.count` (per-provider) | ⚠️ counter 缺 `_total` 結尾 — 列入 spec drift 候選 |
| `lobsterpulse_provider_completed_sessions_total` | counter | `gen_ai.client.completion.count` (draft) | ✅ `_total` 結尾 |
| `lobsterpulse_provider_completed_sessions_total_duration_seconds` | counter | (無 OTel 對應 — 衍生 metric pair) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_average_duration_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_max_duration_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_min_duration_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_stddev_seconds` | gauge | (無 OTel 對應) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_p95_duration_seconds` | gauge | `gen_ai.client.operation.duration` quantile=0.95 (semconv 草案) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_p50_duration_seconds` | gauge | `gen_ai.client.operation.duration` quantile=0.50 (semconv 草案) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_p99_duration_seconds` | gauge | `gen_ai.client.operation.duration` quantile=0.99 (semconv 草案) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_p75_duration_seconds` | gauge | `gen_ai.client.operation.duration` quantile=0.75 (semconv 草案) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_p25_duration_seconds` | gauge | `gen_ai.client.operation.duration` quantile=0.25 (semconv 草案) | ✅ `_seconds` 結尾 |
| `lobsterpulse_provider_completed_sessions_interarrival_avg_seconds` | gauge | (無 OTel 對應 — 自定 interarrival metric) | ✅ `_seconds` 結尾 |

### 6. Quota signal（1 條）

| LobsterPulse metric | Type | OTel semconv 對應 | Prometheus convention 檢查 |
|---|---|---|---|
| `lobsterpulse_provider_quota_remaining_pct` | gauge | (無 OTel 對應) | ✅ `_pct` 結尾 |

### 7. Event / process accounting（9 條）

| LobsterPulse metric | Type | OTel semconv 對應 | Prometheus convention 檢查 |
|---|---|---|---|
| `lobsterpulse_provider_events_total` | counter | (無 OTel 對應 — 系統內部事件計數) | ✅ `_total` 結尾 |
| `lobsterpulse_provider_event_type_total` | counter | (無 OTel 對應 — event type 細分) | ✅ `_total` 結尾 |
| `lobsterpulse_provider_sessions_by_state` | gauge | (無 OTel 對應 — state 維度 session 計數) | ✅ 非 `_total` 結尾，gauge OK |
| `lobsterpulse_discord_health` | gauge | (無 OTel 對應 — Discord 進程 liveness 0/1) | ✅ 0/1 gauge 慣例 |
| `lobsterpulse_discord_send_failures_total` | counter | (無 OTel 對應 — Discord 送訊失敗累計) | ✅ `_total` 結尾 |
| `lobsterpulse_discord_last_event_unix` | gauge | (無 OTel 對應 — Discord 最後事件 unix timestamp) | ✅ `_unix` 結尾 |
| `lobsterpulse_hook_parse_failures_total` | counter | (無 OTel 對應 — Hook JSON 解析失敗累計) | ✅ `_total` 結尾 |
| `lobsterpulse_hook_responses_total` | counter | (無 OTel 對應 — Hook HTTP 回應累計) | ✅ `_total` 結尾 |
| `lobsterpulse_hook_unknown_provider_fallbacks_total` | counter | (無 OTel 對應 — 未列名 provider 路由累計) | ✅ `_total` 結尾 |

## Spec drift 候選（不修，列入 follow-up）

> 7 條 metric 結尾違反 Prometheus counter convention（counter 必須 `_total`）：
> `lobsterpulse_sessions_total` (gauge 卻用 `_total`)、`lobsterpulse_tokens_input`、
> `lobsterpulse_tokens_output`、`lobsterpulse_provider_tokens_input`、
> `lobsterpulse_provider_tokens_output`、`lobsterpulse_provider_failure_count`、
> `lobsterpulse_provider_session_count`。
> 修正 = 改 metric 名稱 = 破既有 Prometheus 抓取 + alert 規則 + Grafana dashboard
> = 1 輪不可承受的 scope 風險。列入 R104+ owner follow-up，**本 change 不重命名**。
>
> R103 補查：section 7 新增 9 條後，**未引入新的 convention 違規**（4 條 counter
> 全部 `_total` 收尾、3 條 gauge 收尾合規、2 條 0/1 + timestamp gauge 無 issue）。

## 護欄 test 設計（lib.rs `#[cfg(test)] mod tests`，3 條）

```rust
// 護欄 test 1: 集合大小 + 內部無重複
#[test]
fn lp_metrics_contract_size_is_41_matching_emit_paths() {
    assert_eq!(LP_METRICS.len(), 41, "LP_METRICS 應為 41 條, 目前 {} 條", LP_METRICS.len());
    let unique: std::collections::HashSet<&str> = LP_METRICS.iter().copied().collect();
    assert_eq!(unique.len(), LP_METRICS.len(),
        "LP_METRICS 不可有重複項, 重複會破 contract 護欄語意");
}

// 護欄 test 2: 空 state 也要 ⊆ LP_METRICS
#[test]
fn render_prometheus_body_empty_state_all_emits_in_lp_metrics_contract() { ... }

// 護欄 test 3: 完整 state (多 provider × 多 metric × 多 state) 也要 ⊆ LP_METRICS
#[test]
fn render_prometheus_body_full_state_all_emits_in_lp_metrics_contract() { ... }
```

護欄 test 邏輯（共通）：
1. 呼叫 `render_prometheus_body(...)` 拿到字串
2. 逐行掃描：忽略 `# HELP` / `# TYPE` / 空行 / value-only 換行
3. 對剩餘 metric 行，取起始 token（直到 `{` 或空白）
4. 斷言 token ∈ `LP_METRICS`；fail 時列出「未列名 metric: {token}」清單

防 spec drift：未來新加 metric 必須先列進 `LP_METRICS` const + 本對照表新 row，
否則護欄 test fail → 強制 developer 走 spec 更新流程。

## OTel resource attribute 對齊（不在本 change scope）

> 本 change 不接 OTel SDK（純 spec 對照），但 spec 對照表需預留 OTel resource
> attribute 命名空間給後續 SDK 整合用：
> - `service.name` = `"lobsterpulse"`
> - `service.version` = 從 `Cargo.toml` 讀
> - `host.name` = 從 system info 讀
> - `gen_ai.provider.name` = `provider` label 改名對齊（列入 R104+ follow-up）

## Provider label 語意

`provider="..."` label 值語意：
- OpenAB bot（9 隻）：`cicx` / `gitx` / `giminix` / `codex_bot` / `openx` /
  `irisx_bot` / `grokx` / `lpbot` / `mimo`
- 本機 CLI（4 隻）：`claude` / `codex` / `copilot` / `gemini`
- 合計 13 provider = `KNOWN_PROVIDERS` 集合（hook_server.rs 護欄 chain 1 守）

OTel `gen_ai.provider.name` attribute 語意 = 上述 provider label 值（1:1 對應），
無 namespace prefix（OTel convention 是 bare name，不帶 vendor prefix）。

## R103 spec 對齊檢查清單（一次性，未來不再列）

- [x] design.md section 4 從 5 條補到 7 條（加 `idle_ratio` / `max_session_age_seconds`）
- [x] design.md section 5 從 9 條補到 13 條（加 `p25` / `p75` / `p99` / `interarrival_avg`）
- [x] design.md 新增 section 7 (Event / process accounting 9 條)
- [x] design.md 標題「26 條」改「41 條」、section 註解「1. through 6.」改「1. through 7.」
- [x] spec.md Requirement「exactly 26 entries」改「exactly 41 entries」
- [x] spec.md Requirement「6-section grouping」改「7-section grouping」+ 補 section 7 名稱
- [x] spec.md Scenario 對齊 41 / 7
- [x] spec.md test 名稱 `render_prometheus_body_only_emits_metrics_in_lp_metrics_contract`
      改成實際 3 條 test 名稱（lib.rs 已實作）
- [x] tasks.md T-MET2 / T-MET3 標 [x]
