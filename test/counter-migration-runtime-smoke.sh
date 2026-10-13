#!/usr/bin/env bash
# Packaged exporter -> disposable Prometheus -> query -> app restart receipt.
# Runs only in the isolated Linux CI home; never discovers external consumers.
set -euo pipefail
cd "$(dirname "$0")/.."

APP="$PWD/src-tauri/target/release/lobster-pulse"
SIDECAR="$PWD/src-tauri/target/release/lobster-pulse-hook"
[ -x "$APP" ] && [ -x "$SIDECAR" ] || { echo '[counter-smoke] missing packaged binaries'; exit 1; }
command -v docker >/dev/null || { echo '[counter-smoke] Docker is required for disposable Prometheus'; exit 1; }

TEST_HOME=$(mktemp -d /tmp/lp-counter-migration-XXXXXX)
[ -n "$TEST_HOME" ] || { echo '[counter-smoke] mktemp failed'; exit 1; }
# The Prometheus container runs without root privileges and must read only the
# two disposable config files mounted from this directory.
chmod 755 "$TEST_HOME"
APP_PID=''
CONTAINER="lp-counter-migration-$$"
cleanup() {
  docker stop "$CONTAINER" >/dev/null 2>&1 || true
  if [ -n "$APP_PID" ]; then
    kill -- -"$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  fi
  umount "$TEST_HOME/.cache/doc" 2>/dev/null || fusermount3 -u "$TEST_HOME/.cache/doc" 2>/dev/null || true
  rm -rf -- "$TEST_HOME" 2>/dev/null || true
}
trap cleanup EXIT

export HOME="$TEST_HOME"
export XDG_CONFIG_HOME="$TEST_HOME/.config"
export XDG_CACHE_HOME="$TEST_HOME/.cache"
export XDG_DATA_HOME="$TEST_HOME/.local/share"
export XDG_STATE_HOME="$TEST_HOME/.local/state"
export XDG_RUNTIME_DIR="$TEST_HOME/.run"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME"
mkdir -m 700 "$XDG_RUNTIME_DIR"

start_app() {
  rm -f "$TEST_HOME/.lobsterpulse/port"
  env LOBSTERPULSE_HEADLESS_INSTALL=codex setsid dbus-run-session -- xvfb-run -a "$APP" \
    >"$TEST_HOME/app.log" 2>&1 &
  APP_PID=$!
  PORT=''
  for _ in $(seq 1 60); do
    if [ -s "$TEST_HOME/.lobsterpulse/port" ]; then
      PORT=$(cat "$TEST_HOME/.lobsterpulse/port")
      break
    fi
    kill -0 "$APP_PID" 2>/dev/null || { cat "$TEST_HOME/app.log"; echo '[counter-smoke] app exited'; exit 1; }
    sleep 0.5
  done
  [ -n "$PORT" ] || { cat "$TEST_HOME/app.log"; echo '[counter-smoke] no port file'; exit 1; }
  METRICS_URL="http://127.0.0.1:$((PORT + 100))/metrics"
  for _ in $(seq 1 40); do
    curl -fsS "$METRICS_URL" -o "$TEST_HOME/metrics.txt" 2>/dev/null && return
    sleep 0.5
  done
  echo '[counter-smoke] metrics endpoint unavailable'; exit 1
}

stop_app() {
  kill -- -"$APP_PID" 2>/dev/null || true
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=''
  sleep 1
}

check_exporter() {
  EVENT='{"hook_event_name":"SessionStart","sessionId":"counter-migration-fixture","cwd":"/tmp"}'
  printf '%s' "$EVENT" | "$SIDECAR" codex
  curl -fsS "$METRICS_URL" -o "$TEST_HOME/metrics.txt"
  python3 - "$TEST_HOME/metrics.txt" <<'PY'
import pathlib, sys
body = pathlib.Path(sys.argv[1]).read_text()
legacy = (
    'lobsterpulse_tokens_input', 'lobsterpulse_tokens_output',
    'lobsterpulse_provider_tokens_input', 'lobsterpulse_provider_tokens_output',
    'lobsterpulse_provider_failure_count', 'lobsterpulse_provider_session_count',
)
for name in legacy:
    help_line = next((line for line in body.splitlines() if line.startswith(f'# HELP {name} ')), '')
    assert 'migration POSTPONED since 2026-09-26; review 2026-10-31' in help_line, help_line
    assert f'# TYPE {name}_total counter' in body
    old = {line.replace(name, '', 1) for line in body.splitlines() if line.startswith(name + ' ') or line.startswith(name + '{')}
    new = {line.replace(name + '_total', '', 1) for line in body.splitlines() if line.startswith(name + '_total ') or line.startswith(name + '_total{')}
    assert old == new, (name, old, new)
assert 'lobsterpulse_provider_session_count_total{provider="codex"} 1' in body
print('[counter-smoke] /metrics: six HELP/TYPE/value pairs and migration dates verified')
PY
}

start_app
check_exporter
METRICS_PORT=$((PORT + 100))
cat > "$TEST_HOME/prometheus.yml" <<EOF
global:
  scrape_interval: 1s
  evaluation_interval: 1s
scrape_configs:
  - job_name: lobsterpulse
    static_configs:
      - targets: ['127.0.0.1:$METRICS_PORT']
rule_files:
  - /etc/prometheus/rules.yml
EOF
cat > "$TEST_HOME/rules.yml" <<'EOF'
groups:
  - name: counter-migration
    rules:
      - record: lobsterpulse:tokens_input:sum
        expr: sum(lobsterpulse_tokens_input_total)
      - alert: LobsterpulseCounterPresent
        expr: lobsterpulse_provider_session_count_total{provider="codex"} >= 1
        for: 0s
EOF

docker run -d --rm --name "$CONTAINER" --network host \
  -v "$TEST_HOME/prometheus.yml:/etc/prometheus/prometheus.yml:ro" \
  -v "$TEST_HOME/rules.yml:/etc/prometheus/rules.yml:ro" \
  prom/prometheus:v3.14.0 --config.file=/etc/prometheus/prometheus.yml \
  --storage.tsdb.path=/prometheus --web.listen-address=127.0.0.1:9099 >/dev/null

check_queries() {
  python3 - <<'PY'
import json, time, urllib.parse, urllib.request

def query(expr):
    url = 'http://127.0.0.1:9099/api/v1/query?' + urllib.parse.urlencode({'query': expr})
    with urllib.request.urlopen(url, timeout=3) as response:
        doc = json.load(response)
    assert doc['status'] == 'success', doc
    return doc['data']['result']

deadline = time.monotonic() + 45
while time.monotonic() < deadline:
    try:
        up = query('up{job="lobsterpulse"}')
        recorded = query('lobsterpulse:tokens_input:sum')
        alert = query('ALERTS{alertname="LobsterpulseCounterPresent",alertstate="firing"}')
        dashboard = query('sum(lobsterpulse_provider_session_count_total)')
        equal = query('lobsterpulse_tokens_input_total - lobsterpulse_tokens_input')
        if (up and up[0]['value'][1] == '1' and recorded and alert
                and dashboard and float(dashboard[0]['value'][1]) >= 1
                and equal and float(equal[0]['value'][1]) == 0):
            print('[counter-smoke] Prometheus: scrape, recording rule, alert, dashboard expression and alias equality verified')
            break
    except Exception:
        pass
    time.sleep(1)
else:
    raise SystemExit('[counter-smoke] Prometheus queries did not become healthy in 45s')
PY
}

check_queries
stop_app
start_app
[ "$((PORT + 100))" -eq "$METRICS_PORT" ] || { echo '[counter-smoke] exporter port changed after restart'; exit 1; }
check_exporter
check_queries

echo "[counter-smoke] git_sha=$(git rev-parse HEAD)"
sha256sum "$APP" docs/metrics/counter-migration.json "$TEST_HOME/prometheus.yml" "$TEST_HOME/rules.yml"
printf '%s' '{"hook_event_name":"SessionStart","sessionId":"counter-migration-fixture","cwd":"/tmp"}' | sha256sum
echo '[counter-smoke] PASS — packaged endpoint, disposable Prometheus queries, and restart'
