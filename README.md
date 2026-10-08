# LobsterPulse / 龍蝦監控

龍蝦監控是以 `AgentPulse` 為基底改寫的自訂桌面監控器，用來追蹤 AI coding CLI 的即時狀態。這版已經改成你的品牌與工作流：

- 品牌名稱改成 `龍蝦監控 / LobsterPulse`
- 主程式與 demo 改成繁體中文介面
- 預設以 `Claude -> Codex -> Copilot -> Gemini` 為主順序
- 設定、音效、sidecar、runtime port 全部改到 `lobsterpulse`
- Windows provider 偵測改用 `where.exe`
- icon / tray / docs logo 全部換成新的龍蝦膠囊圖示

## 目前這版的重點

- 自行建置後的主程式：`src-tauri/target/release/lobster-pulse.exe`
- 自行建置後的 hook sidecar：`src-tauri/target/release/lobster-pulse-hook.exe`
- app 設定檔：Windows 為 `%APPDATA%\lobsterpulse\config.json`；Linux 為 `~/.config/lobsterpulse/config.json`
- 音效資料夾：設定目錄下的 `lobsterpulse/sounds/`
- runtime port 檔：`~/.lobsterpulse/port`

## 預設工作流

這版不是照 upstream 原封不動保留，而是直接往你的使用習慣收斂：

- `Claude Code`：主監控與主要實作流程
- `Codex CLI`：第二順位，預設有完成/等待提示音
- `GitHub Copilot CLI`：第三順位，預設有完成/等待提示音
- `Gemini CLI`：保留支援，但預設不幫它配通知音

注意：provider 預設仍然是 `未啟用`，避免第一次打開就直接改你本機 hook 設定；但音效預設與顯示順序已經換成你的工作流。

## 監控清單

目前原始碼同時監控兩條路徑，共 **13 provider**（🤖 OpenAB 9 + 💻 本機 4）。

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

啟用本機 Codex 監控後，請開啟互動式 Codex CLI，使用啟動時的 **Review hooks** 提示或輸入 `/hooks`，檢查 LobsterPulse 的 hook 命令後將它們標記為可信任。Codex 會略過尚未信任的 hook；安裝設定成功不代表監控事件已開始送出。hook 定義改變時須重新審查並信任。LobsterPulse 不會替你作出信任決定。詳見 [Codex Hooks 文件](https://learn.chatgpt.com/docs/hooks#review-and-trust-hooks)。

若啟用前 `[features].hooks` 或舊別名 `codex_hooks` 明確設為 `false`，LobsterPulse 會在 Codex 設定目錄記錄自己改動的鍵與設定檔身分；移除 Codex 監控時只把這些鍵還原為 `false`，原本由使用者設為 `true` 的鍵保持不變。重新安裝與程序中斷後會用記錄辨識已提交或待重試的設定。偵測到符號連結改指向別處或設定檔被整份替換時，會拒絕還原並保留紀錄。外部編輯器不受 LobsterPulse 的檔案鎖約束；最後一次身分檢查與檔案替換之間仍可能發生競爭，請避免在啟用或移除監控的同一瞬間改寫 `config.toml` 或 `hooks.json`。

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

**目前僅提供[從原始碼建置](#從原始碼建置-windows)。**
[GitHub Releases](https://github.com/Reese-max/lobsterpulse/releases) 尚無已發佈版本；
clone 或下載原始碼不會包含 exe。原始碼中的 app 版本是 `0.5.4`，不是已發行版本。
是否提供可下載二進位檔仍待維護者決定（[issue #6](https://github.com/Reese-max/lobsterpulse/issues/6)）。

### 從原始碼建置 Windows

先安裝 Git、[Rust stable MSVC toolchain、Microsoft C++ Build Tools（Desktop development with C++）與 WebView2 Runtime](https://v2.tauri.app/start/prerequisites/)；
Windows 11 通常已有 WebView2，缺少時依 Tauri 說明安裝。這條路徑需要開發工具鏈。
開啟新的 PowerShell 視窗，從 repo 根目錄執行：

```powershell
git clone https://github.com/Reese-max/lobsterpulse.git
cd lobsterpulse
cargo install tauri-cli --version "^2.0.0" --locked
cargo tauri build --no-bundle
Test-Path .\src-tauri\target\release\lobster-pulse.exe
Test-Path .\src-tauri\target\release\lobster-pulse-hook.exe
```

兩個 `Test-Path` 都應回傳 `True`。**必須使用 `cargo tauri build --no-bundle`**；
`cargo build --release` 不會嵌入前端，可能以開發伺服器 URL 啟動而顯示白畫面。

兩個執行檔必須留在同一資料夾。建置完成後從 repo 根目錄啟動：

```powershell
.\src-tauri\target\release\lobster-pulse.exe
```

Windows 設定檔位置是 `$env:APPDATA\lobsterpulse\config.json`；首次執行後在 tray 選單啟用所需 provider。
啟用會修改對應 CLI 的 hook 設定，請先在自己的測試環境確認其內容。

更新原始碼時先關閉程式，再執行 `git pull --ff-only` 和 `cargo tauri build --no-bundle`。
若複製產物到另一資料夾使用，請一併複製主程式與 sidecar，並保留舊的一對檔案供回復。
目前沒有自動更新或已發行版本可供下載回滾。

### 發行準備（maintainer，尚未執行）

維護者決定發行二進位檔並核准版本後，`v<app version>` tag 可觸發
[`release.yml`](.github/workflows/release.yml)。工作流程核對 Cargo、Tauri、package.json、
docs 和 tag，檢查主程式與 sidecar，建立 zip 與 SHA-256 檔，產生 **draft** Release
供人工審查。請先確認實際產物、簽章狀態和乾淨 Windows 11 的下載、啟動、sidecar 測試，
再另行決定是否發佈。CI 產物不是穩定下載頁。

目前沒有 installer、簽章或已發佈的 portable archive；Windows SmartScreen 提示仍須在實際發行前驗證並如實寫入 release notes。

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

這兩塊都已經改成你的品牌版，建置說明指向本 repo，不指向 upstream release。

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

Build CI 在 Linux、macOS、Windows 編譯並測試原始碼；它不等於已發佈安裝包。
從全新 Windows 11 下載與執行的驗收尚未完成，結果追蹤於 [issue #6](https://github.com/Reese-max/lobsterpulse/issues/6)。

## 已知保留項

- `CLAUDE.md` 仍主要是 upstream 專案說明，這回合沒有一起重寫
- 目前沒有已發佈的 GitHub Release；release workflow 僅為尚未實際發行的準備路徑
