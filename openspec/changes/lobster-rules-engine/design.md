# Design: Lobster Rules Engine

## TriggerRule schema (single source of truth)

> Code-level 落地: `src-tauri/src/config.rs` `rule_engine` module
> 跨 3 同步點: `AppConfig.rules` (config.rs) / `SessionManager::evaluate_rules`
> (session.rs) / `list_rules` tauri command (lib.rs)

### Rule 結構

```rust
pub struct TriggerRule {
    pub id: String,                // uuid v4, 穩定 id
    pub enabled: bool,             // master per-rule toggle
    pub description: String,       // 使用者自填 label, 預設 3 條有 preset label
    pub when: RuleWhen,
    pub then: Vec<RuleAction>,     // 1 條規則可同時觸發多 action
}

pub struct RuleWhen {
    pub provider: Option<String>,  // None = any provider (對齊 13 provider 全覆蓋)
    pub event: Option<String>,     // HookEvent name (Stop / SessionStart / ...),
                                   // None = any event
    pub state_to: Option<String>,  // SessionTransition 字串 ("Completed" /
                                   // "StartedWaiting" / "None"), None = any
}

pub enum RuleAction {
    Toast { title: String, body: String },
    Sound { clip: String, volume: f32 },
    Log   { file: String, format: LogFormat },  // format: Jsonl | Plain
}
```

### 3 預設規則 (AppConfig.rules 預設值)

| id_prefix | description | when | then |
|---|---|---|---|
| `r115-default-claude-completed` | Claude Stop → 提醒我回來看 | provider=`claude`, event=`Stop`, state_to=`Completed` | `Toast{title:"✅ Claude 完成", body:"{project_name}"}`, `Sound{clip:"claude.mp3", volume:1.0}` |
| `r115-default-waiting-toast` | 任何 provider 進入 WaitingForUser → toast | provider=None, state_to=`StartedWaiting` | `Toast{title:"⏳ {provider} 在等你回", body:"{project_name}"}` |
| `r115-default-failure-log` | 任何 provider PostToolUseFailure → 寫 log | event=`PostToolUseFailure` | `Log{file:"~/.lobsterpulse/audit.jsonl", format:Jsonl}` |

預設規則 1 對齊既有 task-completed 音效 (R78 spec), 預設 2 加 Windows toast
提示使用者「agent 在等」, 預設 3 把失敗事件持久化供 debug。

## evaluate_rules 流程 (SessionManager)

```
SessionManager::handle_event(event) -> SessionTransition
  ├─ (既有) provider_totals 累加 (R78)
  ├─ (既有) 狀態機轉移 (Working / WaitingForUser / Idle / Stale)
  ├─ (既有) emit task-completed / task-waiting
  └─ (新增) evaluate_rules(event, transition):
       for rule in self.config.rules.iter().filter(|r| r.enabled) {
         if rule.when.matches(event, &transition) {
           for action in &rule.then {
             match action {
               Toast{..}   => self.app_handle.emit("rule-fired", RuleFiredPayload::Toast{..}),
               Sound{..}   => self.app_handle.emit("rule-fired", RuleFiredPayload::Sound{..}),
               Log{..}     => append_jsonl(&action.file, &payload),
             }
             self.rule_match_count += 1;  // 護衛 test 計數用
           }
         }
       }
```

### 匹配語意

`RuleWhen::matches(event, transition)` 三條件 AND:
- `provider`: `Some(p)` 比對 `event.provider_id == p`, `None` 跳過
- `event`: `Some(e)` 比對 `event.event_name == e`, `None` 跳過
- `state_to`: `Some(s)` 比對 `format!("{:?}", transition) == s`, `None` 跳過

## 護衛 test (3 條, 不擴張 R66/R82 chain)

1. **`r115_rule_evaluation_match_count`**: 模擬 13 種典型事件流 (每個
   provider 1 條 Stop / 1 條 PostToolUseFailure / 1 條 Notification 觸發
   WaitingForUser), 跑完 evaluate_rules 後 `rule_match_count == 3` (3 預設
   規則各至少 1 次匹配)

2. **`r115_rule_action_emission`**: 跑同樣 13 種事件, 統計 emit
   `rule-fired` 的 action 類型, 必須 `toast ≥ 2` (預設 1+2 各一) +
   `sound ≥ 1` (預設 1) + `log ≥ 1` (預設 3)

3. **`r115_rule_when_filter`**: 驗 `RuleWhen::matches` 三條件 AND: provider
   設 `Some("cicx")` 時 event.provider_id="claude" 不匹配; event 設
   `Some("Stop")` 時 event.event_name="SessionStart" 不匹配; state_to 設
   `Some("Completed")` 時 transition=StartedWaiting 不匹配; 任一 None 跳過
   該條件

## 跨同步點對齊

| 同步點 | 內容 | reference |
|---|---|---|
| `AppConfig.rules` Vec<TriggerRule> | 配置持久化 | `~/.config/lobsterpulse/config.json` |
| `default_rules()` const | 3 預設規則 SSoT | `config.rs` `rule_engine` module |
| `SessionManager::evaluate_rules` | 規則匹配 + emit | `session.rs` handle_event 結尾 |
| `list_rules` tauri command | 給前端讀 | `lib.rs` invoke_handler |

## Forward migration (對齊 R114 pattern)

`load_config` 用 `.and_modify(|c| c.rules = default.rules.clone())` 強制刷新
預設規則但保留使用者自訂規則。`rules_enabled: bool` master switch 預設 `true`,
既有 config.json 缺欄位時補 `true`。

## 不做的事 (Out of Scope 對齊 proposal)

- ❌ Webhook action (網路 IO + retry, 獨立 spec)
- ❌ Cooldown / 去重 (進階規則, 留 R116+)
- ❌ Aggregate trigger (跨 session, 留 R117+)
- ❌ 規則 import/export (留 R118+)
- ❌ 規則視覺化編輯器 (drag-drop, 留 R119+)
- ❌ 動態新增 action type (MVP 鎖 3 種: Toast / Sound / Log)
