# Tasks: R114 K0 Quota coverage + dual-emit value-equality guard

- [ ] **T-R114.1: 寫 proposal.md** — 目標 + 背景 + 範圍 + capabilities 段齊
- [ ] **T-R114.2: 寫 design.md 三段改動一覽** — 段 A R113.1 guard 設計 / 段 B KNOWN_PROVIDERS pub const 設計 / 段 C k0_measure K0-Q + openx alias 設計
- [ ] **T-R114.3: 寫 spec.md ADDED Requirements** — 4 個 Requirement + 8 個 Scenario（R113.1 value-equality guard / K42 不擴張 / KNOWN_PROVIDERS pub const / 既有護衛不破壞 / K0-Q JSON+console / 既有 K0-A1/A2/B 不破壞 / openx 雙 base name / openx 修前修後差）
- [ ] **T-R114.4: 寫 .openspec.yaml metadata** — schema/id/created/updated/status
- [ ] **T-R114.5: 寫 tasks.md** — 本檔
- [ ] **T-R114.6: lib.rs 加 R113.1 護衛 test** — `render_prometheus_body_dual_emit_values_match_per_provider`（6 條 dual-emit pair HashMap 全等 assertion）
- [ ] **T-R114.7: hook_server.rs `const` → `pub const`** — KNOWN_PROVIDERS SSoT 預備
- [ ] **T-R114.8: k0_measure.py openx legacy alias 修** — `scan_quota_snapshots` openx 加 `usage-bot` 第二個 base name
- [ ] **T-R114.9: k0_measure.py 加 K0-Q 維度** — `main` 加 `k0q_quota_coverage` JSON output + console 印
- [ ] **T-R114.10: 跑 cargo test --lib 確認 437/437 持續綠** — 護衛 chain 17 → 17 不擴張守住
- [ ] **T-R114.11: 跑 python scripts/k0_measure.py 確認 K0-Q 9/13 + openx 計入** — 修前修後對照
- [ ] **T-R114.12: 3 個 commit 落地 + engineering-log R114 紀錄** — commit 1 fix(metrics) / commit 2 feat(scripts) / commit 3 refactor(hook_server) + KPI 進展表
- [ ] **T-R114.13: 收 closure** — tasks.md 13 個 [x] 全勾 + .openspec.yaml status=closed + phase=1/1
