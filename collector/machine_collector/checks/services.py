"""服務類巡檢：port owner / healthz / log 增長（硬規則 8：健康 != 活著）。"""
from __future__ import annotations

import re

import psutil

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
