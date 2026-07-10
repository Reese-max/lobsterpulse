"""Telegram 推播（含失敗 queue 重試）。token 空時 fallback 讀 LobsterPulse config.json。"""
from __future__ import annotations

import json
import logging
import os
from pathlib import Path

import requests

from .config import NotifyCfg

log = logging.getLogger(__name__)
LOBSTERPULSE_CONFIG = Path(os.environ.get("APPDATA", "")) / "lobsterpulse" / "config.json"
MAX_QUEUE = 100


def _fallback_from_lobsterpulse() -> tuple[str, str]:
    try:
        d = json.loads(LOBSTERPULSE_CONFIG.read_text(encoding="utf-8"))
        a = d.get("appearance", {})
        return a.get("telegram_bot_token", ""), a.get("telegram_chat_id", "")
    except (OSError, json.JSONDecodeError):
        return "", ""


class TelegramNotifier:
    def __init__(self, token: str, chat_id: str):
        self.token = token
        self.chat_id = chat_id
        self.queue: list[str] = []

    @classmethod
    def from_config(cls, cfg: NotifyCfg) -> "TelegramNotifier":
        token, chat_id = cfg.telegram_bot_token, cfg.telegram_chat_id
        if not token or not chat_id:
            fb_token, fb_chat = _fallback_from_lobsterpulse()
            token = token or fb_token
            chat_id = chat_id or fb_chat
        return cls(token, chat_id)

    @property
    def enabled(self) -> bool:
        return bool(self.token and self.chat_id)

    def _redact(self, text: str) -> str:
        return text.replace(self.token, "<token>") if self.token else text

    def _post(self, text: str) -> bool:
        try:
            r = requests.post(
                f"https://api.telegram.org/bot{self.token}/sendMessage",
                json={"chat_id": self.chat_id, "text": text}, timeout=10)
            if r.status_code != 200:
                try:
                    body = r.text[:200]
                except Exception:
                    body = ""
                log.warning("telegram HTTP %s: %s", r.status_code, self._redact(body))
            return r.status_code == 200
        except requests.RequestException as e:
            log.warning("telegram 送出失敗: %s: %s", type(e).__name__, self._redact(str(e)))
            return False

    def send(self, text: str) -> bool:
        if not self.enabled:
            log.info("telegram 未設定，僅記 log：%s", text)
            return False
        if self._post(text):
            return True
        if len(self.queue) < MAX_QUEUE:
            self.queue.append(text)
        else:
            log.warning("telegram queue 已滿（%d），丟棄訊息", MAX_QUEUE)
        return False

    def flush(self) -> None:
        if not self.enabled or not self.queue:
            return
        remaining = [t for t in self.queue if not self._post(t)]
        self.queue = remaining
