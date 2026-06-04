# Contributing to LobsterPulse

## 新增 OpenAB bot 監控

新增一隻被監控的 OpenAB bot 需同步改 **4+1 處**：

### 1. 音效檔（sounds/）

建立兩個 silent placeholder：
```powershell
cp sounds/grokx.mp3 sounds/{bot_id}.mp3
cp sounds/grokx-waiting.mp3 sounds/{bot_id}-waiting.mp3
```

### 2. config.rs — 4 個同步點

| # | 函式 | 改動 |
|---|------|------|
| 1 | `default_providers()` | 加 `ProviderConfig { enabled, name, settings_path: None }` |
| 2 | `default_provider_sounds()` | 加 `("{bot_id}", "{bot_id}.mp3")` |
| 3 | `default_provider_waiting_sounds()` | 加 `("{bot_id}", "{bot_id}-waiting.mp3")` |
| 4 | `detect_providers()` 的 `which_exists` 清單 | 若 bot 有對應 CLI binary 則加 |

### 3. hook_server.rs — 白名單

`KNOWN_PROVIDERS` 加 `"{bot_id}"`。

### 4. lib.rs — seed_default_sounds

`seed_default_sounds` 的 `defaults` array 加兩行：
```rust
("{bot_id}.mp3", include_bytes!("../../sounds/{bot_id}.mp3")),
("{bot_id}-waiting.mp3", include_bytes!("../../sounds/{bot_id}-waiting.mp3")),
```

### 5. 測試更新

更新所有 assertion 中的 provider 數量：
- `hook_server.rs`: `KNOWN_PROVIDERS.len()` 相關測試
- `lib.rs`: `seed_default_sounds` 相關測試

### 原則

- **bot_id 以 openab `config-*.toml` 的 `[lobsterpulse] bot_id` 為 source of truth**
- display name 格式：`"🤖 {BOT_ID} · OpenAB {後端名}"`（後端名以 config-*.toml 第 1 行「後端: X」為準）
- 音效 placeholder 先用 silent mp3，等真實 TTS 部署後再換
- 新增後執行 `cargo test --lib` 確認全數通過

### 命名慣例

| 類型 | 前綴 | 範例 |
|------|------|------|
| OpenAB bot | 🤖 | 🤖 CICX · OpenAB Claude |
| 本機 CLI | 💻 | 💻 Claude Code（本機） |
