# LobsterPulse 龍蝦監控 — project context for Claude Code

> **本專案 = AgentPulse fork + OpenAB 整合**。底部是 AgentPulse 原始文件供架構參考；頂部這一段是 LobsterPulse v5.1 實際差異，優先於下面。

## LobsterPulse 差異總覽（v5.1）

**本質**：桌面膠囊指示器，同時監控**兩條路徑**——
1. **本機 CLI**（直接 hook）：`claude` / `codex` / `copilot` / `gemini`，CLI 呼叫 `lobster-pulse-hook.exe` sidecar
2. **OpenAB bot**（push 事件）：`cicx` / `gitx` / `giminix` / `codex_bot` / `openx`，OpenAB process 直接 HTTP POST `/hook/{bot_id}`

共 9 provider（🤖 OpenAB 5 + 💻 本機 4）。

## 關鍵端點

| Endpoint | Port | 用途 |
|---|---|---|
| `POST /hook/{provider}` | 19280-19289 | Hook server 收 event |
| `GET /metrics` | `port+100`（預設 19380） | Prometheus exporter（`lobsterpulse_sessions_total`、`_provider_sessions{provider="..."}`、`_tokens_input|output`）|
| `~/.lobsterpulse/port` | — | 主 port 檔，sidecar 讀這個 |
| `~/.lobsterpulse/usage-{bot}.json` | — | OpenAB 寫的 quota snapshot（6 個：cicx/gitx/giminix/codex_bot/openx + legacy bot）|

## 召喚入口（5 條）

1. Tray icon 左鍵單擊 → toggle show/hide（Windows 慣例）
2. `Ctrl+Shift+L` 全域快捷鍵 → toggle
3. `Ctrl+Shift+D` → 直接開 Bot 總覽 view
4. `Ctrl+Shift+E` → 直接開事件診斷 view
5. Tray 右鍵 → 9 項 menu

## Build SOP（重要）

**必須** `cargo tauri build --no-bundle`，不可純 `cargo build --release`（Tauri v2 release webview 會 fallback 到 devUrl 白屏）。前置 `cargo install tauri-cli --locked`。

釋放 exe lock：`python -c "import os; os.replace('exe-path', 'exe-path.bak')"`（cmd.exe del 在 MSYS2 下無效）。

## Plugin 清單（Cargo.toml 實際）

- `tauri-plugin-autostart` — Windows Task Scheduler AtLogon
- `tauri-plugin-notification` — Windows toast
- `tauri-plugin-single-instance` — 防雙啟動
- `tauri-plugin-global-shortcut` — Ctrl+Shift+L/D/E
- `tauri-plugin-log` — dev 模式 log

## 5 個視圖

1. **膠囊**（常駐 300×46）：active provider icons + project_name + state + time + count + 失敗紅點（近 10 min PostToolUseFailure 數）
2. **展開面板**：filter-bar / session list (compact/展開) / chat-bar (claude_chat_quick) / quota bar (runtime totals + OpenAB snapshot 去重)
3. **Bot 總覽**：☁️ OpenAB Bot 5 卡 + 💻 本機 CLI 4 卡（每卡 quota 按 `BOT_RUNNER_KEYWORDS` 過濾只顯示自己 backend）
4. **事件診斷**：11 filter tabs（全部/❌失敗/5 OpenAB/4 本機動態隱藏）+ 2s auto-refresh
5. **設定**：助手 / 音效 / 外觀 三 tab

## 關鍵設計決策

- **Session 狀態本地累計**：`SessionManager.provider_totals` 在 handle_event 裡累加（TokenUpdate 取 max、SessionStart/PostToolUseFailure 各 ++）。Quota 不依賴外部 snapshot。
- **OPENX legacy alias**：OpenAB `BackendType::Other` 寫 `usage-bot.json`，hook_server 的 `parse_provider("bot") → "openx"` 自動 rewrite。
- **Metrics server 獨立 runtime**：`std::thread::spawn` + `tokio::runtime::Runtime::new()`，**不能** tokio::spawn（setup rt 已 `std::mem::forget`）。
- **Forward migration 強制刷新 name**：`load_config` 用 `.and_modify(|ex| ex.name = default.name)` 覆寫 name 但保留 enabled/settings_path。
- **Tray 左鍵 toggle**：`show_menu_on_left_click(false) + on_tray_icon_event` 接 `MouseButton::Left + ButtonState::Up`；叫回來時自動置中避膠囊跑出螢幕。

## 典型問題與 SOP

- **找不到 tray icon**：Win11 摺進「^」→ `ms-settings:taskbar` → 釘出來
- **設定頁兩個同名**：tray registry 殭屍 → 清 `HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\TrayNotify\IconStreams` + restart explorer
- **webview 白屏**：用 `cargo tauri build --no-bundle`，不要純 `cargo build --release`
- **Rebuild exe lock**：python os.replace rename .bak 再 build

---

# AgentPulse upstream（以下為原始 fork 文件，僅供架構參考）

> ⚠️ **READER NOTE**：下方為 fork 自 AgentPulse 的原始 CLAUDE.md，保留做架構參考用。**以頂部 LobsterPulse v5.1 章節為準**，下方凡與頂部衝突（例如 config 路徑 `~/.config/agentpulse/` vs LobsterPulse 的 `AppData\Roaming\lobsterpulse\`、port file `~/.agentpulse/port` vs `~/.lobsterpulse/port`、provider 數量、migration 流程、integration 結構等），一律**以頂部為準**。下方文字**不保證同步**，僅說明 Tauri/Rust/hook 基礎架構設計脈絡。

Dynamic Island-style floating status indicator for AI coding CLIs (Claude Code,
Gemini CLI, Codex CLI, GitHub Copilot CLI). Tauri v2 cross-platform fork of the
macOS-only [tzangms/ClaudePulse](https://github.com/tzangms/ClaudePulse).

## Stack

- **Tauri v2** (Rust backend + HTML/CSS/JS webview)
- **rodio** for native audio (avoids browser CSP issues)
- **tokio** TCP server with raw HTTP parsing (receives hook events)
- **tauri-plugin-single-instance** — second launch focuses the running window
  instead of spawning a dead tray icon
- Frontend is **embedded into the binary at build time** (not loaded from disk in release)
- Linux uses `webkit2gtk-4.1` — expect ghosting / transparent-window quirks on X11

## File layout

```
src-tauri/src/
  main.rs                   # thin entrypoint
  lib.rs                    # Tauri commands, tray menu, cursor polling, event wiring
  config.rs                 # AppConfig (theme/sounds/providers), provider defaults
  session.rs                # state machine + SessionTransition (Completed / StartedWaiting)
  hook_event.rs             # RawHookEvent normalizer (field aliases across CLIs)
  hook_server.rs            # tokio HTTP listener, provider routing, event-name mapping
  hooks_configurator.rs     # per-provider hook install/remove — writes sidecar invocations
  bin/
    agent-pulse-hook.rs     # standalone sidecar binary CLIs invoke via hook config
src/
  index.html  main.js  styles.css   # webview frontend (embedded at build time)
sounds/                     # 8 bundled TTS clips: {provider}.mp3 + {provider}-waiting.mp3
docs/                       # GitHub Pages landing site
  index.html  styles.css
  demo-app/                 # in-iframe interactive AgentPulse with a mock Tauri shim
assets/                     # screenshots + demo.gif/mp4 referenced by README + landing
.github/workflows/
  build.yml                 # push-to-main build check on all 3 OSes (--no-bundle)
  release.yml               # tag-triggered release (4 zips: linux/macos-arm64/macos-x64/windows)
```

## Dev workflow — choose the right script

| Script | When | Command |
|---|---|---|
| `./watch.sh` | active frontend iteration (HTML/CSS/JS changes) | `cargo tauri dev` — reload window to see changes |
| `./dev.sh` | Rust changes, debug build | `cargo build` (debug) |
| `./dev.sh release` | Rust changes, release test | `cargo tauri build --no-bundle` |
| `./build.sh` | official release + installer bundles | `cargo tauri build` |
| `./reload.sh` | restart existing binary (no rebuild) | just kills + relaunches |

**Critical**: release builds **must** use `cargo tauri build`. Plain
`cargo build --release` skips frontend embedding — the webview falls back to
`devUrl` (localhost:1420) and shows "Could not connect to localhost". Same
caveat applies to `cargo build` in debug mode: the webview expects a vite dev
server unless you use `cargo tauri dev`.

Frontend files are embedded at build time. Any `src/*` change needs a rebuild
unless you're in `watch.sh` mode.

All scripts use `pkill -9 -x agent-pulse` (exact match) — earlier versions used
`-f "agent-pulse"` which could match `agent-pulse-hook` or the invoking shell.

## Hook architecture — sidecar binary, not bash

Each enabled CLI writes a hook config that invokes `agent-pulse-hook <provider>`
as a plain executable. The sidecar (`src-tauri/src/bin/agent-pulse-hook.rs`)
reads the event JSON from stdin, looks up the server port from
`~/.agentpulse/port`, and POSTs to `http://localhost:{port}/hook/{provider}`.

Command string generated by `hook_cmd(provider_id)` in `hooks_configurator.rs`:

```
"/absolute/path/to/agent-pulse-hook" <provider_id>
```

**Why a sidecar instead of an inline curl one-liner?** v0.1 used
`curl -sf -d "$(cat)" http://localhost:$(cat ~/.agentpulse/port)/hook/...`
which only works where bash-style substitutions work. On Windows, each CLI
picks a different shell: Claude Code has an opt-in `"shell": "powershell"`
field, Gemini CLI hardcodes `powershell.exe -NoProfile -Command`, Copilot CLI
takes parallel `bash` and `powershell` fields, and Codex CLI disables hooks on
Windows entirely. Maintaining four quotation dialects of the same command is
fragile; a native binary invocation works the same on every shell.

### Event-name normalization (hook_server.rs)

- **Claude Code**: already PascalCase — passes through
- **Gemini CLI**: `BeforeAgent` → `SessionStart`, **`AfterAgent` → `Stop`** (NOT `SessionEnd` — that removes the session from UI)
- **Codex**: kebab-case → PascalCase (`user-prompt-submit` → `UserPromptSubmit`)
- **Copilot**: similar variations

### Field aliasing (hook_event.rs)

Different CLIs use different field names for the same thing:
- `session_id` / `sessionId` / `session` → normalized to `session_id`
- `hook_event_name` / `hookEventName` / `event` → normalized
- Missing `session_id` → defaulted to `{provider}-default`

### Hook install formats (hooks_configurator.rs)

- **Claude** — `~/.claude/settings.json`: `hooks: {EventName: [{hooks: [{type, command}]}]}`
- **Gemini** — `~/.gemini/settings.json`: same shape as Claude, different event names
- **Codex** — `~/.codex/hooks.json` + enables `codex_hooks = true` in `~/.codex/config.toml`
- **Copilot** — `~/.copilot/config.json`: uses `bash` field (not `command`)

Install process **auto-removes** any existing AgentPulse hooks before writing
new ones (identified by `agentpulse` substring in the command/bash field).

All providers default to `enabled: false`. User explicitly toggles each one
on — that flips the config *and* writes the hook. Previously Claude defaulted
to `enabled: true` but the install flow didn't fire at startup, leaving the
checkbox "on" with no actual hook installed.

## Config & data locations

- App config: `~/.config/agentpulse/config.json`
- Sound pack: `~/.config/agentpulse/sounds/` (seeded from `sounds/` on **every
  launch** via `include_bytes!` — idempotent, so existing installs pick up
  newly bundled defaults automatically)
- Runtime port file: `~/.agentpulse/port` (sidecar reads this to find the listener)

## Session state machine (session.rs)

States: `Idle` / `Working` / `WaitingForUser` / `Stale`

- **Working** — actively receiving events
- **WaitingForUser** — notification/tool-permission event seen
- **Idle** — no events for 30 s
- **Stale** — no events for 10 min
- Removed from UI after 30 min of silence

`SessionManager::handle_event` returns a `SessionTransition` enum instead of a
bool. Two transitions trigger frontend events:

- `Working → Idle` → emits `task-completed` with provider id → JS plays
  `provider_sounds[provider]`
- anything → `WaitingForUser` (first entry) → emits `task-waiting` → JS plays
  `provider_waiting_sounds[provider]`

## Theme system (styles.css)

- CSS custom properties with `[data-theme="light"]` overrides
- `--found-color`, `--waiting-color` are theme-adaptive
- `color-mix()` for derived colors; `box-shadow` for glow accents
- **Keep transitions rare and short** — long CSS transitions/animations cause
  ghosting on X11 transparent windows. The capsule-collapse bounce is a brief
  transform-only keyframe (260 ms) and seems to stay clean, but anything
  persistent (theme-switch crossfade, hover tint) has been removed.
- Accent color picker uses a `--dot` per-button CSS var + glow via box-shadow

## Smart re-render (main.js)

DOM is rebuilt only when a `structureKey` (active session ID, list of sessions,
etc.) changes. Timers update in place to avoid destroying hover state — this was
the fix for "X button disappearing during hover".

## Sound system

External MP3/WAV/OGG in `~/.config/agentpulse/sounds/`. Each provider has two
independent sounds:

- `appearance.provider_sounds[provider]` — played on `task-completed`
- `appearance.provider_waiting_sounds[provider]` — played on `task-waiting`

The empty-string `""` would be treated as falsy and auto-reset, so **`__none__`
sentinel** means "user explicitly chose no sound".

Bundled defaults ship 8 TTS clips (Taiwanese voice `zh-TW-HsiaoChenNeural` /
曉臻): `{provider}.mp3` + `{provider}-waiting.mp3`. Auto-matching on first
launch matches filename prefix (e.g. `claude.mp3` → claude completion,
`claude-waiting.mp3` → claude waiting).

## Single instance

`tauri-plugin-single-instance` catches a second launch and forwards it to the
running instance (`window.show() + set_focus()`), then exits. Fixes the prior
bug where a duplicate launch left a ghost tray icon whose hook server had
refused the port but whose UI still came up.

## Cross-platform status

Previously Linux-first. Major fixes landed in v0.2:

- **Sidecar binary** — resolved the biggest Windows blocker (see Hook
  architecture). Every CLI now invokes a plain executable, not a bash one-liner.
- **Single-instance plugin** — no more ghost tray icons on duplicate launch.
- **Windows DWM border** — `"shadow": false` in `tauri.conf.json` removes the
  halo/ghost border. An earlier attempt at `DWMWA_NCRENDERING_POLICY=DISABLED`
  backfired into an XP-style frame and was reverted.
- **CI** — `release.yml` produces four zips per tag: `linux`, `macos-arm64`
  (macos-latest / Apple Silicon), `macos-x64` (macos-13 / Intel), `windows`.

Still unresolved:

1. **`which` subprocess** in `config.rs::which_exists` — Windows doesn't have
   `which`. Impact: provider auto-detection reports false for some CLIs on
   Windows. Fix: use the `which` crate.
2. **Provider settings paths** — `~/.claude/`, `~/.gemini/` etc. are assumed,
   but macOS CLIs may use `~/Library/Application Support/`. Needs
   per-platform verification.
3. **Tauri bundle packaging of the sidecar** — `cargo tauri build` (full
   installer) doesn't yet know about `agent-pulse-hook`. For now v0.2
   ships as zip only (both binaries at the same folder level). `externalBin`
   + runtime path resolution is a TODO before shipping .msi/.deb/.dmg
   installers.
4. **No code signing** — macOS users need `xattr -cr` to bypass Gatekeeper;
   Windows users see SmartScreen warning. Apple Developer cert ($99/yr) is
   the real fix — intentionally deferred.

CI uses **ubuntu-22.04** (not 24.04) for Linux artifacts — glibc back-compat.
`cargo-binstall` pulls prebuilt Tauri CLI binaries (dropped Windows install
from ~11 min to ~45 s).

## GitHub Pages landing site (docs/)

`docs/index.html` is served as a single-page marketing site at
`https://yazelin.github.io/AgentPulse/` (configured via repo Settings → Pages
→ Deploy from branch → main / `/docs`).

The hero embeds a real interactive AgentPulse instance in an iframe at
`docs/demo-app/`, powered by `docs/demo-app/mock-tauri.js` — a shim that routes
`window.__TAURI_INTERNALS__.invoke()` calls to in-memory handlers. The real
`src/main.js` is used verbatim. A small timer cycles three fake sessions
through Working / Idle / WaitingForUser so the UI has life before the visitor
interacts.

Download buttons on the landing page auto-fill their hrefs on load by calling
the GitHub releases API; failure falls back to the generic `/releases/latest`
page. Per-OS buttons (Linux / macOS arm64 / macOS Intel / Windows) with an
"Intel Mac?" link in the secondary CTA row.

## Removed features (don't re-add)

- **Click-to-focus** — tried xdotool getactivewindow, /proc walk, `$WINDOWID`,
  `X-Window-Id` header. All failed because `gnome-terminal-server` uses one
  PID for all windows. Original Swift version doesn't have this either.
- **Rust `bounce_window`** — the window-position jitter on collapse moved to a
  pure CSS `@keyframes capsuleCollapseBounce` animation. Transform-only so it
  stays on the GPU.
- **Landing-page iframe drag** — making the capsule inside the iframe draggable
  produced a long tail of cross-frame edge cases (screenX/movementX DPR
  inconsistencies, postMessage async races against fast click-release, overlay
  pointer-capture confusion). Pulled for now. A clean fix is a Shadow DOM
  refactor — inline the demo app into the landing page instead of iframing it,
  so everything shares one document/event loop.

## Useful commands

```bash
gh run list --limit 3                       # recent CI runs
gh run watch <id>                           # live CI log
gh release list --limit 3                   # releases
cat ~/.config/agentpulse/config.json        # inspect saved state

# Exercise the sidecar directly (replace path with your build output)
echo '{"hook_event_name":"UserPromptSubmit","session_id":"test","cwd":"/tmp"}' \
  | ./src-tauri/target/release/agent-pulse-hook claude

# Raw HTTP smoke test (same thing the sidecar does internally)
curl -X POST localhost:$(cat ~/.agentpulse/port)/hook/claude \
  -H 'Content-Type: application/json' \
  -d '{"hook_event_name":"UserPromptSubmit","session_id":"test","cwd":"/tmp"}'
```
