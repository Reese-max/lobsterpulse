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
