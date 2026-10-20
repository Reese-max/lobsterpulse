# Issue #5 本機交付證據

- 分支：`issue-loop/issue-5`
- 基準：`e1adcdb66e2efe46c951619368ef2748bbfecdfa`
- 基準已含 #5 的主要實作；本次沿用 registry、UI 計算器及 K0 domain functions，補齊狀態、收據驗證與後續歷史保護。

## TDD 與驗證

新增測試先執行再修正：最初 18 failed（UI/KPI 狀態與能力資料）、另 5 failed（缺漏 provider 證據）、1 failed（新增基線沒有指紋仍通過），以及 6 failed（舊收據相容性、未知 schema 與 CLI 未檢查指定收據）。失敗摘錄保存在根目錄 `.issue-loop-evidence.json`。

可由 Git Bash 重跑的交付指令：

```bash
python -m pytest -q scripts/test_k0_measure.py scripts/test_k0_target_baseline_check.py && node --test test/version-consistency.test.js test/installation-docs.test.js test/quota-cards-lib.test.js scripts/test_provider_coverage.mjs && python scripts/k0_target_baseline_check.py && node scripts/provider_inventory_guard.mjs
```

結束碼 **0**：Python **75 passed**、Node **35 passed**，共 **110 passed / 0 failed**；同一範圍相較基準為 **79 → 110**。

其他驗證：

| 指令 | 結束碼與關鍵結果 |
|---|---|
| `cargo test --locked --offline --manifest-path src-tauri/Cargo.toml` | 0；608 passed、0 failed、5 ignored |
| `npm run test:ui-no-fake-data` | 0；12 個 Chromium 情境通過，使用 mock Tauri，並非 packaged runtime |
| `python -m compileall -q scripts/k0_measure.py scripts/k0_target_baseline_check.py` | 0 |
| `git diff --check` | 0 |
| 首次 `python -m pytest -q` | 1；221 passed、3 failed、2 skipped；包含兩個 chain-staleness 與當時 dirty-worktree sentinel 失敗 |
| `node --test test/*.test.js scripts/test_provider_coverage.mjs` | 1；35 passed、1 failed：本機 `config.template.json` 與 runtime 副本不同 |

以 `git archive` 匯出指定基準，確認乾淨狀態後重現：`scripts/test_chain_staleness.py` 為 **9 passed / 2 failed**（檔數預期 17、實際 29；stale 預期 29、實際 25），`test/runtime-sync.test.js` 同樣因 `config.template.json` 失敗。沒有同步或覆寫使用者設定。

## 驗收對照

| 條件 | 結果與機制 |
|---|---|
| 13 個 canonical provider 與完整能力欄位 | 沿用現有 registry；v2 receipt 保存全部能力欄位，驗證 owner、scope、signal、freshness 等資料 |
| 區分 registered/configured/live/session/quota | 沿用五個維度；新增跨 UI/KPI 測試，修正無事件 OpenAB 的狀態差異 |
| receipt 的版本、時間、分子、分母及完整排除原因 | 沿用維度契約，補上 schema v2 與能力快照；`--receipt` 唯讀驗證原始或包裝報告 |
| 變更 scope 建立新基線，保留歷史 | 每個新增 baseline 必須登錄固定指紋；測試連續兩次 scope 變更後改寫前一份歷史會失敗；舊 receipt 不補寫、不重算 |
| validator 拒絕 README/UI/KPI 不一致 | 沿用文件與 inventory guard；新增狀態一致性與 receipt 能力漂移、缺漏、未知 schema、CLI 回歸測試 |
| 13 registered、2 live、4 exclusions 無法聲稱零缺口 | fixture 仍顯示 live 2/9、registered 13、gap 11，並保留每個排除原因與逐項狀態 |
| 不新增整合或收集憑證 | 符合；只使用既有資料路徑與合成 fixture |

## 審查與未完成項目

依 issue-loop 完成獨立唯讀審查；新增缺陷未解決數 **0**。主要修改在 `scripts/k0_measure.py`、`scripts/k0_target_baseline_check.py`、對應測試與 README；沒有改動既有歷史基線或 provider 清單。

**尚未完成整個 issue 的 runtime 驗收。** 本機沒有可安全替換的 Windows 使用者 profile；`test/codex-runtime-smoke.ps1` 明確限 disposable GitHub hosted runner，啟動 app 會寫入設定並可能修復既有 Codex hooks。因此未繞過保護、未啟動正式 app，也未宣稱 `/metrics` 或 native WebView 驗收通過。OpenAB owner 確認及 45/50 persona 理解研究仍未驗證。

專案沒有設定 coverage ratchet、CRAP、mutation 或獨立 lint gate，未新增工具鏈。未 push、開 PR、merge、部署或傳送外部訊息；外部操作由 driver 負責。
