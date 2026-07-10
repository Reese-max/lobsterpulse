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
        # 本輪未回報的 check_id 不累計「連續」失敗（缺席 ≠ 失敗）
        seen = {r.check_id for r in results}
        for cid in list(self._consecutive):
            if cid not in seen:
                self._consecutive[cid] = 0
        return out
