# Spec: OpenAB bot registry synchronization

> Delta spec for change `openab-bot-sync`. Source of truth for the
> cross-mapping between `openab/config-*.toml` `[lobsterpulse] bot_id`
> declarations and LobsterPulse's runtime provider registry, sound
> registry, and usage/quota poller.

## ADDED Requirements

### Requirement: OpenAB bot registration is a 4-point sync

When a new OpenAB bot is registered with LobsterPulse, the system MUST
add the bot to all 4 synchronization points in `src-tauri/src/config.rs`:

1. `default_providers()` — add a `ProviderConfig` entry
2. `default_provider_sounds()` — add `(bot_id, "<bot_id>.mp3")` tuple
3. `default_provider_waiting_sounds()` — add `(bot_id, "<bot_id>-waiting.mp3")` tuple
4. usage/quota poller loop — add `bot_id` to the iterated set

The `bot_id` MUST match the value declared in the corresponding
`openab/config-*.toml` `[lobsterpulse] bot_id` field. The display
`name` MUST reflect the actual backend engine as declared in
`config-*.toml` line 1 (`後端: <engine>`).

#### Scenario: IRISX hermes agent registered across all 4 points

- **WHEN** operator adds `irisx_bot` to `openab/config-hermes.toml` with
  `[lobsterpulse] bot_id = "irisx_bot"` and `enabled = true`
- **THEN** LobsterPulse MUST include `irisx_bot` in `default_providers()`
  with `enabled = true` and a name reflecting the Hermes backend
- **AND** LobsterPulse MUST include `irisx_bot` in
  `default_provider_sounds()` and `default_provider_waiting_sounds()`
- **AND** the usage/quota poller MUST iterate over `irisx_bot` so the
  `usage-irisx_bot.json` snapshot is read

#### Scenario: mimo registered as disabled to mirror config-mimo.toml

- **WHEN** `openab/config-mimo.toml` declares
  `[lobsterpulse] bot_id = "mimo"` with `enabled = false`
- **THEN** LobsterPulse MUST include `mimo` in `default_providers()`
  with `enabled = false` and the matching sound file tuples
- **AND** the usage/quota poller MUST iterate over `mimo` (so enabling
  it later requires only a config flip, not a code change)

#### Scenario: grokx and lpbot registered after id-collision split

- **WHEN** operator splits GROKX from GITX into `bot_id = "grokx"` and
  adds LPBOT with `bot_id = "lpbot"` in `openab/config-copilot-native.toml`
  and `openab/config-lpbot.toml` respectively
- **THEN** LobsterPulse MUST include both `grokx` and `lpbot` in
  `default_providers()` (4 sync points each) with names reflecting
  their actual backends (Grok/hermes-grokx, Claude/claude-agent-acp)
- **AND** GROKX MUST NOT share `bot_id` with GITX (collision eliminated)

### Requirement: Provider id alias resolves legacy / drift ids

When `hook_server` receives an event on a path segment that does not
match any registered provider key directly, the system MUST resolve
the id via the `parse_provider` alias map before deciding
"unknown provider". The alias map MUST include legacy ids that
`openab/config-*.toml` historically used (e.g. `"bot" -> "openx"`)
and any drift ids confirmed by the operator.

Unknown ids (not in registry and not in alias map) MUST be collapsed
to a known fallback and the original id MUST be logged as a warning
so the drift is observable.

#### Scenario: legacy "bot" alias routes to openx

- **WHEN** an event POSTs to `/hook/bot`
- **THEN** `parse_provider("bot")` MUST return `"openx"` and the
  event MUST be attributed to the openx provider session

#### Scenario: cicx2 drift id is resolved before fallback

- **WHEN** an event POSTs to `/hook/cicx2` and the operator confirms
  the actual production endpoint path
- **THEN** if `/hook/cicx2` is the live path, `parse_provider("cicx2")`
  MUST return `"cicx"` (alias) and the event MUST be attributed to
  the cicx provider session
- **AND** if `/hook/cicx` is the live path (openab already normalizes),
  no alias entry is required and the system MUST NOT add a redundant
  alias

### Requirement: Drift guard prevents silent provider re-drift

The system MUST provide a guard test that asserts the set of
`openab`-declared enabled bot ids (as compiled into the registry
test fixture) is a subset of the keys present in
`default_providers()` / `default_provider_sounds()` /
`default_provider_waiting_sounds()`. The guard MUST also assert that
no two distinct OpenAB bot ids map to the same LobsterPulse
provider key (collision guard).

The guard test MUST be added under the existing cross-K consistency
guard chain (currently chain 17) so a future drift causes CI to fail
loudly rather than silently.

#### Scenario: removing a provider from one sync point fails the guard

- **WHEN** a developer removes `irisx_bot` from
  `default_provider_sounds()` but leaves it in `default_providers()`
  and the usage poller
- **THEN** the guard test MUST fail with a message identifying which
  sync point is missing the entry

#### Scenario: introducing a bot-id collision fails the collision guard

- **WHEN** a developer adds a new OpenAB bot whose `bot_id` collides
  with an existing one (e.g. two enabled bots both mapping to `gitx`)
- **THEN** the collision guard MUST fail with a message identifying
  the colliding ids and the shared LobsterPulse key

### Requirement: Missing sound file MUST NOT panic playback

When a provider's sound file (`.mp3` / `-waiting.mp3`) is referenced
by `default_provider_sounds()` or `default_provider_waiting_sounds()`
but is absent from the runtime sounds directory, the system MUST
return early from the playback path without panicking. A `seed_default_sounds`
bootstrap MUST be idempotent — running it twice MUST NOT duplicate or
corrupt files in the target directory, and it MUST seed any
registered bot's silent placeholder if the file is missing.

#### Scenario: play_sound_file early-returns on missing file

- **WHEN** `play_sound_file(provider="irisx_bot", kind="complete")`
  is invoked and `sounds/irisx_bot.mp3` does not exist on disk
- **THEN** the function MUST return `Ok(())` (or the equivalent
  no-op result) without panic, error toast, or log noise above
  debug level
- **AND** the session state machine MUST continue processing events
  for the provider

#### Scenario: seed_default_sounds is idempotent

- **WHEN** `seed_default_sounds` is called twice in succession with
  the same source bundle and target directory
- **THEN** the file count and total bytes in the target directory
  MUST be identical after the second call
- **AND** a placeholder silent audio file MUST be present for every
  registered bot whose `.mp3` is missing from the target

### Requirement: Backend label reflects actual backend engine

When the `name` field of an OpenAB provider in `default_providers()`
describes the AI backend engine, the value MUST match the engine
declared in the corresponding `openab/config-*.toml` line 1
(`後端: <engine>`). The local `gemini` CLI provider
(`"💻 Gemini CLI（本機）"`, `~/.gemini/settings.json`) is independent
of the OpenAB `giminix` provider and MUST NOT be modified as part
of any backend-label realignment.

#### Scenario: giminix label realigned from gemini to agy-acp-wrapper

- **WHEN** `openab/config-gemini.toml` line 1 declares
  `後端: agy-acp-wrapper (Antigravity)` (GIMINIX backend swap)
- **THEN** the `giminix` provider `name` in `default_providers()`
  MUST reflect the Antigravity/agy backend (e.g.
  `"🤖 GIMINIX · OpenAB Antigravity"`)
- **AND** the bot_id MUST remain `giminix` (no provider key change)
- **AND** the local `gemini` CLI provider `name` and key MUST remain
  unchanged

#### Scenario: full backend label audit covers all OpenAB providers

- **WHEN** a backend label audit is performed across all OpenAB
  providers
- **THEN** every OpenAB provider's `name` MUST match the engine
  declared in its `openab/config-*.toml` line 1
- **AND** any stale labels MUST be updated in the same commit,
  with a `backend-label-audit` line in the commit body
