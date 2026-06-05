# Proposal: Provider Contract Matrix Guard (13×3)

## Goal

把 LobsterPulse 既有的 **provider contract 一致性護欄** 從「set 對稱」泛化到
「**13 row × 3 attribute value-equal**」矩陣式 contract test:

1. **Attribute 1: name prefix + emoji** (🤖 OpenAB bot / 💻 本機 CLI)
2. **Attribute 2: enabled_default** (9 OpenAB bot 預設 enabled — mimo 例外 disabled; 4 本機 CLI 預設 disabled)
3. **Attribute 3: sound file mapping** (filename 對齊 default_provider_sounds / default_provider_waiting_sounds)

加 1 條 Rust 護欄 test `r106_provider_contract_13_by_3_matrix`，掃 13 row × 3 attribute
逐一對齊，防以下 spec drift 模式：

- 漏 provider: 4 同步點 (default_providers / default_provider_sounds /
  default_provider_waiting_sounds / lib.rs:39 OPENAB_BOT_IDS) 缺一不可
- attribute value 漂移: e.g. cicx 預期 `enabled=true` 但 code 寫成 `false`
- sound key/value 不對: e.g. cicx 預期 sound `"cicx.mp3"` 但 code 寫成 `"cicx.MP3"`
- 命名前綴漏: e.g. 新 provider 沒加 🤖/💻 前綴

## Background

R100 策略顧問 (2026-06-04) 行動 #2 follow-up 明確要求:

> 開新 change 補 13 provider × 3 attribute matrix

R67 (2026-06-03) 護欄 chain 第 16 條 (config.rs:909) 已有 `provider_registration_guard_tests`，
驗「sounds keys ⊆ providers keys」+「sounds ≡ waiting_sounds set」+
「enabled 🤖 ≥ 5」+「name prefix」+「後端關鍵字兩兩不同」共 5 條斷言。

R67 護欄的弱點:
- **(a) + (b)**: 只驗 keys 對稱, 不驗 value 對齊 (e.g. cicx 預期 sound "cicx.mp3" 但
  code 寫成任何其他檔名都通過, 因為 key cicx 在 set 內)
- **(c)**: 只驗 set 等價, 不驗 value 對齊 (sounds[cicx] 和 waiting_sounds[cicx]
  寫成同一個檔名也通過)
- **(d)**: 只驗 enabled 🤖 ≥ 5, 不驗每個 🤖 預設值 (e.g. cicx 被誤關成 disabled 也不抓)
- **(e)**: 沒驗 sound file mapping filename 對齊

R106 補這 4 個盲點: **13 row × 3 attribute value-equal**, 護欄強度從「set 對稱」升到
「attribute matrix 對齊」。

## Scope

### In Scope

- 開新 `openspec/changes/contract-matrix-guard/` change 資料夾
- 寫 4 個 spec 檔: `proposal.md` / `design.md` / `tasks.md` / `.openspec.yaml`
- 寫 1 個 capability spec: `specs/contract-matrix-guard/spec.md`
- 在 `src-tauri/src/config.rs` 新增 module `provider_contract_matrix_tests`
  內含 `r106_provider_contract_13_by_3_matrix` 護衛 test
- 13 row × 3 attribute 期望值寫成 `CONTRACT: &[(&str, &str, bool, &str, &str)]` const
  作為 single source of truth (對齊 4 同步點)

### Out of Scope

- **不**改既有 R67 護欄 (config.rs:909) — R106 是**新增**護衛不是取代, 兩條並存
  (R67 護 keys 對稱 / R106 護 value 對齊, 防護範疇互補)
- **不**動 4 同步點本體 (default_providers / default_provider_sounds /
  default_provider_waiting_sounds / OPENAB_BOT_IDS) — R106 只驗對齊, 不修對齊源
- **不**接 OTel SDK / 不動 6 條 counter 命名 (留 R103+ follow-up)
- **不**加新 provider (留 MISSION.md 納入標準 review)
- **不**動 main.js / quota/ 模組

## Capabilities

- `contract-matrix-guard` — LobsterPulse 13 provider × 3 attribute 矩陣式 contract
  test, 護 4 同步點 (default_providers / default_provider_sounds /
  default_provider_waiting_sounds / OPENAB_BOT_IDS) attribute value 對齊, 涵蓋
  13 row 完整性 / name prefix emoji / enabled_default / sound file mapping /
  OpenAB bot 集合 membership。
