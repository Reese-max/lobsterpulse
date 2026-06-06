# LobsterPulse — KPI 量測歷史

> **本檔是 MISSION.md 90 天 KPI 的歷史補頁落點**。MISSION.md 主表只留 R81 baseline + 最新欄位；中間的 R108~R130 各輪補敘述整段搬進本檔，恢復 MISSION 決策可讀性。
>
> **書寫約定**：每輪補 1 段，標題 `[YYYY-MM-DD] Rxxx — <一句話標題>`，段內 3 行：為什麼 / 量化結果 / 對下一輪影響。
> **不要**把補段寫回 MISSION.md —— MISSION 是「決策錨點」，歷史屬本檔。

---

## R108 — K0 Quota 數字從 6 OpenAB → 拆 K0-B fresh / K0-Q 兩維

**為什麼**: supervisor 報 `consecutive_drifts: 3` + top_risk = 「K0 Quota 數字」。追源頭發現 MISSION/CLAUDE 量化值停在 R81、跟現實分叉（R81 寫 6 OpenAB 現 5 stale + 1 fresh）。

**量化**:
- K0-B fresh 4/13 (4 本機 CLI: claude/codex/copilot/gemini)
- K0-Q 8/13 (4 fresh + 4 stale OpenAB: cicx/gitx/giminix/codex_bot + legacy bot)
- 1 個 `usage-*.json` 從未寫入 → K0 Quota 真正缺 1

**下一輪影響**: 拆 K0-B (fresh) / K0-Q (snapshot 存在) 兩維口徑，後續補段都對齊這兩個分母。

---

## R109 — copilot live quota 落地 (4/4 本機 CLI 滿覆蓋)

**為什麼**: 缺 copilot live quota → 本機 CLI 段 K0-B 3/4 → 4/4 滿覆蓋。

**量化**:
- K0-B fresh 3/4 → **4/4** (copilot Tauri command 新增)
- K0-Q 8/13 → 9/13 (copilot 計入 fresh)
- 缺 4 個 OpenAB bot (`irisx_bot` / `grokx` / `lpbot` / `mimo` 完全 missing — 仍非本機 scope)

**下一輪影響**: 本機 CLI 段 100% 滿覆蓋已達標；K0 全 13/13 卡在 OpenAB 端。

---

## R111 — `/metrics` 端點復活 (K0-A1 emit 0/13 → 5/13)

**為什麼**: k0_measure 端點 DOWN (main app 沒跑) ≠ emit 邏輯壞。R111 端點活著就復活。

**量化**:
- K0-A1 emit 0/13 → **5/13** (claude/codex/copilot/gemini/cicx 5 label 端點實際 emit)
- K0-A2 sample 0/13 → **2/13** (claude=11 + cicx=1 真有 sessions 累加)
- 端點 DOWN ≠ emit 邏輯壞：純粹是 main app 沒跑沒在 emit

**下一輪影響**: K0-A1/A2 卡 OpenAB bot 事件流，需 OpenAB 端跑起來才有 13/13 真正達成 — **非本機 scope**。

---

## R114 — openx legacy alias 修 (K0-Q 8/13 → 9/13)

**為什麼**: `usage-bot.json` (legacy `BackendType::Other`) 從未被 k0_measure 認到，openx 永遠 missing。修 `parse_provider("bot") → "openx"` rewrite。

**量化**:
- K0-Q 8/13 → **9/13** (openx 從 missing 變 stale, +1)
- K0 程式碼定義層 13/13 持續 (R101 達標守住)
- K0-B fresh 4/13 持平

**下一輪影響**: openx snapshot 終於接上 data path；剩 4 missing (`irisx_bot` / `grokx` / `lpbot` / `mimo`) 仍非本機 scope。

---

## R119 — K42 chain 17 → 19 (R97 飽和契約 +2 例外)

**為什麼**: R122 ship `timeline::tests` mod (跨 mod 邊界護衛) + R127 ship `.gitignore` content 護衛，K42 chain 突破紅線 17 → 19。

**量化**:
- K42 chain 17 → **19** (R97 後 +2 例外, 架構理由明確)
- baseline 446/446 全綠
- K0 量化 5/1/4/9 持平 R114

**下一輪影響**: R97 飽和契約例外速率「< +1/2 輪」紅線觸發監督，後續加護衛須有跨 mod 邊界架構理由。

---

## R122 — timeline 護衛 (R97 後 +1 例外)

**為什麼**: cross-provider-timeline 護衛 test 3 條 (`timeline_ring_buffer_invariants` / `timeline_ring_state_alignment_with_session` / `timeline_jump_target_contract`) 跨 mod 邊界 (timeline.rs ↔ session.rs ↔ lib.rs)，既有 R97 飽和 chain 沒涵蓋。

**量化**:
- K42 chain 18 → **19** (R97 後 +2 例外: R122 + R127)
- baseline 446/446 → 447/447 (R113 T-CPT9 lib.rs 護衛 1 條)
- K0 量化 持平

**下一輪影響**: R-CPT M1 T-CPT8 (session.rs 串接 record_event) 接力順位給 owner M。

---

## R127 — .gitignore 護衛 (R97 後 +1 例外)

**為什麼**: 拓荒 .gitignore content 護衛 (避免敏感檔入 repo)，跨 mod 邊界 (build artifacts / config secrets)。

**量化**:
- K42 chain 18 → **19** (R97 後 +2 例外: R122 + R127)
- baseline 447/447 全綠

**下一輪影響**: K42 紅線觸發「< +1/2 輪」監督；後續護衛需明確架構理由。

---

## R128 — T-CPT10 frontend 端到端 ship (R-CPT M1 8/8 closure)

**為什麼**: R-CPT M1 接力鏈卡在 T-CPT10 (main.js 第 6 視圖) 6+ 輪，是 owner M 拖最大未 ship 件。R128 走「純 frontend 6 視圖擴張」最小切面 ship。

**量化**:
- src/index.html: 加 `<div id="view-timeline">` (29 行)
- src/styles.css: 加 `--stale-color` + 11 條新 class (171 行)
- src/main.js: renderTimeline + showView + 5s auto-refresh + cell click 跨視圖 jump (155 行, 1 行替換)
- K42 chain 19 → 19 守住 (純 frontend, 0 護衛 +1)
- baseline 447/447 → 448/448 (R122 timeline::tests mod 護衛 2 條)
- K0 量化 持平 R119

**下一輪影響**: R-CPT M1 進度條 7/8 → 8/8 closure；R130 spec drift 翻 T-CPT10 [x]。

---

## R130 — T-CPT10 spec closure 接力 + MISSION R130 column 補對齊

**為什麼**: R128 commit a0e02f1 真 ship main.js 第 6 視圖，但 R-CPT tasks.md T-CPT10 仍寫 `[ ]` (R119 closure 接力時 T-CPT10 還沒 ship)，形成「實跑已 ship / spec 仍 [ ]」分叉。

**量化**:
- `openspec/changes/cross-provider-timeline/tasks.md`: 翻 T-CPT10 [x] 對齊 R128 真 ship
- MISSION.md: 補 R130 column + R131+ 候選
- K42 chain 19 條持平 R119
- baseline 448/448 全綠

**下一輪影響**: R-CPT change 整體 closure 收 (.openspec.yaml status 翻 closed) + R131+ 接力 K0-A1 emit 5/13 → 6/13 護衛 + R117 capsule-brief JS 配套等 owner M 收。

---

## R131 — 4 missing bot 結構性量化確認 (0 spec drift)

**為什麼**: 監督者警示「連 2 輪沒改善」+ R130 spec closure 後 4 missing bot 量化值讓人懷疑可能 spec drift。R131 換軸做「4 missing bot 在 codebase 真實狀態結構性量化」。

**量化**:
- 4 missing bot (`irisx_bot` / `grokx` / `lpbot` / `mimo`) 在本機端 13/13 全部已對齊 KNOWN_PROVIDERS + 4 同步點 + parse_provider 護衛 + read path
- 4 missing 物理原因：OpenAB 端 `usage-{bot}.json` snapshot 從未寫入 (OpenAB 端進程未啟動推送，純 runtime 物理事實)
- MISSION 量化值口徑與 code 真實一致
- K0 5/1/4/9 持平 (結構性確認 0 drift)
- 0 code 0 spec 0 髒檔污染

**下一輪影響**: R131+ 接力清單新維度：(a) main.js 結構性分層 plan 量化；(b) docs/demo-app E2E 護衛；(c) R97 飽和契約例外速率監控。

---

## R132 — R-CPT 整體 15/15 closure + K40 9/9 + K42 chain 20 + baseline 451 量化對齊

**為什麼**: R135 commit 6dfa66b .gitignore 補網 __pycache__/ 收網後，9 個 openspec change 已全 closed（含 cross-provider-timeline phase 2/2 15/15 跟 prometheus-counter-rename-2026-q3 phase 1/1 6/6），但 MISSION R131 column 量化值仍寫「K40 7/7 持續 + R-CPT 整體待 R131+ 收 closure」、「K42 19 條持平 R119」，結構性跟現實分叉。R119 PUA 換角度 (2 輪沒改善) 翻轉軸做「事實驅動結構性審計」發現此 spec drift，類 R108 M0 修 k0_measure.py docstring drift 同性質。

**量化**:
- `openspec/changes/*/` 9 個 change 全 closed (contract-matrix-guard 8/8, cross-provider-timeline 15/15, lobster-rules-engine 25/25, openab-bot-sync 12/12, otel-provider-metrics-contract 9/9, prometheus-counter-convention 8/8, prometheus-counter-rename-2026-q3 6/6, r114-k0-coverage-and-dual-emit-guard 13/13, archive 0/0)
- MISSION.md K40 規格覆蓋率 7/7 → **9/9** 對齊 9 個 change 全 closed (R-CPT 跟 prometheus-counter-rename 之前未量化入 K40 統計口徑，本輪補入)
- MISSION.md K42 護衛 chain 19 → **20** (R131 ship plugin registry 護衛 +1 走既 `auto_rules::tests` mod, R97 後 +3 例外架構理由明確; R135 .gitignore 補網 __pycache__/ 護衛 test +1 走既護衛, chain 不擴張)
- baseline `cargo test --lib` 450/450 → **451/451** (R135 +1 test 守 `__pycache__/` token 確認)
- 0 code 0 髒檔污染 (R13 6 owner M dirty 一個未動)
- clippy 0 / fmt 0 diff 守住

**下一輪影響**: R133+ 接力清單 4 條：(a) 護衛 過期契約審計 (護衛對應 spec 最後更新時間掃描); (b) R117 capsule-brief JS 配套等 owner M 收; (c) K0 Quota 4 missing 補鏈路 (OpenAB scope); (d) K0-A1 emit 5/13 → 6/13 護衛。
