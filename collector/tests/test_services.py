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
