# Spec: R114 K0 Quota coverage + dual-emit value-equality guard

## ADDED Requirements

### Requirement: R113.1 dual-emit value-equality guard

`render_prometheus_body` 在 T-1 dual-emit 期間必須對 6 條 `_total` 雙名 pair
emit **value 相等**的 sample line。當任一 emit path 的 source 跟另一條分叉時，
guard 必須失敗（HashMap value 不等），防止 silent contract drift。

#### Scenario: 6 條 dual-emit pair 對每個 label 集合都 value 相等

Given `render_prometheus_body` 收到 3 個 provider（cicx/claude/openx）
的 ProviderTotals + quota_ages + quota_pct + last_completed_age fixture

When 呼叫 `render_prometheus_body(...)` 拿 Prometheus text body

Then 對 6 條 dual-emit pair：
- `lobsterpulse_tokens_input` / `lobsterpulse_tokens_input_total`
- `lobsterpulse_tokens_output` / `lobsterpulse_tokens_output_total`
- `lobsterpulse_provider_tokens_input{provider=X}` / `lobsterpulse_provider_tokens_input_total{provider=X}`
- `lobsterpulse_provider_tokens_output{provider=X}` / `lobsterpulse_provider_tokens_output_total{provider=X}`
- `lobsterpulse_provider_failure_count{provider=X}` / `lobsterpulse_provider_failure_count_total{provider=X}`
- `lobsterpulse_provider_session_count{provider=X}` / `lobsterpulse_provider_session_count_total{provider=X}`

每條 pair 的 `HashMap<labels, value>` 必須完全相等（key 集合 + value 都對得到）。

#### Scenario: 護衛 test 寫進既有 `render_prometheus_tests` mod

Given K42 chain 17 條飽和契約（R81 守住）

When 加 R113.1 護衛 test

Then 必須寫進既有 `render_prometheus_tests` mod，**不開新 mod**，chain 17 → 17 不擴張。

### Requirement: KNOWN_PROVIDERS pub const SSoT 預備

`hook_server::KNOWN_PROVIDERS` 必須是 `pub const`（不是 module-private
`const`），讓將來 `lib.rs` 引用 13 provider 列表時不需要重複 list，避免
provider 列表在 lib.rs 跟 hook_server.rs 兩處漂移。

#### Scenario: 將來 lib.rs 引用 KNOWN_PROVIDERS 不用重複列

Given `hook_server::KNOWN_PROVIDERS: &[&str]` 13 元素（4 本機 CLI + 9 OpenAB bot）

When `lib.rs` 將來加 `use hook_server::KNOWN_PROVIDERS;`

Then 編譯通過，無需重複 13 個 id 字串。

#### Scenario: 既有 R73 / R78 護衛不破壞

Given 既有護衛 test `KNOWN_PROVIDERS.contains(p)` 跟
`KNOWN_PROVIDERS.len() == 13`

When 從 `const` 改 `pub const`

Then 既有護衛全綠，chain 17 → 17 不擴張。

### Requirement: k0_measure K0-Q coverage 維度

`scripts/k0_measure.py` 必須新增 `k0q_quota_coverage` 維度（任何狀態：
fresh / stale 都算 quota data path 已接上），對齊 MISSION.md K0 Quota
監控即時性 13/13 目標的「usage-*.json 或等價 metric 是否被讀到」口徑
（不只 fresh）。

#### Scenario: K0-Q 維度 JSON 輸出

Given 13 provider 的 quota snapshot 狀態（4 fresh + 5 stale + 4 missing）

When 跑 `python scripts/k0_measure.py`

Then JSON 必須含 `k0q_quota_coverage: {covered: 9, total: 13, pct: 69.2}`。

#### Scenario: K0-Q 維度 console 印

Given 同上

When 跑 `python scripts/k0_measure.py`

Then console 必須印
`K0-Q  Quota 覆蓋率 (fresh+stale 都有 data path): 9/13 (69.2%)`。

#### Scenario: 既有 K0-A1 / K0-A2 / K0-B 維度不破壞

Given 同上

When 跑 `python scripts/k0_measure.py`

Then K0-A1 / K0-A2 / K0-B 維度數值不變（修前修後對照一致）。

### Requirement: openx legacy alias 修

`scripts/k0_measure.py` 必須把 OpenAB `BackendType::Other` legacy 寫的
`usage-bot.json` 計入 `openx` bucket，對齊 `hook_server.rs:393-401`
`parse_provider("bot") → "openx"` 別名語意。

#### Scenario: openx glob 雙 base name

Given `~/.lobsterpulse/usage-bot.json` 存在（OpenAB legacy 寫入）

When `scan_quota_snapshots` 掃描

Then openx bucket 必須納入 `usage-bot.json`（跟 `usage-openx.json` 併行）。

#### Scenario: 修前修後 openx 計數差

Given 同上 + OpenAB 沒寫 `usage-openx.json`

When 跑 `python scripts/k0_measure.py`

Then openx provider 在 K0-Q 維度計入 covered（修前永遠 missing）。
