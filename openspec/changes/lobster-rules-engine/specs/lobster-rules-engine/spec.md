# Spec: Lobster Rules Engine

> 對應 change: `lobster-rules-engine` (見 `openspec/changes/lobster-rules-engine/`)
> 對應 code-level 落地: `src-tauri/src/config.rs` `rule_engine` module
> 對應 3 同步點: `AppConfig.rules` (config.rs) / `SessionManager::evaluate_rules`
> (session.rs) / `list_rules` tauri command (lib.rs)
> 護衛 chain: 不擴張 R66/R82 (chain 飽和契約 R113.1 守住, 走 3 條獨立 test)

## ADDED Requirements

### Requirement: R-1 — TriggerRule 結構是規則引擎的 single source of truth

LobsterPulse 必須定義 `TriggerRule` 結構包含 `id` / `enabled` / `description` /
`when: RuleWhen` / `then: Vec<RuleAction>`, 序列化進 `AppConfig.rules: Vec<TriggerRule>`。
`RuleWhen` 含 `provider: Option<String>` / `event: Option<String>` /
`state_to: Option<String>` 三個 Option 欄位 (None = any)。
`RuleAction` 鎖 3 種變體: `Toast { title, body }` / `Sound { clip, volume }` /
`Log { file, format }` (`format` 限 `Jsonl` 或 `Plain`)。

#### Scenario: TriggerRule JSON round-trip 保留所有欄位

`serde_json::to_string(&rule)` 然後 `serde_json::from_str` 必須等價還原。
`enabled: false` / `provider: None` / `then: []` 等邊界值都不可丟失。

#### Scenario: AppConfig.rules 預設 3 條

`AppConfig::default().rules.len() == 3`, 3 條 id 前綴為
`r115-default-claude-completed` / `r115-default-waiting-toast` /
`r115-default-failure-log` (對齊 design.md 表)。

### Requirement: R-2 — evaluate_rules 在 SessionManager handle_event 結尾執行, 觸發 rule-fired emit

`SessionManager::handle_event` 結尾必須呼叫 `evaluate_rules(&event, &transition)`,
對 `self.config.rules` 中每條 `enabled=true` 規則執行 `RuleWhen::matches` 判斷,
匹配時對每個 action emit Tauri event `rule-fired` (payload 帶 rule_id + action 細節)
或寫 log (Log action 走 `tokio::fs::OpenOptions::append`, 不 emit)。

#### Scenario: 3 預設規則在典型事件流下各匹配至少 1 次

跑 13 種事件 (每個 provider 1 條 Stop + 1 條 PostToolUseFailure + 1 條
Notification 觸發 WaitingForUser, 共 39 條), evaluate_rules 後
`rule_match_count == 3` (對應 3 預設規則各至少 1 次匹配)。

#### Scenario: rule-fired emit 包含 action 細節 payload

emit payload 必須含 `rule_id: String` + `action_kind: "toast" | "sound" | "log"`
+ 對應 action 的所有欄位 (toast 帶 title/body, sound 帶 clip/volume, log
帶 file/format), 缺任一欄位 fail 此 scenario。

### Requirement: R-3 — RuleWhen::matches 三條件 AND, 任一 None 跳過

`RuleWhen::matches(event, transition)` 必須對 `provider` / `event` /
`state_to` 三欄位 AND 比對, 任一欄位為 `None` 跳過該欄位, 全部通過回傳
`true`。

#### Scenario: provider=Some("cicx") 不匹配 event.provider_id="claude"

`RuleWhen { provider: Some("cicx"), .. }` 對 event.provider_id="claude" 回
`false`, 對 event.provider_id="cicx" 回 `true`。

#### Scenario: event=Some("Stop") 不匹配 event.event_name="SessionStart"

`RuleWhen { event: Some("Stop"), .. }` 對 event.event_name="SessionStart"
回 `false`, 對 event.event_name="Stop" 回 `true`。

#### Scenario: state_to=Some("Completed") 不匹配 transition=StartedWaiting

`RuleWhen { state_to: Some("Completed"), .. }` 對 transition=StartedWaiting
回 `false`, 對 transition=Completed 回 `true`。

#### Scenario: 三欄位全 None 匹配任何 event

`RuleWhen { provider: None, event: None, state_to: None }` 對任意
event + transition 都回 `true` (萬用規則)。

### Requirement: R-4 — Tauri command 4 條 (list_rules / toggle_rule / add_rule / remove_rule)

`lib.rs` 必須註冊 4 條 Tauri command 給前端呼叫:
- `list_rules() -> Vec<TriggerRule>`
- `toggle_rule(id: String, enabled: bool) -> Result<(), String>`
- `add_rule(rule: TriggerRule) -> Result<(), String>` (id 由 caller 傳入, 不自動生)
- `remove_rule(id: String) -> Result<(), String>`

#### Scenario: list_rules 回傳目前 AppConfig.rules 完整列表

`list_rules()` 必須等於 `state.config.lock().rules.clone()`, 順序保持。

#### Scenario: toggle_rule 反轉 enabled 旗標, 持久化

`toggle_rule("r115-default-waiting-toast", false)` 後 `list_rules()` 該條
`enabled == false`, 並呼叫 `save_config` 寫回 `~/.config/lobsterpulse/config.json`。

#### Scenario: add_rule 補進 Vec 尾端, 同 id 已存在回 Err

`add_rule(rule)` 必須 `rule.id` 不重複才加入, 重複回
`Err(format!("rule id {} already exists", rule.id))`。

### Requirement: R-5 — 護衛 test 3 條守住不擴張既有 chain

新增 3 條獨立 Rust test (不併入 R66/R82 護衛鏈):
- `r115_rule_evaluation_match_count`
- `r115_rule_action_emission`
- `r115_rule_when_filter`

#### Scenario: r115_rule_evaluation_match_count 在 39 條事件下 == 3

跑完 39 條事件後 `session.rule_match_count == 3`, fail 報實際值。

#### Scenario: r115_rule_action_emission 統計 toast ≥ 2 + sound ≥ 1 + log ≥ 1

跑完 39 條事件後 emit 統計 `toast_count ≥ 2` (預設 1+2) + `sound_count ≥ 1`
(預設 1) + `log_count ≥ 1` (預設 3), 任一不符 fail。

#### Scenario: r115_rule_when_filter 三條件 AND 全覆蓋

建 8 條 RuleWhen 組合 (2×2×2 provider×event×state_to), 配對 8 條對應 /
不對應 event, 逐一驗 `matches` 結果, 任一不符 fail。
