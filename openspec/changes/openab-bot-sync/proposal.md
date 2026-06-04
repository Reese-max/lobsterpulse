# Proposal: OpenAB bot 監控同步 — openclaw→hermes agent 遷移後的 provider 對齊

## Goal

把 LobsterPulse 的 OpenAB bot 監控清單與「openclaw → hermes agent」遷移後的現實對齊：補上新的 hermes agent（IRISX）、修正既有 ID 漂移（cicx2）、補缺漏（mimo），並加防再漂機制，讓每一隻 openab bot 宣告的 `[lobsterpulse] bot_id` 都真正被監控（膠囊 UI / quota / metrics 三者齊全）。

## Background

operator 已於 2026-05-13 把 openclaw 端點全面遷移為 hermes agent（Nous Research Hermes v0.14.0，WSL profile 隔離）。IRISX bot 現在後端是 `hermes -p irisx → gpt-5.5`，並在 `openab/config-hermes.toml` 宣告 `[lobsterpulse] bot_id = "irisx_bot"`、enabled=true。

但 LobsterPulse 端沒跟上：`src-tauri/src/config.rs` 的 provider 清單仍硬寫死 5 個舊 bot（cicx / gitx / giminix / codex_bot / openx），原始碼對 `irisx`/`hermes` 零提及（`grep -rni 'irisx|hermes' src-tauri/src` 全空）。結果：IRISX 事件 POST 到 `/hook/irisx_bot` → hook_server 收下，但 SessionManager 沒有對應 ProviderConfig → **事件被靜默吞掉，無膠囊 UI、無 quota、無 metrics**。

對照所有 `openab/config-*.toml` 宣告的 `[lobsterpulse] bot_id` 與 LobsterPulse 已知清單，漂移如下：

| openab bot config | 宣告 bot_id | enabled | LobsterPulse 認得 |
|---|---|---|---|
| config-hermes.toml（IRISX / hermes agent） | irisx_bot | true | ❌ 缺（核心 gap） |
| config-cicx2.toml | cicx2 | true | ⚠️ LobsterPulse 用 `cicx`，ID 對不上（待查） |
| config-mimo.toml | mimo | false | ❌ 缺 |
| config-codex / openx | codex_bot / openx | true | ✅ 對上 |
| config-gemini.toml（GIMINIX） | giminix | true | ⚠️ bot_id 對得上，但**後端標籤過時**：後端已 gemini→agy（Antigravity），標籤仍寫「OpenAB Gemini」 |
| config-copilot.toml（GITX/Copilot） | gitx | true | ✅ 對上（GROKX 已於 2026-06-04 拆出，不再撞 id）|
| config-copilot-native.toml（GROKX/hermes-grokx） | grokx（2026-06-04 由 gitx 拆出）| true | ❌ 缺 → T-BOT11 加 |
| config-lpbot.toml（LPBOT） | lpbot（2026-06-04 新增 [lobsterpulse] section）| true | ❌ 缺 → T-BOT12 加 |

> 註：2026-06-04 operator 已在 openab 端先修好兩個非 LP 範圍的前置：(1) GROKX 從撞 id 的 `gitx` 拆為獨立 `grokx`；(2) LPBOT 補上 `[lobsterpulse]` section。本 change 的 T-BOT11/T-BOT12 即對應在 LobsterPulse 加上 `grokx` / `lpbot` provider。

## Scope

### In Scope
- 加 `irisx_bot`（hermes agent / IRISX）到 default_providers + sounds + usage poller，恢復完整監控
- 釐清並修正 cicx2 ID 漂移（config 宣告 `cicx2` vs LobsterPulse 用 `cicx`）
- 補 `mimo` provider（disabled，對應 config-mimo.toml enabled=false）
- 對齊過時的後端標籤：GIMINIX 後端已 gemini→agy（Antigravity）但 LobsterPulse 仍標「OpenAB Gemini」；並做全 bot 後端標籤稽核
- 加「新增 OpenAB bot 同步 SOP」+ drift 守護測試，防未來再漂
- 更新 docs（CLAUDE.md / README）的 bot inventory

### Out of Scope
- 不把 `hermes.exe`（grok/xAI proxy sidecar，port 8318）當 bot 監控 —— 它是 passthrough proxy、非 AI agent
- 不改 openab 端 config（bot_id 宣告以 openab 為準，LobsterPulse 對齊它，非反向）
- 不重構 hook_server 事件管線，只補 provider 註冊與既有漂移修正

## Capabilities

- `openab-bot-registry` — provider 4 同步點註冊、ID alias 解析、drift 守護、
  缺檔 fallback、後端標籤對齊（涵蓋 4 同步點 / cicx2 ID drift / mimo 補 /
  grokx+lpbot 拆撞 id / drift guard 護欄 / 缺檔 fallback / 後端標籤對齊 /
  全 bot 後端稽核 / SOP 與 docs 更新）。
