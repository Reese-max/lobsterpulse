"""服務類巡檢：port owner / healthz / log 增長（硬規則 8：健康 != 活著）。"""
from __future__ import annotations

import re
import time
from pathlib import Path

import psutil
import requests

from ..config import ServiceCfg
from .base import CheckResult


def _listening_pid(port: int) -> int | None:
    """監聽指定 port 的進程 pid。

    回傳值：
    - pid > 0：進程 id
    - 0：有人監聽但無法取得 owner（如 Windows 權限不足）
    - None：無人監聽
    """
    for c in psutil.net_connections(kind="tcp"):
        if c.laddr and c.laddr.port == port and c.status == psutil.CONN_LISTEN:
            return c.pid if c.pid is not None else 0
    return None


def check_port_owner(svc: ServiceCfg) -> CheckResult:
    """踩雷 §22：port 有人聽不代表是對的進程，owner 必須匹配預期 pattern。"""
    cid = f"service.{svc.id}.port_owner"
    try:
        pid = _listening_pid(svc.port)
    except (psutil.Error, OSError) as e:
        return CheckResult(cid, False, f"連線列舉失敗: {type(e).__name__}: {e}")

    if pid is None:
        return CheckResult(cid, False, f"port {svc.port} 無人監聽")
    if pid == 0:
        return CheckResult(cid, False, f"port {svc.port} 有人監聽但無法取得 owner（權限不足？）")

    try:
        p = psutil.Process(pid)
        name = p.name()
        ident = f"{name} {' '.join(p.cmdline())}"
    except psutil.Error as e:
        return CheckResult(cid, False, f"port {svc.port} owner pid={pid} 無法讀取: {e}")

    if re.search(svc.process_pattern, ident, re.IGNORECASE):
        return CheckResult(cid, True, f"pid={pid} {name}")
    return CheckResult(
        cid, False,
        f"port {svc.port} owner 不符: pid={pid} {name}（預期 {svc.process_pattern}）")


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
            ("port_owner", lambda s=svc: check_port_owner(s)),
            ("healthz", lambda s=svc: check_healthz(s)),
            ("log_growth", lambda s=svc: check_log_growth(s, log_state)),
        )
        for name, probe in probes:
            try:
                r = probe()
            except Exception as e:
                r = CheckResult(f"service.{svc.id}.{name}", False,
                                f"探針例外: {type(e).__name__}: {e}")
            if r is not None:
                results.append(r)
    return results
