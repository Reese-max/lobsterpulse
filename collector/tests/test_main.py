import json

import machine_collector.main as main_mod
from machine_collector.alerts import AlertEngine
from machine_collector.checks.base import CheckResult
from machine_collector.config import MonitorConfig
from machine_collector.notify import TelegramNotifier
from machine_collector.storage import Storage


def test_run_once_end_to_end(monkeypatch, tmp_path):
    monkeypatch.setattr(main_mod, "run_service_checks",
                        lambda services, log_state: [
                            CheckResult("service.pp.port_owner", False, "port 8317 無人監聽")])
    monkeypatch.setattr(main_mod, "collect_resources",
                        lambda cfg: ([CheckResult("resource.cpu", True, "CPU 10%")],
                                     {"resource.cpu_pct": 10.0}))
    storage = Storage(tmp_path / "t.db")
    engine = AlertEngine(storage)
    notifier = TelegramNotifier("", "")  # 未啟用：不會真的推
    snap = tmp_path / "machine-status.json"
    hb = tmp_path / "hb"

    for _ in range(2):  # 連續失敗 2 輪 -> 開告警
        main_mod.run_once(MonitorConfig(), storage, engine, notifier, {}, snap, hb)

    d = json.loads(snap.read_text(encoding="utf-8"))
    assert len(d["checks"]) == 2
    assert "service.pp.port_owner" in d["open_alerts"]
    assert hb.exists()
    n_checks = storage.conn.execute("SELECT COUNT(*) FROM check_results").fetchone()[0]
    assert n_checks == 4  # 2 輪 x 2 checks


def test_run_once_survives_module_crash(monkeypatch, tmp_path):
    def boom(services, log_state):
        raise RuntimeError("整批炸")
    monkeypatch.setattr(main_mod, "run_service_checks", boom)
    monkeypatch.setattr(main_mod, "collect_resources",
                        lambda cfg: ([CheckResult("resource.cpu", True, "CPU 10%")],
                                     {"resource.cpu_pct": 10.0}))
    storage = Storage(tmp_path / "t.db")
    snap = tmp_path / "s.json"
    hb = tmp_path / "hb"
    main_mod.run_once(MonitorConfig(), storage, AlertEngine(storage),
                      TelegramNotifier("", ""), {}, snap, hb)
    d = json.loads(snap.read_text(encoding="utf-8"))
    assert len(d["checks"]) == 1  # resources 那半照常完成


def test_setup_logging_overrides_preconfigured_root(tmp_path):
    import logging
    # 模擬宿主先配置過 root logger
    logging.basicConfig(level=logging.WARNING)
    log_file = tmp_path / "m.log"
    main_mod.setup_logging(log_file)
    root = logging.getLogger()
    assert any(isinstance(h, logging.FileHandler) for h in root.handlers)
    # 清理：避免影響其他測試
    for h in list(root.handlers):
        root.removeHandler(h)
