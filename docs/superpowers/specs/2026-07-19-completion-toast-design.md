# 完成即時通知：app 內 toast ＋ 最近完成清單

日期：2026-07-19。使用者需求：「跳出哪些 AI CLI 已經完成的即時通訊」→ 選定
「app 內跳出小視窗 ＋ 完成紀錄清單，直接在介面顯示」（Windows toast 既有且已開，
不動；IM 推播不做）。

## 設計（全前端，不動 Rust）

### 1. App 內 toast（`#lp-toast`）
- 掛在既有 `task-completed` / `task-waiting` 事件監聽器（main.js），與 systemNotify 並行
- 膠囊下緣延伸小卡（與 capsule-brief 同視覺語言）：provider icon＋名稱＋「✅ 完成」/「⏸ 等待處理」
- 4.5 秒自動收；顯示與收合各觸發一次 `fitWindow()`（膠囊收合時視窗才量得到高度）
- 同 provider＋文字 5 秒去重（`lpToastLast` Map），防連環洗版
- normal flow 元素（非 absolute）：放 capsule-brief 之後、view-usage 之前

### 2. 最近完成清單（`#uv-recent`）
- usage 面板卡片區下方，卡片同底色區塊：「最近完成」標題＋每列 icon＋名稱＋相對時間
- 資料源：既有 `get_recent_events` command（記憶體 buffer 最近 50 筆 hook event）
- 純邏輯 `QuotaCards.recentCompletions(events, allowedIds, limit=8)`（quota-cards-lib.js，node:test 覆蓋）：
  - 只收 `Stop` / `SessionEnd`
  - provider 限本機 CLI（detect_installed_clis 的 id）——OpenAB bot 24/7 loop 會洗版，不列入
  - 同 provider+session 去重留最新，新→舊排序取 8 筆
- 更新時機：usage view 既有 60s refresh；收到 `task-completed` 時 main.js 呼叫
  `window.UsageView.render()` 即時插入

## 已知限制（規格內接受）
- 事件 buffer 為記憶體，app 重啟清單歸零（不做持久化，需要歷史再加）
- toast 在展開視圖時會把面板往下推 4.5 秒（flow 元素，接受）
- gemini CLI 已被 Google 停用個人帳號（2026-07 查證），hook 事件實際不會再來

## 驗證（2026-07-19）
- `node --test test/quota-cards-lib.test.js` 14 測試全過
- 實測：POST 假 hook 事件（UserPromptSubmit→Stop）至 `/hook/claude`、`/hook/codex`
  → toast「claude ✅ 完成」截圖、清單「Codex CLI — 0 秒前」截圖，均正確
