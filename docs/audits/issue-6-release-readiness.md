# Issue #6：可攜式發行準備與驗收

此文件是本機變更的驗收清單，不是已發佈或乾淨 Windows 驗收的收據。
Owner 尚未選定 `DISTRIBUTED_BINARY` 或 `SOURCE_ONLY`；既有原始碼安裝入口保持有效。
本次沒有建立 tag、觸發 Actions、建立 Release 或發佈二進位。

## 版本與可重現的封裝

App 版本以 `src-tauri/tauri.conf.json` 的 `version` 為唯一基準；目前為 `0.5.4`。
`package.json`、`src-tauri/Cargo.toml` 與 `src-tauri/Cargo.lock` 必須鏡映它。
README 的 v5.1 只是監控清單版本。

以下從 repository root 執行，需要 Node.js 與 Python 3.11+；不需要安裝 Python 套件：

```bash
npm test
python -m unittest scripts/test_package_release.py
python scripts/package_release.py v0.5.4
```

最後一個命令只檢查版本與 CHANGELOG 段落，不會建置、打包或發佈。Owner 核准分發與版本後，才可對既有
`v0.5.4` tag 執行 release workflow；在分支上手動 dispatch 會在 build 前失敗。
workflow 沿用四個平台的 Tauri build，只建立可供審查的 draft。

Windows ZIP 必須只含同層的 `lobster-pulse.exe` 與 `lobster-pulse-hook.exe`。
每個平台另有 `<archive>.zip.sha256`，內容是 ZIP 的 SHA-256 與檔名；draft notes
記錄 tag 與 source commit。缺少任一執行檔、版本不一致或缺少上傳檔案都會失敗。
這些護欄不代表已產生公開 artifact，也不取代實機驗收。

## 發佈前的乾淨 Windows 11 驗收

使用可丟棄的 Windows 11 VM 與全新測試帳號，不安裝 Rust、Node 或 Tauri。
WebView2 Runtime 仍是執行需求。不要在日常帳號上測試 hook 寫入；Windows 的 Known Folder
解析不保證能用 `HOME` 或 `CODEX_HOME` 隔離。既有 `test/codex-runtime-smoke.ps1`
僅限可丟棄的 GitHub-hosted Windows runner，不能當成一般 VM 的安裝程式。

1. Owner 記錄分發決策、版本、tag 與 commit。若選原始碼分發，明確標示 `SOURCE_ONLY`，
   從 README 前置需求開始在乾淨環境重現 source build；不宣稱有可下載的二進位。
2. 若選二進位分發，從 draft 取得同版本的 Windows ZIP 與 `.zip.sha256`，核對如下。
   發佈後還須透過 README 的正式下載連結重做一次，不以 Actions artifact 代替。
3. 用鍵盤開啟檔案總管（`Win+E`），移到解壓縮目錄並選取主程式，按 `Enter` 啟動。
   確認畫面與系統匣可操作；記錄任何 SmartScreen 訊息。此流程沒有程式碼簽章，checksum
   只驗證完整性，不能證明發行者身分。若平台封鎖，停止並回報，不關閉 Windows 保護。
4. 在測試帳號啟用一個測試 provider，確認寫入的 hook 指向解壓縮目錄中的 sidecar，
   事件確實到達；再停用並核對原有測試設定。Codex 完整 smoke 仍須滿足 Issue #3 的門檻。
5. 從系統匣結束主程式，再次啟動，確認 sidecar 仍可被找到。保存 OS/WebView2 版本、
   下載 URL、tag/commit、ZIP hash、sidecar 路徑與每步結果；未實際執行者標為未驗證。

以下核對範例只用於取得真實的同版本檔案之後。hash 或 manifest 有誤會停止，不啟動程式：

```powershell
$zip = '.\lobster-pulse-v0.5.4-windows.zip'
$entry = (Get-Content -LiteralPath "$zip.sha256" -Raw).Trim() -split '  ', 2
if ($entry.Count -ne 2 -or $entry[0] -notmatch '^[0-9a-fA-F]{64}$' -or
    $entry[1] -cne (Split-Path -Leaf $zip)) { throw 'Invalid checksum manifest' }
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash -ine $entry[0]) {
  throw 'SHA-256 mismatch; do not extract or launch'
}
Expand-Archive -LiteralPath $zip -DestinationPath '.\LobsterPulse'
```

Windows 設定在 `%APPDATA%\lobsterpulse\config.json`，音效在同層的 `sounds\`，
runtime port 在 `%USERPROFILE%\.lobsterpulse\port`。更新前結束程式並備份設定；保留
上一版 ZIP、checksum 與設定備份，一起替換同一次 build 的兩個執行檔。回復時結束程式，
換回上一版雙執行檔與相容設定。固定安裝目錄可保留 hook 的絕對路徑；搬移後須重新設定。
發佈新版本時保留先前 Release，才能提供公開的回復來源。

## Acceptance mapping

| Issue #6 條件 | 本次結果 |
| --- | --- |
| Owner 分發決策 | 待 Owner 記錄；未代為選擇 |
| 單一 App 版本契約 | Tauri 為基準；封裝前核對其餘三個版本來源與 tag |
| 非 draft Windows 雙執行檔與 SHA-256 | 封裝與 draft workflow 已備妥；公開發行未執行 |
| README 下載入口、設定、更新、回復 | 現有 source build 入口保留；設定與更新／回復已補；正式下載連結待發佈後驗證 |
| 乾淨 Windows 下載、啟動、sidecar、結束／重啟 | 未驗證；須取得正式 artifact 與乾淨 VM 收據 |
| 簽章與 SmartScreen 誠實揭露 | draft notes 明示 unsigned，不要求停用保護 |
| SOURCE_ONLY 乾淨 source build | 決策未定，未執行新的乾淨環境建置 |
| 不新增自動更新、套件管理、遙測、簽章採購 | 無新增 |

目前不應關閉 Issue #6；本機測試不等於公開下載或使用者安裝成功。
