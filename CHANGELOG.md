# Changelog

## v0.5.4 · 2026-04-17 — 完整度補齊

### 新增
- 🤖 **9 provider 雙軌架構**：5 OpenAB bot（CICX/GITX/GIMINIX/CODEX/OPENX）+ 4 本機 CLI（claude/codex/copilot/gemini）
- ⚡ **全域快捷鍵**：Ctrl+Shift+L 切換 / D 開 Bot 總覽 / E 開事件診斷
- 📊 **Prometheus `/metrics` exporter** on port+100
- 🔔 **Windows toast 系統通知** + 失敗警示紅點
- 🚀 **Task Scheduler 開機自啟**（tauri-plugin-autostart）
- ☁️ **Bot 總覽 Dashboard 雙區塊**：OpenAB + 本機 CLI
- 🔍 **事件診斷 view**：2s auto-refresh + 手動刷新 + 10 filter tabs + errors 專 tab
- 📱 **Telegram 長任務推播**（curl.exe 不依賴 reqwest）+ 測試發送按鈕
- 🖥️ **OpenAB 重啟按鈕**（Tray menu）+ 可 config 的重啟指令
- 🎨 **5 個折疊入口**：session row / Dashboard 雙區塊 / Quota bar / Chat close / 膠囊整體隱藏
- ⏱️ **Session 閾值可調**：idle/stale/remove 三個秒數
- 🦞 **Tray 左鍵單擊 toggle**（對齊 Discord/Steam 慣例）
- 🗑️ **清空所有 session** action-bar 按鈕
- 💻 **OPENX (OpenCode bot)** — 專屬 terminal SVG icon + 粉紅 #f472b6 配色 + Grid 跨全寬對稱

### 改動
- **Provider name 統一前綴**：🤖 OpenAB / 💻 本機，強制 forward migration
- **Quota 雙資料源**：runtime `provider_totals` +  OpenAB snapshot，BOT_RUNNER_KEYWORDS 按 backend 過濾避重複
- **CWD 跨平台縮短**：Linux `/home/xxx`、macOS `/Users/xxx`、Windows `C:\Users\xxx`、MSYS2 `/c/Users/xxx` 全縮 `~`
- **build 指令**：必用 `cargo tauri build --no-bundle`（純 `cargo build --release` 會 webview 白屏）

### 修復
- **Tauri setup tokio::spawn panic**：metrics server 改 std::thread + Runtime::new
- **Windows `.cmd` shim spawn**：Rust `Command::new` 只找 .exe，改 claude.cmd + cmd.exe /C fallback
- **OPENX 身份混淆**：拆 `codex_bot` / `codex` 獨立 id；`bot→openx` legacy alias
- **BOT_RUNNER_KEYWORDS null 語意**：null=skip snapshot，undefined=show all，有值=filter

### 下架
- 膠囊內 chat 輸入框（UX 不佳，使用者拍板移除）
- 位置記憶（使用者拒）

### 架構
- `~/.lobsterpulse/port` hook server port file
- `~/.lobsterpulse/usage-{bot}.json` OpenAB 寫 snapshot
- Tauri v2.10 · tauri-cli 2.10.1 · Rust 1.77+

## v0.2.2 · 前身 AgentPulse fork 基線
- Dynamic Island-style floating status indicator
- macOS/Linux/Windows 跨平台
