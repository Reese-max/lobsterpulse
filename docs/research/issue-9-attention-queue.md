# Issue #9 — Attention Queue / Triage Inbox（研究設計）

Research deliverable for `Reese-max/lobsterpulse#9`（P2 Research）。
**前置**：#1/#3 Codex 設定安全、#5 provider truth、#6 安裝路徑未完成前，
本研究只用 synthetic/replayed events，不宣稱 production attention
reliability，不以新 UX 掩蓋既有 P0/P1。

## 1. Schema（versioned；未知 → 明確 `UNKNOWN`）

### 1.1 `AttentionEvent`（raw evidence）

```jsonc
{ "eventId": "ae_...", "provider": "...", "sessionId": "...",
  "kind": "COMPLETED | WAITING | RECOVERED | ERROR | STALE | QUOTA",
  "observedAt": "...", "payloadHash": "..." }
```

### 1.2 `AttentionItem`（human-facing，dedup 後）

```jsonc
{ "itemId": "ai_...", "severity": "INFO | NEEDS_DECISION | BLOCKING | CRITICAL",
  "whyNow": "<deterministic reason code>",
  "sourceFreshness": "...", "sourceEventIds": ["ae_..."],
  "state": "OPEN | SNOOZED | ACKED | RESOLVED | AUTO_RESOLVED",
  "snoozeUntil": "..." }
```

### 1.3 `AttentionReceipt`

```jsonc
{ "itemId": "...", "policyVersion": "...",
  "correlationInputs": ["ae_..."], "disposition": "...",
  "decidedAt": "..." }
```

## 2. 規則

- 同 provider/session `WAITING → RECOVERED` → auto-resolve，保留兩個原始
  event IDs。
- duplicate/cascade → 單一 human-facing item，raw evidence 完整可查。
- `COMPLETED` routine 預設不與 `BLOCKING` 同中斷等級；history 可查。
- `STALE / NOT_MONITORED / EXTERNAL_DEPENDENCY / UNKNOWN` 不得被當
  success/auto-resolved。
- 每個 `NEEDS_DECISION/BLOCKING/CRITICAL` 有 deterministic `why_now` +
  source freshness。
- Snooze 到期若原因仍在 → 重新進 queue；resolved 不重複喚醒。
- `CRITICAL` 不支援永久 dismiss；提供可審計 acknowledge，不無限轟炸。
- Bounded pre-investigation 負向測試：不讀任意檔案/prompt/credential、
  不啟動 shell/network write——只限 allowlisted telemetry。
- 相同 raw events + policy version → correlation/severity deterministic。
- Policy 改版 → 新 version；舊 receipt 保留當時判斷，不 rewrite history。
- no-cloud mode 完整可用；關閉 queue 時既有 monitoring/metrics 不回歸。

## 3. Runtime 驗證（NEEDS_RUNTIME_VERIFICATION）

Disposable config home 啟動 packaged app → replay 多 provider 事件 → 驗證
correlation/snooze expiry/restart persistence/dup suppression/critical ack →
`/metrics` 與 queue 一致 → stale/unknown 不 silent-resolve →
filesystem/network instrumentation 證明 pre-investigation 未越界。
