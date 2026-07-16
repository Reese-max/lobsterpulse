# OpenUsage 風格 AI Usage 面板 — 設計（2026-07-16）

## 背景

使用者以 /goal 指示：對本目錄做與龍蝦監控（session 狀態機）無關的大幅度改造，並提供兩張 OpenUsage 0.7.3（macOS menu-bar AI 訂閱額度面板）截圖作為目標樣式。本設計把 LobsterPulse 改造成 **OpenUsage 風格的 AI 訂閱額度監控面板**：膠囊點開直接看到 per-provider 額度卡片堆疊，而非 session 列表。

使用者不在線上，設計未經即時核准，由 agent 依 /goal 指令自主定案；全部改動在 `feature/openusage-style` 分支，可整支丟棄。

## 產品定義

桌面常駐額度面板（Windows / Tauri v2）。每個 AI 訂閱 provider 一張卡：

```
Claude  Max 5x                          [色點]
  Session
  ▓▓▓▓▓▓▓▓▓▓░░                       （進度條）
  97% left                  Resets in 4h 17m
  Weekly
  ▓▓▓▓▓░░░░░░░
  69% left                   Resets in 4d 3h
  Usage Trend                      ▁▂▁▃▅▂▁▇▅ （canvas bar sparkline）
              ⌄（chevron 展開）
  Today            401.7K tokens
  Yesterday        926.2M tokens
  Last 30 Days     2.3B tokens · $1.6K
```

Footer：`LobsterPulse Usage <版本> · Next update in Ns` ＋ 切回其他視圖的按鈕。

## 資料現實（探索結論）

| 資料 | 現況 |
|---|---|
| session/weekly % + reset | 僅 anthropic live fetch 有（rate-limit headers） |
| codex/copilot/gemini 配額 % | 無（只有 plan / token 到期）→ 卡片顯示 No data 列 |
| today/yesterday/30d tokens | `~/.claude/stats-cache.json` 的 `dailyModelTokens` 可聚合（僅 Claude） |
| cost | `modelUsage[*].costUSD` 只有累計值 → 30d 列顯示累計成本，today/yesterday 不顯示成本 |
| trend sparkline | `quota-history.csv`（ts,name,pct）既有，畫 used% 趨勢 |

OpenUsage 原品也大量顯示 "No data"，缺資料屬正常降級，不硬造數字。

## 架構決策

1. **新視圖 `view-usage`，成為膠囊點擊的預設視圖**。舊 expanded/session 視圖保留，經 action bar 可達。這是「產品重心轉移」的最小侵入實作：不刪 hook/session 引擎（膠囊狀態與事件診斷仍依賴它），但使用者面對的主畫面換成額度面板。
2. **新檔承載，不塞 main.js**：`src/usage-view.js`（渲染器＋refresh 迴圈）＋ `src/usage-view.css`（OpenUsage 深色半透明主題）。main.js 只加 showView 接線與膠囊點擊導向。
3. **資料重用**：`read_usage_snapshots` + `get_live_quota_snapshot` + `selectQuotaSnapshot` + `QuotaCards.normalizeRunnerCard`（既有 normalize 管線）＋ `get_quota_history`（sparkline）。
4. **Rust 只加一個 command**：`get_claude_daily_stats` 讀 stats-cache.json 回 `{today_tokens, yesterday_tokens, tokens_30d, cost_30d_usd?, computed_date}`；並修掉 anthropic.rs 硬編 `today_tokens=0`。不做 codex/copilot/gemini 的配額 API 研究（v1 範圍外，卡片走 No data）。

## 錯誤處理

- stats-cache.json 不存在/壞 JSON → command 回 `null`，前端該區塊顯示 No data。
- stats-cache 過期（lastComputedDate 舊）→ 照實顯示日期含 `computed_date`，UI 標註資料日期，不假裝即時。
- provider 無 window 資料 → 卡片保留骨架，列顯示 `—  No data`（對齊 OpenUsage 樣式）。

## 測試/驗證

- Rust：`cargo tauri build --no-bundle` 必過（Build SOP）；`get_claude_daily_stats` 聚合邏輯附單元測試（純函數，餵假 JSON）。
- JS：`node --check` 語法閘；渲染器核心（token 格式化、聚合顯示）為純函數，附最小 assert 測試（test/ 既有模式）。
- 既有 test/ 測試不得變紅。

## 範圍外（明列不做）

- codex/copilot/gemini 真實配額 API 接入
- 即時 cost 計價（需模型單價表）
- 重命名 app/exe/tray（產品識別不動，只換主視圖）
- 刪除 session/hook 舊功能
