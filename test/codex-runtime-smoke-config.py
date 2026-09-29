#!/usr/bin/env python3
"""Assert the installed Codex TOML settings in the Windows runtime smoke."""

from pathlib import Path
import json
import sys
import tomllib


config_path = Path(sys.argv[1])
hooks_path = Path(sys.argv[2])
raw = config_path.read_bytes()
document = tomllib.loads(raw.decode("utf-8"))
features = document.get("features")
assert isinstance(features, dict), "[features] missing or not a table"
assert features.get("codex_hooks") is True, "codex_hooks is not enabled"
assert features.get("hooks") is True, "canonical hooks is not enabled"
for comment in (
    b"# user comment must survive",
    b"# user disabled hooks",
    b"# canonical key also disabled",
):
    assert comment in raw, f"TOML decor lost: {comment!r}"

hooks_raw = hooks_path.read_text(encoding="utf-8")
hooks_document = json.loads(hooks_raw)


def strings_in(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for key, item in value.items():
            yield from strings_in(key)
            yield from strings_in(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings_in(item)


hook_strings = tuple(strings_in(hooks_document))
assert any("lobster-pulse-hook.exe" in value for value in hook_strings), (
    "installed Windows sidecar command missing from hooks.json"
)
assert "third-party-pre" in hook_strings, "pre-existing third-party hook missing from hooks.json"
print("[smoke] config.toml: effective hook flags true; comments preserved")
print("[smoke] hooks.json: Windows sidecar installed; third-party hook preserved")
