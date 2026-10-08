# Issue #9 — Attention Queue / Triage Inbox（研究設計）

Research deliverable for `Reese-max/lobsterpulse#9`（P2 Research）。
**前置**：#1/#3 Codex 設定安全、#5 provider truth、#6 安裝路徑未完成前，
本研究只用 synthetic/replayed events，不宣稱 production attention
reliability，不以新 UX 掩蓋既有 P0/P1。

可重現的離線 spike：`issue9_attention_replay.py`。只接受下列 allowlisted
metadata，不讀 provider config、prompt、credential、log 或網路；沒有接入 app、
tray、hooks 或 `/metrics`。`UNKNOWN` 是保留值，不能靠 UI 文案猜出 session 或狀態。

## 1. Schema（versioned；未知 → 明確 `UNKNOWN`）

### 1.1 `AttentionEvent`（raw evidence）

```jsonc
{ "schemaVersion": 1, "eventId": "ae_...", "providerId": "claude",
  "sessionId": "UNKNOWN", "correlationId": "UNKNOWN",
  "sourceContractVersion": "synthetic-v1",
  "kind": "COMPLETED | WAITING | RECOVERED | ERROR | QUOTA | STALE | NOT_MONITORED | EXTERNAL_DEPENDENCY | UNKNOWN",
  "observedAtMs": 1000, "sourceFreshness": "FRESH | STALE | UNKNOWN",
  "reasonCode": "WAITING_FOR_USER", "payloadHash": "<sha256 hex>" }
```

`kind` 表示觀測事件／來源狀態，`sourceFreshness` 獨立描述證據新鮮度。
`NOT_MONITORED`、`EXTERNAL_DEPENDENCY`、`UNKNOWN` 必須留在 schema；
`UNKNOWN` provider/session/correlation 不跨事件 dedupe，也不據此 auto-resolve。
`schemaVersion` 必須為整數 `1`，不接受布林值或浮點數。
拒收未列欄位，尤其 prompt、tool content、
token、credential。`payloadHash` 只是 synthetic evidence 參照，不包含 payload。

### 1.2 `AttentionItem`（human-facing，dedup 後）

```jsonc
{ "schemaVersion": 1, "itemId": "ai_...", "providerId": "claude",
  "sessionId": "UNKNOWN", "correlationId": "UNKNOWN",
  "policyVersion": "issue9-synthetic-v1",
  "severity": "INFO | NEEDS_DECISION | BLOCKING | CRITICAL",
  "whyNow": "<deterministic reason code>", "sourceFreshness": "UNKNOWN",
  "sourceEventIds": ["ae_..."],
  "state": "OPEN | SNOOZED | ACKED | RESOLVED | AUTO_RESOLVED",
  "snoozeUntilMs": null, "createdAtMs": 1000, "lastSeenAtMs": 1000 }
```

### 1.3 `AttentionReceipt`

```jsonc
{ "schemaVersion": 1, "itemId": "ai_...", "policyVersion": "issue9-synthetic-v1",
  "sourceEventIds": ["ae_..."], "fromState": "OPEN", "toState": "AUTO_RESOLVED",
  "disposition": "FRESH_RECOVERY", "actor": "system", "atMs": 2000,
  "evidenceHash": "<sha256 hex>" }
```

Receipt 是當時的判斷快照，不因 policy 改版而重算。純研究 spike 目前把 state
序列化成 JSON 做 restart 模型測試；尚未實作 app 持久化或端到端 UI。

## 2. 規則

- 同 provider/session/correlation、三個 ID 都已知、兩邊證據都是 `FRESH`
  且在 5 分鐘 correlation window 內的
  `WAITING → RECOVERED` → auto-resolve，保留所有原始 event IDs。
- 同 provider/session/correlation/kind/reason/freshness 的 5 分鐘
  duplicate → 單一 human-facing item，raw evidence 完整可查；
  未知 session/correlation 不跨事件合併。
- 不跨 provider/session 推測 shared root cause；目前沒有可信的全域 cause ID。
  跨來源 cascade 合併須等真實事件證明共同 identity 後另行設計，避免誤壓人類決策。
- `COMPLETED` routine 預設不與 `BLOCKING` 同中斷等級；history 可查。
- `STALE / NOT_MONITORED / EXTERNAL_DEPENDENCY / UNKNOWN` 不得被當
  success/auto-resolved。
- 每個 `NEEDS_DECISION/BLOCKING/CRITICAL` 有 deterministic `why_now` +
  source freshness。
- `NEEDS_DECISION` 可 snooze；到期若原因仍在 → 重新進 queue；resolved 不重複喚醒。
- `CRITICAL` 不支援永久 dismiss；提供可審計 acknowledge，不無限轟炸。
- 事件、決策與推進時鐘的時間都必須是非負整數毫秒；snooze 到期時間
  必須是晚於決策時間的整數。非法時間在修改 state 或 receipt 前拒收。
- Bounded pre-investigation 負向測試：不讀任意檔案/prompt/credential、
  不啟動 shell/network write——只限 allowlisted telemetry。
- 相同 raw events + policy version → correlation/severity deterministic。
- Policy 改版 → 新 version；舊 receipt 保留當時判斷，不 rewrite history。
- no-cloud mode 完整可用；關閉 queue 時既有 monitoring/metrics 不回歸。

### 固定 synthetic replay receipt

在 repo 根目錄執行：

```sh
python docs/research/issue9_attention_replay.py
python -m unittest discover -s docs/research -p 'test_issue9_attention_replay.py' -v
```

第一個命令的內建 6-event fixture 預期輸出 `rawEventCount=6`、
`itemCount=4`、`humanQueueCount=2`、`autoResolvedCount=1`。
也可以把符合 schema 的 JSON event array 路徑當成命令參數重播。

100-event fixture 固定為 80 個 routine completion、10 個同 session waiting
duplicates、8 個獨立 error、1 個 waiting/recovered pair（2 events）。
預期：100 個 raw IDs 全部可追、9 個 human queue items、1 個 auto-resolved
item；輸入順序打亂後輸出完全一致。另測 source stale/unknown、
`NOT_MONITORED`、`EXTERNAL_DEPENDENCY`、critical ack、snooze expiry、
JSON roundtrip、跨 provider 不推測合併、未知身分不與合法 ID 撞 key、
不同 reasonCode 不合併、schema version／時間型別驗證、content-bearing 欄位拒收與禁用
file/shell/network 的負向測試。
這些是 synthetic 結果，不代表真實多 agent session 的壓縮率或可用性。

## 3. Runtime 驗證（NEEDS_RUNTIME_VERIFICATION）

Disposable config home 啟動 packaged app → replay 多 provider 事件 → 驗證
correlation/snooze expiry/restart persistence/dup suppression/critical ack →
`/metrics` 與 queue 一致 → stale/unknown 不 silent-resolve →
filesystem/network instrumentation 證明 pre-investigation 未越界。
