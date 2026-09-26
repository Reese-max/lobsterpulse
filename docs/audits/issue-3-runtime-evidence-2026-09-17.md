# Issue #3 — packaged Codex runtime evidence

Date: 2026-09-17
Issue: Reese-max/lobsterpulse#3 — *Enabling Codex monitoring must turn an existing `codex_hooks=false` to true*

> Round-3 audit (`50-persona-round-3-2026-09-10.md`) kept #3 open under its
> runtime gate: "no recorded packaged LobsterPulse run against a disposable
> real Codex home followed by a real Codex hook event." This file records
> that run.

## What was run

`test/codex-runtime-smoke.sh` on a Linux x86_64 host (Rust 1.98.1,
WebKitGTK 2.52.6, Xvfb + dbus-run-session):

1. `cargo tauri build --no-bundle` → real release binaries
   (`lobster-pulse` + `lobster-pulse-hook`).
2. Disposable `$HOME` seeded with `~/.codex/config.toml` containing
   `[features] codex_hooks = false` plus user comments, and a `hooks.json`
   containing a third-party hook.
3. The packaged app was launched headless on that HOME with
   `LOBSTERPULSE_HEADLESS_INSTALL=codex`, which drives the same
   `hooks_configurator::install_provider` call the settings checkbox makes
   (the Tauri command wrapper is a thin shell around it).
4. A real `lobster-pulse-hook codex` sidecar invocation — the exact
   command Codex would spawn — delivered a `SessionStart` event.

## Receipt (verbatim)

```
[smoke] disposable HOME=/tmp/lp-codex-smoke-XXX (codex_hooks=false seeded)
[smoke] hook server up on port 19280
[smoke] config.toml: codex_hooks = true, comments preserved
[smoke] hooks.json: sidecar installed, third-party hook preserved
[smoke] metrics: codex session counted
[smoke] --- config.toml after enable ---
# user comment must survive
[features]
codex_hooks = true # user disabled hooks
[smoke] --- /metrics codex lines ---
lobsterpulse_provider_sessions{provider="codex"} 1
lobsterpulse_provider_active{provider="codex"} 1
lobsterpulse_provider_session_count_total{provider="codex"} 1
lobsterpulse_provider_sessions_by_state{provider="codex",state="working"} 1
lobsterpulse_provider_events_total{provider="codex"} 1
lobsterpulse_provider_event_type_total{provider="codex",type="SessionStart"} 1
[smoke] PASS — packaged enable flips codex_hooks false→true and a real Codex hook event lands
```

Exit code 0; no leaked processes, mountpoints, or temp dirs.

## What this proves

- The **packaged** binary (not a unit test, not `cargo check`) enables Codex
  monitoring on a home where `codex_hooks = false`, flips the flag to
  `true`, preserves comments/layout, and installs sidecar hooks without
  clobbering third-party hooks.
- A real sidecar POST lands on the running app's hook server and is counted
  (`lobsterpulse_provider_sessions{provider="codex"} 1`,
  `SessionStart` event total = 1).

## What this does not prove

- A real `codex` CLI process emitting the event (the Codex binary is not
  installed on this host; the sidecar is invoked exactly as Codex's hook
  runner would invoke it, with a real Codex-shaped payload). Re-running the
  script on a host with Codex installed and firing a real CLI turn is the
  remaining delta.
- Windows/macOS runtime — the script is POSIX + Xvfb oriented; the same
  install path is OS-agnostic Rust, but per-OS packaged receipts remain a
  manual step.

## Fixture matrix (cargo test, `hooks_configurator` module — 28 tests)

| Required fixture | Test |
|---|---|
| flag absent (missing file / missing key / EOF header / other-section mention) | `codex_install_creates_missing_config_and_roundtrips_hooks`, `codex_install_adds_feature_flag_when_only_other_section_mentions_it`, `codex_install_handles_eof_features_header_without_newline`, `codex_install_does_not_confuse_other_section_codex_hooks_key` |
| flag `true` already set | `codex_install_leaves_already_enabled_flag_byte_identical` |
| flag `false` → `true` | `codex_install_enables_existing_false_feature_flag` (also asserts 5 hook entries written) |
| malformed TOML | `codex_install_fails_closed_on_malformed_toml` |
| duplicate/conflicting entries | `codex_install_rejects_duplicate_feature_flags` |
| non-boolean flag | `codex_install_rejects_non_boolean_feature_flag` |
| `codex_hooks` text in comments/strings | `codex_install_enables_existing_false_feature_flag` |
| install → reinstall → remove | `codex_install_reinstall_remove_keeps_shared_feature_enabled`, `codex_install_creates_missing_config_and_roundtrips_hooks` |
| user-owned `true` not clobbered by remove | `codex_remove_preserves_user_owned_true_flag` |
| dotted / inline-table flag forms | `codex_install_enables_dotted_feature_assignment`, `codex_install_enables_inline_feature_assignment` |
| multiline strings preserved | `codex_install_preserves_multiline_toml_strings` |

## Flag-ownership contract (remove path)

`codex_hooks` is treated as a **shared Codex capability**: removal never
writes `config.toml`, so a user-owned `true` cannot be clobbered and no
ownership tracking is required. This is the documented product contract —
see `codex_install_reinstall_remove_keeps_shared_feature_enabled` and
`codex_remove_preserves_user_owned_true_flag`.
