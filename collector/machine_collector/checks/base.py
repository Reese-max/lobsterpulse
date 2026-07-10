"""所有 check 的共用結果型別。"""
from __future__ import annotations

from dataclasses import dataclass


@dataclass
class CheckResult:
    check_id: str
    ok: bool
    detail: str = ""
    value: float | None = None
