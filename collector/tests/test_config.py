from machine_collector.config import load_config


def test_load_config_full(tmp_path):
    f = tmp_path / "m.yaml"
    f.write_text(
        """
interval_secs: 30
notify:
  telegram_bot_token: tok
  telegram_chat_id: "123"
services:
  - id: hermes
    port: 8318
    process_pattern: python
resources:
  disks: ["C:", "D:"]
""",
        encoding="utf-8",
    )
    cfg = load_config(f)
    assert cfg.interval_secs == 30
    assert cfg.services[0].id == "hermes"
    assert cfg.services[0].port == 8318
    assert cfg.services[0].healthz_url is None
    assert cfg.resources.disks == ["C:", "D:"]
    assert cfg.resources.commit_charge_alert_pct == 85.0
    assert cfg.notify.telegram_bot_token == "tok"


def test_load_config_empty_file_uses_defaults(tmp_path):
    f = tmp_path / "m.yaml"
    f.write_text("", encoding="utf-8")
    cfg = load_config(f)
    assert cfg.interval_secs == 60
    assert cfg.services == []
    assert cfg.notify.cooldown_secs == 1800
