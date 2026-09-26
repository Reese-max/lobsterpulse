#!/usr/bin/env python3
"""Fail closed when the current K0 denominator or public coverage labels drift.

The R81 and R182/R197 rows in MISSION.md are archival receipts. Current scope is
defined by the dated baseline and versioned provider registry, never by a
retroactive rewrite of those rows.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "src" / "provider-capabilities.json"
BASELINES = ROOT / "docs" / "k0-baselines.json"
REQUIRED_FIELDS = ("id", "lifecycle", "scope", "platform_scope", "required_signal",
                   "freshness_seconds", "quota_signal", "quota_freshness_seconds",
                   "owner", "support_level")
REQUIRED_DIMENSIONS = ("registered", "configured", "live_emitting",
                       "nonzero_sessions", "quota_observable")
HISTORICAL_IDS = ("r81-90-day-target", "r182-r197-path-a")
ORIGINAL_TARGETS = {"live_emit": "13/13", "nonzero_sessions": "13/13", "quota": "13/13"}
ORIGINAL_OBSERVATIONS = {"live_emit": "2/13", "nonzero_sessions": "1/13",
                         "fresh_quota": "2/13", "quota_path": "7/13"}
ORIGINAL_EXCLUSIONS = {"irisx_bot", "grokx", "lpbot", "mimo"}
INITIAL_BASELINE_ID = "k0-2026-09-26"


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def validate_receipt(receipt: dict, registry: dict) -> list[str]:
    errors = []
    if receipt.get("registry_version") != registry.get("registry_version"):
        errors.append("receipt registry version differs from canonical registry")
    if receipt.get("baseline_id") != registry.get("baseline_id"):
        errors.append("receipt baseline differs from canonical registry")
    if not receipt.get("timestamp"):
        errors.append("receipt timestamp missing")
    dimensions = receipt.get("dimensions", {})
    registered = len(registry["providers"])
    scoped = [p for p in registry["providers"] if p["lifecycle"] == "active"]
    excluded = {p["id"]: f"lifecycle:{p['lifecycle']}" for p in registry["providers"]
                if p["lifecycle"] != "active"}
    for name in REQUIRED_DIMENSIONS:
        d = dimensions.get(name, {})
        expected_denominator = registered if name == "registered" else len(scoped)
        if d.get("denominator") != expected_denominator or d.get("registered_denominator") != registered:
            errors.append(f"{name}: denominator disagrees with registry")
        expected_exclusions = {} if name == "registered" else excluded
        actual_exclusions = {x.get("id"): x.get("reason") for x in d.get("exclusions", [])}
        if actual_exclusions != expected_exclusions:
            errors.append(f"{name}: exclusions incomplete or incorrect")
        numerator = d.get("numerator")
        if numerator is not None and (not isinstance(numerator, int) or
                                      not 0 <= numerator <= expected_denominator):
            errors.append(f"{name}: invalid numerator")
        if numerator is not None and d.get("gap_to_registered") != registered - numerator:
            errors.append(f"{name}: registered gap is hidden")
        if numerator is None and d.get("status") != "UNAVAILABLE":
            errors.append(f"{name}: unknown source shown as measured")
    return errors


def validate(registry: dict, baselines: dict, *, root: Path = ROOT) -> list[str]:
    errors = []
    records = registry.get("providers", [])
    ids = [p.get("id") for p in records]
    if len(ids) != 13 or len(set(ids)) != 13:
        errors.append("registry must contain the 13 unique advertised IDs")
    if not registry.get("registry_version") or not registry.get("baseline_id"):
        errors.append("registry version or baseline ID missing")
    for p in records:
        missing = [field for field in REQUIRED_FIELDS if field not in p]
        if missing:
            errors.append(f"{p.get('id')}: missing {missing}")
            continue
        if p["lifecycle"] not in ("active", "deprecated", "out_of_scope"):
            errors.append(f"{p['id']}: unknown lifecycle")
        if p["scope"] not in ("local_cli", "openab_push"):
            errors.append(f"{p['id']}: unknown scope")
        if p["support_level"] not in ("hook_and_quota_reader", "hook_intake_quota_external"):
            errors.append(f"{p['id']}: unknown support level")
        if not p["owner"] or not p["platform_scope"] or not p["required_signal"] or not p["quota_signal"]:
            errors.append(f"{p['id']}: signal or owner missing")
        if p["freshness_seconds"] <= 0 or p["quota_freshness_seconds"] <= 0:
            errors.append(f"{p['id']}: freshness must be positive")

    baseline_rows = baselines.get("baselines", [])
    by_id = {b.get("id"): b for b in baseline_rows}
    if len(by_id) != len(baseline_rows):
        errors.append("duplicate baseline IDs")
    if tuple(b.get("id") for b in baseline_rows[:2]) != HISTORICAL_IDS:
        errors.append("historical baselines were removed or reordered")
    if by_id.get(HISTORICAL_IDS[0], {}).get("targets") != ORIGINAL_TARGETS:
        errors.append("original 90-day targets were rewritten")
    if by_id.get(HISTORICAL_IDS[1], {}).get("recorded_observations") != ORIGINAL_OBSERVATIONS:
        errors.append("R182/R197 observed result was rewritten")
    if set(by_id.get(HISTORICAL_IDS[1], {}).get("excluded_ids", [])) != ORIGINAL_EXCLUSIONS:
        errors.append("R182/R197 archived exclusions were rewritten")
    if INITIAL_BASELINE_ID not in by_id:
        errors.append("initial dated baseline was removed")
    initial = by_id.get(INITIAL_BASELINE_ID, {})
    if initial.get("effective_date") != "2026-09-26" or len(initial.get("health_scope_ids", [])) != 13 or len(initial.get("quota_scope_ids", [])) != 13:
        errors.append("initial dated baseline was rewritten")
    current = [b for b in baseline_rows if b.get("status") == "current"]
    if len(current) != 1:
        errors.append("exactly one dated current baseline required")
    else:
        baseline = current[0]
        if baseline.get("id") != registry.get("baseline_id"):
            errors.append("registry baseline ID disagrees with current baseline")
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", baseline.get("effective_date", "")):
            errors.append("current baseline needs an effective date")
        elif not str(registry.get("registry_version", "")).startswith(baseline["effective_date"] + "."):
            errors.append("registry version must carry current baseline date")
        if set(baseline.get("registered_ids", [])) != set(ids):
            errors.append("current registered denominator disagrees with registry")
        scoped = {p["id"] for p in records if p.get("lifecycle") == "active"}
        for dimension in ("health_scope_ids", "quota_scope_ids"):
            if set(baseline.get(dimension, [])) != scoped:
                errors.append(f"{dimension} changed without a new dated baseline")
        if baseline["id"] == INITIAL_BASELINE_ID and len(scoped) != 13:
            errors.append("initial baseline scope changed; create a new dated baseline")
        if baseline["id"] != INITIAL_BASELINE_ID and by_id.get(INITIAL_BASELINE_ID, {}).get("status") != "historical":
            errors.append("scope change must retain the prior baseline as history")
        if baseline["id"] != INITIAL_BASELINE_ID and baseline.get("effective_date", "") <= "2026-09-26":
            errors.append("new scope baseline must have a later date")

    try:
        readme = (root / "README.md").read_text(encoding="utf-8")
        website = (root / "docs" / "index.html").read_text(encoding="utf-8")
        mission = (root / "MISSION.md").read_text(encoding="utf-8")
        ui = (root / "src" / "index.html").read_text(encoding="utf-8")
        ui_main = (root / "src" / "main.js").read_text(encoding="utf-8")
        ui_coverage = (root / "src" / "provider-coverage.js").read_text(encoding="utf-8")
        config_rs = (root / "src-tauri" / "src" / "config.rs").read_text(encoding="utf-8")
        hook_rs = (root / "src-tauri" / "src" / "hook_server.rs").read_text(encoding="utf-8")
    except OSError as exc:
        return errors + [f"required source missing: {exc}"]
    for name, content in (("README", readme), ("website", website)):
        advertised_counts = [int(x) for x in re.findall(r"(?:已註冊|註冊)\s*(\d+)", content)]
        if not advertised_counts or any(x != len(ids) for x in advertised_counts):
            errors.append(f"{name}: registered count label missing or mismatched")
        if any(x not in content for x in ("已設定", "非零 session", "quota")):
            errors.append(f"{name}: distinct runtime coverage labels missing")
        if "本地可達上限 9" in content or "0 結構性差距" in content:
            errors.append(f"{name}: obsolete nine-provider or zero-gap claim")
    if "2026-09-26 現行口徑" not in mission or "歷史紀錄：R182" not in mission:
        errors.append("MISSION must distinguish current and historical baselines")
    if 'id="provider-coverage"' not in ui or 'src="provider-coverage.js"' not in ui:
        errors.append("product UI coverage panel missing")
    if 'fetch("provider-capabilities.json")' not in ui_main:
        errors.append("product UI is not using the canonical registry")
    for marker in ("registered", "configured", "liveEmitting", "nonzeroSessions", "quotaObservable",
                   "EXTERNAL_DEPENDENCY", "UNSUPPORTED_ON_THIS_HOST", "STALE", "NOT_CONFIGURED"):
        if marker not in ui_main + ui_coverage:
            errors.append(f"product UI status/coverage marker missing: {marker}")

    # Restrict to the provider registry function rather than other config maps.
    config_fn = config_rs.split("fn default_providers()", 1)[-1].split("impl Default for AppConfig", 1)[0]
    config_ids = set(re.findall(r'm\.insert\(\s*"([a-z_]+)"\.into\(\)', config_fn))
    if config_ids != set(ids):
        errors.append("config default_providers IDs disagree with registry")
    known_match = re.search(r"KNOWN_PROVIDERS[^=]*=\s*&\[(.*?)\]", hook_rs, re.DOTALL)
    if not known_match or set(re.findall(r'"([a-z_]+)"', known_match.group(1))) != set(ids):
        errors.append("hook_server KNOWN_PROVIDERS IDs disagree with registry")

    try:
        from k0_measure import build_coverage_receipt, parse_provider_sessions, scan_quota_snapshots
        # Deliberately empty evidence: registry membership cannot become monitored coverage.
        receipt = build_coverage_receipt("", parse_provider_sessions(""),
                                         {pid: {"state": "missing"} for pid in ids}, None,
                                         registry=registry, timestamp="2026-09-26T00:00:00+0000")
        errors.extend(validate_receipt(receipt, registry))
        if receipt["dimensions"]["live_emitting"]["numerator"] != 0:
            errors.append("registry membership fabricated live coverage")
    except (ImportError, KeyError, ValueError, TypeError) as exc:
        errors.append(f"KPI producer disagrees with registry: {exc}")
    return errors


def main() -> int:
    try:
        errors = validate(read_json(REGISTRY), read_json(BASELINES))
    except (OSError, ValueError) as exc:
        print(f"[K0 coverage] required registry/baseline unavailable: {exc}")
        return 2
    if errors:
        for error in errors:
            print(f"[K0 coverage] FAIL: {error}")
        return 1
    print("[K0 coverage] registry, dated baseline, README, UI, Rust inventory and KPI receipt agree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
