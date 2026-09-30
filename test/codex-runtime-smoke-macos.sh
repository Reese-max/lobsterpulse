#!/usr/bin/env bash
# MacOS packaged-runtime receipt for issue #3.
# Runs only the exact no-bundle release binaries built in the current CI job.
# All app state is redirected to a unique home below GitHub's disposable RUNNER_TEMP.
set -euo pipefail

fail() {
  printf '[smoke] FAIL: %s\n' "$1" >&2
  exit 1
}

if [[ "${GITHUB_ACTIONS:-}" != "true" || "${RUNNER_OS:-}" != "macOS" ||
      "${RUNNER_ENVIRONMENT:-}" != "github-hosted" || -z "${RUNNER_TEMP:-}" ]]; then
  fail 'this smoke may run only on a GitHub-hosted macOS runner'
fi
if [[ "$#" -ne 1 || "${1:-}" != "--skip-build" ]]; then
  fail 'usage: test/codex-runtime-smoke-macos.sh --skip-build'
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
APP="$REPO_ROOT/src-tauri/target/release/lobster-pulse"
SIDECAR="$REPO_ROOT/src-tauri/target/release/lobster-pulse-hook"
for binary in "$APP" "$SIDECAR"; do
  [[ -x "$binary" ]] || fail "missing macOS release artifact: $(basename "$binary")"
done
RUNNER_TEMP_REAL="$(cd "$RUNNER_TEMP" && pwd -P)" || fail 'RUNNER_TEMP is not an existing directory'

HOME_DIR=''
APP_PID=''
APP_GROUP_STOPPED=1
cleanup() {
  local test_status="$?"
  local cleanup_failed=0
  trap - EXIT

  if [[ -n "$APP_PID" ]]; then
    if /bin/kill -0 "-$APP_PID" 2>/dev/null; then
      /bin/kill -TERM "-$APP_PID" 2>/dev/null || true
      for _ in $(seq 1 40); do
        /bin/kill -0 "-$APP_PID" 2>/dev/null || break
        sleep 0.25
      done
      if /bin/kill -0 "-$APP_PID" 2>/dev/null; then
        /bin/kill -KILL "-$APP_PID" 2>/dev/null || true
        sleep 0.5
      fi
    fi
    if /bin/kill -0 "-$APP_PID" 2>/dev/null; then
      APP_GROUP_STOPPED=0
      cleanup_failed=1
    fi
    wait "$APP_PID" 2>/dev/null || true
  fi

  if [[ "$APP_GROUP_STOPPED" -eq 1 && -n "$HOME_DIR" ]]; then
    if [[ "$HOME_DIR" == "$RUNNER_TEMP_REAL"/lp-codex-smoke-macos.* ]]; then
      rm -rf -- "$HOME_DIR" || cleanup_failed=1
    else
      cleanup_failed=1
    fi
  elif [[ -n "$HOME_DIR" ]]; then
    cleanup_failed=1
  fi

  if [[ "$cleanup_failed" -ne 0 ]]; then
    printf '[smoke] cleanup incomplete; isolated runner-temp home retained for runner teardown\n' >&2
    exit 1
  fi
  exit "$test_status"
}
trap cleanup EXIT

HOME_DIR="$(mktemp -d "$RUNNER_TEMP_REAL/lp-codex-smoke-macos.XXXXXX")" || fail 'could not create isolated runner-temp home'
HOME_DIR="$(cd "$HOME_DIR" && pwd -P)" || fail 'could not resolve isolated runner-temp home'
[[ "$HOME_DIR" == "$RUNNER_TEMP_REAL"/lp-codex-smoke-macos.* ]] || fail 'isolated home escaped RUNNER_TEMP'

# dirs::home_dir() and the app's config/data directories resolve under HOME on
# macOS. Do not inspect, quarantine, or restore the runner account's real home.
export HOME="$HOME_DIR"
export XDG_CONFIG_HOME="$HOME_DIR/.config"
export XDG_CACHE_HOME="$HOME_DIR/.cache"
export XDG_DATA_HOME="$HOME_DIR/.local/share"
export XDG_STATE_HOME="$HOME_DIR/.local/state"
export XDG_RUNTIME_DIR="$HOME_DIR/.run"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_RUNTIME_DIR" "$HOME_DIR/tmp"
chmod 700 "$XDG_RUNTIME_DIR"

mkdir -p "$HOME_DIR/.codex"
cat > "$HOME_DIR/.codex/config.toml" <<'EOF'
# user comment must survive
[features]
codex_hooks = false # user disabled hooks
hooks = false # canonical key also disabled
EOF
cat > "$HOME_DIR/.codex/hooks.json" <<'EOF'
{"hooks":{"PreToolUse":[{"matcher":"third-party","hooks":[{"type":"command","command":"third-party-pre"}]}]}}
EOF
printf '[smoke] isolated HOME prepared under RUNNER_TEMP\n'

# Create a process group for the app so cleanup can signal only this smoke's
# launch tree. Python execs the exact build output without changing its PID.
env -i PATH="$PATH" HOME="$HOME_DIR" TMPDIR="$HOME_DIR/tmp" LANG="${LANG:-C.UTF-8}" \
  XDG_CONFIG_HOME="$XDG_CONFIG_HOME" XDG_CACHE_HOME="$XDG_CACHE_HOME" \
  XDG_DATA_HOME="$XDG_DATA_HOME" XDG_STATE_HOME="$XDG_STATE_HOME" \
  XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" LOBSTERPULSE_HEADLESS_INSTALL=codex \
  python3 -c 'import os, sys; os.setsid(); os.execv(sys.argv[1], sys.argv[1:])' "$APP" \
  >"$HOME_DIR/app.stdout.log" 2>"$HOME_DIR/app.stderr.log" &
APP_PID="$!"

PORT_FILE="$HOME_DIR/.lobsterpulse/port"
PORT=''
for _ in $(seq 1 60); do
  if [[ -s "$PORT_FILE" ]]; then
    PORT="$(tr -d '[:space:]' < "$PORT_FILE")"
    break
  fi
  if ! /bin/kill -0 "$APP_PID" 2>/dev/null; then
    fail 'packaged app exited before creating the isolated hook-server port file'
  fi
  sleep 0.5
done
[[ -n "$PORT" ]] || fail 'packaged app did not publish its port within 30 seconds'
[[ "$PORT" =~ ^[0-9]+$ ]] || fail 'app published an invalid port'
(( PORT >= 1 && PORT <= 65435 )) || fail 'app published an out-of-range port'
printf '[smoke] packaged app started with isolated home\n'

python3 "$REPO_ROOT/test/codex-runtime-smoke-config.py" \
  "$HOME_DIR/.codex/config.toml" "$HOME_DIR/.codex/hooks.json" "lobster-pulse-hook" \
  || fail 'effective Codex flags or hook preservation assertion failed'

# Exercise the exact macOS sidecar configured for Codex with a synthetic event.
EVENT_FILE="$HOME_DIR/session-start.json"
printf '%s\n' '{"hook_event_name":"SessionStart","sessionId":"macos-smoke-codex-1","cwd":"/tmp/lp-smoke"}' > "$EVENT_FILE"
if ! env -i PATH="$PATH" HOME="$HOME_DIR" TMPDIR="$HOME_DIR/tmp" LANG="${LANG:-C.UTF-8}" \
  XDG_CONFIG_HOME="$XDG_CONFIG_HOME" XDG_CACHE_HOME="$XDG_CACHE_HOME" \
  XDG_DATA_HOME="$XDG_DATA_HOME" XDG_STATE_HOME="$XDG_STATE_HOME" \
  XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
  python3 - "$SIDECAR" "$EVENT_FILE" "$HOME_DIR" <<'PY'
import os
import subprocess
import sys

binary, event_path, home = sys.argv[1:]
try:
    with open(event_path, "rb") as event, \
         open(os.path.join(home, "sidecar.stdout.log"), "wb") as stdout, \
         open(os.path.join(home, "sidecar.stderr.log"), "wb") as stderr:
        result = subprocess.run([binary, "codex"], stdin=event, stdout=stdout,
                                stderr=stderr, timeout=20, check=False)
except subprocess.TimeoutExpired:
    print("[smoke] sidecar timed out", file=sys.stderr)
    raise SystemExit(124)
raise SystemExit(result.returncode)
PY
then
  fail 'packaged sidecar failed to deliver the synthetic event'
fi
[[ ! -s "$HOME_DIR/sidecar.stderr.log" ]] || fail 'sidecar reported a delivery failure'

METRICS_FILE="$HOME_DIR/metrics.txt"
METRICS_URI="http://127.0.0.1:$((PORT + 100))/metrics"
for _ in $(seq 1 40); do
  if curl --fail --silent "$METRICS_URI" -o "$METRICS_FILE" 2>/dev/null &&
     grep -Eq 'lobsterpulse_provider_event_type_total\\{provider="codex",type="SessionStart"\\} [1-9][0-9]*([[:space:]]|$)' "$METRICS_FILE"; then
    break
  fi
  sleep 0.25
done
grep -Eq 'lobsterpulse_provider_event_type_total\\{provider="codex",type="SessionStart"\\} [1-9][0-9]*([[:space:]]|$)' "$METRICS_FILE" \
  || fail 'synthetic Codex SessionStart was not counted by app metrics'

printf '[smoke] metrics: synthetic Codex SessionStart counted\n'
printf '[smoke] PASS — macOS release app enabled the Codex flags and its sidecar event reached app metrics\n'
