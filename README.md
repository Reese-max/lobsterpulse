# LobsterPulse / 龍蝦監控

龍蝦監控是以 `AgentPulse` 為基底改寫的自訂桌面監控器，用來追蹤 AI coding CLI 的即時狀態。這版已經改成你的品牌與工作流：

- 品牌名稱改成 `龍蝦監控 / LobsterPulse`
- 主程式與 demo 改成繁體中文介面
- 預設以 `Claude -> Codex -> Copilot -> Gemini` 為主順序
- 設定、音效、sidecar、runtime port 全部改到 `lobsterpulse`
- Windows provider 偵測改用 `where.exe`
- icon / tray / docs logo 全部換成新的龍蝦膠囊圖示

## 目前這版的重點

- 主程式執行檔：`src-tauri/target/release/lobster-pulse.exe`
- hook sidecar：`src-tauri/target/release/lobster-pulse-hook.exe`
- app 設定檔：`~/.config/lobsterpulse/config.json`
- 音效資料夾：`~/.config/lobsterpulse/sounds/`
- runtime port 檔：`~/.lobsterpulse/port`

## 預設工作流

這版不是照 upstream 原封不動保留，而是直接往你的使用習慣收斂：

- `Claude Code`：主監控與主要實作流程
- `Codex CLI`：第二順位，預設有完成/等待提示音
- `GitHub Copilot CLI`：第三順位，預設有完成/等待提示音
- `Gemini CLI`：保留支援，但預設不幫它配通知音

注意：provider 預設仍然是 `未啟用`，避免第一次打開就直接改你本機 hook 設定；但音效預設與顯示順序已經換成你的工作流。

## 監控清單（v5.1+）

LobsterPulse v5.1 同時監控兩條路徑，共 **13 provider**（🤖 OpenAB 9 + 💻 本機 4）。

### 🤖 OpenAB 9 bot

OpenAB process 直接 HTTP POST `/hook/{bot_id}`，bot_id 以 openab `config-*.toml` 為 source of truth：

- `cicx` → 🤖 CICX · OpenAB Claude（後端 claude-agent-acp）
- `gitx` → 🤖 GITX · OpenAB Copilot
- `giminix` → 🤖 GIMINIX · OpenAB **Antigravity**（agy-acp-wrapper；R75 T-BOT9 從 gemini 換來）
- `codex_bot` → 🤖 CODEX · OpenAB Codex（codex-acp）
- `openx` → 🤖 OPENX · OpenAB OpenCode（opencode）
- `irisx_bot` → 🤖 IRISX · OpenAB **Hermes**（hermes -p irisx → gpt-5.5；openclaw→hermes 遷移，R70 T-BOT1）
- `grokx` → 🤖 GROKX · OpenAB Grok（hermes -p grokx；R78 T-BOT11 從 gitx 拆獨立 id）
- `lpbot` → 🤖 LPBOT · OpenAB Claude（quota 監控；R78 T-BOT12 納管）
- `mimo` → 🤖 MIMO · OpenAB MIMO（R78 T-BOT5 新增，預設 disabled）

### 💻 本機 CLI 4

CLI 呼叫 `lobster-pulse-hook.exe` sidecar，settings path 為各 CLI 標準位置：

- `claude` → 💻 Claude Code（本機） → `~/.claude/settings.json`
- `codex` → 💻 Codex CLI（本機） → `~/.codex/hooks.json`
- `copilot` → 💻 Copilot CLI（本機） → `~/.copilot/config.json`
- `gemini` → 💻 Gemini CLI（本機） → `~/.gemini/settings.json`

預設全部 `enabled: false`（避免第一次開啟就改你本機 hook 設定）；要監控時從 tray 9 項 menu 開啟，會自動寫對應 CLI 的 hook config。

Source of truth：`src-tauri/src/config.rs::default_providers()`（line 356-449），跨 4 同步點（providers / sounds / waiting_sounds / usage poller）必須對齊；R67 護欄測試守住一致性，跨點新增 provider 會被 CI 1 秒抓。

## Prometheus `/metrics` endpoint

LobsterPulse 內 41 條 Prometheus metric 透過 port+100 exporter emit
（預設 `http://127.0.0.1:19380/metrics`）。完整契約見
[`openspec/changes/otel-provider-metrics-contract/`](openspec/changes/otel-provider-metrics-contract/)
（41 條 7 段組織 + R102/R103 護衛 chain 守住 set 與 emit 對齊）。

> ⚠️ **DEPRECATION 公告 (2026-06-05)**：6 條 counter-typed metric 將於
> **2026-07-03** rename 為 `_total` 結尾（對齊 Prometheus naming convention）。
> 抓取端 / alert / Grafana dashboard 對**現名**的引用將失效。完整對照表見
> [CHANGELOG.md](CHANGELOG.md) v0.5.5 段，5 週廣播時程見
> [`openspec/changes/prometheus-counter-convention/design.md`](openspec/changes/prometheus-counter-convention/design.md)。

## 主要檔案

- [src/index.html](src/index.html)
- [src/main.js](src/main.js)
- [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json)
- [src-tauri/src/config.rs](src-tauri/src/config.rs)
- [src-tauri/src/hooks_configurator.rs](src-tauri/src/hooks_configurator.rs)
- [src-tauri/src/bin/lobster-pulse-hook.rs](src-tauri/src/bin/lobster-pulse-hook.rs)
- [docs/index.html](docs/index.html)
- [docs/demo-app/index.html](docs/demo-app/index.html)
- [scripts/generate_brand_assets.ps1](scripts/generate_brand_assets.ps1)
- [scripts/bootstrap_git.ps1](scripts/bootstrap_git.ps1)
- [REPO_SETUP.md](REPO_SETUP.md)

## 安裝 / 執行

> **目前沒有已發佈的 GitHub Release**——clone 這個 repo 不會拿到 exe。
> 可用的安裝路徑是「自行 build」；第一個 tag（`v*`）推送後，
> `.github/workflows/release.yml` 會自動產出各平台 zip 到 Releases。

### 自行 build（唯一目前可用的安裝路徑）

```powershell
cargo install tauri-cli --locked
cargo tauri build --no-bundle
```

> ⚠️ **必用 `cargo tauri build`，不可純 `cargo build --release`**。
> 純 cargo build --release 會跳過 frontend embed，release webview fallback
> 到 devUrl（localhost:1420）→ 啟動白屏 / "Could not connect to localhost"。
> 對齊 `CLAUDE.md`「Build SOP（重要）」段 + `build.sh` L10-12 註解。

產物在 `src-tauri/target/release/`：

```powershell
.\src-tauri\target\release\lobster-pulse.exe
```

主程式與 sidecar 要放在同一層，因為主程式會找相鄰的 `lobster-pulse-hook.exe`。

### 發佈 Release（maintainer）

推送 `v*` tag 觸發 `release.yml` → 建出 Windows/macOS/Linux 各平台的 zip
（含 `lobster-pulse` + `lobster-pulse-hook` 兩個 binary）→ 建立 **draft** Release，
手動 publish 後即為對外下載頁。也支援 `workflow_dispatch` 手動觸發。

要 `.msi` / `.deb` / `.AppImage` installer 時改用 `cargo tauri build`（不打 `--no-bundle`）。

## 品牌資產

repo 內附了一個可以重生品牌圖示的腳本：

```powershell
.\scripts\generate_brand_assets.ps1
```

它現在會一起更新：

- `src-tauri/icons/32x32.png`
- `src-tauri/icons/64x64.png`
- `src-tauri/icons/128x128.png`
- `src-tauri/icons/128x128@2x.png`
- `src-tauri/icons/icon.png`
- `src-tauri/icons/icon.ico`
- `src-tauri/icons/icon.icns`
- `src-tauri/icons/ios/*`
- `src-tauri/icons/android/*`
- `src-tauri/icons/Square*.png`
- `docs/brand-logo.png`

## docs / demo

- landing page：`docs/index.html`
- interactive demo：`docs/demo-app/`

這兩塊都已經改成你的品牌版，不再指回 upstream 的 GitHub release、logo 或 repo API。

## git 結構

這份 repo 已經整理成適合你自己長期維護的形態：

- upstream remote：保留原始 `AgentPulse`
- `lobsterpulse/main`：你的品牌主線
- `feature/*`：日常功能分支

如果你之後要掛自己的 GitHub repo，可以用：

```powershell
.\scripts\bootstrap_git.ps1 -NewOriginUrl https://github.com/<you>/lobsterpulse.git
```

詳細說明在 [REPO_SETUP.md](REPO_SETUP.md)。

## 驗證狀態

這版目前已驗證過：

- `cargo check`
- `cargo tauri build --no-bundle`（release 二進位，frontend 已 embed）
- `lobster-pulse.exe` 可成功啟動

## 已知保留項

- `CLAUDE.md` 仍主要是 upstream 專案說明，這回合沒有一起重寫
- 目前沒有已發佈的 GitHub Release；首個 `v*` tag 會觸發 release.yml 產出各平台 zip（draft → 手動 publish）
