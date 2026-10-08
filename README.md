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

> **注意**：上述執行檔只會在自行建置後出現；clone 本身不含執行檔。

## 安裝（目前僅提供可驗證的原始碼建置）

這個 repository 目前沒有可供安裝者下載的已發佈 bundle 或 installer。以下內容只描述
如何由固定 commit 建置並核對產物；這是目前 repository 的事實狀態，不代替 Owner 的
發行方式決策，也不建立原始碼限定的分發政策。本節不會建立 tag、release 或 installer。

建置 release webview 時必須使用 `cargo tauri build --no-bundle`。不要以
`cargo build --release` 代替，否則會跳過 Tauri 的 frontend embedding。

### Windows 11 前置需求

- Rust `1.77.2` 以上，使用 `stable-x86_64-pc-windows-msvc` toolchain。
- Visual Studio Build Tools 2022 的 **Desktop development with C++**，包含 MSVC x64/x86
  build tools 與 Windows 10/11 SDK。
- Microsoft Edge WebView2 Runtime。
- Tauri CLI `2.11.4`。`--locked` 只要求使用該 crate 發佈時的 lockfile；必須同時指定
  `--version 2.11.4` 才是固定 CLI 版本。
- Node.js 只用於執行 `npm test` 文件與版本護欄，不是這個靜態 frontend 的 build prerequisite。

可先核對既有工具；缺少工具時請依
[Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) 安裝，不要略過平台安全元件：

```powershell
rustc --version
cargo --version
cargo tauri --version
node --version
```

若尚未安裝指定 CLI，使用精確版本：

```powershell
cargo install tauri-cli --version 2.11.4 --locked
```

從 repository root 建置；不要先切換到 `src-tauri`，下列產物路徑也都以 repository root
為基準：

```powershell
cargo tauri build --no-bundle
```

Windows 產物：

- `src-tauri\target\release\lobster-pulse.exe`（主程式）
- `src-tauri\target\release\lobster-pulse-hook.exe`（hook sidecar）

只核對檔案存在與 SHA-256，不需要啟動任何程式：

```powershell
$artifacts = @(
  '.\src-tauri\target\release\lobster-pulse.exe',
  '.\src-tauri\target\release\lobster-pulse-hook.exe'
)
$artifacts | ForEach-Object {
  if (-not (Test-Path -LiteralPath $_ -PathType Leaf)) { throw "Missing artifact: $_" }
}
Get-FileHash -Algorithm SHA256 -LiteralPath $artifacts
```

目前 release workflow 沒有產生 checksum manifest；上面的 hash 是本機建置產物驗證，
不能描述成已發佈 bundle 的 checksum。

### macOS 前置需求與產物

先安裝 Xcode Command Line Tools、Rust MSVC 以外的 macOS stable toolchain，以及相同的
Tauri CLI 精確版本。以下命令均從 repository root 執行：

```bash
cargo install tauri-cli --version 2.11.4 --locked
cargo tauri build --no-bundle
test -x src-tauri/target/release/lobster-pulse
test -x src-tauri/target/release/lobster-pulse-hook
shasum -a 256 src-tauri/target/release/lobster-pulse \
  src-tauri/target/release/lobster-pulse-hook
```

### Linux 前置需求與產物

先依發行版安裝 Rust 與 Tauri v2 所列的 WebKitGTK／系統開發套件，再使用相同的 Tauri
CLI 精確版本。以下命令均從 repository root 執行：

```bash
cargo install tauri-cli --version 2.11.4 --locked
cargo tauri build --no-bundle
test -x src-tauri/target/release/lobster-pulse
test -x src-tauri/target/release/lobster-pulse-hook
sha256sum src-tauri/target/release/lobster-pulse \
  src-tauri/target/release/lobster-pulse-hook
```

主程式與 sidecar 必須保留在同一目錄。專案目前沒有 auto-update；切換到另一個 source
revision 時，應一起替換兩個由同一次 build 產生的檔案。

設定與資料位置依平台解析 home directory：app 設定為
`~/.config/lobsterpulse/config.json`，音效為 `~/.config/lobsterpulse/sounds/`，runtime
port 為 `~/.lobsterpulse/port`。

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

## 建置摘要

完整前置需求、各平台產物路徑、存在性檢查與 hash 命令都在上方「安裝」一節。所有 build
命令由 repository root 執行，本節不另外提供 installer 或 release 流程。release webview
驗證仍必須執行：

```text
cargo tauri build --no-bundle
```

這項 build 只建立二進位，不會啟動主程式或 sidecar，也不會改動真實的 CLI profile、hooks
或自動啟動設定。

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

候選變更應執行下列 admission checks；build 成功本身不代表桌面 runtime 已驗收：

- `npm test`（版本與安裝文件護欄）
- `cargo test --locked`
- `cargo tauri build --no-bundle`（release 二進位，frontend 已 embed）
- 兩個平台對應產物的存在性與 SHA-256 檢查

## 已知保留項

- `CLAUDE.md` 仍主要是 upstream 專案說明，這回合沒有一起重寫
- repository 目前沒有可下載的正式 bundle；本文件只證明 source build 路徑，不作 Owner 發行方式決策
