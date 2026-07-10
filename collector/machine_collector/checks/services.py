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
