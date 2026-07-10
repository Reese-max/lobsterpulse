from machine_collector.alerts import AlertEngine, severity_of
from machine_collector.checks.base import CheckResult
from machine_collector.storage import Storage


def _fail(cid="service.pp.port_owner"):
    return CheckResult(cid, False, "port 8317 無人監聽")


def _ok(cid="service.pp.port_owner"):
    return CheckResult(cid, True, "pid=1 proxypilot.exe")


def test_alert_after_two_consecutive_failures(tmp_path):
    eng = AlertEngine(Storage(tmp_path / "t.db"))
    assert eng.process([_fail()]) == []          # 第 1 輪失敗：還不告警
    notes = eng.process([_fail()])               # 第 2 輪：開告警
    assert len(notes) == 1
    assert notes[0].kind == "alert"
    assert "🔴" in notes[0].text
    assert eng.process([_fail()]) == []          # 第 3 輪：已開啟，不重推


def test_recovery_notification(tmp_path):
    eng = AlertEngine(Storage(tmp_path / "t.db"))
    eng.process([_fail()])
    eng.process([_fail()])
    notes = eng.process([_ok()])
    assert len(notes) == 1
    assert notes[0].kind == "recovery"
    assert "✅" in notes[0].text


def test_ok_resets_consecutive_counter(tmp_path):
    eng = AlertEngine(Storage(tmp_path / "t.db"))
    eng.process([_fail()])
    eng.process([_ok()])
    assert eng.process([_fail()]) == []  # 重新從 1 算


def test_cooldown_blocks_reopen(tmp_path):
    s = Storage(tmp_path / "t.db")
    eng = AlertEngine(s, cooldown_secs=1800)
    eng.process([_fail()])
    eng.process([_fail()])       # 開告警
    eng.process([_ok()])         # 恢復
    eng.process([_fail()])
    assert eng.process([_fail()]) == []  # 冷卻中，不重開


def test_fail_rounds_override_for_cpu(tmp_path):
    eng = AlertEngine(Storage(tmp_path / "t.db"),
                      fail_rounds={"resource.cpu": 3})
    cpu_fail = CheckResult("resource.cpu", False, "CPU 95%")
    assert eng.process([cpu_fail]) == []
    assert eng.process([cpu_fail]) == []
    notes = eng.process([cpu_fail])  # 第 3 輪才告警
    assert len(notes) == 1
    assert "🟡" in notes[0].text     # resource.* 是黃色


def test_severity_mapping():
    assert severity_of("resource.cpu") == "yellow"
    assert severity_of("service.pp.port_owner") == "red"


def test_probe_crash_then_recover_closes_alert(tmp_path):
    eng = AlertEngine(Storage(tmp_path / "t.db"))
    crash = CheckResult("service.pp.port_owner", False, "探針例外: RuntimeError")
    assert eng.process([crash]) == []             # 第 1 輪：探針例外，還不告警
    notes = eng.process([crash])                  # 第 2 輪：開告警
    assert len(notes) == 1
    assert notes[0].kind == "alert"
    recovery = eng.process([_ok("service.pp.port_owner")])
    assert len(recovery) == 1
    assert recovery[0].kind == "recovery"
    assert eng.storage.open_alerts() == {}


def test_absent_check_id_resets_consecutive(tmp_path):
    eng = AlertEngine(Storage(tmp_path / "t.db"))
    fail_a = CheckResult("service.a", False, "fail")
    assert eng.process([fail_a]) == []   # 第 1 輪失敗（id A）
    assert eng.process([]) == []         # 本輪不含 A -> 缺席歸零
    assert eng.process([fail_a]) == []   # 重新從 1 算，還不告警
