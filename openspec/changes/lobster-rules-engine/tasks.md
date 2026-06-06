# Tasks: Lobster Rules Engine

> 對應 change: `lobster-rules-engine`
> 護衛 chain: 不擴張 (R66/R82 飽和契約 R113.1 守住), 走 3 條獨立 test

## 1. Spec 文件

- [x] **T-1: 寫 proposal.md** — Goal + Background + Scope (In/Out) + 北極星對齊 + 風險 (R115 接力)
- [x] **T-2: 寫 design.md** — TriggerRule schema + 3 預設規則表 + evaluate_rules 流程 + 護衛 test 3 條 + 跨同步點
- [x] **T-3: 寫 spec.md** — 5 個 ADDED Requirement (R-1~R-5) + 9 個 Scenario
- [x] **T-4: 寫 .openspec.yaml** — schema/id/created/status/phase metadata
- [x] **T-5: 寫 tasks.md** — 本檔

## 2. config.rs 落地

- [x] **T-6: 加 rule_engine module** — TriggerRule / RuleWhen / RuleAction 結構 + Default impl + serde
- [x] **T-7: AppConfig 加 rules: Vec<TriggerRule> + rules_enabled: bool** — forward migration 對齊 R114
- [x] **T-8: 寫 default_rules() const** — 3 預設規則 SSoT (id 前綴 r115-default-*)

## 3. session.rs 落地

- [x] **T-9: SessionManager 加 rule_match_count 計數欄位**
- [x] **T-10: handle_event 結尾串接 evaluate_rules(&event, &transition)** — 既有 emit task-completed/waiting 之後
- [x] **T-11: RuleWhen::matches 三條件 AND 實作** — provider / event / state_to 各 Option 跳過

## 4. lib.rs 落地

- [x] **T-12: 註冊 4 條 Tauri command** — list_rules / toggle_rule / add_rule / remove_rule
- [x] **T-13: list_rules 從 state.config.lock() 讀**
- [x] **T-14: toggle_rule / add_rule / remove_rule 走 save_config 持久化**

## 5. 前端落地 (src/index.html)

- [x] **T-15: 設定頁加 Rules section** — 顯示現有 rules + toggle + 刪除按鈕
- [x] **T-16: 「+ 新增規則」UI** — dropdown 選 provider + event + state_to, 預設填 Toast action
- [x] **T-17: JS 接 4 個 tauri command** — list_rules 載入, toggle / add / remove 呼叫對應 command

## 6. 護衛 test

- [x] **T-18: 寫 r115_rule_evaluation_match_count** — 39 條事件下 count == 3
- [x] **T-19: 寫 r115_rule_action_emission** — toast ≥ 2 + sound ≥ 1 + log ≥ 1
- [x] **T-20: 寫 r115_rule_when_filter** — 8 條 RuleWhen 組合 × 對應/不對應 event 配對

## 7. 驗證 (CLAUDE.md 「宣稱完成前必須驗證」)

- [x] **T-21: cargo tauri build --no-bundle** — release build 不白屏
- [x] **T-22: cargo test** — baseline 443/443 守住（437+6 R115）, R66/R82 護衛鏈不退
- [x] **T-23: clippy --all-targets -- -D warnings** — 0 warning
- [x] **T-24: rg "TODO|FIXME" src-tauri/src/** — 0 hit
- [ ] **T-25: smoke 跑 sidecar** — `echo '{...}' | ./lobster-pulse-hook claude` 觸發 1 條預設規則
