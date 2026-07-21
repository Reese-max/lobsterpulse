# runtime — 本機執行期檔案的版本控制副本

app 的額度數字不是 Rust 算的，是這些 Node 腳本打各家 API 產生的。它們原本只存在
`~/.lobsterpulse/scripts/`（不在任何 git repo、沒有遠端備份），機器掛了就得重寫。
這個目錄是它們的單向備份。

```
runtime/scripts/            ← ~/.lobsterpulse/scripts/*.{js,cmd}
runtime/config.template.json ← %APPDATA%\lobsterpulse\config.json（token 類欄位已清空）
```

## 用法

```bash
node runtime/sync-from-local.mjs          # 本機 → repo
node runtime/sync-from-local.mjs --check  # 只檢查漂移，有差異 exit 1
```

改過 `~/.lobsterpulse/scripts/` 底下任何東西之後，跑一次同步再 commit。

## 還原到新機器

1. `runtime/scripts/*` → `~/.lobsterpulse/scripts/`
2. `config.template.json` → `%APPDATA%\lobsterpulse\config.json`
3. 填回被清空的欄位（`telegram_bot_token`、`discord.bot_token` 等，鍵名符合
   `sync-from-local.mjs` 裡的 `SECRET_KEY_RE` 者一律是空字串）

## 為什麼 config 只存範本

實檔含 Discord bot token。同步腳本會把符合 `token|secret|webhook|password|api_key`
的字串值清成空字串，並在輸出仍偵測到金鑰樣式時直接中止（exit 2），寧可不同步也不外洩。
