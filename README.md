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

> **注意**：上述執行檔**只會在你自行建置後出現**，clone 本身不含任何執行檔
> （目前為 `SOURCE_ONLY` 路徑，詳見下方「安裝與下載」）。

## 安裝與下載 (Installation / Download)

> ⚠️ **Issue #6 稽核狀態**：截至目前為止這個 repo **尚未發行任何 GitHub Release**
> （沒有 tag、沒有 release）。因此所有安裝路徑都是「可重現的原始碼重建」；任何
> 指向已發行二進位下載的連結在發行前都不應假設存在。

### 當前真實的安裝路徑：從原始碼建置

LobsterPulse 目前以「可重現的原始碼建置」為正式安裝路徑（Owner 決策：`SOURCE_ONLY`
直到 Owner 發行可下載 bundle）。使用者需要先建置 Tauri 工具鏈，之後每個版本都能完整重現。

前置（Windows 11 / macOS / Linux 通用）：

```powershell
# 鎖定 Tauri CLI 版本，避免 Breaking change
cargo install tauri-cli --locked

# 建置：只產出兩個執行檔（無 installer）
cd src-tauri
cargo tauri build --no-bundle
```

產出：
- `src-tauri/target/release/lobster-pulse.exe`（主程式）
- `src-tauri/target/release/lobster-pulse-hook.exe`（hook sidecar）

### 啟動與兩個檔案必須同層

主程式會以相鄰路徑尋找 `lobster-pulse-hook.exe`，建置後兩個檔案放在同一目錄即可啟動：

```powershell
.\src-tauri\target\release\lobster-pulse.exe
```

### 設定與資料

- 設定檔：`~/.config/lobsterpulse/config.json`（Windows：`%APPDATA%\lobsterpulse\config.json`）
- 音效資料夾：`~/.config/lobsterpulse/sounds/`
- runtime port 檔：`~/.lobsterpulse/port`

### 升級與回滾 (Upgrade / Rollback)

- 升級：關閉程式後重新建置（或日後下載新版本），**同時**換掉主程式與 sidecar 兩個檔案。
- 回滾：目前沒有 auto-update 也沒有已發佈版本可下載。保留上一組兩個執行檔
  （建議放在以版本命名的資料夾，如 `lobster-pulse-v0.5.4/`），出問題時把兩個檔案
  一起換回即可。

### 如何產出一個 release（發行者注意）

本專案使用 `.github/workflows/release.yml` 產出 release：
1. 與 Owner 確認版本（以 `CHANGELOG.md` 的 `## v0.5.x` 段為準）。
2. 標記 tag：`git tag v0.5.4 && git push origin v0.5.4`
3. workflow 會在 `v*` tag 觸發，產出 Linux / macOS-arm64 / macOS-x64 / Windows 四套 zip，
   每套包含主程式 + sidecar，並建立 draft release 與 SHA-256 checksum。
4. Owner 審查後手動 publish（本 Issue 不執行 publish / sign / deploy）。

> ℹ️ **SmartScreen / 簽章**：預設發行的是**未簽章**的可攜式 bundle；Windows SmartScreen
> 可能會顯示警告。若日後需要簽章，請改為發行簽章的 installer，並在 Release notes 中說明。
> 請勿提供繞過平台安全（關閉 SmartScreen、修改執行原則）的指令。

### Owner 決策欄位（Issue #6）

在 `DISTRIBUTED_BINARY` 與 `SOURCE_ONLY` 之間由 Owner 記錄決定：
- 當前實踐路徑：`SOURCE_ONLY`（以可重現建置為正式分發，直至 Owner 發行可下載的二進位 bundle）。
- Owner 決策：`<OWNER DECISION: DISTRIBUTED_BINARY | SOURCE_ONLY>`（由 Owner 記錄於 PR / issue 更新）。

### 聲明：本 Issue 不需要

本 Issue 不要求、也不引入：自動更新服務、套件管理器（Homebrew / AUR 等）、遥測（telemetry）、
或付費 code signing。

## 預設工作流

這版不是照 upstream 原封不動保留，而是直接往你的使用習慣收斂：

- `Claude Code`：主監控與主要實作流程
- `Codex CLI`：第二順位，預設有完成/等待提示音
- `GitHub Copilot CLI`：第三順位，預設有完成/等待提示音
- `Gemini CLI`：保留支援，但預設不幫它配通知音

注意：provider 預設仍然是 `未啟用`，避免第一次打開就直接改你本機 hook 設定；但音效預設與顯示順序已經換成你的工作流。

## 監控清單（v5.1+）

> ℹ️「v5.1+」指上方監控清單的列表版本（與 provider 總數），**不是 App 主版本**。
> App 主版本以 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`
> 為準（目前 `v0.5.4`），三者已對齊，護欄見 `test/version-consistency.test.js`。

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

## 建置與啟動

從原始碼重建後，兩個執行檔（`lobster-pulse.exe` / `lobster-pulse-hook.exe`）必須在同一層，
主程式會以相鄰路徑尋找 sidecar：

```powershell
.\src-tauri\target\release\lobster-pulse.exe
```

> ⚠️ **重新建置必須用 `cargo tauri build`（或 `cargo tauri build --no-bundle`），
> 不可純 `cargo build --release`**。**
> 純 `cargo build --release` 會跳過 frontend embed，release webview fallback
> 到 devUrl（localhost:1420）→ 啟動白屏 / "Could not connect to localhost"。
> 對齊 `CLAUDE.md`「Build SOP（重要）」段 + `build.sh` L10-12 註解。

兩種變體：

- **快速驗證**（只要 `.exe`，不打 installer）：

  ```powershell
  cd .\src-tauri
  cargo tauri build --no-bundle
  ```

  前置：`cargo install tauri-cli --locked`（鎖版避免 Tauri CLI breaking change）。

- **完整 installer**（要 `.msi` / `.deb` / `.AppImage` 等）：

  ```powershell
  cargo install tauri-cli --locked
  cargo tauri build
  ```

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
- README / docs 已記錄可重現的安裝路徑（`SOURCE_ONLY`，詳見「安裝與下載」）；正式發佈位置待 Owner 發行 GitHub Release 後取代此說明
