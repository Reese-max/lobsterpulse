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
