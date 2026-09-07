# 50-Persona Audit — Round 2

Date: 2026-09-07
Protocol: `Reese-max/autodev-ng/docs/portfolio-audit/2026-09-06-50-persona-audit.md`

> Fixed 50-persona model simulation plus current default-branch repository evidence. This is not a 50-human study. Static evidence is not represented as packaged-app or real-user Codex runtime validation.

## Audited product state

Product/default-branch SHA before this audit-document commit: `9aa67523e36947beaef77fa3d420e186900e716b`.

The Round-1 P0 (#1: destructive Codex hooks overwrite) was remediated and merged by PR #2. Current main preserves unrelated hooks, uses atomic writes/backups, and has failure/reinstall/remove regression coverage. GitHub Actions Build run `34052981259` completed successfully on the merged SHA. That is real CI/filesystem evidence for the repository build/tests, not proof of a packaged LobsterPulse installation against a real user's Codex home.

## Round 2 result

Status: **P1 OPEN — NOT CLEAN**

### P1 — existing `codex_hooks = false` is never enabled

The documented Codex setup contract says LobsterPulse enables `codex_hooks = true` in `~/.codex/config.toml`. Current `install_codex_hooks()` only modifies the file when the raw text does **not** contain the substring `codex_hooks`:

```rust
if !content.contains("codex_hooks") {
    if content.contains("[features]") {
        content = content.replace("[features]", "[features]\ncodex_hooks = true");
    } else {
        content.push_str("\n[features]\ncodex_hooks = true\n");
    }
}
```

A valid pre-existing configuration such as:

```toml
[features]
codex_hooks = false
```

is therefore left as `false`. LobsterPulse can install its `hooks.json` entries and return success while Codex hooks remain disabled. The same raw-substring gate is also vulnerable to unrelated appearances in comments/strings and does not establish the effective TOML value.

Actionable Issue: #3 — `[P1][50-persona audit] Enabling Codex monitoring must turn an existing codex_hooks=false to true`.

### Fixed-persona impact

- H01/H02/H03: an existing developer/CI Codex configuration can explicitly disable the feature; LobsterPulse setup does not reconcile it.
- H05/E03: setup can appear successful while monitoring stays inactive, with no clear recovery path.
- I05: partial success is especially misleading because `hooks.json` changes land while the feature remains off.
- J05: customized Codex configuration is a normal expert-user state and must be preserved while updating the effective flag.

## Required regression gates

1. Parse/update TOML semantically enough to distinguish absent/true/false from comments or unrelated strings.
2. Enabling LobsterPulse must establish effective `[features].codex_hooks = true` even when the prior value is `false`.
3. Malformed/duplicate-conflicting configuration must fail closed rather than append another ambiguous key.
4. Preserve unrelated configuration and define reversible ownership for the flag so disable does not overwrite a user-owned setting.
5. Add fixtures for absent, true, false, malformed, duplicate/conflicting, comment/string false-positive, reinstall and remove.
6. After the fix lands, re-run the same H01/H02/H03/H05/I05/J05 scenarios on the merged SHA and record actual CI/runtime evidence separately.

## Runtime status

- **Confirmed:** merged-SHA GitHub Actions Build run `34052981259` succeeded.
- **Confirmed static:** current source leaves an existing `codex_hooks = false` unchanged because it only tests substring presence.
- **Not claimed:** no packaged LobsterPulse install or live Codex process was executed in this audit turn, so no real-user Codex runtime pass/failure is claimed.

## CLEAN gate

**NOT CLEAN.** Issue #3 is open and this round produced a new P1 after the Round-1 P0 remediation. The two-consecutive-clean-round counter therefore cannot start. After #3 is fixed, the fixed 50 personas must be re-run against current/recent code with the required runtime paths evidenced before CLEAN can be considered.