#!/usr/bin/env bash
# ============================================================
#  codex-runtime-smoke.sh — issue #3 packaged-runtime evidence
#
#  Proves on real binaries (not static parsing tests) that:
#    1. the packaged lobster-pulse app starts and opens the hook server
#    2. enabling Codex on a home where [features] codex_hooks = false
#       flips the flag to true via the real install path
#       (LOBSTERPULSE_HEADLESS_INSTALL drives the same
#       hooks_configurator::install_provider call the settings
#       checkbox makes, minus the webview click)
#    3. existing third-party hooks in hooks.json survive
#    4. a real lobster-pulse-hook sidecar invocation (the exact thing
#       Codex runs) delivers a Codex event the app actually counts
#
#  Everything happens under a disposable $HOME — nothing touches the
#  developer's real ~/.codex or ~/.lobsterpulse.
#
#  Requires: cargo, cargo-tauri, Xvfb, dbus-run-session, curl, python3.
#  Usage: test/codex-runtime-smoke.sh [--skip-build]
# ============================================================
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "[smoke] FAIL: cannot cd to project root"; exit 1; }

APP=src-tauri/target/release/lobster-pulse
SIDECAR=src-tauri/target/release/lobster-pulse-hook
SKIP_BUILD=0
[ "${1:-}" = "--skip-build" ] && SKIP_BUILD=1

fail() { echo "[smoke] FAIL: $*"; exit 1; }
info() { echo "[smoke] $*"; }

# ── 1. packaged binaries ──────────────────────────────────────
if [ "$SKIP_BUILD" -eq 0 ]; then
  info "building release binaries (cargo tauri build --no-bundle)..."
  (cd src-tauri && cargo tauri build --no-bundle) || fail "tauri build"
fi
[ -x "$APP" ] || fail "missing $APP"
[ -x "$SIDECAR" ] || fail "missing $SIDECAR"

# ── 2. disposable home with pre-existing codex_hooks = false ──
HOME_DIR=$(mktemp -d /tmp/lp-codex-smoke-XXXXXX)
APP_PID=""
cleanup() {
  if [ -n "$APP_PID" ]; then
    # setsid put the whole launch chain (dbus → xvfb-run → app) in one process
    # group; kill the group so the lobster-pulse grandchild cannot outlive us.
    kill -- -"$APP_PID" 2>/dev/null
    wait "$APP_PID" 2>/dev/null  # let webkit/mesa flush + exit before rm
    # safety net scoped to this worktree's binary — a real user install of
    # lobster-pulse on the same box must not be killed.
    pkill -f "^$PWD/$APP\$" 2>/dev/null
  fi
  # webkit may fuse-mount the document portal at $HOME/.cache/doc — a mountpoint
  # cannot be rm'd, so unmount it first (best-effort; harmless if absent).
  if [ -n "$HOME_DIR" ]; then
    umount "$HOME_DIR/.cache/doc" 2>/dev/null \
      || fusermount3 -u "$HOME_DIR/.cache/doc" 2>/dev/null \
      || fusermount -u "$HOME_DIR/.cache/doc" 2>/dev/null
    rm -rf "$HOME_DIR" 2>/dev/null; sleep 0.5; rm -rf "$HOME_DIR" 2>/dev/null
  fi
}
trap cleanup EXIT

mkdir -p "$HOME_DIR/.codex"
cat > "$HOME_DIR/.codex/config.toml" <<'EOF'
# user comment must survive
[features]
codex_hooks = false # user disabled hooks
EOF
cat > "$HOME_DIR/.codex/hooks.json" <<'EOF'
{"hooks":{"PreToolUse":[{"matcher":"third-party","hooks":[{"type":"command","command":"third-party-pre"}]}]}}
EOF
info "disposable HOME=$HOME_DIR (codex_hooks=false seeded)"

# ── 3. launch the real app headless ───────────────────────────
env HOME="$HOME_DIR" LOBSTERPULSE_HEADLESS_INSTALL=codex \
  setsid dbus-run-session -- xvfb-run -a "$PWD/$APP" \
  >"$HOME_DIR/app.log" 2>&1 &
APP_PID=$!

PORT_FILE="$HOME_DIR/.lobsterpulse/port"
PORT=""
for _ in $(seq 1 60); do
  [ -s "$PORT_FILE" ] && { PORT=$(cat "$PORT_FILE"); break; }
  kill -0 "$APP_PID" 2>/dev/null || { cat "$HOME_DIR/app.log"; fail "app exited before writing port file"; }
  sleep 0.5
done
[ -n "$PORT" ] || { cat "$HOME_DIR/app.log"; fail "no port file after 30s"; }
info "hook server up on port $PORT"

# ── 4. flag flipped false → true, comments preserved ──────────
sleep 1  # headless install runs during setup; port file proves setup done
python3 - "$HOME_DIR/.codex/config.toml" <<'EOF' || fail "config.toml assertions"
import sys, tomllib, pathlib
raw = pathlib.Path(sys.argv[1]).read_bytes()
doc = tomllib.loads(raw.decode())
assert doc["features"]["codex_hooks"] is True, f"codex_hooks not true: {doc}"
assert b"# user comment must survive" in raw, "lost top comment"
assert b"# user disabled hooks" in raw, "lost inline comment"
print("[smoke] config.toml: codex_hooks = true, comments preserved")
EOF
grep -c "lobster-pulse-hook" "$HOME_DIR/.codex/hooks.json" >/dev/null \
  || fail "hooks.json missing sidecar command"
grep -q "third-party-pre" "$HOME_DIR/.codex/hooks.json" \
  || fail "third-party hook was clobbered"
info "hooks.json: sidecar installed, third-party hook preserved"

# ── 5. real Codex hook event through the real sidecar ─────────
EVENT='{"hook_event_name":"SessionStart","sessionId":"smoke-codex-1","cwd":"/tmp"}'
SIDECAR_ERR=$(printf '%s' "$EVENT" | env HOME="$HOME_DIR" "$PWD/$SIDECAR" codex 2>&1)
RC=$?
[ "$RC" -eq 0 ] || fail "sidecar exited non-zero ($RC): $SIDECAR_ERR"
[ -n "$SIDECAR_ERR" ] && fail "sidecar reported delivery failure: $SIDECAR_ERR"

METRICS=""
for _ in $(seq 1 20); do
  METRICS=$(curl -sf "http://127.0.0.1:$((PORT + 100))/metrics" 2>/dev/null) && break
  sleep 0.5
done
[ -n "$METRICS" ] || fail "metrics endpoint unreachable on $((PORT + 100))"
echo "$METRICS" | grep -E 'lobsterpulse_provider_sessions[{]provider="codex"[}] [1-9]' \
  || { echo "$METRICS" | grep -i "codex"; fail "codex session not counted in /metrics"; }
info "metrics: codex session counted"

# ── 6. receipt ────────────────────────────────────────────────
echo "[smoke] --- config.toml after enable ---"; cat "$HOME_DIR/.codex/config.toml"
echo "[smoke] --- /metrics codex lines ---"
echo "$METRICS" | grep -i "codex"
echo "[smoke] PASS — packaged enable flips codex_hooks false→true and a real Codex hook event lands"
