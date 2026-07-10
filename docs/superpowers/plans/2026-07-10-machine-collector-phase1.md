# machine-collector Phase 1 實作計畫

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立獨立的全機監控採集 daemon（services + resources 兩類巡檢、SQLite 歷史、Telegram 告警、heartbeat + watchdog），不依賴 LobsterPulse UI 存活。

**Architecture:** 純 Python daemon，每 60 秒巡檢一輪：port owner / healthz / log 增長（services 類）與 CPU / commit charge / 磁碟 / GPU（resources 類），結果寫 SQLite 與 `machine-status.json` 快照，異常經告警狀態機（連續失敗門檻 + 30 分冷卻 + 恢復通知）直推 Telegram。schtasks 註冊開機常駐 + 每 5 分鐘 watchdog。

**Tech Stack:** Python 3.11.9（系統既有）、psutil 7.2.2、PyYAML、requests、sqlite3（stdlib）、pytest。

**Spec:** `docs/superpowers/specs/2026-07-10-machine-monitor-design.md`（本計畫僅涵蓋 Phase 1）

## Global Constraints

- 平台：Windows 11 專用（commit charge 走 psapi `GetPerformanceInfo`，watchdog 用 schtasks/taskkill）
- Python 3.11.9；相依 psutil / PyYAML / requests 已在系統安裝，**不需 pip install**
- 所有使用者可見訊息用繁體中文（zh-TW）
- 資料檔一律放 `~/.lobsterpulse/`：`machine-monitor.db`、`machine-status.json`、`machine-collector.heartbeat`、`machine-collector.log`、`machine-collector.restarts`
- 程式碼放 repo 內 `collector/`；測試 `collector/tests/`；**不放 repo 根目錄**
- 踩雷防護必須保留：§23（pythonw 下 `sys.stdout is None` 不掛 StreamHandler）、§6（殺進程雙鍵匹配：cmdline 含入口腳本 + 專案路徑）、§20（watchdog 不自己 detach spawn，改 `schtasks /Run` 拉起）、§9（schtasks 預設 BelowNormal 優先權——collector 輕量可接受，已知即可）
- `.ps1` 檔內容維持純 ASCII（硬規則 7），執行用 `pwsh`
- Telegram 設定 fallback 鏈：`monitor-config.yaml` 的 `notify.*` → `%APPDATA%\lobsterpulse\config.json` 的 `appearance.telegram_bot_token` / `appearance.telegram_chat_id`。**注意：實測兩者目前皆為空**，token 未補齊前 notifier 只記 log 不推送（Task 12 E2E 需要真 token）
- 工作分支：`feature/machine-collector`（自 `auto-dev/burn-20260601` 切出；README 慣例 feature/* 為日常功能分支）
- commit 格式：`type(scope): description`；每個 task 至少一個 commit
- 測試指令一律從 repo 根跑：`python -m pytest collector/tests -v`

## File Structure（Phase 1 全貌）

```
collector/
  run_collector.py              # 入口（schtasks 指到這裡；把自身目錄加進 sys.path）
  monitor-config.yaml           # 監控清單（環境實測種子：8317/8318/5678/19380）
  machine_collector/
    __init__.py
    config.py                   # YAML → dataclasses
    storage.py                  # SQLite samples/check_results/alerts + prune
    alerts.py                   # 告警狀態機（連續失敗、冷卻、恢復）
    notify.py                   # TelegramNotifier + 失敗 queue
    snapshot.py                 # machine-status.json + heartbeat（原子寫）
    main.py                     # 主迴圈 + logging（§23 防護）+ --once
    checks/
      __init__.py
      base.py                   # CheckResult dataclass
      services.py               # port owner / healthz / log 增長
      resources.py              # CPU / RAM / commit charge / 磁碟 / GPU / 進程數
  scripts/
    watchdog.py                 # heartbeat 檢查 + 雙鍵匹配 kill + schtasks /Run
    register_tasks.ps1          # schtasks 註冊（ASCII only）
  tests/
    conftest.py                 # sys.path 注入
    test_config.py
    test_storage.py
    test_resources.py
    test_services.py
    test_alerts.py
    test_notify.py
    test_snapshot.py
    test_main.py
```

---

### Task 0: 建立工作分支

**Files:** 無（僅 git 操作）

- [ ] **Step 1: 確認起點乾淨並切分支**

```bash
cd "/d/Users/Administrator/Desktop/監控"
git status --short        # 預期：空（乾淨）
git checkout -b feature/machine-collector
git branch --show-current # 預期：feature/machine-collector
```

---

### Task 1: 骨架 + config 載入

**Files:**
- Create: `collector/machine_collector/__init__.py`（空檔）
- Create: `collector/machine_collector/checks/__init__.py`（空檔）
- Create: `collector/machine_collector/config.py`
- Create: `collector/tests/conftest.py`
- Test: `collector/tests/test_config.py`

**Interfaces:**
- Produces: `load_config(path) -> MonitorConfig`；dataclasses `ServiceCfg(id, port, process_pattern, healthz_url=None, healthz_headers={}, log_path=None, log_max_idle_secs=None)`、`ResourceCfg(commit_charge_alert_pct=85.0, disk_min_free_pct=10.0, cpu_alert_pct=90.0, cpu_alert_sustain_rounds=10, disks=["C:"])`、`NotifyCfg(telegram_bot_token="", telegram_chat_id="", cooldown_secs=1800)`、`MonitorConfig(interval_secs=60, services, resources, notify)`

- [ ] **Step 1: 寫 conftest.py（讓 tests 找得到套件）**

```python
# collector/tests/conftest.py
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
```

- [ ] **Step 2: 寫失敗測試**

```python
# collector/tests/test_config.py
from machine_collector.config import load_config


def test_load_config_full(tmp_path):
    f = tmp_path / "m.yaml"
    f.write_text(
        """
interval_secs: 30
notify:
  telegram_bot_token: tok
  telegram_chat_id: "123"
services:
  - id: hermes
    port: 8318
    process_pattern: python
resources:
  disks: ["C:", "D:"]
""",
        encoding="utf-8",
    )
    cfg = load_config(f)
    assert cfg.interval_secs == 30
    assert cfg.services[0].id == "hermes"
    assert cfg.services[0].port == 8318
    assert cfg.services[0].healthz_url is None
    assert cfg.resources.disks == ["C:", "D:"]
    assert cfg.resources.commit_charge_alert_pct == 85.0
    assert cfg.notify.telegram_bot_token == "tok"


def test_load_config_empty_file_uses_defaults(tmp_path):
    f = tmp_path / "m.yaml"
    f.write_text("", encoding="utf-8")
    cfg = load_config(f)
    assert cfg.interval_secs == 60
    assert cfg.services == []
    assert cfg.notify.cooldown_secs == 1800
```

- [ ] **Step 3: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_config.py -v`
Expected: FAIL（`ModuleNotFoundError: No module named 'machine_collector'`）

- [ ] **Step 4: 實作 config.py（含兩個空 `__init__.py`）**

```python
# collector/machine_collector/config.py
"""monitor-config.yaml 載入與驗證。"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

import yaml


@dataclass
class ServiceCfg:
    id: str
    port: int
    process_pattern: str
    healthz_url: str | None = None
    healthz_headers: dict[str, str] = field(default_factory=dict)
    log_path: str | None = None
    log_max_idle_secs: int | None = None


@dataclass
class ResourceCfg:
    commit_charge_alert_pct: float = 85.0
    disk_min_free_pct: float = 10.0
    cpu_alert_pct: float = 90.0
    cpu_alert_sustain_rounds: int = 10
    disks: list[str] = field(default_factory=lambda: ["C:"])


@dataclass
class NotifyCfg:
    telegram_bot_token: str = ""
    telegram_chat_id: str = ""
    cooldown_secs: int = 1800


@dataclass
class MonitorConfig:
    interval_secs: int = 60
    services: list[ServiceCfg] = field(default_factory=list)
    resources: ResourceCfg = field(default_factory=ResourceCfg)
    notify: NotifyCfg = field(default_factory=NotifyCfg)


def load_config(path: str | Path) -> MonitorConfig:
    raw = yaml.safe_load(Path(path).read_text(encoding="utf-8")) or {}
    return MonitorConfig(
        interval_secs=raw.get("interval_secs", 60),
        services=[ServiceCfg(**s) for s in raw.get("services", [])],
        resources=ResourceCfg(**raw.get("resources", {})),
        notify=NotifyCfg(**raw.get("notify", {})),
    )
```

- [ ] **Step 5: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_config.py -v`
Expected: 2 passed

- [ ] **Step 6: Commit**

```bash
git add collector/
git commit -m "feat(collector): config 載入骨架（monitor-config.yaml -> dataclasses）"
```

---

### Task 2: SQLite 儲存層

**Files:**
- Create: `collector/machine_collector/storage.py`
- Test: `collector/tests/test_storage.py`

**Interfaces:**
- Produces: `Storage(db_path)`，方法：`record_sample(metric, value, labels="", ts=None)`、`record_check(check_id, ok, detail="", ts=None)`、`open_alerts() -> dict[str, dict]`（key=check_id，value 含 severity/message/opened_ts）、`open_alert(check_id, severity, message) -> bool`（已開啟則 False）、`close_alert(check_id) -> bool`、`last_alert_opened(check_id) -> float | None`、`prune(days=90)`

- [ ] **Step 1: 寫失敗測試**

```python
# collector/tests/test_storage.py
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
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_storage.py -v`
Expected: FAIL（no module `machine_collector.storage`）

- [ ] **Step 3: 實作 storage.py**

```python
# collector/machine_collector/storage.py
"""SQLite 儲存層：samples / check_results / alerts + 90 天 prune。"""
from __future__ import annotations

import sqlite3
import time
from pathlib import Path

_SCHEMA = """
CREATE TABLE IF NOT EXISTS samples (
    ts REAL NOT NULL,
    metric TEXT NOT NULL,
    labels TEXT NOT NULL DEFAULT '',
    value REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_samples_metric_ts ON samples (metric, ts);
CREATE TABLE IF NOT EXISTS check_results (
    ts REAL NOT NULL,
    check_id TEXT NOT NULL,
    ok INTEGER NOT NULL,
    detail TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_checks_id_ts ON check_results (check_id, ts);
CREATE TABLE IF NOT EXISTS alerts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    check_id TEXT NOT NULL,
    severity TEXT NOT NULL,
    message TEXT NOT NULL,
    opened_ts REAL NOT NULL,
    closed_ts REAL
);
CREATE INDEX IF NOT EXISTS idx_alerts_check ON alerts (check_id, opened_ts);
"""


class Storage:
    def __init__(self, db_path: str | Path):
        self.conn = sqlite3.connect(str(db_path))
        self.conn.executescript(_SCHEMA)
        self.conn.commit()

    def record_sample(self, metric: str, value: float, labels: str = "",
                      ts: float | None = None) -> None:
        self.conn.execute("INSERT INTO samples VALUES (?,?,?,?)",
                          (ts if ts is not None else time.time(), metric, labels, value))
        self.conn.commit()

    def record_check(self, check_id: str, ok: bool, detail: str = "",
                     ts: float | None = None) -> None:
        self.conn.execute("INSERT INTO check_results VALUES (?,?,?,?)",
                          (ts if ts is not None else time.time(), check_id, int(ok), detail))
        self.conn.commit()

    def open_alerts(self) -> dict[str, dict]:
        rows = self.conn.execute(
            "SELECT check_id, severity, message, opened_ts FROM alerts "
            "WHERE closed_ts IS NULL").fetchall()
        return {r[0]: {"severity": r[1], "message": r[2], "opened_ts": r[3]} for r in rows}

    def open_alert(self, check_id: str, severity: str, message: str) -> bool:
        if check_id in self.open_alerts():
            return False
        self.conn.execute(
            "INSERT INTO alerts (check_id, severity, message, opened_ts) VALUES (?,?,?,?)",
            (check_id, severity, message, time.time()))
        self.conn.commit()
        return True

    def close_alert(self, check_id: str) -> bool:
        cur = self.conn.execute(
            "UPDATE alerts SET closed_ts=? WHERE check_id=? AND closed_ts IS NULL",
            (time.time(), check_id))
        self.conn.commit()
        return cur.rowcount > 0

    def last_alert_opened(self, check_id: str) -> float | None:
        row = self.conn.execute(
            "SELECT MAX(opened_ts) FROM alerts WHERE check_id=?", (check_id,)).fetchone()
        return row[0]

    def prune(self, days: int = 90) -> None:
        cutoff = time.time() - days * 86400
        self.conn.execute("DELETE FROM samples WHERE ts < ?", (cutoff,))
        self.conn.execute("DELETE FROM check_results WHERE ts < ?", (cutoff,))
        self.conn.execute(
            "DELETE FROM alerts WHERE closed_ts IS NOT NULL AND closed_ts < ?", (cutoff,))
        self.conn.commit()
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_storage.py -v`
Expected: 2 passed

- [ ] **Step 5: Commit**

```bash
git add collector/machine_collector/storage.py collector/tests/test_storage.py
git commit -m "feat(collector): SQLite 儲存層（samples/check_results/alerts + prune）"
```

---

### Task 3: resources 採集模組

**Files:**
- Create: `collector/machine_collector/checks/base.py`
- Create: `collector/machine_collector/checks/resources.py`
- Test: `collector/tests/test_resources.py`

**Interfaces:**
- Produces: `CheckResult(check_id: str, ok: bool, detail: str = "", value: float | None = None)`（dataclass，全專案共用）；`collect_resources(cfg: ResourceCfg) -> tuple[list[CheckResult], dict[str, float]]`（results 給告警引擎，samples 給時序庫）；`commit_charge_pct() -> float`；`gpu_stats() -> dict | None`
- check_id 命名：`resource.cpu`、`resource.commit_charge`、`resource.disk.C:`；sample 名：`resource.cpu_pct`、`resource.ram_pct`、`resource.commit_charge_pct`、`resource.disk_free_pct.C:`、`resource.gpu_util_pct`、`resource.process_count`

- [ ] **Step 1: 寫失敗測試**

```python
# collector/tests/test_resources.py
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
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_resources.py -v`
Expected: FAIL（no module）

- [ ] **Step 3: 實作 base.py 與 resources.py**

```python
# collector/machine_collector/checks/base.py
"""所有 check 的共用結果型別。"""
from __future__ import annotations

from dataclasses import dataclass


@dataclass
class CheckResult:
    check_id: str
    ok: bool
    detail: str = ""
    value: float | None = None
```

```python
# collector/machine_collector/checks/resources.py
"""資源類採集：CPU / RAM / commit charge / 磁碟 / GPU / 進程數（踩雷 §16 前兆預警）。"""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import subprocess

import psutil

from ..config import ResourceCfg
from .base import CheckResult


class _PERFORMANCE_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("cb", wt.DWORD),
        ("CommitTotal", ctypes.c_size_t),
        ("CommitLimit", ctypes.c_size_t),
        ("CommitPeak", ctypes.c_size_t),
        ("PhysicalTotal", ctypes.c_size_t),
        ("PhysicalAvailable", ctypes.c_size_t),
        ("SystemCache", ctypes.c_size_t),
        ("KernelTotal", ctypes.c_size_t),
        ("KernelPaged", ctypes.c_size_t),
        ("KernelNonpaged", ctypes.c_size_t),
        ("PageSize", ctypes.c_size_t),
        ("HandleCount", wt.DWORD),
        ("ProcessCount", wt.DWORD),
        ("ThreadCount", wt.DWORD),
    ]


def commit_charge_pct() -> float:
    """0xc0000142 前兆指標（踩雷 §16）：commit charge 佔 commit limit 百分比。"""
    pi = _PERFORMANCE_INFORMATION()
    pi.cb = ctypes.sizeof(pi)
    if not ctypes.windll.psapi.GetPerformanceInfo(ctypes.byref(pi), pi.cb):
        raise OSError("GetPerformanceInfo 失敗")
    return pi.CommitTotal / pi.CommitLimit * 100.0


def gpu_stats() -> dict | None:
    """無 NVIDIA GPU / nvidia-smi 不在 PATH 時回 None（不視為錯誤）。"""
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=utilization.gpu,memory.used,memory.total",
             "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=10)
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0 or not out.stdout.strip():
        return None
    util, used, total = [float(x) for x in out.stdout.strip().splitlines()[0].split(",")]
    return {"gpu_util_pct": util, "gpu_mem_used_mb": used, "gpu_mem_total_mb": total}


def collect_resources(cfg: ResourceCfg) -> tuple[list[CheckResult], dict[str, float]]:
    results: list[CheckResult] = []
    samples: dict[str, float] = {}

    try:
        cpu = psutil.cpu_percent(interval=1)
        samples["resource.cpu_pct"] = cpu
        results.append(CheckResult("resource.cpu", cpu < cfg.cpu_alert_pct,
                                   f"CPU {cpu:.0f}%", value=cpu))
    except Exception as e:
        results.append(CheckResult("resource.cpu", False, f"{type(e).__name__}: {e}"))

    try:
        samples["resource.ram_pct"] = psutil.virtual_memory().percent
    except Exception:
        pass  # RAM 僅記 sample，不設告警規則（spec §5 未列）

    try:
        cc = commit_charge_pct()
        samples["resource.commit_charge_pct"] = cc
        results.append(CheckResult("resource.commit_charge",
                                   cc < cfg.commit_charge_alert_pct,
                                   f"commit charge {cc:.0f}%", value=cc))
    except Exception as e:
        results.append(CheckResult("resource.commit_charge", False,
                                   f"{type(e).__name__}: {e}"))

    for disk in cfg.disks:
        cid = f"resource.disk.{disk}"
        try:
            du = psutil.disk_usage(disk + "\\")
            free_pct = du.free / du.total * 100.0
            samples[f"resource.disk_free_pct.{disk}"] = free_pct
            results.append(CheckResult(cid, free_pct > cfg.disk_min_free_pct,
                                       f"{disk} 剩餘 {free_pct:.0f}%", value=free_pct))
        except Exception as e:
            results.append(CheckResult(cid, False, f"{type(e).__name__}: {e}"))

    gpu = gpu_stats()
    if gpu:
        samples.update({f"resource.{k}": v for k, v in gpu.items()})

    try:
        samples["resource.process_count"] = float(len(psutil.pids()))
    except Exception:
        pass

    return results, samples
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_resources.py -v`
Expected: 3 passed

- [ ] **Step 5: 實機 smoke（真跑一次，不 mock）**

Run: `python -c "import sys; sys.path.insert(0, 'collector'); from machine_collector.checks.resources import collect_resources; from machine_collector.config import ResourceCfg; r, s = collect_resources(ResourceCfg(disks=['C:', 'D:'])); [print(x) for x in r]; print(s)"`
Expected: 印出 4 個 CheckResult（cpu / commit_charge / disk.C: / disk.D:，正常機況下全 ok=True）與 samples dict（含 commit_charge_pct 實際值）

- [ ] **Step 6: Commit**

```bash
git add collector/machine_collector/checks/ collector/tests/test_resources.py
git commit -m "feat(collector): resources 採集（CPU/commit charge/磁碟/GPU，探針級隔離）"
```

---

### Task 4: services 採集——port owner

**Files:**
- Create: `collector/machine_collector/checks/services.py`
- Test: `collector/tests/test_services.py`

**Interfaces:**
- Consumes: `ServiceCfg`（Task 1）、`CheckResult`（Task 3）
- Produces: `check_port_owner(svc: ServiceCfg) -> CheckResult`（check_id=`service.{id}.port_owner`）；內部 helper `_listening_pid(port: int) -> int | None`。本 task 先建檔並實作 port owner；healthz 與 log 增長在 Task 5 加進同一檔

- [ ] **Step 1: 寫失敗測試**

```python
# collector/tests/test_services.py
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
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_services.py -v`
Expected: FAIL（no module）

- [ ] **Step 3: 實作 services.py（本階段只有 port owner）**

```python
# collector/machine_collector/checks/services.py
"""服務類巡檢：port owner / healthz / log 增長（硬規則 8：健康 != 活著）。"""
from __future__ import annotations

import re

import psutil

from ..config import ServiceCfg
from .base import CheckResult


def _listening_pid(port: int) -> int | None:
    for c in psutil.net_connections(kind="tcp"):
        if c.laddr and c.laddr.port == port and c.status == psutil.CONN_LISTEN:
            return c.pid
    return None


def check_port_owner(svc: ServiceCfg) -> CheckResult:
    """踩雷 §22：port 有人聽不代表是對的進程，owner 必須匹配預期 pattern。"""
    cid = f"service.{svc.id}.port_owner"
    pid = _listening_pid(svc.port)
    if pid is None:
        return CheckResult(cid, False, f"port {svc.port} 無人監聽")
    try:
        p = psutil.Process(pid)
        ident = f"{p.name()} {' '.join(p.cmdline())}"
    except psutil.Error as e:
        return CheckResult(cid, False, f"port {svc.port} owner pid={pid} 無法讀取: {e}")
    if re.search(svc.process_pattern, ident, re.IGNORECASE):
        return CheckResult(cid, True, f"pid={pid} {p.name()}")
    return CheckResult(
        cid, False,
        f"port {svc.port} owner 不符: pid={pid} {p.name()}（預期 {svc.process_pattern}）")
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_services.py -v`
Expected: 3 passed

- [ ] **Step 5: 實機 smoke（對真實 port 8317 跑）**

Run: `python -c "import sys; sys.path.insert(0, 'collector'); from machine_collector.checks.services import check_port_owner; from machine_collector.config import ServiceCfg; print(check_port_owner(ServiceCfg(id='pp', port=8317, process_pattern='proxypilot')))"`
Expected: `CheckResult(check_id='service.pp.port_owner', ok=True, detail='pid=... proxypilot.exe', ...)`（若 proxypilot 當下沒跑則 ok=False，屬正確行為，記錄實際輸出即可）

- [ ] **Step 6: Commit**

```bash
git add collector/machine_collector/checks/services.py collector/tests/test_services.py
git commit -m "feat(collector): services port owner 巡檢（§22 owner 匹配）"
```

---

### Task 5: services 採集——healthz + log 增長 + 整合 runner

**Files:**
- Modify: `collector/machine_collector/checks/services.py`（追加三個函式）
- Test: `collector/tests/test_services.py`（追加測試）

**Interfaces:**
- Produces: `check_healthz(svc, timeout=5.0) -> CheckResult | None`（無 healthz_url 回 None；check_id=`service.{id}.healthz`）；`check_log_growth(svc, state: dict) -> CheckResult | None`（無 log_path 或 log_max_idle_secs 回 None；check_id=`service.{id}.log_growth`；state 為跨輪 dict：`{svc.id: (size, last_change_ts)}`）；`run_service_checks(services: list[ServiceCfg], log_state: dict) -> list[CheckResult]`（check 級隔離：單項炸掉記為 `service.{id}.error` 失敗結果，不中斷整輪）

- [ ] **Step 1: 追加失敗測試（附加到 test_services.py 尾端）**

```python
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
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_services.py -v`
Expected: 新增 5 個測試 FAIL（AttributeError: no attribute 'check_healthz' 等），既有 3 個 PASS

- [ ] **Step 3: 追加實作（services.py 檔頭 import 加 `import time`、`from pathlib import Path`、`import requests`，檔尾加三函式）**

```python
def check_healthz(svc: ServiceCfg, timeout: float = 5.0) -> CheckResult | None:
    if not svc.healthz_url:
        return None
    cid = f"service.{svc.id}.healthz"
    try:
        r = requests.get(svc.healthz_url, headers=svc.healthz_headers, timeout=timeout)
        return CheckResult(cid, r.status_code < 500, f"HTTP {r.status_code}")
    except requests.RequestException as e:
        return CheckResult(cid, False, f"{type(e).__name__}: {e}")


def check_log_growth(svc: ServiceCfg, state: dict) -> CheckResult | None:
    """硬規則 8：有流量時 log 要在長。state 由呼叫端跨輪保存 {id: (size, last_change_ts)}。"""
    if not svc.log_path or not svc.log_max_idle_secs:
        return None
    cid = f"service.{svc.id}.log_growth"
    path = Path(svc.log_path)
    if not path.exists():
        return CheckResult(cid, False, f"log 檔不存在: {path}")
    size = path.stat().st_size
    prev = state.get(svc.id)
    last_change = time.time() if prev is None or size != prev[0] else prev[1]
    state[svc.id] = (size, last_change)
    idle = time.time() - last_change
    return CheckResult(cid, idle <= svc.log_max_idle_secs,
                       f"log 已 {int(idle)}s 未增長", value=float(size))


def run_service_checks(services: list[ServiceCfg], log_state: dict) -> list[CheckResult]:
    """check 級隔離（spec §3）：單項例外不影響同輪其他 check。"""
    results: list[CheckResult] = []
    for svc in services:
        probes = (
            lambda s=svc: check_port_owner(s),
            lambda s=svc: check_healthz(s),
            lambda s=svc: check_log_growth(s, log_state),
        )
        for probe in probes:
            try:
                r = probe()
            except Exception as e:
                r = CheckResult(f"service.{svc.id}.error", False,
                                f"{type(e).__name__}: {e}")
            if r is not None:
                results.append(r)
    return results
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_services.py -v`
Expected: 8 passed

- [ ] **Step 5: Commit**

```bash
git add collector/machine_collector/checks/services.py collector/tests/test_services.py
git commit -m "feat(collector): services healthz/log 增長巡檢 + check 級隔離 runner"
```

---

### Task 6: 告警狀態機

**Files:**
- Create: `collector/machine_collector/alerts.py`
- Test: `collector/tests/test_alerts.py`

**Interfaces:**
- Consumes: `Storage`（Task 2）、`CheckResult`（Task 3）
- Produces: `Notification(kind: str, check_id: str, text: str)`（kind ∈ "alert" | "recovery"）；`severity_of(check_id) -> str`（"yellow" if `resource.` 開頭 else "red"）；`AlertEngine(storage, cooldown_secs=1800, fail_rounds: dict[str, int] | None = None)`，方法 `process(results: list[CheckResult]) -> list[Notification]`。規則：連續失敗 ≥ 門檻（預設 2；fail_rounds 依 check_id 前綴覆寫）才開告警；已開啟不重推；同 check_id 距上次開啟 < cooldown 不重開；ok 且有開啟告警 → 關閉 + 恢復通知

- [ ] **Step 1: 寫失敗測試**

```python
# collector/tests/test_alerts.py
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
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_alerts.py -v`
Expected: FAIL（no module）

- [ ] **Step 3: 實作 alerts.py**

```python
# collector/machine_collector/alerts.py
"""告警狀態機：連續失敗門檻 -> 開告警；恢復 -> 關告警並通知；冷卻防轟炸。"""
from __future__ import annotations

import time
from collections import defaultdict
from dataclasses import dataclass

from .checks.base import CheckResult
from .storage import Storage

DEFAULT_FAIL_ROUNDS = 2


def severity_of(check_id: str) -> str:
    return "yellow" if check_id.startswith("resource.") else "red"


@dataclass
class Notification:
    kind: str  # "alert" | "recovery"
    check_id: str
    text: str


class AlertEngine:
    def __init__(self, storage: Storage, cooldown_secs: int = 1800,
                 fail_rounds: dict[str, int] | None = None):
        self.storage = storage
        self.cooldown_secs = cooldown_secs
        self.fail_rounds = fail_rounds or {}
        self._consecutive: dict[str, int] = defaultdict(int)

    def _rounds_needed(self, check_id: str) -> int:
        for prefix, n in self.fail_rounds.items():
            if check_id.startswith(prefix):
                return n
        return DEFAULT_FAIL_ROUNDS

    def process(self, results: list[CheckResult]) -> list[Notification]:
        out: list[Notification] = []
        open_alerts = self.storage.open_alerts()
        for r in results:
            if r.ok:
                self._consecutive[r.check_id] = 0
                if r.check_id in open_alerts and self.storage.close_alert(r.check_id):
                    out.append(Notification(
                        "recovery", r.check_id, f"✅ {r.check_id} 已恢復（{r.detail}）"))
                continue
            self._consecutive[r.check_id] += 1
            if self._consecutive[r.check_id] < self._rounds_needed(r.check_id):
                continue
            if r.check_id in open_alerts:
                continue  # 已在告警中，不重推
            last = self.storage.last_alert_opened(r.check_id)
            if last is not None and time.time() - last < self.cooldown_secs:
                continue  # 冷卻中
            sev = severity_of(r.check_id)
            icon = "🔴" if sev == "red" else "🟡"
            msg = f"{icon} {r.check_id} 異常：{r.detail}"
            if self.storage.open_alert(r.check_id, sev, msg):
                out.append(Notification("alert", r.check_id, msg))
        return out
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_alerts.py -v`
Expected: 6 passed

- [ ] **Step 5: Commit**

```bash
git add collector/machine_collector/alerts.py collector/tests/test_alerts.py
git commit -m "feat(collector): 告警狀態機（連續失敗門檻/冷卻/恢復通知）"
```

---

### Task 7: Telegram notifier

**Files:**
- Create: `collector/machine_collector/notify.py`
- Test: `collector/tests/test_notify.py`

**Interfaces:**
- Consumes: `NotifyCfg`（Task 1）
- Produces: `TelegramNotifier(token, chat_id)`：`.enabled` property、`send(text) -> bool`（失敗進 queue，上限 100）、`flush()`（重試 queue）、classmethod `from_config(cfg: NotifyCfg) -> TelegramNotifier`（token/chat_id 空時 fallback 讀 `%APPDATA%\lobsterpulse\config.json` 的 `appearance.telegram_bot_token` / `appearance.telegram_chat_id`）

- [ ] **Step 1: 寫失敗測試**

```python
# collector/tests/test_notify.py
import json

import machine_collector.notify as notify_mod
from machine_collector.config import NotifyCfg
from machine_collector.notify import TelegramNotifier


def test_disabled_when_no_token():
    n = TelegramNotifier("", "")
    assert n.enabled is False
    assert n.send("hi") is False
    assert n.queue == []  # 未啟用不進 queue


def test_send_failure_queues_then_flush_retries(monkeypatch):
    n = TelegramNotifier("tok", "chat")
    calls = {"n": 0}

    def flaky_post(url, json=None, timeout=None):
        calls["n"] += 1
        class _R:
            status_code = 500 if calls["n"] == 1 else 200
        return _R()

    monkeypatch.setattr(notify_mod.requests, "post", flaky_post)
    assert n.send("msg1") is False
    assert n.queue == ["msg1"]
    n.flush()
    assert n.queue == []


def test_queue_cap(monkeypatch):
    n = TelegramNotifier("tok", "chat")
    monkeypatch.setattr(notify_mod.requests, "post",
                        lambda *a, **k: type("R", (), {"status_code": 500})())
    for i in range(150):
        n.send(f"m{i}")
    assert len(n.queue) == 100


def test_from_config_fallback_to_lobsterpulse(monkeypatch, tmp_path):
    lp = tmp_path / "config.json"
    lp.write_text(json.dumps({"appearance": {
        "telegram_bot_token": "lp-tok", "telegram_chat_id": "lp-chat"}}),
        encoding="utf-8")
    monkeypatch.setattr(notify_mod, "LOBSTERPULSE_CONFIG", lp)
    n = TelegramNotifier.from_config(NotifyCfg())
    assert n.token == "lp-tok"
    assert n.chat_id == "lp-chat"


def test_from_config_own_values_win(monkeypatch, tmp_path):
    monkeypatch.setattr(notify_mod, "LOBSTERPULSE_CONFIG", tmp_path / "nope.json")
    n = TelegramNotifier.from_config(
        NotifyCfg(telegram_bot_token="own", telegram_chat_id="c1"))
    assert n.token == "own"
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_notify.py -v`
Expected: FAIL（no module）

- [ ] **Step 3: 實作 notify.py**

```python
# collector/machine_collector/notify.py
"""Telegram 推播（含失敗 queue 重試）。token 空時 fallback 讀 LobsterPulse config.json。"""
from __future__ import annotations

import json
import logging
import os
from pathlib import Path

import requests

from .config import NotifyCfg

log = logging.getLogger(__name__)
LOBSTERPULSE_CONFIG = Path(os.environ.get("APPDATA", "")) / "lobsterpulse" / "config.json"
MAX_QUEUE = 100


def _fallback_from_lobsterpulse() -> tuple[str, str]:
    try:
        d = json.loads(LOBSTERPULSE_CONFIG.read_text(encoding="utf-8"))
        a = d.get("appearance", {})
        return a.get("telegram_bot_token", ""), a.get("telegram_chat_id", "")
    except (OSError, json.JSONDecodeError):
        return "", ""


class TelegramNotifier:
    def __init__(self, token: str, chat_id: str):
        self.token = token
        self.chat_id = chat_id
        self.queue: list[str] = []

    @classmethod
    def from_config(cls, cfg: NotifyCfg) -> "TelegramNotifier":
        token, chat_id = cfg.telegram_bot_token, cfg.telegram_chat_id
        if not token or not chat_id:
            fb_token, fb_chat = _fallback_from_lobsterpulse()
            token = token or fb_token
            chat_id = chat_id or fb_chat
        return cls(token, chat_id)

    @property
    def enabled(self) -> bool:
        return bool(self.token and self.chat_id)

    def _post(self, text: str) -> bool:
        try:
            r = requests.post(
                f"https://api.telegram.org/bot{self.token}/sendMessage",
                json={"chat_id": self.chat_id, "text": text}, timeout=10)
            return r.status_code == 200
        except requests.RequestException as e:
            log.warning("telegram 送出失敗: %s", e)
            return False

    def send(self, text: str) -> bool:
        if not self.enabled:
            log.info("telegram 未設定，僅記 log：%s", text)
            return False
        if self._post(text):
            return True
        if len(self.queue) < MAX_QUEUE:
            self.queue.append(text)
        return False

    def flush(self) -> None:
        if not self.enabled or not self.queue:
            return
        remaining = [t for t in self.queue if not self._post(t)]
        self.queue = remaining
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_notify.py -v`
Expected: 5 passed

- [ ] **Step 5: Commit**

```bash
git add collector/machine_collector/notify.py collector/tests/test_notify.py
git commit -m "feat(collector): Telegram notifier（queue 重試 + LobsterPulse config fallback）"
```

---

### Task 8: snapshot + heartbeat

**Files:**
- Create: `collector/machine_collector/snapshot.py`
- Test: `collector/tests/test_snapshot.py`

**Interfaces:**
- Consumes: `CheckResult`（Task 3）
- Produces: `write_snapshot(path, results: list[CheckResult], samples: dict[str, float], open_alerts: dict[str, dict])`（原子寫：先 `.tmp` 再 `replace`；JSON 欄位 `generated_at`/`checks`/`samples`/`open_alerts`）；`write_heartbeat(path)`（檔案內容 = unix ts 字串）

- [ ] **Step 1: 寫失敗測試**

```python
# collector/tests/test_snapshot.py
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
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_snapshot.py -v`
Expected: FAIL（no module）

- [ ] **Step 3: 實作 snapshot.py**

```python
# collector/machine_collector/snapshot.py
"""machine-status.json 快照 + heartbeat（原子寫：tmp + replace）。"""
from __future__ import annotations

import json
import time
from pathlib import Path

from .checks.base import CheckResult


def write_snapshot(path: str | Path, results: list[CheckResult],
                   samples: dict[str, float], open_alerts: dict[str, dict]) -> None:
    payload = {
        "generated_at": time.time(),
        "checks": [{"check_id": r.check_id, "ok": r.ok,
                    "detail": r.detail, "value": r.value} for r in results],
        "samples": samples,
        "open_alerts": open_alerts,
    }
    path = Path(path)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(payload, ensure_ascii=False, indent=1), encoding="utf-8")
    tmp.replace(path)


def write_heartbeat(path: str | Path) -> None:
    Path(path).write_text(str(time.time()), encoding="utf-8")
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_snapshot.py -v`
Expected: 2 passed

- [ ] **Step 5: Commit**

```bash
git add collector/machine_collector/snapshot.py collector/tests/test_snapshot.py
git commit -m "feat(collector): machine-status.json 快照 + heartbeat（原子寫）"
```

---

### Task 9: 主迴圈 main.py

**Files:**
- Create: `collector/machine_collector/main.py`
- Test: `collector/tests/test_main.py`

**Interfaces:**
- Consumes: 前面所有模組
- Produces: `run_once(cfg, storage, engine, notifier, log_state, snapshot_path, heartbeat_path)`（一輪巡檢：跑 checks → 落庫 → 告警 → 推播 → flush queue → 寫快照與 heartbeat）；`setup_logging(log_path)`（§23：`sys.stdout is None` 時只掛 FileHandler）；`main(argv=None)` 支援 `--once`（跑一輪就退出，供驗證）。模組級常數：`BASE_DIR = Path.home()/".lobsterpulse"`、`DB_PATH`、`SNAPSHOT_PATH`、`HEARTBEAT_PATH`、`LOG_PATH`、`CONFIG_PATH = <collector 目錄>/monitor-config.yaml`

- [ ] **Step 1: 寫失敗測試**

```python
# collector/tests/test_main.py
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
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_main.py -v`
Expected: FAIL（no module）

- [ ] **Step 3: 實作 main.py**

```python
# collector/machine_collector/main.py
"""machine-collector 主迴圈。"""
from __future__ import annotations

import argparse
import logging
import sys
import time
from pathlib import Path

from .alerts import AlertEngine
from .checks.resources import collect_resources
from .checks.services import run_service_checks
from .config import MonitorConfig, load_config
from .notify import TelegramNotifier
from .snapshot import write_heartbeat, write_snapshot
from .storage import Storage

BASE_DIR = Path.home() / ".lobsterpulse"
DB_PATH = BASE_DIR / "machine-monitor.db"
SNAPSHOT_PATH = BASE_DIR / "machine-status.json"
HEARTBEAT_PATH = BASE_DIR / "machine-collector.heartbeat"
LOG_PATH = BASE_DIR / "machine-collector.log"
CONFIG_PATH = Path(__file__).resolve().parent.parent / "monitor-config.yaml"

log = logging.getLogger("machine_collector")


def setup_logging(log_path: Path = LOG_PATH) -> None:
    # 踩雷 §23：pythonw 下 sys.stdout 為 None，不能無條件掛 StreamHandler
    handlers: list[logging.Handler] = [logging.FileHandler(log_path, encoding="utf-8")]
    if sys.stdout is not None:
        handlers.append(logging.StreamHandler())
    logging.basicConfig(level=logging.INFO, handlers=handlers,
                        format="%(asctime)s %(levelname)s %(name)s %(message)s")


def run_once(cfg: MonitorConfig, storage: Storage, engine: AlertEngine,
             notifier: TelegramNotifier, log_state: dict,
             snapshot_path: Path, heartbeat_path: Path) -> None:
    results = []
    samples: dict[str, float] = {}
    try:
        results.extend(run_service_checks(cfg.services, log_state))
    except Exception:
        log.exception("service checks 整批失敗")
    try:
        res_results, samples = collect_resources(cfg.resources)
        results.extend(res_results)
    except Exception:
        log.exception("resource 採集整批失敗")

    for r in results:
        storage.record_check(r.check_id, r.ok, r.detail)
    for metric, value in samples.items():
        storage.record_sample(metric, value)

    for n in engine.process(results):
        log.info("[%s] %s", n.kind, n.text)
        notifier.send(n.text)
    notifier.flush()

    write_snapshot(snapshot_path, results, samples, storage.open_alerts())
    write_heartbeat(heartbeat_path)


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(description="machine-collector 全機監控採集")
    ap.add_argument("--once", action="store_true", help="跑一輪就退出（驗證用）")
    args = ap.parse_args(argv)

    BASE_DIR.mkdir(exist_ok=True)
    setup_logging()
    cfg = load_config(CONFIG_PATH)
    storage = Storage(DB_PATH)
    engine = AlertEngine(
        storage, cooldown_secs=cfg.notify.cooldown_secs,
        fail_rounds={"resource.cpu": cfg.resources.cpu_alert_sustain_rounds})
    notifier = TelegramNotifier.from_config(cfg.notify)
    log.info("machine-collector 啟動 interval=%ss services=%d telegram=%s",
             cfg.interval_secs, len(cfg.services),
             "enabled" if notifier.enabled else "disabled")

    log_state: dict = {}
    last_prune = 0.0
    while True:
        start = time.time()
        try:
            run_once(cfg, storage, engine, notifier, log_state,
                     SNAPSHOT_PATH, HEARTBEAT_PATH)
        except Exception:
            log.exception("run_once 未捕捉例外")
        if time.time() - last_prune > 86400:
            try:
                storage.prune(90)
            except Exception:
                log.exception("prune 失敗")
            last_prune = time.time()
        if args.once:
            break
        time.sleep(max(5.0, cfg.interval_secs - (time.time() - start)))


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_main.py -v`
Expected: 2 passed

- [ ] **Step 5: 跑全套測試**

Run: `python -m pytest collector/tests -v`
Expected: 全部 passed（約 28 個）

- [ ] **Step 6: Commit**

```bash
git add collector/machine_collector/main.py collector/tests/test_main.py
git commit -m "feat(collector): 主迴圈（run_once/--once/§23 logging 防護）"
```

---

### Task 10: monitor-config.yaml 種子 + 入口腳本 + 實機 smoke

**Files:**
- Create: `collector/monitor-config.yaml`
- Create: `collector/run_collector.py`

**Interfaces:**
- Consumes: `main()`（Task 9）
- Produces: schtasks 可直接執行的入口 `run_collector.py`；環境實測種子 config（2026-07-10 實測 port owner：8317=proxypilot、8318=python3.11（Hermes）、5678=node（n8n）、19380=lobster-pulse）

- [ ] **Step 1: 寫 run_collector.py**

```python
# collector/run_collector.py
"""schtasks 入口：把 collector 目錄加進 sys.path 後啟動主迴圈。"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from machine_collector.main import main

if __name__ == "__main__":
    main()
```

- [ ] **Step 2: 寫 monitor-config.yaml（環境實測種子）**

```yaml
# collector/monitor-config.yaml
# 監控清單：加對象改這裡，不改碼。process_pattern 是對 "name cmdline" 的
# case-insensitive regex（踩雷 §6：匹配盡量帶路徑/專案雙鍵，勿只用泛名）。
interval_secs: 60

notify:
  # 留空 = fallback 讀 %APPDATA%\lobsterpulse\config.json 的 appearance.telegram_*
  telegram_bot_token: ""
  telegram_chat_id: ""
  cooldown_secs: 1800

services:
  - id: proxypilot
    port: 8317
    process_pattern: "proxypilot"
  - id: hermes-proxy
    port: 8318
    process_pattern: "python"   # Hermes proxy 實測以 python3.11 進程監聽
  - id: n8n
    port: 5678
    process_pattern: "node"
    healthz_url: "http://127.0.0.1:5678/healthz"
  - id: lobsterpulse-metrics
    port: 19380
    process_pattern: "lobster-pulse"
    healthz_url: "http://127.0.0.1:19380/metrics"

resources:
  commit_charge_alert_pct: 85
  disk_min_free_pct: 10
  cpu_alert_pct: 90
  cpu_alert_sustain_rounds: 10   # 60s x 10 輪 = 持續 10 分鐘才告警
  disks: ["C:", "D:"]
```

- [ ] **Step 3: 實機 smoke——跑一輪**

Run: `python collector/run_collector.py --once`
Expected: 無例外退出；然後驗證產物：

```bash
python -c "
import json, pathlib, time
base = pathlib.Path.home() / '.lobsterpulse'
d = json.loads((base / 'machine-status.json').read_text(encoding='utf-8'))
print('checks:', len(d['checks']))
for c in d['checks']: print(' ', 'OK ' if c['ok'] else 'FAIL', c['check_id'], '-', c['detail'])
print('heartbeat age:', time.time() - float((base / 'machine-collector.heartbeat').read_text()))
"
```

Expected: checks ≥ 9（4 服務的 port_owner + 2 個 healthz + cpu + commit_charge + disk×2）；heartbeat age < 60。**貼實際輸出**（硬規則 1）。若某服務當下真的沒跑而 FAIL，屬正確偵測，照實記錄。

- [ ] **Step 4: 驗 SQLite 有落資料**

Run: `python -c "import sqlite3, pathlib; c = sqlite3.connect(pathlib.Path.home() / '.lobsterpulse' / 'machine-monitor.db'); print('checks:', c.execute('SELECT COUNT(*) FROM check_results').fetchone()[0], 'samples:', c.execute('SELECT COUNT(*) FROM samples').fetchone()[0])"`
Expected: 兩個計數皆 > 0

- [ ] **Step 5: Commit**

```bash
git add collector/monitor-config.yaml collector/run_collector.py
git commit -m "feat(collector): 環境實測種子 config + schtasks 入口腳本"
```

---

### Task 11: watchdog + schtasks 註冊

**Files:**
- Create: `collector/scripts/watchdog.py`
- Create: `collector/scripts/register_tasks.ps1`
- Test: `collector/tests/test_watchdog.py`

**Interfaces:**
- Consumes: heartbeat 檔（Task 8/9）、`TelegramNotifier`（Task 7）、`load_config`（Task 1）
- Produces: `watchdog.py`——`heartbeat_age() -> float`、`kill_stale_collector()`（§6 雙鍵：cmdline 含 `run_collector.py` **且**含 collector 目錄路徑）、`record_restart_and_maybe_alert() -> bool`（30 分內 ≥3 次重啟回 True 並推 Telegram）、`main() -> int`（heartbeat 新鮮回 0；過期則 kill + `schtasks /Run /TN MachineCollector` 回 1）。schtasks 任務名固定：`MachineCollector`（ONLOGON）、`MachineCollectorWatchdog`（每 5 分鐘）

- [ ] **Step 1: 寫失敗測試**

```python
# collector/tests/test_watchdog.py
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
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `python -m pytest collector/tests/test_watchdog.py -v`
Expected: FAIL（no module watchdog）

- [ ] **Step 3: 實作 watchdog.py**

```python
# collector/scripts/watchdog.py
"""collector watchdog：heartbeat 過期 -> 雙鍵匹配 kill（§6）-> schtasks 拉起（§20 不 detach）。"""
from __future__ import annotations

import subprocess
import sys
import time
from pathlib import Path

import psutil

COLLECTOR_DIR = Path(__file__).resolve().parent.parent  # collector/
sys.path.insert(0, str(COLLECTOR_DIR))

HEARTBEAT = Path.home() / ".lobsterpulse" / "machine-collector.heartbeat"
RESTARTS = Path.home() / ".lobsterpulse" / "machine-collector.restarts"
STALE_SECS = 300  # interval 60s x 3 + 緩衝
TASK_NAME = "MachineCollector"
ENTRY_KEY = "run_collector.py"  # 雙鍵之一：入口腳本名
BURST_WINDOW_SECS = 1800
BURST_COUNT = 3


def heartbeat_age() -> float:
    try:
        return time.time() - float(HEARTBEAT.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return float("inf")


def kill_stale_collector() -> None:
    """踩雷 §6：cmdline 需同時含入口腳本名 + collector 專案路徑才殺。"""
    me = psutil.Process().pid
    for p in psutil.process_iter(["pid", "cmdline"]):
        if p.info["pid"] == me:
            continue
        cl = " ".join(p.info["cmdline"] or [])
        if ENTRY_KEY in cl and str(COLLECTOR_DIR).lower() in cl.lower():
            subprocess.run(["taskkill", "/PID", str(p.info["pid"]), "/T", "/F"],
                           capture_output=True)


def _alert_telegram(text: str) -> None:
    from machine_collector.config import load_config
    from machine_collector.notify import TelegramNotifier
    cfg = load_config(COLLECTOR_DIR / "monitor-config.yaml")
    TelegramNotifier.from_config(cfg.notify).send(text)


def record_restart_and_maybe_alert() -> bool:
    now = time.time()
    try:
        times = [float(x) for x in RESTARTS.read_text(encoding="utf-8").split()]
    except (OSError, ValueError):
        times = []
    times = [t for t in times if now - t < BURST_WINDOW_SECS] + [now]
    RESTARTS.write_text(" ".join(str(t) for t in times), encoding="utf-8")
    if len(times) >= BURST_COUNT:
        _alert_telegram(
            f"🔴 machine-collector 於 30 分鐘內第 {len(times)} 次被 watchdog 重啟，"
            f"請檢查 ~/.lobsterpulse/machine-collector.log")
        return True
    return False


def main() -> int:
    if heartbeat_age() <= STALE_SECS:
        return 0
    kill_stale_collector()
    record_restart_and_maybe_alert()
    # §20：隱藏啟動鏈內不自己 detach spawn，改請 schtasks 拉起
    subprocess.run(["schtasks", "/Run", "/TN", TASK_NAME], capture_output=True)
    return 1


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: 跑測試確認通過**

Run: `python -m pytest collector/tests/test_watchdog.py -v`
Expected: 5 passed

- [ ] **Step 5: 寫 register_tasks.ps1（ASCII only，硬規則 7）**

```powershell
# collector/scripts/register_tasks.ps1
# Register MachineCollector (ONLOGON) + MachineCollectorWatchdog (every 5 min).
# Run with: pwsh -File register_tasks.ps1
# Note: schtasks default priority is BelowNormal (pitfall 9) - acceptable for
# this lightweight collector; do not host latency-critical work in these tasks.
$ErrorActionPreference = "Stop"

$CollectorDir = Split-Path -Parent $PSScriptRoot
$PythonW = (Get-Command pythonw.exe).Source
$RunScript = Join-Path $CollectorDir "run_collector.py"
$WatchdogScript = Join-Path $PSScriptRoot "watchdog.py"

if (-not (Test-Path $RunScript)) { throw "run_collector.py not found: $RunScript" }

schtasks /Create /F /TN "MachineCollector" /SC ONLOGON `
  /TR "`"$PythonW`" `"$RunScript`""
schtasks /Create /F /TN "MachineCollectorWatchdog" /SC MINUTE /MO 5 `
  /TR "`"$PythonW`" `"$WatchdogScript`""

schtasks /Run /TN "MachineCollector"
Write-Output "Registered: MachineCollector (ONLOGON) + MachineCollectorWatchdog (5 min)"
```

- [ ] **Step 6: 驗 ps1 純 ASCII（硬規則 7）**

Run: `grep -P "[^\x00-\x7F]" collector/scripts/register_tasks.ps1; echo "exit=$?"`
Expected: 無輸出、`exit=1`（找不到非 ASCII 字元 = 通過）

- [ ] **Step 7: 實機註冊並驗收（改變系統狀態——執行前確認使用者已同意部署）**

Run: `pwsh -File collector/scripts/register_tasks.ps1`
Expected: 兩個 "SUCCESS" + Registered 訊息

驗收（踩雷 §18：不信 spawn exit code，信 readiness）：

```bash
sleep 70 && python -c "
import pathlib, time
hb = pathlib.Path.home() / '.lobsterpulse' / 'machine-collector.heartbeat'
age = time.time() - float(hb.read_text())
print('heartbeat age:', round(age), 's ->', 'OK' if age < 120 else 'FAIL')
"
```

Expected: `heartbeat age: <120 s -> OK`（證明 schtasks 拉起的常駐 collector 真的在跑）

- [ ] **Step 8: 驗 watchdog 真的能救活（殺掉 collector 等 watchdog 拉回）**

```powershell
Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*run_collector.py*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
schtasks /Run /TN "MachineCollectorWatchdog"
Start-Sleep -Seconds 90
python -c "import pathlib, time; hb = pathlib.Path.home() / '.lobsterpulse' / 'machine-collector.heartbeat'; age = time.time() - float(hb.read_text()); print('heartbeat age after watchdog:', round(age), 's ->', 'OK' if age < 120 else 'FAIL')"
```

Expected: `OK`（watchdog 偵測 heartbeat 過期並經 schtasks 重啟 collector）

- [ ] **Step 9: Commit**

```bash
git add collector/scripts/ collector/tests/test_watchdog.py
git commit -m "feat(collector): watchdog（雙鍵匹配/schtasks 拉起）+ 排程註冊腳本"
```

---

### Task 12: E2E 告警演練（最終驗收）

**Files:** 無新檔（純演練 + 記錄）

**前置：Telegram token。** 實測 `%APPDATA%\lobsterpulse\config.json` 的 `appearance.telegram_bot_token` 目前為空。執行本 task 前需請使用者提供 bot token + chat id（填入 lobsterpulse config.json 或 `collector/monitor-config.yaml` 的 `notify.*` 皆可）。**token 未提供時**：仍執行演練，驗收改看 `machine-collector.log` 內 `[alert]`/`[recovery]` 記錄與 alerts 資料表，並在結案報告標注「Telegram 實推未驗證」。

- [ ] **Step 1: 起一個犧牲用測試服務並加進監控**

```bash
# 起一個 python http.server 佔 port 18999（背景）
python -m http.server 18999 --bind 127.0.0.1 &
```

在 `collector/monitor-config.yaml` 的 `services:` 暫時加入（演練後移除）：

```yaml
  - id: e2e-victim
    port: 18999
    process_pattern: "python"
    healthz_url: "http://127.0.0.1:18999/"
```

collector 是常駐進程、只在啟動時讀 config，改完要重啟讓它吃到新設定：

```bash
schtasks //End //TN "MachineCollector" 2>/dev/null; powershell -Command "Get-CimInstance Win32_Process | Where-Object { \$_.CommandLine -like '*run_collector.py*' } | ForEach-Object { Stop-Process -Id \$_.ProcessId -Force }"; schtasks //Run //TN "MachineCollector"
```

- [ ] **Step 2: 確認 victim 顯示健康**

等 70 秒後檢查 snapshot：

```bash
sleep 70 && python -c "
import json, pathlib
d = json.loads((pathlib.Path.home() / '.lobsterpulse' / 'machine-status.json').read_text(encoding='utf-8'))
v = [c for c in d['checks'] if 'e2e-victim' in c['check_id']]
for c in v: print('OK ' if c['ok'] else 'FAIL', c['check_id'], c['detail'])
"
```

Expected: `service.e2e-victim.port_owner` 與 `service.e2e-victim.healthz` 皆 OK

- [ ] **Step 3: 殺掉 victim，計時等告警（spec §8：60 秒內；狀態機需連續 2 輪失敗，實際容許 ≤ 150 秒）**

殺掉 Step 1 起的 http.server（精確殺 port 18999 的 owner，不誤殺其他 python）：

```powershell
Get-NetTCPConnection -State Listen -LocalPort 18999 | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force }
```

記下時刻，然後每 30 秒看一次：

```bash
python -c "
import sqlite3, pathlib, time
c = sqlite3.connect(pathlib.Path.home() / '.lobsterpulse' / 'machine-monitor.db')
rows = c.execute(\"SELECT check_id, message, opened_ts, closed_ts FROM alerts WHERE check_id LIKE '%e2e-victim%' ORDER BY opened_ts\").fetchall()
for r in rows: print(r)
"
```

Expected: 出現 `service.e2e-victim.port_owner`（與 healthz）的 open alert（closed_ts=None）；若 Telegram 已設定，手機同步收到 🔴 訊息（截圖存證）。記錄「殺掉 → 告警落庫」實際耗時。

- [ ] **Step 4: 拉回 victim，驗恢復通知**

```bash
python -m http.server 18999 --bind 127.0.0.1 &
```

等 ~90 秒後重跑 Step 3 的查詢。
Expected: 該 alert 的 `closed_ts` 有值；log（或 Telegram）出現 `✅ service.e2e-victim.port_owner 已恢復`。

- [ ] **Step 5: 清理演練現場**

1. 殺掉 http.server
2. 從 `monitor-config.yaml` 移除 `e2e-victim` 區塊
3. 重啟 collector（同 Step 1 的重啟指令）
4. 確認 snapshot 內不再有 e2e-victim 條目

- [ ] **Step 6: 全套測試最後一跑 + 結案 commit**

Run: `python -m pytest collector/tests -v`
Expected: 全部 passed

```bash
git add -A && git status --short   # 嚴格核對：此時應只有 monitor-config.yaml 的演練殘留還原
git commit -m "test(collector): E2E 告警演練完成（殺服務->告警->恢復通知）" --allow-empty
```

（若無檔案變更，用 `--allow-empty` 留下演練完成的紀錄點；演練證據輸出貼在結案報告。）

---

## 驗收總表（硬規則 1 + 4）

| 項目 | 證據 |
|---|---|
| 單元測試 | `python -m pytest collector/tests -v` 全綠輸出 |
| 常駐運作 | heartbeat age < 120s（schtasks 啟動後） |
| watchdog 救活 | 殺 collector → 90s 內 heartbeat 恢復新鮮 |
| E2E 告警 | 殺 victim → alerts 表 open（+Telegram 🔴）；拉回 → closed（+✅） |
| 驗收主體 | 實作完成後派 fresh-context agent 驗收（read-back + 實跑），實作者不自驗 |

## 明確不在本計畫（Phase 2+）

- schedules 模組（schtasks 結果碼 / n8n API / loop state 檔）→ Phase 2
- projects 模組（git 積壓）與每日摘要 → Phase 3
- LobsterPulse 膠囊徽章與全機健康面板、collector HTTP API → Phase 4
- Prometheus `/metrics` 出口（spec 明列預留，未排期）
