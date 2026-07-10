import json

import machine_collector.notify as notify_mod
from machine_collector.config import NotifyCfg
from machine_collector.notify import TelegramNotifier


def test_disabled_when_no_token():
    n = TelegramNotifier("", "")
    assert n.enabled is False
    assert n.send("hi") is False
    assert n.queue == []  # 未啟用不進 queue


def test_send_failure_queues_then_flush_retries(monkeypatch):
    n = TelegramNotifier("tok", "chat")
    calls = {"n": 0}

    def flaky_post(url, json=None, timeout=None):
        calls["n"] += 1
        class _R:
            status_code = 500 if calls["n"] == 1 else 200
        return _R()

    monkeypatch.setattr(notify_mod.requests, "post", flaky_post)
    assert n.send("msg1") is False
    assert n.queue == ["msg1"]
    n.flush()
    assert n.queue == []


def test_queue_cap(monkeypatch):
    n = TelegramNotifier("tok", "chat")
    monkeypatch.setattr(notify_mod.requests, "post",
                        lambda *a, **k: type("R", (), {"status_code": 500})())
    for i in range(150):
        n.send(f"m{i}")
    assert len(n.queue) == 100


def test_from_config_fallback_to_lobsterpulse(monkeypatch, tmp_path):
    lp = tmp_path / "config.json"
    lp.write_text(json.dumps({"appearance": {
        "telegram_bot_token": "lp-tok", "telegram_chat_id": "lp-chat"}}),
        encoding="utf-8")
    monkeypatch.setattr(notify_mod, "LOBSTERPULSE_CONFIG", lp)
    # Isolate from real home directory by monkeypatching collector config to nonexistent path
    monkeypatch.setattr(notify_mod, "COLLECTOR_TELEGRAM_CONFIG", tmp_path / "nonexistent_collector.json")
    n = TelegramNotifier.from_config(NotifyCfg())
    assert n.token == "lp-tok"
    assert n.chat_id == "lp-chat"


def test_from_config_own_values_win(monkeypatch, tmp_path):
    monkeypatch.setattr(notify_mod, "LOBSTERPULSE_CONFIG", tmp_path / "nope.json")
    n = TelegramNotifier.from_config(
        NotifyCfg(telegram_bot_token="own", telegram_chat_id="c1"))
    assert n.token == "own"


def test_post_failure_does_not_leak_token(monkeypatch, caplog):
    import logging
    n = TelegramNotifier("SECRET-TOKEN-123", "chat")

    def boom(url, json=None, timeout=None):
        raise notify_mod.requests.ConnectionError(
            f"Max retries exceeded with url: /botSECRET-TOKEN-123/sendMessage")
    monkeypatch.setattr(notify_mod.requests, "post", boom)
    with caplog.at_level(logging.WARNING):
        assert n.send("hi") is False
    assert "SECRET-TOKEN-123" not in caplog.text
    assert "<token>" in caplog.text


def test_non_200_logs_status(monkeypatch, caplog):
    import logging
    n = TelegramNotifier("tok", "chat")

    def bad_post(url, json=None, timeout=None):
        class _R:
            status_code = 401
            text = "Unauthorized"
        return _R()

    monkeypatch.setattr(notify_mod.requests, "post", bad_post)
    with caplog.at_level(logging.WARNING):
        assert n.send("hi") is False
    assert "401" in caplog.text


def test_from_config_collector_file_wins_over_lobsterpulse(monkeypatch, tmp_path):
    """collector file exists and has values -> use them over lobsterpulse"""
    collector_cfg = tmp_path / "collector_telegram.json"
    collector_cfg.write_text(
        json.dumps({"telegram_bot_token": "col-tok", "telegram_chat_id": "col-chat"}),
        encoding="utf-8")

    lobster_cfg = tmp_path / "lobster_config.json"
    lobster_cfg.write_text(
        json.dumps({"appearance": {
            "telegram_bot_token": "lp-tok", "telegram_chat_id": "lp-chat"}}),
        encoding="utf-8")

    monkeypatch.setattr(notify_mod, "COLLECTOR_TELEGRAM_CONFIG", collector_cfg)
    monkeypatch.setattr(notify_mod, "LOBSTERPULSE_CONFIG", lobster_cfg)

    n = TelegramNotifier.from_config(NotifyCfg())
    assert n.token == "col-tok"
    assert n.chat_id == "col-chat"


def test_from_config_collector_file_missing_falls_to_lobsterpulse(monkeypatch, tmp_path):
    """collector file doesn't exist -> fallback to lobsterpulse"""
    collector_cfg = tmp_path / "nonexistent.json"

    lobster_cfg = tmp_path / "lobster_config.json"
    lobster_cfg.write_text(
        json.dumps({"appearance": {
            "telegram_bot_token": "lp-tok", "telegram_chat_id": "lp-chat"}}),
        encoding="utf-8")

    monkeypatch.setattr(notify_mod, "COLLECTOR_TELEGRAM_CONFIG", collector_cfg)
    monkeypatch.setattr(notify_mod, "LOBSTERPULSE_CONFIG", lobster_cfg)

    n = TelegramNotifier.from_config(NotifyCfg())
    assert n.token == "lp-tok"
    assert n.chat_id == "lp-chat"
