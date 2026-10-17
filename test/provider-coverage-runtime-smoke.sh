#!/usr/bin/env bash
# Run inside codex-runtime-smoke.sh's disposable packaged-app home.
# This exercises a real OpenAB hook, real /metrics, stale/fresh snapshots,
# default-disabled config, and the resulting K0 receipt.
set -euo pipefail
cd "$(dirname "$0")/.."
HOME_DIR="${1:?disposable app home required}"
PORT="${2:?hook port required}"
METRICS_URL="http://127.0.0.1:$((PORT + 100))/metrics"

curl -fsS -X POST "http://127.0.0.1:${PORT}/hook/irisx_bot" \
  -H 'Content-Type: application/json' \
  --data '{"hook_event_name":"SessionStart","session_id":"coverage-irisx-1","cwd":"/tmp"}' \
  >"$HOME_DIR/openab-response.txt"

for _ in $(seq 1 20); do
  if curl -fsS "$METRICS_URL" | grep -q 'lobsterpulse_provider_sessions{provider="irisx_bot"} 1'; then
    break
  fi
  sleep 0.5
done
curl -fsS "$METRICS_URL" >"$HOME_DIR/coverage-metrics.txt"
grep -q 'lobsterpulse_provider_sessions{provider="irisx_bot"} 1' "$HOME_DIR/coverage-metrics.txt"
grep -q 'lobsterpulse_provider_idle_seconds{provider="irisx_bot"}' "$HOME_DIR/coverage-metrics.txt"

mkdir -p "$HOME_DIR/.lobsterpulse"
python3 - "$HOME_DIR/.lobsterpulse" <<'EOF'
import json, pathlib, sys, time
directory = pathlib.Path(sys.argv[1])
now = int(time.time())
for provider, runner, timestamp in (("local", "codex", now), ("cicx", "claude", now - 172800)):
    (directory / f"usage-{provider}.json").write_text(json.dumps({
        "runners": [{"name": runner, "ok": True}], "updated_at": timestamp,
    }), encoding="utf-8")
EOF

LOBSTERPULSE_METRICS_URL="$METRICS_URL" \
LOBSTERPULSE_QUOTA_DIR="$HOME_DIR/.lobsterpulse" \
LOBSTERPULSE_CONFIG_FILE="$HOME_DIR/.config/lobsterpulse/config.json" \
  python3 scripts/k0_measure.py >"$HOME_DIR/coverage-report.txt"

python3 - .harness-k0.json <<'EOF'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
receipt = report["coverage_receipt"]
d = receipt["dimensions"]
assert receipt["registry_version"] == "2026-09-26.1"
assert receipt["baseline_id"] == "k0-2026-09-26"
assert d["registered"]["numerator"] == d["registered"]["denominator"] == 13
assert d["configured"]["numerator"] is not None, "config source unavailable"
assert d["live_emitting"]["numerator"] >= 2, d["live_emitting"]
assert d["nonzero_sessions"]["numerator"] >= 2, d["nonzero_sessions"]
assert d["quota_observable"]["numerator"] >= 1, d["quota_observable"]
assert receipt["providers"]["irisx_bot"]["health_status"] == "LIVE_EMITTING"
assert receipt["providers"]["irisx_bot"]["quota_status"] == "EXTERNAL_DEPENDENCY"
assert receipt["providers"]["cicx"]["quota_status"] == "STALE"
assert receipt["providers"]["mimo"]["health_status"] == "NOT_CONFIGURED"
for dimension in d.values():
    assert dimension["registered_denominator"] == 13
    assert dimension["exclusions"] == []
print("[coverage smoke] packaged app: Codex and IRISX hooks emitted; disabled/stale/external states retained in K0 receipt")
EOF
