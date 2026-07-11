# Quota 卡片面板設計（LobsterPulse 前端改版）

日期：2026-07-11
狀態：已與使用者確認（位置/範圍/視覺/方案均口頭核准）
範圍：LobsterPulse 膨脹視圖的 quota runner 區塊改版為卡片式面板

## 1. 背景

使用者提供一張 macOS menu-bar quota 監控 app 截圖（Claude/Codex/Cursor/
Antigravity/Grok 每個 provider 一張卡：Session/Weekly 進度條、% left、
Resets in 倒數、Extra Usage 金額、Usage Trend sparkline），希望 LobsterPulse
打造成類似的呈現。

資料面盤點（2026-07-11 實測 `~/.lobsterpulse/usage-local.json`）：

- claude runner：`session_5h_used/remaining`、`session_5h_reset`（"3h19m" 字串）、
  `week_7d_used/remaining`、`week_7d_reset`、`tier` —— 完整卡所需欄位齊全
- codex runner：`h5_used/remaining/reset`、`wk_used/remaining/reset`、`plan` —— 齊全
- OpenAB bot（`usage-{bot}.json`）：單一 quota %，無雙窗結構
- grokx/lpbot/mimo/openx：無 verified quota 資料（現有程式即留空）
- Extra Usage 金額：**無資料來源**（claude raw 的 `today_cost` 為 "N/A"）
- 趨勢：`quota-history.csv` 已有，現有 7/30d sparkline 即由此供資料

## 2. 決策

- **位置**：取代膨脹視圖現有的 quota runner 區塊（舊簡易顯示退場）
- **範圍**：依資料自動分級（完整卡 / 簡化卡 / 無資料不佔版面）
- **視覺**：仿截圖佈局，配色跟隨 LobsterPulse 現有 accent color 與深/淺主題變數
- **方案**：純前端改版（方案 A）——新 `renderQuotaCards()` 取代現有渲染，
  資料來源與 Rust 端 API 完全不動；欄位 normalize 在前端做（`runnerPct` 已有先例）
  - 否決方案 B（lib.rs 加標準化 API）：動 12,000 行檔案 + 每次視覺調整都要 Rust 重建

## 3. 卡片規格

```
┌─ 🤖 Claude Code  Max ────────────────────┐
│ Session                                   │
│ ████████████░░░░░░░   （bar = 剩餘量）     │
│ 64% left             Resets in 3h 19m     │
│ Weekly                                    │
│ ████████████░░░░░░░                       │
│ 61% left             Resets in 41h 29m    │
│ Usage Trend          ▁▃▂▁▁▅▂▁▁▃ (7d)      │
│                  ˅（收合 chevron）         │
└───────────────────────────────────────────┘
```

### 分級規則

| 級別 | 條件 | 內容 |
|---|---|---|
| 完整卡 | runner raw 具 session+weekly 兩窗與 reset 欄位（claude、codex） | 雙 bar + % left + Resets in + 7d sparkline + tier/plan 副標 |
| 簡化卡 | 僅有單一 quota %（OpenAB bot usage snapshot） | 單 bar + % left + snapshot 更新時間 |
| 不顯示 | 無資料或現有邏輯即留空者（grokx/lpbot/mimo/openx 等） | 不佔版面 |

### 欄位 normalize（前端）

```
claude:  session = session_5h_{used,remaining,reset}; weekly = week_7d_{...}; 副標 = tier
codex:   session = h5_{used,remaining,reset};       weekly = wk_{...};      副標 = plan
```
reset 欄位為 "3h19m" 格式字串，顯示為 "Resets in 3h 19m"（僅格式化，不自行倒數計時；
資料隨 usage poller 更新頻率刷新）。

### 狀態與互動

- **警示**：任一窗剩餘 < 20% → 該 bar 轉紅 + 🔥 icon（對照截圖 Cursor 卡）。
  僅視覺呈現；推播告警沿用現有 quota_low auto rule，不新做
- **收合**：每卡 chevron 收成單行摘要（icon + 名稱 + 最緊窗的 %），
  收合狀態存 localStorage（key 含 provider name）
- **主題**：全部走現有 CSS 主題變數（accent、深/淺），不 hardcode 截圖配色
- **排序**：沿用現有 provider 顯示順序

### 明確不做

- Extra Usage 金額列（無資料來源；記入 backlog，找到 API 再加）
- 新的資料採集 / Rust 端變更
- 13 卡全列（無資料者不出灰卡）

## 4. 錯誤處理

- runner `ok: false` 或必要欄位缺失 → 自動降級為簡化卡；連 % 都沒有 → 不顯示。
  絕不顯示假數字
- reset 字串 parse 失敗 → 該行顯示 "—"，bar 照常
- sparkline 無歷史資料 → 該列隱藏

## 5. 驗收

- `cargo tauri build --no-bundle` 成功（前端 embed 版，遵守 README Build SOP）
- 實際啟動 app 截圖對照：完整卡（claude/codex）、簡化卡（任一 OpenAB bot）、
  紅色警示（假資料注入 <20% 情境）三種狀態
- 既有功能不迴歸：膨脹視圖其他區塊、膠囊 quota 摘要、trend grid 照常
