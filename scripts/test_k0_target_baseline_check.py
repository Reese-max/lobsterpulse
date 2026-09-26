"""Regression fixtures for the current K0 capability and denominator contract."""
import copy
import shutil
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
import k0_measure as measure  # noqa: E402
import k0_target_baseline_check as guard  # noqa: E402


def current():
    return guard.read_json(guard.REGISTRY), guard.read_json(guard.BASELINES)


def empty_quota(registry):
    return {p["id"]: {"state": "missing"} for p in registry["providers"]}


def test_current_13_registry_and_public_claims_agree():
    registry, baselines = current()
    assert guard.validate(registry, baselines) == []
    assert len(registry["providers"]) == 13
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""),
                                             empty_quota(registry), None, registry=registry)
    assert guard.validate_receipt(receipt, registry) == []
    assert receipt["dimensions"]["configured"]["numerator"] is None
    assert receipt["dimensions"]["registered"]["numerator"] == 13
    assert receipt["dimensions"]["live_emitting"]["numerator"] == 0


def test_disabled_provider_is_not_configured_even_with_hook_capability():
    registry, _ = current()
    configured = {p["id"]: True for p in registry["providers"]}
    configured["mimo"] = False
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""),
                                             empty_quota(registry), configured, registry=registry)
    assert receipt["dimensions"]["configured"]["numerator"] == 12
    assert receipt["providers"]["mimo"]["health_status"] == "NOT_CONFIGURED"


def test_openab_hook_can_be_live_while_quota_owner_is_external():
    registry, _ = current()
    metrics = ('lobsterpulse_provider_events_total{provider="irisx_bot"} 1\n'
               'lobsterpulse_provider_idle_seconds{provider="irisx_bot"} 10\n'
               'lobsterpulse_provider_sessions{provider="irisx_bot"} 1\n')
    receipt = measure.build_coverage_receipt(metrics, measure.parse_provider_sessions(metrics),
                                             empty_quota(registry), {"irisx_bot": True}, registry=registry)
    assert receipt["providers"]["irisx_bot"]["health_status"] == "LIVE_EMITTING"
    assert receipt["providers"]["irisx_bot"]["quota_status"] == "EXTERNAL_DEPENDENCY"
    assert receipt["dimensions"]["live_emitting"]["numerator"] == 1
    assert receipt["dimensions"]["quota_observable"]["numerator"] == 0


def test_stale_snapshot_is_visible_but_not_quota_observable():
    registry, _ = current()
    quota = empty_quota(registry)
    quota["cicx"] = {"state": "stale", "mtime_age_hours": 25}
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""),
                                             quota, {"cicx": True}, registry=registry)
    assert receipt["providers"]["cicx"]["quota_status"] == "STALE"
    assert receipt["dimensions"]["quota_observable"]["numerator"] == 0


def test_scope_change_requires_new_dated_baseline_and_preserves_history():
    registry, baselines = current()
    registry = copy.deepcopy(registry)
    baselines = copy.deepcopy(baselines)
    next(p for p in registry["providers"] if p["id"] == "mimo")["lifecycle"] = "deprecated"
    assert any("baseline" in e for e in guard.validate(registry, baselines))
    registry["registry_version"] = "2026-09-27.1"
    registry["baseline_id"] = "k0-2026-09-27"
    baselines["baselines"][-1]["status"] = "historical"
    new_baseline = copy.deepcopy(baselines["baselines"][-1])
    new_baseline.update(id="k0-2026-09-27", status="current", effective_date="2026-09-27")
    new_baseline["health_scope_ids"].remove("mimo")
    new_baseline["quota_scope_ids"].remove("mimo")
    baselines["baselines"].append(new_baseline)
    assert guard.validate(registry, baselines) == []
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""),
                                             empty_quota(registry), None, registry=registry)
    assert receipt["dimensions"]["live_emitting"]["denominator"] == 12
    assert receipt["dimensions"]["live_emitting"]["exclusions"] == [
        {"id": "mimo", "reason": "lifecycle:deprecated"}]
    assert guard.validate_receipt(receipt, registry) == []
    assert baselines["baselines"][0]["targets"] == guard.ORIGINAL_TARGETS


def test_13_advertised_two_live_four_excluded_never_becomes_zero_gap():
    registry, _ = current()
    registry = copy.deepcopy(registry)
    for p in registry["providers"]:
        if p["id"] in guard.ORIGINAL_EXCLUSIONS:
            p["lifecycle"] = "out_of_scope"
    metrics = "".join(
        f'lobsterpulse_provider_events_total{{provider="{pid}"}} 1\n'
        f'lobsterpulse_provider_idle_seconds{{provider="{pid}"}} 10\n'
        for pid in ("claude", "cicx")
    )
    receipt = measure.build_coverage_receipt(metrics, measure.parse_provider_sessions(metrics),
                                             empty_quota(registry), None, registry=registry)
    dimension = receipt["dimensions"]["live_emitting"]
    assert dimension["numerator"] == 2
    assert dimension["denominator"] == 9
    assert dimension["registered_denominator"] == 13
    assert dimension["gap_to_registered"] == 11
    assert {x["id"] for x in dimension["exclusions"]} == guard.ORIGINAL_EXCLUSIONS
    assert guard.validate_receipt(receipt, registry) == []


def test_historical_target_rewrite_and_receipt_mismatch_fail():
    registry, baselines = current()
    baselines = copy.deepcopy(baselines)
    baselines["baselines"][0]["targets"]["live_emit"] = "9/9"
    assert "original 90-day targets were rewritten" in guard.validate(registry, baselines)
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""),
                                             empty_quota(registry), None, registry=registry)
    receipt["dimensions"]["live_emitting"]["denominator"] = 9
    assert any("denominator disagrees" in e for e in guard.validate_receipt(receipt, registry))


def test_readme_mismatch_fails(tmp_path):
    registry, baselines = current()
    files = ["README.md", "MISSION.md", "docs/index.html", "src/index.html",
             "src/main.js", "src/provider-coverage.js", "src-tauri/src/config.rs",
             "src-tauri/src/hook_server.rs"]
    for name in files:
        source = guard.ROOT / name
        target = tmp_path / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    readme = tmp_path / "README.md"
    readme.write_text(readme.read_text(encoding="utf-8").replace("註冊 13", "註冊 12"),
                      encoding="utf-8")
    assert any("README: registered count" in e for e in guard.validate(registry, baselines, root=tmp_path))
