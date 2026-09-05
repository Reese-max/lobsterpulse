# 50-Persona Audit — Round 1

Date: 2026-09-06
Protocol: `Reese-max/autodev-ng/docs/portfolio-audit/2026-09-06-50-persona-audit.md`

> Fixed 50-persona model simulation plus repository evidence review; not 50 human participants.

## Round 1 result

Status: **P0 OPEN — NOT CLEAN**

### P0 — enabling Codex monitoring overwrites existing hooks

`src-tauri/src/hooks_configurator.rs::install_codex_hooks()` constructs a new `hooks_json` containing only LobsterPulse entries and writes it over `~/.codex/hooks.json`. Existing third-party/custom Codex hooks are not loaded or merged first.

The Claude/Gemini/Copilot installers do merge into existing JSON, so this destructive behavior is specific and reproducible in the Codex path.

Actionable issue: #1 — `[P0][50-persona audit] Preserve existing Codex hooks when enabling LobsterPulse`.

## Positive evidence

- Providers default disabled, avoiding surprise first-run hook changes.
- `remove_provider` is intended to remove only LobsterPulse-marked entries.
- Corrupt JSON and IO errors are surfaced instead of silently swallowed.
- The repository documents the Tauri-specific build requirement and local-only metrics endpoint.

## Regression gates

1. Merge LobsterPulse entries into existing Codex hooks while preserving unrelated content.
2. Make install/reinstall/remove idempotent and non-destructive.
3. Fail closed on malformed config and preserve original bytes.
4. Use atomic writes plus a bounded backup/recovery path.
5. Test populated multi-hook fixtures and injected write failures.
6. Run the actual enable/reinstall/disable flow on a temporary user profile.
7. Require two consecutive fixed-persona rounds without new P0/P1/P2 after remediation.

## Runtime status

**Pending.** The overwrite is deterministic from the code path, but this round did not modify a real local Codex configuration.