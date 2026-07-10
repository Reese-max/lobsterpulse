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
CREATE UNIQUE INDEX IF NOT EXISTS idx_alerts_open_unique ON alerts (check_id) WHERE closed_ts IS NULL;
"""


class Storage:
    def __init__(self, db_path: str | Path):
        # 單進程單線程設計：daemon 只有一個執行路徑，無需 check_same_thread
        self.conn = sqlite3.connect(str(db_path), timeout=10)
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
        try:
            self.conn.execute(
                "INSERT INTO alerts (check_id, severity, message, opened_ts) VALUES (?,?,?,?)",
                (check_id, severity, message, time.time()))
            self.conn.commit()
            return True
        except sqlite3.IntegrityError:
            return False

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
