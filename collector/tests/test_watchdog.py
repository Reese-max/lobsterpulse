import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
import watchdog as wd


def test_heartbeat_age_fresh(tmp_path, monkeypatch):
    hb = tmp_path / "hb"
    hb.write_text(str(time.time()), encoding="utf-8")
    monkeypatch.setattr(wd, "HEARTBEAT", hb)
    assert wd.heartbeat_age() < 5


def test_heartbeat_age_missing_is_inf(tmp_path, monkeypatch):
    monkeypatch.setattr(wd, "HEARTBEAT", tmp_path / "nope")
    assert wd.heartbeat_age() == float("inf")


def test_main_fresh_heartbeat_noop(tmp_path, monkeypatch):
    hb = tmp_path / "hb"
    hb.write_text(str(time.time()), encoding="utf-8")
    monkeypatch.setattr(wd, "HEARTBEAT", hb)
    called = []
    monkeypatch.setattr(wd, "kill_stale_collector", lambda: called.append("kill"))
    assert wd.main() == 0
    assert called == []


def test_main_stale_kills_and_restarts(tmp_path, monkeypatch):
    hb = tmp_path / "hb"
    hb.write_text(str(time.time() - 9999), encoding="utf-8")
    monkeypatch.setattr(wd, "HEARTBEAT", hb)
    monkeypatch.setattr(wd, "RESTARTS", tmp_path / "restarts")
    actions = []
    monkeypatch.setattr(wd, "kill_stale_collector", lambda: actions.append("kill"))
    monkeypatch.setattr(wd.subprocess, "run",
                        lambda *a, **k: actions.append(("run", a[0][:3])))
    monkeypatch.setattr(wd, "_alert_telegram", lambda text: actions.append("tg"))
    assert wd.main() == 1
    assert "kill" in actions
    assert any(isinstance(x, tuple) and x[1] == ["schtasks", "/Run", "/TN"]
               for x in actions)


def test_restart_burst_alerts(tmp_path, monkeypatch):
    monkeypatch.setattr(wd, "RESTARTS", tmp_path / "restarts")
    sent = []
    monkeypatch.setattr(wd, "_alert_telegram", lambda text: sent.append(text))
    assert wd.record_restart_and_maybe_alert() is False
    assert wd.record_restart_and_maybe_alert() is False
    assert wd.record_restart_and_maybe_alert() is True  # 第 3 次 -> 告警
    assert len(sent) == 1
