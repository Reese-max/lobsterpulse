# Design: OpenAB bot 監控同步

## provider 註冊的 4 個同步點（src-tauri/src/config.rs）

新增一隻被監控的 OpenAB bot 需同步改 4 處（這也是防再漂 SOP 的核心）：

1. `default_providers()`（約 line 355-390）：加 `ProviderConfig { enabled, name, settings_path, ... }`
2. `default_provider_sounds()`（約 line 331-336）：加 `(bot_id, "xxx.mp3")`
3. `default_provider_waiting_sounds()`（約 line 342-347）：加 `(bot_id, "xxx-waiting.mp3")`
4. usage/quota poller 迴圈（line 547 `for id in ["cicx", "gitx", "giminix", "codex_bot", "openx"]`）：加 bot_id

> 行號為 2026-06-03 實測值，loop 編輯後會位移；以 symbol/文字定位為準，勿硬綁行號。

## IRISX（hermes agent）

- bot_id = `irisx_bot`（以 config-hermes.toml `[lobsterpulse]` 宣告為準，勿自創）
- name 建議 `"🤖 IRISX · OpenAB Hermes"`，enabled = true
- quota 檔：`~/.lobsterpulse/usage-irisx_bot.json`
- 音效缺檔須 fallback 到 default，不得 panic

## cicx2 ID 漂移（先查再修，勿盲改）

config-cicx2.toml 宣告 bot_id=`cicx2`，但 LobsterPulse default 用 `cicx`。先確認 CICX2 實際 POST 到 `/hook/{?}`：

- 若實際 POST `/hook/cicx2` → LobsterPulse 漏接：加 `cicx2` provider，或在 `hook_server.rs` 的 `parse_provider()` 加 alias `cicx2 → cicx`（比照既有 `"bot" → "openx"` 的 legacy alias 寫法）
- 若實際 POST `/hook/cicx`（openab 端已 normalize）→ 無漏接：僅在 docs 註記 id 對應，no-op

不要在未確認實際 POST 行為前盲改 provider id。

## 防再漂

- **SOP**：在 CONTRIBUTING.md 或 docs 加「新增 OpenAB bot」checklist，列上面 4 個同步點 + 原則「bot_id 一律以 openab `config-*.toml` 的 `[lobsterpulse] bot_id` 為準」
- **守護測試**：加一條 test 斷言「已知 enabled 的 openab bot 清單 ⊆ `default_providers()` keys」（或對 provider 清單做不變式檢查）；故意移除一個 provider → 該 test 應 fail。比照本專案既有「跨 K 不變式護欄」風格落地。

## 後端標籤對齊（bot 換後端，bot_id 不變）

有些 bot 換了後端但 bot_id 不變，導致 LobsterPulse 的 display name 過時。這類**只改 name 字串、不改 bot_id、不新增 provider**。

- **GIMINIX**：後端 gemini → `agy-acp-wrapper`（Antigravity）。config.rs `default_providers()` 的 giminix name `"🤖 GIMINIX · OpenAB Gemini"` → 改為反映 Antigravity/agy 後端。
- **致命陷阱**：config.rs 另有一個**本機 `gemini` CLI provider**（`"💻 Gemini CLI（本機）"`，`~/.gemini/settings.json`，約 line 420 + line 564 的 `which_exists("gemini")` 偵測）。那是獨立的本機 Gemini CLI，**與 GIMINIX bot 無關，絕不可動**。改標籤時只動 `giminix` 這個 OpenAB provider，勿誤改本機 `gemini`。

### 後端對照表（name 應反映的後端，來源 = openab config-*.toml 第 1 行）

| LobsterPulse provider | 實際後端 | openab config |
|---|---|---|
| cicx | Claude（claude-agent-acp） | config-cicx2.toml |
| gitx | Copilot（copilot-agent-acp） | config-copilot.toml |
| giminix | **Antigravity（agy-acp-wrapper）** | config-gemini.toml |
| codex_bot | Codex（codex-acp） | config-codex.toml |
| openx | OpenCode（opencode） | config-openx.toml |
| irisx_bot | Hermes（hermes -p irisx → gpt-5.5） | config-hermes.toml |
| grokx | Grok（hermes -p grokx） | config-copilot-native.toml |
| lpbot | Claude（claude-agent-acp，quota 監控；後端同 cicx） | config-lpbot.toml |
| mimo | MIMO（disabled，R78 T-BOT5 新增） | config-mimo.toml |

> 新增/改後端時，display name 的後端字樣一律以 openab `config-*.toml` 第 1 行「後端: X」為準。
>
> R79 spec drift 修：R78 T-BOT5/T-BOT11/T-BOT12 落地後，config.rs `r78_t_bot10_all_openab_backend_labels_match_config` 護欄測試 9 條對照齊全（cicx/gitx/giminix/codex_bot/openx/irisx_bot/grokx/lpbot/mimo），但本表先前只列 6 條（漏 grokx/lpbot/mimo）。對齊實作補完，T-BOT10 audit pass 條件成立。
