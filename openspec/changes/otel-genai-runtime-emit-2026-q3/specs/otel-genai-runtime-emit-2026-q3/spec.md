# Spec: OpenTelemetry GenAI Runtime Emit

> Delta spec for change `otel-genai-runtime-emit-2026-q3`. Source of truth
> for the OTel SDK initialization + 4-event-point span emit + provider
> `gen_ai.provider.name` mapping contract, extending the spec alignment
> table from `otel-provider-metrics-contract/design.md` (R103 closure,
> 41 條 LP metric → OTel attribute 對照) to runtime emit 層.

## ADDED Requirements

### Requirement: OGRE-R1 OpenTelemetry SDK initialization contract

The OpenTelemetry SDK MUST be initialized in `lib.rs::setup()` at Tauri
app startup. The SDK MUST read the `OTEL_EXPORTER_OTLP_ENDPOINT`
environment variable, defaulting to `http://localhost:4317` gRPC. SDK
initialization failure MUST fail-closed: return an `Err(OtelInitError)`
and do NOT silently fall back to a noop exporter, to prevent silent
failure (same fail-closed pattern as R127 `.gitignore` guard +1).

#### Scenario: OGRE-R1-S1 SDK init reads OTLP endpoint from env var

- **WHEN** Tauri app starts and `OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317`
- **THEN** the OTel SDK exporter MUST use that endpoint, not the default
  `localhost:4317`

#### Scenario: OGRE-R1-S2 SDK init fails closed on invalid endpoint

- **WHEN** Tauri app starts and `OTEL_EXPORTER_OTLP_ENDPOINT=not-a-url`
- **THEN** SDK init MUST return `Err(OtelInitError::InvalidEndpoint)`,
  MUST NOT panic, and MUST NOT silent-fallback to a noop exporter

#### Scenario: OGRE-R1-S3 SDK init defaults to localhost:4317

- **WHEN** Tauri app starts and `OTEL_EXPORTER_OTLP_ENDPOINT` is unset
- **THEN** SDK init MUST use the default `http://localhost:4317` gRPC
  endpoint

### Requirement: OGRE-R2 SessionManager 4 event point emit span contract

`SessionManager::handle_event` MUST emit one OTel span for each of the
4 key event points, with span names aligned to the OTel GenAI semconv
`gen_ai.*` namespace and span attributes aligned to the
`otel-provider-metrics-contract/design.md` 41-metric mapping table (R103
closure). The 4 event points are:

1. `SessionStart` → span name `gen_ai.client.session.create`
2. `UserPromptSubmit` → span name `gen_ai.client.user.message`
3. `PostToolUseFailure` → span name `gen_ai.client.tool.error`
4. `SessionEnd` → span name `gen_ai.client.session.end`

Each event point MUST emit its own independent span — they MUST NOT be
merged into a single batch span. The 4 spans within a session MUST have
independent trace_ids, with parent-child relationships established
through OTel context propagation.

#### Scenario: OGRE-R2-S1 SessionStart emits gen_ai.client.session.create span

- **WHEN** `SessionManager::handle_event` receives a `SessionStart` event
- **THEN** it MUST emit exactly 1 OTel span with
  `span.name = "gen_ai.client.session.create"`
- **AND** the span MUST include at least the attribute
  `gen_ai.provider.name` (mapped from the provider id)

#### Scenario: OGRE-R2-S2 PostToolUseFailure emits gen_ai.client.tool.error span

- **WHEN** `SessionManager::handle_event` receives a `PostToolUseFailure`
  event
- **THEN** it MUST emit exactly 1 OTel span with
  `span.name = "gen_ai.client.tool.error"`
- **AND** the span MUST include at least the attributes
  `gen_ai.provider.name` + `gen_ai.client.tool.name` + `error.type`

#### Scenario: OGRE-R2-S3 4 event points each emit independent span

- **WHEN** a single session receives in order `SessionStart` /
  `UserPromptSubmit` / `PostToolUseFailure` / `SessionEnd`
- **THEN** exactly 4 independent spans MUST be emitted in that order
- **AND** the 4 spans MUST have independent trace_ids
- **AND** parent-child relationships MUST be established through OTel
  context propagation (not a single merged batch span)

### Requirement: OGRE-R3 provider to OTel gen_ai.provider.name mapping contract

All 13 LobsterPulse provider ids (4 local CLI + 9 OpenAB bot) MUST map
to the OTel `gen_ai.provider.name` standard namespace. The mapping table
MUST contain exactly 13 entries (matching `KNOWN_PROVIDERS` size) and
MUST be guarded by a test asserting table size = 13 + every KNOWN_PROVIDERS
id has a non-None, non-empty OTel name (no fallback default).

Mapping rules (引述 design.md):
- 4 local CLI: `claude` → `anthropic`, `codex` → `openai`,
  `copilot` → `github`, `gemini` → `google`
- 9 OpenAB bot: `cicx` / `gitx` / `giminix` / `codex_bot` / `openx` /
  `irisx_bot` / `grokx` / `lpbot` / `mimo` → `custom.<bot_id>`
  namespace (OTel semconv has no standard for custom providers; use
  the bot id literal)

#### Scenario: OGRE-R3-S1 mapping table size is 13

- **WHEN** the `provider_mapping()` lookup table is initialized
- **THEN** the table MUST contain exactly 13 entries
- **AND** any add/remove operation MUST update `KNOWN_PROVIDERS` and
  the corresponding guard test in the same commit

#### Scenario: OGRE-R3-S2 unknown provider id returns error

- **WHEN** `provider_mapping(unknown_id)` receives a provider id that
  is not in `KNOWN_PROVIDERS`
- **THEN** it MUST return `Err(ProviderMappingError::UnknownProvider)`
- **AND** it MUST NOT panic
- **AND** it MUST NOT fall back to a default value (same fail-closed
  pattern as OGRE-R1-S2)

#### Scenario: OGRE-R3-S3 OpenAB bot uses custom namespace

- **WHEN** `provider_mapping(cicx)` is called (or any other OpenAB
  bot id)
- **THEN** it MUST return `custom.cicx` (bot id under the `custom.`
  namespace)
- **AND** it MUST NOT use the local CLI standard names
  (`anthropic` / `openai` / `google` / `github`) for OpenAB bot ids

## MODIFIED Requirements

無 (本 change 純新增, 不修改既有 capability)

## REMOVED Requirements

無

## 對齊 R97 紅線

本 spec 純文件, 0 new mod, 0 new 護衛 chain in Phase 1. K42 chain
守 20 條. Phase 3 開工時新護衛 mod `telemetry::tests` 走 R97 後 +4
例外架構理由 (跨 `session.rs` ↔ `lib.rs` ↔ `telemetry.rs` 3 個 mod
邊界, 跟 R122 `timeline::tests` 例外同性質), 需在 Phase 3 開工前
確認 R148+ 0 新護衛 mod 才能守住 +0.5/2 輪修訂上限.

## 補頁者

R126 PUA 換角度 (跟 proposal.md / design.md / tasks.md 同步, 1 輪
1 件, 純 spec-level, runtime code 全部留 owner M M1 接力).
