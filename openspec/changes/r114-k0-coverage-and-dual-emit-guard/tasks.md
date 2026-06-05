# Tasks: R114 K0 Quota coverage + dual-emit value-equality guard

> **狀態對齊 (R114 closure)**：T-R114.1-6 落地 (commit `6551953`)，T-R114.7-13 接力完成
> (commit R114-{2,3,4} 落地)。K42 chain 17→17 不擴張守住，K0 Quota 8/13 → 9/13 (openx
> alias 修: cicx/gitx/giminix/codex_bot/openx 5 stale + 4 fresh 從 usage-local.json +
> copilot 走 R109 live quota = 9 covered, 4 missing = irisx_bot/grokx/lpbot/mimo 仍
> 缺 OpenAB snapshot 寫入鏈路非本機 scope)。

- [x] **T-R114.1: 寫 proposal.md** — 目標 + 背景 + 範圍 + capabilities 段齊 (commit `6551953`, 100 行)
- [x] **T-R114.2: 寫 design.md 三段改動一覽** — 段 A R113.1 guard 設計 / 段 B KNOWN_PROVIDERS pub const 設計 / 段 C k0_measure K0-Q + openx alias 設計 (commit `6551953`, 158 行)
- [x] **T-R114.3: 寫 spec.md ADDED Requirements** — 4 個 Requirement + 8 個 Scenario（R113.1 value-equality guard / K42 不擴張 / KNOWN_PROVIDERS pub const / 既有護衛不破壞 / K0-Q JSON+console / 既有 K0-A1/A2/B 不破壞 / openx 雙 base name / openx 修前修後差）(commit `6551953`, 111 行)
- [x] **T-R114.4: 寫 .openspec.yaml metadata** — schema/id/created/updated/status (commit `6551953`, status=closed phase=1/1)
- [x] **T-R114.5: 寫 tasks.md** — 本檔
- [x] **T-R114.6: lib.rs 加 R113.1 護衛 test** — `render_prometheus_body_dual_emit_values_match_per_provider`（6 條 dual-emit pair HashMap 全等 assertion,commit `6551953` lib.rs +133 行, cargo test --lib 437/437 綠, chain 17→17 不擴張守住)
- [x] **T-R114.7: hook_server.rs `const` → `pub const`** — KNOWN_PROVIDERS SSoT 預備 (R111 接力 commit R114-3, 給 `lib.rs` `get_provider_coverage_report` 引用鋪路, 5 行改動)
- [x] **T-R114.8: k0_measure.py openx legacy alias 修** — `scan_quota_snapshots` openx 加 `usage-bot` 第二個 base name (R111 接力 commit R114-2, 對齊 hook_server.rs:376-378 別名語意)
- [x] **T-R114.9: k0_measure.py 加 K0-Q 維度** — `main` 加 `k0q_quota_coverage` JSON output + console 印 (R111 接力 commit R114-2, 對齊 MISSION 13/13 目標)
- [x] **T-R114.10: 跑 cargo test --lib 確認 438/438 持續綠** — 護衛 chain 17 → 17 不擴張守住 (R111 接力驗證, R114 M0 加 1 test 從 437→438)
- [x] **T-R114.11: 跑 python scripts/k0_measure.py 確認 K0-Q 9/13 + openx 計入** — 修前 K0-Q 8/13 (openx 漏算 missing) → 修後 9/13 (openx 計入 stale), +1 從 openx alias 修 (R111 接力驗證)
- [x] **T-R114.12: 3 個 commit 落地 + engineering-log R114 紀錄** — commit R114-1 (6551953 fix metrics) / commit R114-2 (feat scripts) / commit R114-3 (refactor hook_server) + engineering-log R111 R114 接力紀錄
- [x] **T-R114.13: 收 closure** — tasks.md 13 個 [x] 全勾 + .openspec.yaml status=closed + phase=1/1 (本 commit R114-4)
