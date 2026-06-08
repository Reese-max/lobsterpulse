# 護衛契約審計 (R137, 2026-06-08)

> 來源: R137 PUA 換角度, 1 輪 1 件, 走 R133+ 接力清單「護衛對應 spec 最後更新時間審計」。
> 不寫 meta-discussion, 寫量化審計事實。

## 1. 護衛 chain 20 vs 程式碼護衛 module 對照 (事實清單)

護衛 chain count 20 (MISSION.md K42 補頁, R132 持平 R131 護衛鏈策略) 走
`auto_rules::tests` mod 既有護衛 3 條 + 獨立護衛 module 11 條 (個別 #[cfg(test)] mod
不以「chain 飽和」算新 chain)。下表列 9 條護衛 contract spec change 對應的護衛 module。

| # | 護衛 contract spec (openspec/changes/) | 護衛 module (Rust) | 護衛 test count | 護衛 spec mtime (epoch s) | 護衛 spec age (天, 對 2026-06-08) |
|---:|---|---|---:|---:|---:|
| 1 | openab-bot-sync | lib.rs:818 read_usage_snapshot + openab_bridge.rs:336 read_events_since + openab_bridge.rs:582 dispatch_event | 4+5+4 = 13 | 1780564540 | 4.0 |
| 2 | otel-provider-metrics-contract | lib.rs:3960 render_prometheus_tests | 9 | 1780629595 | 3.4 |
| 3 | contract-matrix-guard | config.rs:1018 provider_contract_matrix_tests | 1 | 1780647220 | 3.2 |
| 4 | prometheus-counter-convention | lib.rs:3960 render_prometheus_tests (與 #2 共用 mod) | (同 #2) | 1780647709 | 3.2 |
| 5 | r114-k0-coverage-and-dual-emit-guard | lib.rs:3960 render_prometheus_tests (與 #2 共用 mod) | (同 #2) | 1780674202 | 3.0 |
| 6 | lobster-rules-engine | config.rs:1432 r115_rule_engine_config_tests + auto_rules.rs:1626 tests | 4+3 = 7 | 1780706294 | 2.8 |
| 7 | cross-provider-timeline | timeline.rs:178 tests (R122 + R131 7d buffer 護衛) | 2 | 1780719488 | 2.8 |
| 8 | prometheus-counter-rename-2026-q3 | lib.rs:3960 render_prometheus_tests (與 #2 共用 mod) | (同 #2) | 1780719511 | 2.8 |
| 9 | otel-genai-runtime-emit-2026-q3 (active 9/16) | (none — Phase 1 純 spec, T-OGRE10~16 護衛 code owner M scope) | 0 | 1780758255 | 2.6 |

> **備註**: lib.rs 內部另有 4 條獨立護衛 module (lib.rs:1173 collect_live_quota_snapshot_tests,
> lib.rs:3898 write_local_usage_snapshot_tests, lib.rs:11817 lib_warn_msg_tests,
> lib.rs:11851 r74_play_sound_file_fallback_tests, lib.rs:11940 r127_daemon_exclusion_gitignore_tests,
> lib.rs:12047 r131_plugin_registry_tests) 不對應獨立 spec change (走 R74/R127/R131 inline 護衛,
> 護衛 spec 寫在護衛 module doc comment 而非 openspec spec.md)。

## 2. 量化審計結論 (4 項)

1. **9 個護衛 contract spec 全部 age < 5 天** — 護衛 spec drift 風險低, 沒有 spec 超 1 週未更新。
   護衛 spec 最舊: openab-bot-sync 4.0 天, 最新: otel-genai-runtime-emit-2026-q3 2.6 天。
2. **1 個 active change (otel-genai) 護衛 code = 0 條** — T-OGRE10~16 7 條護衛 code 任務
   owner M scope (R141 spec 接力), 本機不動。
3. **render_prometheus_tests 護衛 mod 共用** (4 個 spec change 都落此 mod) — 符合 R115
   護衛鏈策略「走既 mod 不開新 mod」, chain 20 → 20 守住。
4. **護衛 chain 20 vs 護衛 module 11 條 (獨立 mod)** — K42 護衛 chain 20 含「同 mod 多條護衛
   test」拆分 (e.g. auto_rules::tests 內 3 條獨立護衛), 不是 1 mod = 1 chain。

## 3. 結構性發現 (3 條, owner M 簽收)

| # | 結構性發現 | 影響 | 建議 |
|---:|---|---|---|
| F1 | render_prometheus_tests 1 個 mod 對 4 個 spec change (#2/#4/#5/#8) | 護衛失敗時, 4 個 spec 哪個 spec 對應哪條護衛 test 需人工 grep | 護衛 fn name 加 `r###_` prefix 對齊 spec change id (e.g. `r102_metric_naming` / `r130_prometheus_rename` / `r114_dual_emit`) — owner M 簽收 |
| F2 | otel-genai-runtime-emit-2026-q3 護衛 code 0 條 | spec 對應護衛 code 缺, spec closure 不算完整 ship | 等 owner M T-OGRE10~16 ship (R141 接力 7 條 placeholder) |
| F3 | inline 護衛 (lib.rs 6 條獨立 mod) 護衛 spec 寫在護衛 doc comment | 護衛 spec 不在 openspec spec.md, 護衛「可發現性」低 (grep 才找得到) | 補 openspec spec.md (R74/R127/R131) 或接受 inline 護衛 (架構理由: 護衛 logic 跟 code 同檔可讀性高) |

## 4. KPI 進展表 (R137)

| KPI | 前值 (R132) | 後值 (R137) | 變化 |
|---|---:|---:|---:|
| K42 護衛 chain 數 | 20 | 20 | 0 (守住) |
| 護衛 contract spec 數 (openspec spec.md) | 9 | 9 | 0 (8 closed + 1 active) |
| 護衛 spec age < 5 天比例 | 未量測 | 9/9 = 100% | new |
| 護衛 module 數 (獨立 mod) | 未量測 | 11 條獨立 mod + 3 條共用 mod | new |
| 結構性發現 owner M 簽收 | n/a | 3 條 (F1/F2/F3) | new |

## 5. 為什麼這輪換角度 (對齊 MISSION R133+ 接力)

R132~R136 共 5 輪 (含 R151/R150-2/R150) 結構性飽和路徑圖 / doc-level closure / 12-step
sign-off conditions / commit history 結構性品質 / HARNESS 3 條提示事實驅動復盤 — 全部
meta-discussion。R137 換角度為**量化護衛契約審計事實**:

- 護衛 chain 20 ≠ 護衛 module 11: 量化解構差異 (第 1 點結論 4)
- 護衛 spec age 全 < 5 天: 量化護衛 spec drift 風險 (結論 1)
- 1 active change 護衛 code = 0: 量化 owner M scope 邊界 (結論 2)
- 4 個 spec change 共用 1 個 mod: 量化護衛鏈策略合規 (結論 3)

3 條結構性發現 (F1/F2/F3) 全部留 owner M 簽收, 不硬 ship, 不破 R97 紅線 (chain 20→20 守住)。
