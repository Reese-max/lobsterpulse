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

---

# Round 2 continuation — 2026-09-06

Audited default-branch SHA before this documentation update: `b1c383439535833ee117c00093e6555d383cf103`.

Status: **P0 STILL PRESENT ON DEFAULT BRANCH / CANDIDATE FIX READY BUT UNMERGED — NOT CLEAN**

No product fix has landed on `main`; the current default-branch change remains the Round 1 audit documentation. Therefore the same H01/H05/J05/I02 configuration-preservation personas still fail on current default-branch code and the P0 remains open.

## Candidate-fix evidence kept separate from default-branch evidence

Issue #1 has an open, mergeable PR #2 (`fix: preserve existing Codex hooks during setup`) at head `1dd59b5957e0e9bc1d0cf23b0ff47aab4cb4cb69`.

The issue/PR evidence now records:

- merge-in-place preservation of unrelated Codex hooks and unknown fields;
- malformed-JSON fail-closed behavior;
- same-directory temporary write + atomic replacement + bounded backup;
- symlink-aware handling;
- injected replace-failure recovery;
- Unix pre-write permission checks proving the temporary file is mode `0600` and zero-length before configuration bytes are written;
- install/reinstall/remove regression fixtures with existing third-party hooks.

Actual GitHub Actions CI run `34001506509` passed the final PR head across Linux, macOS and Windows (14/14 applicable hooks tests on Linux/macOS; 10/10 on Windows, with release builds/artifacts). Final automated review reportedly found no major issues and its review threads were resolved.

This is real CI/filesystem evidence for the **PR head**, but it is not claimed as default-branch or packaged-release validation because PR #2 remains open and unmerged.

## Same-persona disposition

The regression scenario cannot be marked passed on current `main`. After PR #2 or an equivalent patch lands, rerun H01/H05/J05/I02 against the merged SHA and verify the default-branch cross-platform matrix. A disposable populated Codex home/runtime path remains desirable before considering the original P0 resolved for CLEAN purposes.

## CLEAN gate

Still **NOT CLEAN**. The two clean rounds cannot begin while the P0 implementation remains on the current default branch.