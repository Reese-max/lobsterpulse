# Changelog

## v0.5.5 (unreleased) · 2026-06-05 — Prometheus metric rename prep (T-0 公告)

> 📢 **DEPRECATION 公告（T+4 週切換 / 2026-07-03）**：6 條 counter-typed
> Prometheus metric 將從現名 rename 為 `_total` 結尾，對齊
> [Prometheus naming convention](https://prometheus.io/docs/practices/naming/)。
> 抓取端（scrape / recording / alert rule）、Grafana dashboard、文檔引用
> 對**現名**的所有表達式將於 T+4 週失效。本段是 5 週廣播時程的 T-0 公告
> （後續 T-1 dual-emit shim → T-2 廣播 → T-3 監控窗口 → T-4 切換 →
> T-5 post-mortem 走 R107+ owner）。
>
> 對齊 R106 spec [prometheus-counter-convention](openspec/changes/prometheus-counter-convention/)
> R-3 Requirement（廣播 4 層面 + 5 週時程）+ R106 接力清單首位
> 「廣播文檔先備齊」前置工作。**T-0 公告本身不動 code**（R106 1 輪 1 件
> 純 spec → 文檔 prep），實際 rename 留 R107+ owner follow-up。
>
> Source of truth 對照表（6 條 TYPE=counter，2026-06-05 盤點 LP_METRICS
> const + emit site + 35 個 test assertion 三層一致）：

| # | 現名 | 目標 rename 名（`_total` 結尾） | LP_METRICS row |
|---:|---|---|---:|
| 1 | `lobsterpulse_tokens_input` | `lobsterpulse_tokens_input_total` | 92 |
| 2 | `lobsterpulse_tokens_output` | `lobsterpulse_tokens_output_total` | 93 |
| 3 | `lobsterpulse_provider_tokens_input` | `lobsterpulse_provider_tokens_input_total` | 94 |
| 4 | `lobsterpulse_provider_tokens_output` | `lobsterpulse_provider_tokens_output_total` | 95 |
| 5 | `lobsterpulse_provider_failure_count` | `lobsterpulse_provider_failure_count_total` | 97 |
| 6 | `lobsterpulse_provider_session_count` | `lobsterpulse_provider_session_count_total` | 109 |

抓取端 / alert / Grafana dashboard owner 請於 **T-1 (2026-06-12)**
dual-emit shim 落地前更新對應表達式，避免 T+4 週切換日 silent break。
詳見 [`openspec/changes/prometheus-counter-convention/`](openspec/changes/prometheus-counter-convention/)
（含 design 5 週時程 + spec R-3 廣播 4 層面）。

> ℹ️ 不在本公告 scope：gauge `lobsterpulse_sessions_total` 雖用 `_total`
> 結尾（反向違規：gauge 不該 `_total`），但跟本公告 6 條 counter 缺 `_total`
> 是**不同方向**的 spec drift，留 R106+ follow-up 另案處理。

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
