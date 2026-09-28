#!/usr/bin/env python3
"""Check the repo's counter migration decision against docs, OpenSpec and code."""

from __future__ import annotations

import datetime as dt
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = Path("docs/metrics/counter-migration.json")
CHANGE = Path("openspec/changes/prometheus-counter-rename-2026-q3")
PAIRS = (
    ("lobsterpulse_tokens_input", "lobsterpulse_tokens_input_total"),
    ("lobsterpulse_tokens_output", "lobsterpulse_tokens_output_total"),
    ("lobsterpulse_provider_tokens_input", "lobsterpulse_provider_tokens_input_total"),
    ("lobsterpulse_provider_tokens_output", "lobsterpulse_provider_tokens_output_total"),
    ("lobsterpulse_provider_failure_count", "lobsterpulse_provider_failure_count_total"),
    ("lobsterpulse_provider_session_count", "lobsterpulse_provider_session_count_total"),
)
STATES = {"DUAL_EMIT", "READY_TO_CUTOVER", "CUTOVER", "POSTPONED", "CANCELLED"}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def read(path: Path) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def validate_closed_phase(metadata: str, record: dict) -> None:
    """A closed implementation phase must point to tracked, explained successors."""
    closed = re.search(r"(?m)^status:\s*closed\s*$", metadata) is not None
    if not closed:
        return
    require(
        f"successor_tracking: {MANIFEST.as_posix()}" in metadata,
        "closed OpenSpec has no linked successor tracking object",
    )
    phase = record.get("phase_one", {})
    require(phase.get("status") == "closed", "OpenSpec and manifest phase status differ")
    require(
        phase.get("successor_tracking") == "https://github.com/Reese-max/lobsterpulse/issues/11",
        "closed phase has no linked issue for required successors",
    )
    successors = phase.get("required_successors", [])
    require(
        {item.get("id") for item in successors} == {"T-2", "T-3", "T-4", "T-5"},
        "closed phase is missing a required successor",
    )
    for item in successors:
        require(item.get("state") and item.get("reason"), f"{item.get('id')} lacks state or evidence reason")


def validate() -> None:
    record = json.loads(read(MANIFEST))
    state = record.get("state")
    require(state in STATES, f"unknown lifecycle state: {state}")
    require(record.get("owner") and record.get("evidence"), "owner and evidence are required")
    decision = dt.date.fromisoformat(record["decision_date"])
    review = dt.date.fromisoformat(record["next_review_date"])
    require(review > decision, "review date must follow decision date")
    require(record.get("next_action") and record.get("cutover_gate"), "next action and cutover gate are required")
    actual_pairs = [(p.get("legacy"), p.get("canonical")) for p in record.get("pairs", [])]
    require(actual_pairs == list(PAIRS), "manifest must name all six ordered legacy/canonical pairs")

    metadata = read(CHANGE / ".openspec.yaml")
    require(f"migration_state: {state}" in metadata, "OpenSpec metadata disagrees with manifest state")
    validate_closed_phase(metadata, record)
    for path in (
        Path("README.md"),
        Path("CONTRIBUTING.md"),
        Path("CHANGELOG.md"),
        CHANGE / "tasks.md",
        CHANGE / "design.md",
        CHANGE / "specs/prometheus-counter-rename-2026-q3/spec.md",
        Path("openspec/changes/prometheus-counter-convention/proposal.md"),
        Path("openspec/changes/prometheus-counter-convention/design.md"),
        Path("openspec/changes/prometheus-counter-convention/tasks.md"),
        Path("openspec/changes/prometheus-counter-convention/specs/prometheus-counter-convention/spec.md"),
    ):
        body = read(path)
        for marker in (state, record["decision_date"], record["next_review_date"]):
            require(marker in body, f"{path} does not show current {marker}")
        links = re.findall(r"\]\(([^)]+counter-migration\.json)\)", body)
        require(links, f"{path} does not link the migration manifest")
        for link in links:
            require((ROOT / path.parent / link).is_file(), f"{path} has a broken manifest link: {link}")

    source = read(Path("src-tauri/src/lib.rs"))
    require('include_str!("../../docs/metrics/counter-migration.json")' in source, "exporter must embed manifest")
    require("migration {} since {}; review {}" in source, "exporter HELP lacks decision/review diagnostics")
    require("scheduled removal week 4" not in source, "exporter still announces expired removal")
    contract = re.search(r"const LP_METRICS: &\[&str\] = &\[(.*?)\];", source, re.S)
    require(contract is not None, "cannot locate LP_METRICS contract")
    names = re.findall(r'"(lobsterpulse_[a-z0-9_]+)"', contract.group(1))
    require(len(names) == len(set(names)), "LP_METRICS contains duplicate names")
    expected_size = 41 if state == "CUTOVER" else 47
    require(len(names) == expected_size, f"{state} must have {expected_size} contract names; found {len(names)}")
    for legacy, canonical in PAIRS:
        require(canonical in names, f"missing canonical contract name: {canonical}")
        require((legacy in names) == (state != "CUTOVER"), f"legacy contract name conflicts with {state}: {legacy}")
        require(f"# HELP {canonical} " in source, f"missing canonical HELP: {canonical}")
        require((f"# HELP {legacy} " in source) == (state != "CUTOVER"), f"legacy HELP conflicts with {state}: {legacy}")

    if state in {"POSTPONED", "CANCELLED"}:
        require(record["consumers"]["external_status"] == "unknown_no_owner_inventory", "external consumer state must stay explicit")
        require(not record["consumers"]["external"], "external consumers need owner-supplied evidence")
        require("postponed_counter_migration_preserves_values_for_all_13_providers" in source, "missing 13-provider postponement test")
        for field, marker in (("decision_date", record["decision_date"]), ("next_review_date", record["next_review_date"])):
            require(f'assert_eq!(migration.{field}, "{marker}")' in source, f"fixture assertion disagrees with manifest {field}")
        for path in (Path("README.md"), Path("CONTRIBUTING.md"), Path("CHANGELOG.md")):
            body = read(path)
            require("2026-07-03" in body and "did not" in body, f"{path} must retire expired promise explicitly")

    # Scan only repository-owned live configuration locations. Historical prose,
    # test fixtures and source declarations are not deployed PromQL consumers.
    live_roots = (Path(".github"), Path("collector"), Path("runtime"), Path("scripts"), Path("src"))
    live_suffixes = {".json", ".yaml", ".yml", ".prom", ".rules"}
    live_refs = []
    tracked = subprocess.check_output(
        ["git", "-C", str(ROOT), "ls-files", "-z", *(str(p) for p in live_roots)]
    )
    for name in filter(None, tracked.decode("utf-8").split("\0")):
        path = ROOT / name
        require(not path.is_symlink(), f"consumer scan will not follow a symlink: {name}")
        if path.suffix in live_suffixes:
            body = path.read_text(encoding="utf-8", errors="replace")
            if any(re.search(rf"\b{re.escape(legacy)}\b", body) for legacy, _ in PAIRS):
                live_refs.append(path.relative_to(ROOT).as_posix())
    inventoried = record["consumers"]["in_repo_active_promql_rules_dashboards"]
    require(sorted(live_refs) == sorted(inventoried), f"active in-repo consumer inventory drift: {live_refs} != {inventoried}")
    print(f"counter migration: {state}; {len(PAIRS)} pairs; {len(names)} contract names; {len(live_refs)} active in-repo consumer configs")


if __name__ == "__main__":
    try:
        validate()
    except (KeyError, ValueError, TypeError, OSError) as exc:
        print(f"counter migration consistency failed: {exc}", file=sys.stderr)
        sys.exit(1)
