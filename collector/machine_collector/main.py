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
from .notify import TelegramNotifier, DiscordNotifier, MultiNotifier
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
    logging.basicConfig(level=logging.INFO, handlers=handlers, force=True,
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
    tg = TelegramNotifier.from_config(cfg.notify)
    dc = DiscordNotifier.from_config(cfg.notify)
    notifier = MultiNotifier([tg, dc])
    log.info("machine-collector 啟動 interval=%ss services=%d telegram=%s discord=%s",
             cfg.interval_secs, len(cfg.services),
             "enabled" if tg.enabled else "disabled",
             "enabled" if dc.enabled else "disabled")

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
