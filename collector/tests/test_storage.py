import time

from machine_collector.storage import Storage


def test_alert_lifecycle(tmp_path):
    s = Storage(tmp_path / "t.db")
    assert s.open_alerts() == {}
    assert s.open_alert("service.x.port_owner", "red", "port 8318 owner 不符") is True
    # 重複開啟同一告警 -> False（去重）
    assert s.open_alert("service.x.port_owner", "red", "dup") is False
    alerts = s.open_alerts()
    assert alerts["service.x.port_owner"]["severity"] == "red"
    assert s.last_alert_opened("service.x.port_owner") is not None
    assert s.close_alert("service.x.port_owner") is True
    assert s.close_alert("service.x.port_owner") is False  # 已關
    assert s.open_alerts() == {}


def test_samples_checks_and_prune(tmp_path):
    s = Storage(tmp_path / "t.db")
    old = time.time() - 100 * 86400
    s.record_sample("resource.cpu_pct", 42.0, ts=old)
    s.record_sample("resource.cpu_pct", 43.0)
    s.record_check("service.x.healthz", True, "HTTP 200", ts=old)
    s.record_check("service.x.healthz", False, "timeout")
    s.prune(days=90)
    n_samples = s.conn.execute("SELECT COUNT(*) FROM samples").fetchone()[0]
    n_checks = s.conn.execute("SELECT COUNT(*) FROM check_results").fetchone()[0]
    assert n_samples == 1
    assert n_checks == 1
