# Proposal: Lobster Rules Engine — 使用者自訂 event → action

## Goal

把 LobsterPulse 從「寫死觸發」升到「**使用者自訂規則**」:

**TriggerRule** = `when {provider?, event?, state_to?} → then [Toast | Sound | Log]`

膠囊現有 toast / sound 觸發 (lib.rs:1606-1609 `test_toast`, lib.rs:339
`play_sound_file`) 是寫死路徑, 無法讓使用者定義「什麼事件對我重要」。
規則引擎 = **讓使用者告訴膠囊「這條事件發生時, 幫我做 X」**。

對齊北極星 (CLAUDE.md 頂部): 「單一膠囊 + 多 runtime 狀態, 而不是只算 token」—
規則引擎把「狀態轉移」變成「使用者注意力觸發」, 從監控工具變個人指揮中心。

## Background

CLAUDE.md 釘的差異化是「**桌面常駐 + 狀態機 + 雙路徑 + 雙生態**」, 競品
Token Telemetry / tokenusage 全走 web dashboard, 看不到 Idle→Working 轉移瞬間。
我們能 emit task-completed / task-waiting 即時觸發, 但**只能 emit 寫死的兩條**:

1. `task-completed` → 播 `provider_sounds[provider]` (R78 spec)
2. `task-waiting` → 播 `provider_waiting_sounds[provider]`

使用者**不能**自訂:
- 「cicx 任何 PostToolUseFailure → 跳 Windows toast + 寫 log 到 `~/.lobsterpulse/audit.log`」
- 「claude WaitingForUser 維持 5 分鐘 → 跳 toast 提醒我回來看」
- 「任何 provider Stop → 寫一行 JSON 到 `~/lobster-pulse-events.jsonl`」

R100 策略顧問 (2026-06-04) #3 行動 closure 把差異化釘成「**真實任務狀態 + 雙路徑**」,
規則引擎是把「狀態」變「可編程通道」, 補 R100 沒覆蓋的「**狀態 → 使用者注意力**」段。

R106 (2026-06-05) 已收斂 13×3 provider contract 護衛, R108/R109 K0 Quota
接力推 10/13, R114 拿 spec drift 收 R78 KNOWN_PROVIDERS SSoT。
**R115 接力**: 把「狀態變通知」這層使用者控制權補上。

## Scope

### In Scope (MVP)

- 開新 `openspec/changes/lobster-rules-engine/` change 資料夾
- 寫 4 個 spec 檔: `proposal.md` / `design.md` / `tasks.md` / `.openspec.yaml`
- 寫 1 個 capability spec: `specs/lobster-rules-engine/spec.md`
- `src-tauri/src/config.rs` 新增 `TriggerRule` 結構 + `RuleWhen` + `RuleAction`
  enum (Toast / Sound / Log), `AppConfig.rules: Vec<TriggerRule>`, 預設 3 條
- `src-tauri/src/session.rs` `SessionManager::handle_event` 結尾串接
  `evaluate_rules(&event, &transition)`, 匹配時 emit Tauri event
  `rule-fired` + 對應 action 觸發 (走既有 `play_sound_file` / `auto_rules::send_toast`)
- `src-tauri/src/lib.rs` 新增 4 個 Tauri command: `list_rules` / `toggle_rule` /
  `add_rule` / `remove_rule`
- `src/index.html` 設定頁加 Rules section (toggle + add + remove UI)
- 護衛 test: `r115_rule_evaluation_match_count` 護衛 3 預設規則在對應事件
  觸發時 match 計數 = 1
- 護衛 test: `r115_rule_action_emission` 護衛 `rule-fired` Tauri event
  至少 1 次 emit 對應到每個 action 類型
- 不擴張 R66/R82 護衛鏈 (chain 飽和契約 R113.1 守住)

### Out of Scope (留 R116+ 接力)

- Webhook action (需 network IO + retry, 獨立 spec)
- 條件式 cooldown (e.g. 5 分鐘內不重複觸發), 留 R116+ 規則進階
- 跨 session aggregate trigger (e.g. 「同時 3 個 provider fail 才觸發」), 留 R117+
- 規則 import/export / 分享, 留 R118+
- 規則視覺化編輯器 (drag-drop), 留 R119+

## Why Now

- supervisor R105 (2026-06-05) top_risk: 「K0 Quota 停在 10/13, 最近 5 個
  commit 全是 docs/chore, 實際推進動能放緩」, directive_issued=true
- 5 輪無 feature commit, 動能斷
- 規則引擎是 1 輪可落地、UI 視覺衝擊大、北極星對齊的「wow」feature
- 不做規則引擎 = 繼續 R110+/R112+ 文件/觀察窗接力 = 又 5 輪 docs/chore

## 北極星對齊 (MISSION.md 釘的)

- 1️⃣ K0 Quota 飽和: 規則引擎不直接推 K0 數字, 但補「K0 emit → 使用者感知」
  通道, **K0 13/13 完成後規則引擎直接放大 K0 價值** (有 quota 沒通知 = 沒用)
- 2️⃣ 13 provider 狀態飽和: 規則引擎覆蓋全部 13 provider, 不偏廢
- 3️⃣ 多 runtime 狀態機: 規則 `when.state_to` 對齊 SessionTransition (Completed /
  StartedWaiting / None), 把狀態機變可訂閱事件流

## 風險

- **scope 蔓延**: 已用 Out of Scope 段收斂, 嚴守 MVP 3 action type
- **config 序列化**: TriggerRule 加進 AppConfig → 既有使用者 config.json
  缺 `rules` 欄位 → forward migration 用 `.and_modify(|c| c.rules =
  default.rules.clone())` 對齊 R114 既有 pattern
- **emit 雙觸發**: 既有 task-completed / task-waiting emit 在 session.rs
  結尾, 規則引擎 evaluate 必須在它**之後**, 否則 Sound action 會跟
  provider_sounds 重播撞
- **預設規則 disable 開關**: 第一條用戶裝好可能不想要 toast spam, MVP 加
  `AppConfig.rules_enabled: bool` master switch, 預設 true
