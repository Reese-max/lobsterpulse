import machine_collector.checks.resources as res
from machine_collector.config import ResourceCfg


class _FakeMem:
    percent = 50.0


class _FakeDisk:
    free = 5
    total = 100


def _patch_all(monkeypatch, cpu=95.0, cc=90.0):
    monkeypatch.setattr(res.psutil, "cpu_percent", lambda interval=None: cpu)
    monkeypatch.setattr(res.psutil, "virtual_memory", lambda: _FakeMem())
    monkeypatch.setattr(res, "commit_charge_pct", lambda: cc)
    monkeypatch.setattr(res.psutil, "disk_usage", lambda p: _FakeDisk())
    monkeypatch.setattr(res, "gpu_stats", lambda: None)
    monkeypatch.setattr(res.psutil, "pids", lambda: [1, 2, 3])


def test_collect_resources_over_threshold_fails(monkeypatch):
    _patch_all(monkeypatch)
    results, samples = res.collect_resources(ResourceCfg(disks=["C:"]))
    by_id = {r.check_id: r for r in results}
    assert by_id["resource.cpu"].ok is False          # 95 >= 90
    assert by_id["resource.commit_charge"].ok is False  # 90 >= 85
    assert by_id["resource.disk.C:"].ok is False      # 剩 5% < 10%
    assert samples["resource.cpu_pct"] == 95.0
    assert samples["resource.process_count"] == 3.0


def test_collect_resources_healthy(monkeypatch):
    _patch_all(monkeypatch, cpu=10.0, cc=40.0)
    monkeypatch.setattr(res.psutil, "disk_usage",
                        lambda p: type("D", (), {"free": 50, "total": 100})())
    results, _ = res.collect_resources(ResourceCfg(disks=["C:"]))
    assert all(r.ok for r in results)


def test_single_probe_failure_does_not_kill_round(monkeypatch):
    _patch_all(monkeypatch)
    def boom():
        raise OSError("GetPerformanceInfo 失敗")
    monkeypatch.setattr(res, "commit_charge_pct", boom)
    results, samples = res.collect_resources(ResourceCfg(disks=["C:"]))
    by_id = {r.check_id: r for r in results}
    assert by_id["resource.commit_charge"].ok is False
    assert "OSError" in by_id["resource.commit_charge"].detail
    assert "resource.cpu_pct" in samples  # 其他探針照常
