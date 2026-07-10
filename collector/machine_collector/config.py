"""monitor-config.yaml 載入與驗證。"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

import yaml


@dataclass
class ServiceCfg:
    id: str
    port: int
    process_pattern: str
    healthz_url: str | None = None
    healthz_headers: dict[str, str] = field(default_factory=dict)
    log_path: str | None = None
    log_max_idle_secs: int | None = None


@dataclass
class ResourceCfg:
    commit_charge_alert_pct: float = 85.0
    disk_min_free_pct: float = 10.0
    cpu_alert_pct: float = 90.0
    cpu_alert_sustain_rounds: int = 10
    disks: list[str] = field(default_factory=lambda: ["C:"])


@dataclass
class NotifyCfg:
    telegram_bot_token: str = ""
    telegram_chat_id: str = ""
    cooldown_secs: int = 1800


@dataclass
class MonitorConfig:
    interval_secs: int = 60
    services: list[ServiceCfg] = field(default_factory=list)
    resources: ResourceCfg = field(default_factory=ResourceCfg)
    notify: NotifyCfg = field(default_factory=NotifyCfg)


def load_config(path: str | Path) -> MonitorConfig:
    raw = yaml.safe_load(Path(path).read_text(encoding="utf-8")) or {}
    return MonitorConfig(
        interval_secs=raw.get("interval_secs", 60),
        services=[ServiceCfg(**s) for s in raw.get("services", [])],
        resources=ResourceCfg(**raw.get("resources", {})),
        notify=NotifyCfg(**raw.get("notify", {})),
    )
