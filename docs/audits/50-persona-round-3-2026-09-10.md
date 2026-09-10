# 50-Persona Audit — Round 3

Date: 2026-09-10
Protocol: `Reese-max/autodev-ng/docs/portfolio-audit/2026-09-06-50-persona-audit.md`

> Fixed 50-persona model simulation plus current default-branch/repository/CI evidence review; not 50 human participants. CI evidence is not represented as packaged-app or real Codex runtime validation.

## Audited state

Current head before this report: `ebddfdcb6fb9611a7ab8b54916526a729c797d5e` (product-board audit documentation on top of the merged #3 remediation).

The Round-2 P1 #3 source-level remediation is now on `main`: `enable_codex_hooks_feature()` parses TOML with `toml_edit::DocumentMut`, explicitly changes an existing boolean `false` to `true`, preserves value decor, creates the flag when absent, and fails on malformed/non-boolean/non-table forms. Merged default-branch Build run `34114733520` actually completed successfully on Linux, macOS and Windows, including the hooks-configuration test step and release build.

That is sufficient to say the previously reproducible `codex_hooks=false` static/configuration defect is materially remediated on default branch. It is **not** sufficient to close #3 under its runtime gate: there is still no recorded packaged LobsterPulse run against a disposable real Codex home followed by a real Codex hook event.

## Round 3 result

Status: **P2 OPEN — NOT CLEAN**

The full fixed-persona rerun did not reproduce a new variant of #3 in current source, but it incorporates two distinct P2 findings already opened by the current product-board inspection:

1. #5 — public/provider support language and the K0 success denominator are not governed by one versioned capability contract.
2. #6 — the repository presents LobsterPulse as a desktop application but currently publishes no GitHub Release; README's primary executable path points to a local build artifact and still states there is no real release/download link.

These are distinct from #3 and reset the no-new-P0/P1/P2 streak.

## P2 #5 — truthful provider support / denominator

Current README says LobsterPulse monitors **13 providers** (9 OpenAB + 4 local) and identifies `default_providers()` as source of truth. Existing #5 documents that the post-target K0 scorecard can narrow/exclude advertised providers while still reporting a zero structural gap. The user-facing concept of “13 providers” is therefore not guaranteed to mean the same thing as registered, configured, live-emitting, recently active or quota-observable.

Fixed personas materially affected include C01 (trust/audit), C05 (observability), D01/D02 (summary/decision traceability), D05 (evidence scope), H05 (first maintainer), I05 (partial success), J03 (long-running health) and J05 (expert automation).

Required outcome: one versioned provider-capability registry with lifecycle/support/freshness/owner fields must drive or validate README, UI labels and KPI receipts; every current score must expose numerator, denominator, timestamp and exclusions rather than retroactively shrinking a historical target.

## P2 #6 — obtainable desktop product / distribution truth

GitHub Releases currently returns an empty list. README instructs users to run `src-tauri/target/release/lobster-pulse.exe` or build it themselves and explicitly notes that docs do not contain a real repository/release download link.

For A01/A02/B01/E03/F01/F05, this means the first-use desktop journey stops before the product's core monitoring task unless the user can install a Rust/Tauri developer toolchain. H01/H02/H03 can build from source, but that is maintainer evidence, not a truthful downloadable-user path. G05/offline-transfer scenarios also lack a stable versioned artifact/checksum contract.

Required outcome: either publish and verify one Windows-first release path using the existing release workflow, or explicitly classify the project as source-only and rewrite onboarding around reproducible source builds. A distributed-binary choice requires clean-machine download/checksum/launch/sidecar evidence before the runtime gate can pass.

## Fixed-persona regression summary

- #3 static/configuration scenario (`codex_hooks=false` → enable) now passes repository inspection and merged multi-platform CI; packaged Codex runtime remains unverified.
- #5 introduces a trust/observability ambiguity for support/KPI personas.
- #6 blocks or materially degrades first-time non-developer installation personas.
- No additional distinct P0/P1/P2 fingerprint was confirmed after deduplication against current open issues.

## Actual execution evidence

GitHub Actions Build run `34114733520` completed successfully for Linux, Windows and macOS. Each job executed checkout, Rust/Tauri setup, `Test hooks configuration`, `Build (no bundle)`, and binary upload. This is real CI execution evidence for the tested configuration/build path.

## Runtime evidence not present

- No packaged-app acceptance against a disposable real Codex home followed by a real Codex event for #3.
- No representative packaged runtime proving live/stale/external/disabled provider states and `/metrics` denominator semantics for #5.
- No published GitHub Release and no clean Windows download/checksum/launch/sidecar receipt for #6.

## CLEAN gate

1. Keep #3 open until its packaged Codex runtime acceptance evidence exists.
2. Resolve #5 with one truthful versioned provider capability/KPI contract and runtime state verification.
3. Resolve #6 with either a verified public distribution path or a truthful source-only contract; obtain the corresponding clean-machine runtime evidence.
4. Re-run the same fixed 50 personas on the resulting default-branch state.
5. Require two consecutive rounds with no new P0/P1/P2 before CLEAN.

Consecutive no-new-P0/P1/P2 count: **0/2** because Round 3 incorporates P2 #5 and #6.