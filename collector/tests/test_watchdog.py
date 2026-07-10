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


class _FakeProc:
    def __init__(self, info):
        self.info = info


def test_kill_matches_only_python_interpreter(monkeypatch):
    editor_cmdline = ["code.exe", str(wd.COLLECTOR_DIR / "run_collector.py")]
    collector_cmdline = ["pythonw.exe", str(wd.COLLECTOR_DIR / "run_collector.py")]
    procs = [
        _FakeProc({"pid": 100, "cmdline": editor_cmdline}),
        _FakeProc({"pid": 200, "cmdline": collector_cmdline}),
    ]
    monkeypatch.setattr(wd.psutil, "process_iter", lambda *a, **k: procs)

    class _FakeMe:
        pid = -1  # 不等於 100/200，確保 self-skip 邏輯不會誤過濾測試進程

    monkeypatch.setattr(wd.psutil, "Process", lambda: _FakeMe())

    killed_calls = []
    monkeypatch.setattr(wd.subprocess, "run",
                        lambda *a, **k: killed_calls.append(a[0]))

    wd.kill_stale_collector()

    assert len(killed_calls) == 1
    assert killed_calls[0][:2] == ["taskkill", "/PID"]
    assert killed_calls[0][2] == "200"


def test_main_restarts_even_if_record_crashes(tmp_path, monkeypatch):
    hb = tmp_path / "hb"
    hb.write_text(str(time.time() - 9999), encoding="utf-8")
    monkeypatch.setattr(wd, "HEARTBEAT", hb)
    monkeypatch.setattr(wd, "kill_stale_collector", lambda: None)

    def _boom():
        raise RuntimeError("boom")

    monkeypatch.setattr(wd, "record_restart_and_maybe_alert", _boom)

    calls = []
    monkeypatch.setattr(wd.subprocess, "run",
                        lambda *a, **k: calls.append(a[0]))

    assert wd.main() == 1
    assert any(c[:3] == ["schtasks", "/Run", "/TN"] for c in calls)
