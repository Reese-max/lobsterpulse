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

## Copilot hooks

`runtime/hooks/copilot-lobster.json` → `~/.copilot/hooks/lobster.json`（目錄需自建）。
Copilot CLI 的 hook 是設定檔驅動、事件名用 camelCase（`sessionStart` / `userPromptSubmitted`
/ `agentStop` / `sessionEnd`），與 Claude 的 PascalCase 互為別名。
Claude 與 Gemini 的 hook 混在各自的大 `settings.json` 內（含其他設定與憑證），不在此備份。

## 部署時要開維護模式

watchdog 會在 app 停掉超過 3 分鐘時把舊版拉回來，正好卡在 `cargo build` 中間，
鎖住 exe 讓建置失敗（實測 `os error 5`）。部署前後：

```bash
touch ~/.lobsterpulse/watchdog-pause     # 停 app、建置、啟動…
rm ~/.lobsterpulse/watchdog-pause        # 完成後解除
```

忘了刪也沒關係——超過 30 分鐘 watchdog 會自動失效並刪掉它，不會永久啞掉。
