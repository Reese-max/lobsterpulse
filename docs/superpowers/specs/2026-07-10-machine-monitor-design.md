# 全機監控站（machine-collector + LobsterPulse 面板）設計

日期：2026-07-10
狀態：已與使用者確認（架構 Part 1 + 細節 Part 2 均口頭核准）
範圍：把 LobsterPulse 從「AI agent session 監控」擴充為「整台機器的個人維運站」

## 1. 背景與問題

LobsterPulse 目前監控 13 個 AI provider（9 OpenAB bot + 4 本機 CLI）的即時
session 狀態、quota、timeline 與事件流，並有四通道通知（toast / 音效 /
Telegram / Discord）。但這台機器 24/7 跑著大量其他系統——auto-dev loop、
n8n、cron/schtasks 排程、LLM proxy（Hermes 8318 / ProxyPilot 8317）、
OpenAB WSL 後端、多個專案 repo——全部沒有被監控。使用者的體感是
「監控的東西好少、不完整」，確認後的缺口有四個方向且全部要做：

1. 監控對象太少（排程與 loop / 服務與 port / 系統資源 / 專案健康 四類全要）
2. 每個對象的深度不夠
3. 缺歷史與趨勢
4. 缺告警與主動通知

使用方式定調為「兩者並重」：膠囊看得到全機摘要 + 異常主動推 Telegram。

## 2. 架構決策

三個候選方案中選 **B：分離式 collector**：

- A（全塞進 LobsterPulse Rust 本體）：app 沒開 = 全瞎；lib.rs 已 12,000+ 行；否決
- **B（分離式 collector daemon + LobsterPulse 當顯示端）：採用**
- C（Prometheus + Grafana 標準棧）：與膠囊桌面體驗脫節、Windows 維運重；否決，
  但 collector 預留 `/metrics` 出口，未來要接隨時能接

選 B 的核心理由：

- 監控與告警**不依賴 UI app 活著**——LobsterPulse 掛了，採集和 Telegram 告警照跑
- 客製判定邏輯（健康≠活著、commit charge 前兆、死 socket）現成 exporter 給不了
- LobsterPulse 改動集中在 UI 層，不再膨脹 lib.rs

### 組件關係

```
┌─────────────────────────────┐
│ machine-collector（新，獨立） │  開機常駐（schtasks 註冊，含自動重啟 watchdog）
│  每 30–60s 巡檢一輪           │
└──────┬──────────┬───────────┘
       │          │
   SQLite      Telegram ←── 異常直推（不經過 LobsterPulse）
  （歷史庫）
       │
       ├── machine-status.json（即時快照，沿用 LobsterPulse 讀 usage-*.json 模式）
       └── localhost HTTP API（歷史查詢，給面板畫趨勢圖）
                  │
       ┌──────────▼──────────┐
       │ LobsterPulse（改 UI） │  新增「全機健康」面板 + 膠囊摘要徽章
       └─────────────────────┘
```

- collector 用 **Python 3** 實作（採集 schtasks / port / WSL / git 以腳本語言最快）
- 已知雷點防護：pythonw 下 sys.stdout 為 None（踩雷 §23）、schtasks 預設
  BelowNormal 優先權（§9）、隱藏啟動鏈禁 detach（§20）
- 「誰監控監控者」雙保險：collector 每輪寫 heartbeat → LobsterPulse 偵測
  heartbeat 過期即亮紅 + 通知；schtasks 層 watchdog 負責拉起崩潰的 collector

## 3. 採集模組（四類）

監控清單由 `monitor-config.yaml` 驅動——**加對象改設定，不改碼**。

| 模組 | 採集內容 | 編碼進去的既有規則 |
|---|---|---|
| **services** | port owner 是否為預期進程、healthz、有流量時 log 是否在長（Hermes 8318、ProxyPilot 8317、n8n、OpenAB WSL 後端…） | 硬規則 8「健康≠活著」、踩雷 §22 死 socket |
| **schedules** | schtasks 最近結果碼（0x0/0x1/0x41301）、該跑的窗口沒跑、n8n 失敗 execution（n8n REST API）、各 loop state 檔新鮮度 | §23、§26 監督腳本路徑 |
| **resources** | CPU、RAM、commit charge %、磁碟空間、GPU（nvidia-smi）、殭屍/孤兒進程數 | §16 0xc0000142 前兆預警 |
| **projects** | 註冊 repo 的 git dirty 檔數、未推 commit 積壓天數、關鍵產物新鮮度 | — |

每個 check 獨立失敗隔離：單項 try/except，錯誤記入 `check_results`，
不影響同輪其他 check。讀不到的對象（如 WSL 未起）顯示「無法採集」而非假綠。

## 4. 歷史儲存

SQLite：`~/.lobsterpulse/machine-monitor.db`

- `samples`：數值時序（ts, metric, labels, value）——資源類每輪一批
- `check_results`：布林巡檢結果（ts, check_id, ok, detail）
- `alerts`：告警事件（觸發時間、恢復時間、severity、已通知通道）
- 保留 90 天，內建每日 prune
- 面板趨勢圖（7d/30d sparkline）由此供資料，視覺語言對齊現有 quota 趨勢

## 5. 告警規則

全部帶去重與冷卻（同一告警 30 分鐘冷卻），恢復也通知（「✅ Hermes 已恢復」）。
Telegram 沿用 LobsterPulse config 既有 bot 設定，不另建。

| 規則 | 觸發 | severity |
|---|---|---|
| 服務失聯 | port owner 不符 / healthz 連續 2 輪失敗 / 有流量但 log 停長 | 🔴 立即推 |
| 排程失敗 | schtasks 結果碼非 0x0、該跑的窗口沒跑、n8n execution 失敗 | 🔴 立即推 |
| 資源預警 | commit charge > 85%、磁碟 < 10%、CPU 持續 > 90% 逾 10 分鐘 | 🟡 低頻推 |
| git 積壓 | 未推 commit 超過 3 天（可設定） | 🟡 併入每日摘要 |
| collector 自身 | heartbeat 過期（由 LobsterPulse 端偵測） | 🔴 立即推 |

每日摘要一則彙整所有黃色項。

## 6. LobsterPulse UI 改動

只動前端層 + 少量讀檔 Tauri command：

1. **膠囊摘要徽章**：🟢 全機正常 / 🔴 N 項異常，與現有 13 provider 狀態並列，
   點開跳全機健康面板
2. **「全機健康」面板**：四區塊（服務 / 排程 / 資源 / 專案），每項顯示
   當前狀態 + mini 趨勢圖；即時資料讀 `machine-status.json`，
   歷史走 collector HTTP API
3. 面板顯示 collector heartbeat 年齡；過期整面板變灰 + 告警

## 7. 錯誤處理

- check 級隔離（見 §3）
- collector 崩潰由 schtasks watchdog 拉起；連續崩潰 3 次進入退避並推 Telegram
- Telegram 推送失敗：本地 queue 重試，最終失敗記入 alerts 表 detail

## 8. 驗收（硬規則 1：完成要證據）

- 每個採集模組附 pytest 單元測試（mock 系統呼叫）
- 端到端演練：手動殺一個測試服務 → 60 秒內 Telegram 收到告警 →
  拉回服務 → 收到恢復通知，全程貼證據
- 膠囊徽章與面板實跑截圖驗證
- 驗收派 fresh-context agent（硬規則 4，實作者不自驗）

## 9. 分期交付

1. **Phase 1**：collector 骨架 + services + resources + Telegram 告警 + heartbeat
2. **Phase 2**：schedules（schtasks + n8n + loop state）
3. **Phase 3**：projects（git 積壓）+ 每日摘要
4. **Phase 4**：LobsterPulse 膠囊徽章 + 全機健康面板

## 10. 明確不做（YAGNI / 避免重工）

- AI agent 深度指標（quota emit 4/13→13/13 等）：屬 LobsterPulse 自身
  MISSION.md K0 roadmap，已有規劃，本案不重做
- 不做分散式、多機、Grafana（僅預留 `/metrics`）
- 不做孤兒進程自動清理——只偵測與告警，處置由人決定（硬規則 6）
