import machine_collector.checks.services as svc_mod
from machine_collector.config import ServiceCfg


class _FakeProc:
    def __init__(self, pid):
        self.pid = pid

    def name(self):
        return "proxypilot.exe"

    def cmdline(self):
        return ["proxypilot.exe", "-config", "C:\\CLIProxyAPI\\config.yaml"]


def _svc(**kw):
    base = dict(id="pp", port=8317, process_pattern="proxypilot")
    base.update(kw)
    return ServiceCfg(**base)


def test_port_owner_match(monkeypatch):
    monkeypatch.setattr(svc_mod, "_listening_pid", lambda port: 42)
    monkeypatch.setattr(svc_mod.psutil, "Process", _FakeProc)
    r = svc_mod.check_port_owner(_svc())
    assert r.check_id == "service.pp.port_owner"
    assert r.ok is True


def test_port_owner_mismatch(monkeypatch):
    monkeypatch.setattr(svc_mod, "_listening_pid", lambda port: 42)
    monkeypatch.setattr(svc_mod.psutil, "Process", _FakeProc)
    r = svc_mod.check_port_owner(_svc(process_pattern="hermes"))
    assert r.ok is False
    assert "不符" in r.detail


def test_port_nobody_listening(monkeypatch):
    monkeypatch.setattr(svc_mod, "_listening_pid", lambda port: None)
    r = svc_mod.check_port_owner(_svc())
    assert r.ok is False
    assert "無人監聽" in r.detail


def test_port_owner_enumeration_failure(monkeypatch):
    def boom(port):
        raise svc_mod.psutil.AccessDenied(pid=None)
    monkeypatch.setattr(svc_mod, "_listening_pid", boom)
    r = svc_mod.check_port_owner(_svc())
    assert r.ok is False
    assert "連線列舉失敗" in r.detail


def test_port_owner_pid_unknown(monkeypatch):
    monkeypatch.setattr(svc_mod, "_listening_pid", lambda port: 0)
    r = svc_mod.check_port_owner(_svc())
    assert r.ok is False
    assert "無法取得 owner" in r.detail


def test_port_owner_process_vanished(monkeypatch):
    monkeypatch.setattr(svc_mod, "_listening_pid", lambda port: 42)
    def gone(pid):
        raise svc_mod.psutil.NoSuchProcess(pid)
    monkeypatch.setattr(svc_mod.psutil, "Process", gone)
    r = svc_mod.check_port_owner(_svc())
    assert r.ok is False
    assert "無法讀取" in r.detail


import time


def test_healthz_none_when_no_url():
    assert svc_mod.check_healthz(_svc()) is None


def test_healthz_ok(monkeypatch):
    class _R:
        status_code = 200
    monkeypatch.setattr(svc_mod.requests, "get", lambda *a, **k: _R())
    r = svc_mod.check_healthz(_svc(healthz_url="http://127.0.0.1:8317/v1/models"))
    assert r.ok is True and r.check_id == "service.pp.healthz"


def test_healthz_connection_error(monkeypatch):
    def boom(*a, **k):
        raise svc_mod.requests.ConnectionError("refused")
    monkeypatch.setattr(svc_mod.requests, "get", boom)
    r = svc_mod.check_healthz(_svc(healthz_url="http://127.0.0.1:1/x"))
    assert r.ok is False and "ConnectionError" in r.detail


def test_log_growth(tmp_path):
    log = tmp_path / "s.log"
    log.write_text("x", encoding="utf-8")
    svc = _svc(log_path=str(log), log_max_idle_secs=60)
    state = {}
    r1 = svc_mod.check_log_growth(svc, state)
    assert r1.ok is True  # 第一輪視為剛變化
    # 模擬 100 秒沒增長：直接把 state 的 last_change 撥回去
    size, _ = state["pp"]
    state["pp"] = (size, time.time() - 100)
    r2 = svc_mod.check_log_growth(svc, state)
    assert r2.ok is False and "未增長" in r2.detail
    # log 長了 -> 恢復
    log.write_text("xy", encoding="utf-8")
    r3 = svc_mod.check_log_growth(svc, state)
    assert r3.ok is True


def test_run_service_checks_isolated(monkeypatch):
    def boom(svc):
        raise RuntimeError("probe 炸了")
    monkeypatch.setattr(svc_mod, "check_port_owner", boom)
    results = svc_mod.run_service_checks([_svc()], {})
    assert len(results) == 1
    assert results[0].check_id == "service.pp.error"
    assert results[0].ok is False
