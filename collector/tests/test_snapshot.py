import json
import time

from machine_collector.checks.base import CheckResult
from machine_collector.snapshot import write_heartbeat, write_snapshot


def test_write_snapshot(tmp_path):
    p = tmp_path / "machine-status.json"
    results = [CheckResult("service.pp.port_owner", True, "pid=1 proxypilot.exe"),
               CheckResult("resource.cpu", False, "CPU 95%", value=95.0)]
    write_snapshot(p, results, {"resource.cpu_pct": 95.0},
                   {"resource.cpu": {"severity": "yellow", "message": "m",
                                     "opened_ts": 1.0}})
    d = json.loads(p.read_text(encoding="utf-8"))
    assert abs(d["generated_at"] - time.time()) < 5
    assert d["checks"][0]["check_id"] == "service.pp.port_owner"
    assert d["checks"][1]["ok"] is False
    assert d["samples"]["resource.cpu_pct"] == 95.0
    assert "resource.cpu" in d["open_alerts"]
    assert not p.with_suffix(".tmp").exists()  # 原子寫完 tmp 應消失


def test_write_heartbeat(tmp_path):
    p = tmp_path / "hb"
    write_heartbeat(p)
    assert abs(float(p.read_text()) - time.time()) < 5
    assert not p.with_suffix(".tmp").exists()
