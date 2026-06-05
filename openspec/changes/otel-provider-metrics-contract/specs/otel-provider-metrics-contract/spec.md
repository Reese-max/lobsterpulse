# Spec: OpenTelemetry / Prometheus Provider Metrics Contract

> Delta spec for change `otel-provider-metrics-contract`. Source of truth
> for the OTel GenAI semantic-convention + Prometheus naming-convention
> alignment of all `lobsterpulse_*` metrics emitted from
> `render_prometheus_body` in `src-tauri/src/lib.rs`.

## ADDED Requirements

### Requirement: LP_METRICS const is the single source of truth for emitted metric names

The `LP_METRICS: &[&str]` module-level const declared in
`src-tauri/src/lib.rs` MUST enumerate every metric name that
`render_prometheus_body` can emit, in the same order as the 7-section
grouping documented in
`openspec/changes/otel-provider-metrics-contract/design.md`
(Session 數量 / Token accounting / Failure & health / Idle & freshness /
Session count & duration aggregates / Quota signal / Event & process
accounting). The const MUST contain exactly 41 entries — adding a new
emit path that introduces a new metric name MUST be accompanied by
adding that name to `LP_METRICS` in the same commit, and the new metric
MUST also be added to the `design.md` contract table.

#### Scenario: const contains 41 entries matching the design.md 7-section grouping

- **WHEN** a developer reads `LP_METRICS` in `src-tauri/src/lib.rs`
- **THEN** the const MUST contain exactly 41 string entries
- **AND** the entries MUST be grouped by 7 `// 1.` through `// 7.`
  section comments matching `design.md` table sections
  (Session 數量 / Token accounting / Failure & health /
  Idle & freshness / Session count & duration aggregates /
  Quota signal / Event & process accounting)
- **AND** the order within each section MUST match the
  `design.md` table row order

#### Scenario: const drift is caught at compile time

- **WHEN** a developer removes an entry from `LP_METRICS` without
  also removing the corresponding `render_prometheus_body` emit path
- **THEN** the spec-alignment guard test (see Requirement below) MUST
  fail with a message of the form
  `未列名 metric: lobsterpulse_<name> 請先加入 LP_METRICS const + design.md`
  so the drift is observable rather than silent

### Requirement: render_prometheus_body output is a subset of the LP_METRICS contract

Three guard tests in the `#[cfg(test)] mod tests` block of
`src-tauri/src/lib.rs` MUST collectively assert that the
`LP_METRICS` const has exactly 41 entries with no duplicates AND that
for every line of the output of `render_prometheus_body(...)` that is
not a `# HELP` / `# TYPE` comment line and not empty, the leading
token (the metric name, terminated by `{` or whitespace) is an
element of `LP_METRICS`. The three tests are:

1. `lp_metrics_contract_size_is_41_matching_emit_paths` — asserts
   `LP_METRICS.len() == 41` AND `LP_METRICS` has no duplicate entries
   (set semantics implied by the contract).
2. `render_prometheus_body_empty_state_all_emits_in_lp_metrics_contract`
   — calls `render_prometheus_body` with an empty
   `AppSessionManager` and asserts every non-header, non-empty output
   line's leading token is in `LP_METRICS`.
3. `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract`
   — calls `render_prometheus_body` with a representative
   multi-provider, multi-state, quota-populated, discord-populated,
   hook-populated state and asserts (a) every non-header, non-empty
   output line's leading token is in `LP_METRICS` AND (b) the output
   contains at least 30 emit lines (sanity check against a
   falsely-green empty body).

Lines that contain only HELP/TYPE text or whitespace MUST be ignored
by the guard tests. When any of the three tests' assertions fail, the
test MUST report the specific off-contract token(s) it found.

#### Scenario: all current emit paths stay within the contract

- **WHEN** the guard test calls `render_prometheus_body` with a
  representative `AppSessionManager` state (zero or more provider
  sessions, zero or more quota snapshots, zero or more recent events)
- **THEN** every non-header, non-empty line MUST start with a token
  that appears in `LP_METRICS`
- **AND** the test MUST pass with the current 41-entry const

#### Scenario: adding an off-contract metric fails the guard

- **WHEN** a developer adds a new
  `out.push_str("lobsterpulse_new_thing 42\n");` emit path to
  `render_prometheus_body` but forgets to add
  `lobsterpulse_new_thing` to `LP_METRICS`
- **THEN** the guard test MUST fail with output containing
  `未列名 metric: lobsterpulse_new_thing`
- **AND** the fix is to add the name to `LP_METRICS` first
  (and the corresponding `design.md` row), then re-run the test

#### Scenario: empty state still produces a valid contract subset

- **WHEN** the guard test calls `render_prometheus_body` with an
  empty `AppSessionManager` (no sessions, no quota data, no events)
- **THEN** the output MUST contain only `# HELP` / `# TYPE` /
  empty-header lines and the global aggregate lines
  (`lobsterpulse_sessions_total 0`, `lobsterpulse_sessions_active 0`,
  `lobsterpulse_tokens_input 0`, `lobsterpulse_tokens_output 0`,
  `lobsterpulse_quota_history_csv_age_seconds <age>`)
- **AND** the guard test MUST still pass (every non-header line is
  in `LP_METRICS`)

### Requirement: spec drift in active change is a CI-visible failure

Whenever `LP_METRICS`, the `design.md` contract table, or the
`render_prometheus_body` emit paths are edited, the developer MUST
update the other two in the same commit. The three guard tests
(`lp_metrics_contract_size_is_41_matching_emit_paths` +
`render_prometheus_body_empty_state_all_emits_in_lp_metrics_contract`
+ `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract`)
are the automated mechanism that detects drift between the const
declaration, the actual emit output, and the design.md / spec.md
contract.

#### Scenario: const edit + design.md edit land in the same commit

- **WHEN** a developer adds a new metric (e.g. `lobsterpulse_<x>`)
  to `LP_METRICS`
- **THEN** the same commit MUST add a corresponding row to the
  `design.md` contract table with the metric's Type, OTel semconv
  mapping, and Prometheus convention check
- **AND** the commit MUST add a `render_prometheus_body` emit path
  that produces that metric
- **AND** `cargo test --lib` MUST pass with the guard test verifying
  the new metric is in both the const and the output

#### Scenario: removing a metric from const without removing emit fails the guard

- **WHEN** a developer removes `lobsterpulse_<x>` from `LP_METRICS`
  to clean up the contract but leaves the corresponding
  `render_prometheus_body` emit path in place
- **THEN** the guard test MUST fail with
  `未列名 metric: lobsterpulse_<x>`
- **AND** the fix is either to also remove the emit path
  (if the metric is genuinely dead) or to restore the const entry
  (if the metric is still needed)
