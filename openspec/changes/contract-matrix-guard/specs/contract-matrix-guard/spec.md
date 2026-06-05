# Spec: Provider Contract Matrix Guard (13×3)

> 對應 change: `contract-matrix-guard` (見 `openspec/changes/contract-matrix-guard/`)
> 對應 code-level 護衛: `src-tauri/src/config.rs` `provider_contract_matrix_tests` module
> 對應 4 同步點: `default_providers` (config.rs:370) / `default_provider_sounds`
> (config.rs:329) / `default_provider_waiting_sounds` (config.rs:351) /
> `OPENAB_BOT_IDS` (lib.rs:39)
> 護衛 chain: 16 (provider_registration_guard) — 跟 R67 (config.rs:909) 並存, 不擴張 chain

## ADDED Requirements

### Requirement: R-1 — CONTRACT const is the single source of truth for the 13×3 matrix

LobsterPulse 必須維持 `CONTRACT: &[(&str, &str, bool, &str, &str)]` 矩陣內含 13 row,
每 row 5 個欄位 (id / name prefix / enabled_default / sound_file / waiting_sound_file)。
13 row = 4 本機 CLI + 9 OpenAB bot 對齊 R78 spec 13 provider 集合。
改任一 row 必須同步更新 4 同步點本體, 反之亦然。

#### Scenario: CONTRACT contains exactly 13 entries

`CONTRACT.len() == 13` 必須成立, 反映 4 本機 CLI + 9 OpenAB bot 完整集合。
漏 provider (13 → 12) 或加新 provider 未走 MISSION.md 納入標準 (13 → 14) 都 fail 此 scenario。

#### Scenario: 13 row IDs match the 4 sync points without overlap or omission

13 row 的 `id` 欄位集合必須等於 `default_providers().keys()` 集合 (4 同步點之一),
漏 1 個或多 1 個都 fail。

### Requirement: R-2 — name prefix (🤖 OpenAB / 💻 本機 CLI) is preserved per provider

每個 provider 的 `name` 必須以對應 prefix + 空白開頭:
- OpenAB bot (9 row) → `🤖 `
- 本機 CLI (4 row) → `💻 `

#### Scenario: every row name starts with the expected prefix

對 13 row 逐一檢查 `p.name.starts_with(&format!("{prefix} "))`, 任一 fail 報
`[name prefix] "{id}" name "{p.name}" 缺 "{prefix} " 前綴`。

### Requirement: R-3 — enabled_default matches the contract for every provider

每個 provider 的 `enabled` 預設值必須對齊 CONTRACT 內對應 row:
- 8 OpenAB bot (cicx / gitx / giminix / codex_bot / openx / irisx_bot / grokx / lpbot) → `true`
- 1 OpenAB bot (mimo) → `false` (R78 T-BOT5 決策)
- 4 本機 CLI (claude / codex / copilot / gemini) → `false` (user 手動從 tray 開)

#### Scenario: mimo defaults to disabled

`mimo.enabled == false` 必須成立 (R78 T-BOT5 環境 disabled 處理)。
若被誤關成 `true` 環境就緒時 mimo 進 K0 量化但無 bot 拉事件 = 監控盲區, fail 此 scenario。

#### Scenario: the 8 OpenAB bots default to enabled

cicx / gitx / giminix / codex_bot / openx / irisx_bot / grokx / lpbot 8 row 必須
`enabled == true`, 對齊 K0 即時性 (enabled 才能進 K0 量化)。

### Requirement: R-4 — sound file mapping is exactly equal to the contract for every provider

每個 provider 的 `default_provider_sounds` 與 `default_provider_waiting_sounds` 內
key/value 必須對齊 CONTRACT:
- sound_file 非空 → `sounds[id] == sound_file` (filename 完全相等)
- sound_file 空 → `!sounds.contains_key(id)` (本機 CLI 預期無 default sound)
- waiting_sound_file 同上

#### Scenario: cicx sound file is exactly "cicx.mp3"

`default_provider_sounds().get("cicx") == Some("cicx.mp3")` 必須成立。
若改成 `"cicx.MP3"` / `"cicx-bot.mp3"` / 任何其他 filename, fail 此 scenario 並報
`[sound file] "cicx" 預期 "cicx.mp3", 觀察 Some("...")`。

#### Scenario: claude has no default sound entry

`!default_provider_sounds().contains_key("claude")` 必須成立 (留 user 自訂, 對齊 R67 spec)。
若被誤加 default sound, fail 此 scenario 並報
`[sound absent] "claude" 本機 CLI 不該有 default sound entry`。

### Requirement: R-5 — OPENAB_BOT_IDS membership matches the 🤖 prefix (cross-attribute)

每個 row 的 `prefix == "🤖"` ↔ `id ∈ OPENAB_BOT_IDS` 必須雙向對齊:
- 9 row 🤖 prefix 必須在 OPENAB_BOT_IDS 集合內
- 4 row 💻 prefix 必須不在 OPENAB_BOT_IDS 集合內

#### Scenario: every 🤖 provider is in OPENAB_BOT_IDS and every 💻 provider is not

對 13 row 逐一驗, 漏 1 個或多 1 個 fail 此 scenario 並報
`[OPENAB_BOT_IDS membership] "{id}" prefix="..." 預期 in_openab=..., 觀察 OPENAB_BOT_IDS = {...}`。

## 護衛 test 觸發模式

| 改動 | 護衛 fail 訊息 | 修正路徑 |
|---|---|---|
| 加新 provider 但漏加 CONTRACT row | `[matrix size] CONTRACT 應有 13 row, 觀察 = N` | 同步加 CONTRACT row |
| 改 cicx.enabled = false | `[enabled_default] "cicx" 預期 enabled=true, 觀察=false` | 同步改 CONTRACT enabled |
| 改 sounds.get("cicx") = "cicx-v2.mp3" | `[sound file] "cicx" 預期 "cicx.mp3", 觀察 Some("cicx-v2.mp3")` | 同步改 CONTRACT sound_file |
| 漏加 OPENAB_BOT_IDS "cicx" | `[OPENAB_BOT_IDS membership] "cicx" prefix="🤖" 預期 in_openab=true, 觀察 OPENAB_BOT_IDS = {...}` | 同步加 OPENAB_BOT_IDS |
| 漏加 default_providers "cicx" | `[provider missing] "cicx" 不在 default_providers() 內, 觀察 keys = [...]` | 同步加 default_providers |

## 跟 R67 護衛 (config.rs:909) 互補關係

- **R67 護 keys 對稱**: 漏 provider (預期 13 個 keys, 觀察 12) → R67 (a)/(b) fail
- **R106 護 value 對齊**: provider 在但 value 漂移 (e.g. cicx sound filename 改) → R67 不抓, R106 fail
- 兩條並存, 互補覆蓋率。R106 不取代 R67, 兩者都需存在。
