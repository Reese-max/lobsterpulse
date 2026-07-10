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
ENTRY_KEY = "run_collector.py"  # 三鍵之一：入口腳本名
PYTHON_INTERPRETER_NAMES = {
    "python.exe", "pythonw.exe", "python", "pythonw", "python3.11.exe",
}  # 三鍵之三：首個 cmdline token 須為 python 直譯器
BURST_WINDOW_SECS = 1800
BURST_COUNT = 3


def heartbeat_age() -> float:
    try:
        return time.time() - float(HEARTBEAT.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return float("inf")


def kill_stale_collector() -> None:
    """踩雷 §6：cmdline 需同時含入口腳本名 + collector 專案路徑 + 首 token 為 python 直譯器才殺。"""
    me = psutil.Process().pid
    collector_dir_key = str(COLLECTOR_DIR).lower()
    for p in psutil.process_iter(["pid", "cmdline"]):
        if p.info["pid"] == me:
            continue
        cmdline = p.info["cmdline"] or []
        if not cmdline:
            continue
        cl = " ".join(cmdline).lower()
        if ENTRY_KEY.lower() not in cl or collector_dir_key not in cl:
            continue
        interpreter = Path(cmdline[0]).name.lower()
        if interpreter not in PYTHON_INTERPRETER_NAMES:
            continue
        subprocess.run(["taskkill", "/PID", str(p.info["pid"]), "/T", "/F"],
                       capture_output=True)


def _alert_telegram(text: str) -> None:
    try:
        from machine_collector.config import load_config
        from machine_collector.notify import TelegramNotifier
        cfg = load_config(COLLECTOR_DIR / "monitor-config.yaml")
        TelegramNotifier.from_config(cfg.notify).send(text)
    except Exception:
        pass  # config 缺失/YAML 壞不得外洩例外，告警失敗不可影響恢復流程


def record_restart_and_maybe_alert() -> bool:
    now = time.time()
    try:
        times = [float(x) for x in RESTARTS.read_text(encoding="utf-8").split()]
    except (OSError, ValueError):
        times = []
    times = [t for t in times if now - t < BURST_WINDOW_SECS] + [now]
    RESTARTS.parent.mkdir(parents=True, exist_ok=True)
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
    # 先救活再記帳：記帳/告警失敗不得阻斷重啟（§20 恢復優先）
    subprocess.run(["schtasks", "/Run", "/TN", TASK_NAME], capture_output=True)
    try:
        record_restart_and_maybe_alert()
    except Exception:
        pass  # 記帳失敗不影響已完成的重啟
    return 1


if __name__ == "__main__":
    sys.exit(main())
