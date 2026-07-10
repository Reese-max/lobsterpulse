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


# Discord tests
def test_discord_from_config_collector_file_wins(monkeypatch, tmp_path):
    """collector discord.json 值勝過 lobsterpulse appearance.discord"""
    from machine_collector.notify import DiscordNotifier

    collector_cfg = tmp_path / "collector_discord.json"
    collector_cfg.write_text(
        json.dumps({"discord_bot_token": "col-tok", "discord_channel_id": "col-ch"}),
        encoding="utf-8")

    lobster_cfg = tmp_path / "lobster_config.json"
    lobster_cfg.write_text(
        json.dumps({"appearance": {
            "discord": {"bot_token": "lp-tok", "channel_id": "lp-ch"}}}),
        encoding="utf-8")

    monkeypatch.setattr(notify_mod, "COLLECTOR_DISCORD_CONFIG", collector_cfg)
    monkeypatch.setattr(notify_mod, "LOBSTERPULSE_CONFIG", lobster_cfg)

    n = DiscordNotifier.from_config(NotifyCfg())
    assert n.token == "col-tok"
    assert n.channel_id == "col-ch"


def test_discord_send_success_and_non_200_logs(monkeypatch, caplog):
    """200 回 True；401 回 False 且 caplog 含 "401" """
    import logging
    from machine_collector.notify import DiscordNotifier

    n = DiscordNotifier("tok", "ch123")

    # 測試 200 成功
    def ok_post(url, json=None, headers=None, timeout=None):
        class _R:
            status_code = 200
        return _R()

    monkeypatch.setattr(notify_mod.requests, "post", ok_post)
    assert n.send("ok") is True

    # 測試 401 失敗
    def bad_post(url, json=None, headers=None, timeout=None):
        class _R:
            status_code = 401
            text = "Unauthorized"
        return _R()

    monkeypatch.setattr(notify_mod.requests, "post", bad_post)
    with caplog.at_level(logging.WARNING):
        assert n.send("bad") is False
    assert "401" in caplog.text


def test_discord_disabled_no_queue(monkeypatch):
    """無 token 時 send False、queue 空"""
    from machine_collector.notify import DiscordNotifier

    n = DiscordNotifier("", "")
    assert n.enabled is False
    assert n.send("hi") is False
    assert n.queue == []


def test_multi_notifier_fans_out_and_filters(monkeypatch):
    """兩個 stub notifier（一 enabled 一 disabled），send 後只有 enabled 的收到；
    一成功一失敗時回 True；flush 傳遞"""
    from machine_collector.notify import MultiNotifier

    # Stub notifier
    class StubNotifier:
        def __init__(self, enabled, send_result=True):
            self.enabled = enabled
            self.send_result = send_result
            self.sent = []

        def send(self, text):
            if self.enabled:
                self.sent.append(text)
                return self.send_result
            return False

        def flush(self):
            pass

    enabled_ok = StubNotifier(enabled=True, send_result=True)
    enabled_fail = StubNotifier(enabled=True, send_result=False)
    disabled = StubNotifier(enabled=False, send_result=True)

    # 測試過濾掉 disabled
    m = MultiNotifier([enabled_ok, disabled])
    assert m.enabled is True
    assert len(m.notifiers) == 1

    # 測試 send 都送到
    result = m.send("msg1")
    assert result is True
    assert enabled_ok.sent == ["msg1"]

    # 測試一成功一失敗回 True
    m2 = MultiNotifier([enabled_ok, enabled_fail])
    result = m2.send("msg2")
    assert result is True
    assert enabled_ok.sent == ["msg1", "msg2"]
    assert enabled_fail.sent == ["msg2"]

    # 測試 flush 傳遞
    m2.flush()  # Should not raise
