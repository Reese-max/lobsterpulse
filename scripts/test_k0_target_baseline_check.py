"""Regression fixtures for the current K0 capability and denominator contract."""
import copy
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

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


def test_review_metrics_outage_is_unknown(tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(measure, "fetch_metrics", lambda: "")
    monkeypatch.setattr(measure, "scan_quota_snapshots", lambda: empty_quota(measure.REGISTRY))
    monkeypatch.setattr(measure, "read_configured_providers",
                        lambda: dict.fromkeys(measure.KNOWN_PROVIDERS, True))
    monkeypatch.setattr(measure, "__file__", str(tmp_path / "scripts" / "k0_measure.py"))
    assert measure.main() == 0
    receipt = json.loads((tmp_path / ".harness-k0.json").read_text(encoding="utf-8"))["coverage_receipt"]
    for name in ("live_emitting", "nonzero_sessions"):
        assert receipt["dimensions"][name]["numerator"] is None
        assert receipt["dimensions"][name]["status"] == "UNAVAILABLE"
    assert receipt["providers"]["claude"]["health_status"] == "UNAVAILABLE"
    assert receipt["providers"]["claude"]["session_count"] is None
    assert "live_emitting: unknown/13" in capsys.readouterr().out


def test_review_macos_config_uses_application_support(tmp_path, monkeypatch):
    monkeypatch.delenv("LOBSTERPULSE_CONFIG_FILE", raising=False)
    monkeypatch.setattr(measure.sys, "platform", "darwin")
    monkeypatch.setattr(Path, "home", lambda: tmp_path)
    monkeypatch.setenv("APPDATA", str(tmp_path / "wrong-windows-path"))
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "wrong-linux-path"))
    config = tmp_path / "Library" / "Application Support" / "lobsterpulse" / "config.json"
    config.parent.mkdir(parents=True)
    config.write_text(json.dumps({"providers": {"codex": {"enabled": True}}}), encoding="utf-8")
    configured = measure.read_configured_providers()
    assert configured is not None
    assert configured["codex"] is True
    assert configured["mimo"] is False


@pytest.mark.parametrize("age,mtime_age,runners,expected", [
    (90000, 0, [{"name": "codex", "ok": True}], "stale"),
    (10, 90000, [{"name": "codex", "ok": True}], "fresh"),
    (10, 0, [], "missing"),
    (10, 0, [{"name": "codex", "ok": False}], "missing"),
    (10, 0, [{"name": "codex", "ok": "true"}], "missing"),
    (-10, 0, [{"name": "codex", "ok": True}], "missing"),
])
def test_review_quota_uses_successful_payload_timestamp(
        tmp_path, monkeypatch, age, mtime_age, runners, expected):
    now = 1_790_000_000
    monkeypatch.setattr(measure.time, "time", lambda: now)
    monkeypatch.setattr(measure, "QUOTA_DIR", tmp_path)
    for filename in ("usage-local.json", "usage-irisx_bot.json"):
        path = tmp_path / filename
        path.write_text(json.dumps({"updated_at": now - age, "runners": runners}), encoding="utf-8")
        os.utime(path, (now - mtime_age, now - mtime_age))
    quota = measure.scan_quota_snapshots()
    for pid in ("codex", "irisx_bot"):
        assert quota[pid]["state"] == expected
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""), quota, None)
    assert receipt["dimensions"]["quota_observable"]["numerator"] == (2 if expected == "fresh" else 0)
    # Run the actual UI calculator against the same on-disk payloads.
    snapshots = {"__local__": json.loads((tmp_path / "usage-local.json").read_text()),
                 "irisx_bot": json.loads((tmp_path / "usage-irisx_bot.json").read_text())}
    ui = subprocess.run(["node", "-e", """
const { build } = require('./src/provider-coverage.js');
const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
process.stdout.write(JSON.stringify(build(input.registry, null, null, input.snapshots, {}, input.now)));
"""], input=json.dumps({"registry": measure.REGISTRY, "snapshots": snapshots, "now": now}),
        text=True, capture_output=True, check=True, cwd=guard.ROOT)
    ui_coverage = json.loads(ui.stdout)
    assert ui_coverage["dimensions"]["quotaObservable"]["numerator"] == receipt["dimensions"]["quota_observable"]["numerator"]
    for pid in ("codex", "irisx_bot"):
        row = next(r for r in ui_coverage["rows"] if r["id"] == pid)
        assert row["quotaStatus"] == receipt["providers"][pid]["quota_status"]


def test_review_quota_errors_stay_unknown_and_shared_file_is_read_once(tmp_path, monkeypatch):
    monkeypatch.setattr(measure, "QUOTA_DIR", tmp_path)
    (tmp_path / "usage-local.json").write_text("{broken", encoding="utf-8")
    reads = []
    read_snapshot = measure.read_quota_snapshot
    def tracked_read(path, now):
        reads.append(path)
        return read_snapshot(path, now)
    monkeypatch.setattr(measure, "read_quota_snapshot", tracked_read)
    quota = measure.scan_quota_snapshots()
    assert reads.count(tmp_path / "usage-local.json") == 1
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""), quota, None)
    assert receipt["dimensions"]["quota_observable"]["numerator"] is None
    assert receipt["providers"]["codex"]["quota_status"] == "UNAVAILABLE"


def test_review_deprecated_quota_has_out_of_scope_status():
    registry, _ = current()
    registry["providers"][-1]["lifecycle"] = "deprecated"
    quota = empty_quota(registry)
    quota["mimo"] = {"state": "fresh"}
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""), quota, None,
                                             registry=registry)
    assert receipt["providers"]["mimo"]["quota_status"] == "OUT_OF_SCOPE"
    assert receipt["dimensions"]["quota_observable"]["numerator"] == 0


def test_review_openx_falls_back_after_corrupt_primary(tmp_path, monkeypatch):
    monkeypatch.setattr(measure, "QUOTA_DIR", tmp_path)
    (tmp_path / "usage-openx.json").write_text("{broken", encoding="utf-8")
    legacy = tmp_path / "usage-bot.json"
    legacy.write_text(json.dumps({"updated_at": measure.time.time() - 10,
                                  "runners": [{"name": "opencode", "ok": True}]}), encoding="utf-8")
    quota = measure.scan_quota_snapshots()
    assert quota["openx"]["state"] == "fresh"
    assert quota["openx"]["path"] == str(legacy)


@pytest.mark.parametrize("index,field", [
    (0, "registered_ids"), (1, "registered_ids"),
    (2, "registered_ids"), (2, "health_scope_ids"), (2, "quota_scope_ids"),
])
def test_review_archived_provider_ids_cannot_be_rewritten(index, field):
    registry, baselines = current()
    registry["providers"][-1]["lifecycle"] = "deprecated"
    registry.update(registry_version="2026-09-27.1", baseline_id="k0-2026-09-27")
    previous = baselines["baselines"][-1]
    previous["status"] = "historical"
    next_baseline = copy.deepcopy(previous)
    next_baseline.update(id="k0-2026-09-27", effective_date="2026-09-27", status="current")
    next_baseline["health_scope_ids"].remove("mimo")
    next_baseline["quota_scope_ids"].remove("mimo")
    baselines["baselines"].append(next_baseline)
    assert guard.validate(registry, baselines) == []
    baselines["baselines"][index][field] = ["mimo"] * 13
    assert guard.validate(registry, baselines), "historical provider identities were rewritten"


@pytest.mark.parametrize("dimension,changes", [
    ("live_emitting", {"provider_ids": ["fake"]}),
    ("live_emitting", {"status": "UNAVAILABLE"}),
    ("configured", {"provider_ids": ["claude"], "gap_to_registered": 12}),
    ("registered", {"numerator": 0, "gap_to_registered": 13}),
])
def test_review_receipt_rejects_contradictory_evidence(dimension, changes):
    registry, _ = current()
    receipt = measure.build_coverage_receipt("", measure.parse_provider_sessions(""),
                                             empty_quota(registry), None)
    receipt["dimensions"][dimension].update(changes)
    assert guard.validate_receipt(receipt, registry)
