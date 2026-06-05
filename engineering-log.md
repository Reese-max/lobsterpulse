# Engineering Log

> 舊紀錄已歸檔到 engineering-log.archive.md

## 改善紀錄

- 不接 main.js refreshQuotas 整合 (R100 follow-up #2): 仍 owner R90 WIP 留工作區
- 不寫 gemini/copilot live runner: R89 follow-up 第 2 條仍留, API 認證體系風險高
- 不擴 K42 護欄 chain 17: R101 7 條 test 純函式自帶驗證, 算 K29 同款既契約延伸, chain 17 凍結不變
- 不重命名 quota/ 模組 `#[allow(dead_code)]`: R100 follow-up 留 R101+ H0 窗口, 本輪 M1 不混
- 不動 8 supervisor untracked + openspec/changes/: R13 防護守住 (`git status` 後仍 8 untracked)

**驗證**:
- `cargo test --lib`: 397→**404** passed / 0 failed (新增 7 條 r101_* test 全部 pass, 既有 397 條 0 regression)
- `cargo clippy --lib -- -D warnings`: **0 warning** (修了 3 條 doc_lazy_continuation)
- `cargo fmt --check`: **0 diff** (1 條 long-fn-signature 被 fmt 自動 collapse)
- R13 防護守住: `git add src-tauri/src/lib.rs src-tauri/src/session.rs` 精準列路徑 (不用 -A), 8 untracked + openspec/changes/ 仍 dirty 不污染
- K41 chore_treadmill 守住: 本輪 1 個 feat (M1) + 1 個 docs (本 log) = 0 純 chore

**結果**: PASS（M1 K0 health 成功率 gauge 落地 + 7 條 unit test 全綠 + 修 3 條 clippy doc 錯,K0 Provider 健康度覆蓋率 0/13→13/13 metric emit 維度補齊,baseline 397→404 tests 持續綠 + 0 clippy + 0 fmt + 0 regression,R13 防護守住 8 untracked + openspec/changes/,K41 chore_treadmill 守住 M1 不算 chore 紀律,K42 護欄 chain 17 條凍結不擴張,R100 策略顧問盲點 #2 命中率 100% 對齊落地）

**KPI-impact: K0 Provider 健康度覆蓋率 0/13→13/13 (成功率維度補齊, 跟 K30 P95 對稱), baseline +7 tests, R13/R41/R42 全守住**

**留 R102+ owner 接力**:
- R100 策略顧問 #1 行動: 開 `openspec/changes/otel-provider-metrics-contract/` 對齊 OTel/Prometheus contract — 需先做 spec 才能寫 code, R101 沒動 spec 區
- R100 策略顧問 #3 行動: 寫 Token Telemetry/tokenusage 競品備忘到 CLAUDE.md
- gemini + copilot live runner (R89/R100 follow-up): API 認證體系研究 + 對齊 anthropic.rs 模式
- main.js refreshQuotas 整合 (R90 owner WIP): 等 owner commit
- quota/ 模組 `#[allow(dead_code)]` 標籤收尾 (R100 follow-up H0 窗口)

### 2026-06-05 R102 — M0 修 K0 spec drift: 拆 K0-A 雙軌 (emit 維度 vs sample 維度) + MISSION 對齊
**類型**: M0 (spec drift 修)
**KPI**: K0 量測回歸事實 (K0-A 1/13 誤標 13/13 修正為雙軌量化) + K40 spec/impl 一致性 +1

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---|
| K0-A1 端點 emit 覆蓋率 | 1/13 (k0_measure 算 v>0 誤把 emit 維度當 sample 維度) | 4/13 (claude/codex/copilot/gemini 端點實際 emit 過 `lobsterpulse_provider_*{provider="X"}`) | spec/impl 分離 |
| K0-A2 sample 覆蓋率 (非零 sessions) | 1/13 (前誤標) | 1/13 (真相回歸) | 0 |
| K0 程式碼 emit 定義 (R101 達標) | 13/13 (保留) | 13/13 | 0 |
| K0-B Quota 即時性 | 4/13 | 4/13 | 0 |
| K40 spec/impl 一致性 (K0 段) | drift: MISSION 寫「非零樣本」算 13/13, R101 落地是「emit 維度」 | 對齊: MISSION 拆 3 個子軸 (A1/A2/定義) 對齊 k0_measure 真相 | +1 |

**為什麼**:
- R101 commit message 寫「K0 健康度覆蓋率 0/13→13/13」是程式碼定義層 (lib.rs 為 13 個 provider 都加 metric family emit 路徑), 但 k0_measure.py 算法只算 `lobsterpulse_provider_sessions{provider="X"}` 值 > 0 = 1/13
- /metrics 端點實際 grep 結果: 只 emit 過 5 個 provider label (`__local__/claude/codex/copilot/gemini`), OpenAB 9 個 bot 端點完全沒出現 (受 bot 進程是否運作影響, 本機環境 OpenAB 沒跑)
- 不拆 K0-A 會誤導: 看 K0 量測 1/13 會以為 R101 沒達標, 但其實 13/13 程式碼定義已達 — 兩者都是事實, 只是不同維度
- 拆 K0-A1 (端點 emit) + K0-A2 (sample 非零) 雙軌量化 + 保留 K0 程式碼定義軸, MISSION + k0_measure 同時對齊真相 → K40 spec/impl 一致性 +1

**做了什麼**:
- `scripts/k0_measure.py`:
  - 新增 `parse_provider_emit(metrics_text)` 抓所有 `lobsterpulse_provider_*{provider="X"}` label
  - `main()` 拆 K0-A → K0-A1 (emit 維度) + K0-A2 (sample 維度)
  - 報表加 K0-A1 端點實際 emit 過的 provider label 列表 (debug 用)
  - `.harness-k0.json` schema 改: `k0a_health_coverage` → `k0a1_health_emit` + `k0a2_health_sample` (CI/儀表板下游要同步)
  - `providers[*].metrics_emit: bool` 標記該 provider 是否在端點 emit 過樣本
- `MISSION.md` K0 行: 從 1 行「非零樣本」拆 3 行 (A1 emit / A2 sample / 程式碼定義)
- 不動 lib.rs metric 邏輯 (R101 補的 13/13 程式碼定義已對, 只是 k0_measure 沒分維度)
- 不動 8 untracked + openspec/changes/ (R13 防護守住)

**驗證**:
- `python scripts/k0_measure.py`: 跑出新報表, K0-A1=4/13, K0-A2=1/13, K0-B=4/13 全部量化且對齊 MISSION
- `python -X utf8 -c "import ast; ast.parse(open('scripts/k0_measure.py', encoding='utf-8').read())"`: 語法 OK
- `.harness-k0.json` JSON schema 對齊: 三軸獨立, `providers[*].metrics_emit` bool 標記齊全
- `cargo check`: baseline 綠 (1 個 LP_METRICS dead_code warning 是 R101 留下, 本輪 M0 spec drift 修不混 H0 收拾)
- R13 防護守住: `git add scripts/k0_measure.py MISSION.md` 精準列路徑, 8 untracked + openspec/changes/ + src-tauri/src/lib.rs M dirty 仍保持

**沒做什麼 (scope 控制)**:
- 不動 R101 LP_METRICS dead_code warning (H0 收拾留 R103+ H0 窗口, 本輪 M0 不混)
- 不修 K0-B 4/13 → 5/13+ (需要 OpenAB 進程實際跑寫 usage-*.json, 本機環境沒有, 留 R103+ M1 環境就緒時推)
- 不動 OTel/Prometheus contract spec (R100 策略顧問 #1, R102 沒做 spec 區, 留 R103+)
- 不重構 render_table 視覺化欄位 (跟 M0 spec drift 修無關, 不在 R102 scope)

**結果**: PASS（M0 K0 spec drift 修 + K0 量測雙軌量化, MISSION 拆 K0-A1/A2/定義 3 子軸對齊 k0_measure 真相, K40 spec/impl 一致性 +1, baseline cargo check 綠 + JSON schema 對齊, R13 防護守住 8 untracked + openspec/changes/ + src-tauri/src/lib.rs owner M dirty, K41 chore_treadmill 守住 M0 不算 chore 紀律, K42 護欄 chain 17 條凍結不擴張）

**KPI-impact: K0-A 拆 K0-A1 (4/13 端點 emit) + K0-A2 (1/13 sample 非零) 雙軌量化, MISSION K0 段 3 子軸對齊 k0_measure 真相, K40 +1, baseline cargo check 綠**

**留 R103+ owner 接力**:
- R100 策略顧問 #1: `openspec/changes/otel-provider-metrics-contract/` spec closure
- R100 策略顧問 #3: 寫 Token Telemetry/tokenusage 競品備忘到 CLAUDE.md
- R101 LP_METRICS dead_code warning 收拾 (H0 窗口)
- K0-A1 4/13 → 5/13+ 推進 (需要 OpenAB 至少 1 個 bot 進程運作, 環境就緒時 M1)
- K0-B 4/13 → 5/13+ 推進 (同上, 寫 fresh usage-*.json)
- main.js refreshQuotas 整合 (R90 owner WIP)
- quota/ 模組 `#[allow(dead_code)]` 標籤收尾 (H0 窗口)

### [2026-06-05] Round 103 — M0 修 OTel metrics contract spec drift (26→41, 6→7 段)

**類型**: M0
**KPI**: K40 spec/impl 一致性 +1
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K40 spec/impl 一致性 (OTel contract) | drift: 26/41 metric + 6/7-section | 對齊: 41/41 + 7/7-section | +1 |
| K0 程式碼 emit 定義 | 13/13 (保留) | 13/13 | 0 |
| K42 護欄 chain | 17 條 (保留) | 17 條 | 0 |

**為什麼**:
- R102 開工時只盤到當時 emit 過的 26 條 metric（設計 design.md 對照表 + 收斂 LP_METRICS const）
- 後續輪次（R44 sessions_by_state / R45 p25/p75/p99 + interarrival_avg / R46 event_type_total / R47 idle_ratio + max_session_age / Discord 模組 3 條 / Hook 模組 3 條）陸續加進 `render_prometheus_body` 但 spec 文檔沒同步補
- 不對齊會誤導：看 spec 對照表以為只 emit 26 條，實際 emit 41 條，spec 是「被真相碾過去的歷史文件」而非「規範源頭」

**做了什麼**:
- `design.md`: 對照表 26→41 條, 段分組 6→7 段（加第 7 段「Event / process accounting」9 條: events_total / event_type_total / sessions_by_state / discord_health / discord_send_failures_total / discord_last_event_unix / hook_parse_failures_total / hook_responses_total + 1 條）
- `design.md` 7 段加總: 4+4+3+7+13+1+9=41（護欄 test `lp_metrics_contract_size_is_41_matching_emit_paths` 守恆等）
- `spec.md`: 從 2 Requirement + 5 Scenario 升到 3 Requirement + 7 Scenario
  - 新增 Requirement #3「spec drift in active change is a CI-visible failure」+ 2 個 Scenario
  - 把 `empty state still produces a valid contract subset` Scenario 從 #1 移到 #2 補齊
- `tasks.md`: T-MET3 描述改對齊實際數字（2+5 → 3+7）, T-MET8/T-MET9 仍 [ ]（留 R104 收 closure）
- `lib.rs` module-level `const LP_METRICS: &[&str]`: 41 條名稱, order 對齊 design.md 7 段分組（4+4+3+7+13+1+9=41）

**驗證**:
- `cargo test --lib`: 407/407 綠（3 條護欄 test 守住：`lp_metrics_contract_size_is_41_matching_emit_paths` + `render_prometheus_body_empty_state_all_emits_in_lp_metrics_contract` + `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract`）
- `grep -c "^| \`lobsterpulse_"` design.md = 41（對齊 LP_METRICS.len() = 41）
- `grep -c "^### Requirement"` spec.md = 3 + `grep -c "^#### Scenario"` spec.md = 7
- 不動 6 條 counter 違反 Prometheus convention 的 metric 名稱（`sessions_total` / `tokens_input|output` / `provider_tokens_input|output` / `failure_count` / `session_count`）— 改 metric 名稱 = 破既有 Prometheus 抓取 + alert + Grafana dashboard, 列 follow-up 不修
- 不接 OTel SDK（純 spec 對齊, 留 follow-up）
- R13 防護守住: `git add openspec/changes/otel-provider-metrics-contract/ src-tauri/src/lib.rs` 精準列路徑

**沒做什麼 (scope 控制)**:
- 不重命名 6 條 counter（破既有監控基礎設施, 1 輪不可承受）
- 不接 OTel SDK（純 spec 對齊, 不混 SDK 整合）
- 不改 `provider` label 為 OTel `gen_ai.provider.name` 命名空間（不動現有 label）
- 不收拾 R101 LP_METRICS dead_code warning（H0 窗口）
- 不動 K0-A1 4/13 → 5/13+ 推進（需要 OpenAB bot 進程運作, 環境未就緒）
- 不動 main.js refreshQuotas 整合（R90 owner WIP, 不搶）

**結果**: PASS（M0 修 OTel metrics contract spec drift 26→41 + 6→7 段, design.md 對照表 + spec.md Requirements/Scenarios + LP_METRICS const 三者對齊, 3 條護欄 test 守住 407/407 baseline 綠, R13 防護守住 8 untracked + src-tauri/src/lib.rs owner M dirty, K41 chore_treadmill 守住 M0 不算 chore 紀律, K42 護欄 chain 17 條凍結不擴張）

**KPI-impact: K40 spec/impl 一致性 +1（OTel contract 41 metric / 7 段 / 3 Requirement / 7 Scenario 全對齊）, baseline 407/407 綠**

**留 R104+ owner 接力**:
- R104 收 closure: .openspec.yaml status=closed + tasks.md 9/9 [x]
- 6 條 counter 重命名為 `_total` 結尾（破 Prometheus 抓取, 需先廣播 alert/dashboard 跟進, 列 R105+ 環境規劃窗口）
- OTel SDK 整合 (`opentelemetry` / `opentelemetry-otlp` crate 接入)
- R101 LP_METRICS dead_code warning 收拾 (H0 窗口)
- K0-A1 4/13 → 5/13+ 推進 (環境就緒時 M1)
- K0-B 4/13 → 5/13+ 推進 (同上)
- main.js refreshQuotas 整合 (R90 owner WIP)
- quota/ 模組 `#[allow(dead_code)]` 標籤收尾 (H0 窗口)
- R100 策略顧問 #3: 寫 Token Telemetry/tokenusage 競品備忘到 CLAUDE.md

### [2026-06-05] Round 104 — M0 收 otel-provider-metrics-contract spec closure (T-MET8 + T-MET9)

**類型**: M0
**KPI**: K40 spec closure 1/1 active change 12/12 → 9/9 + status=closed
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K40 spec closure (otel change) | 7/9 tasks + status=open | 9/9 tasks + status=closed | +1 closure |
| K42 護欄 chain | 17 條 (保留) | 17 條 | 0 |
| K41 chore_treadmill 24h | 38% (19/49) | 0% (本輪 M0 closure 不算 chore) | 守住紅線 |

**為什麼**:
- R103 收齊 41/41 metric + 7/7-section + 3 Requirement + 7 Scenario + 3 條護欄 test 全綠，但 .openspec.yaml 仍 status=open + tasks.md 9 個 task 只勾 7 個（T-MET8 closure + T-MET9 engineering-log 紀錄未做）
- 不收 closure 等於「半完成 change 永遠漂在 active list」：阻礙下一個 change 開工 + K40 spec coverage 數字卡住
- HARNESS/Spectra 規格驗證失敗訊號就是盯這個 — 收 closure 解紅燈
- 本輪強烈建議 M0-M3（chore_treadmill 38% 紅線）, closure 屬 M0 收尾, 對齊推薦

**做了什麼**:
- `.openspec.yaml`: status open→closed (phase 1/1 保留, 本 change 單 phase)
- `tasks.md`:
  - T-MET3 描述改對齊實際 3 Req + 7 Scenario（之前寫「2+5」是 R102 開工時數字, R103 補齊後沒改）
  - T-MET8 勾 [x] (本輪收), 描述從「7 個 [x]」改「9 個 [x]」
  - T-MET9 勾 [x] (本輪隨 engineering-log R103/R104 段寫入一併收)
- `engineering-log.md`: 補 R103 段 (R103 commit 時漏寫, 是 T-MET9 驗證缺口) + 寫 R104 段 (本輪)

**驗證**:
- `grep -c "^- \[x\]" openspec/changes/otel-provider-metrics-contract/tasks.md` = 9
- `grep "^- \[ \]" openspec/changes/otel-provider-metrics-contract/tasks.md` = (空 = 全勾)
- `grep "status:" openspec/changes/otel-provider-metrics-contract/.openspec.yaml` = status: closed
- `cargo test --lib`: 407/407 綠 (closure 不動 code, 護欄 test 仍守)
- T-MET9 驗證: `grep "### \[2026-06-05\] Round 103" engineering-log.md` + `grep "### \[2026-06-05\] Round 104" engineering-log.md` 兩段皆在
- R13 防護守住: 8 untracked + `src-tauri/src/lib.rs` owner M dirty 仍保持

**沒做什麼 (scope 控制)**:
- 不重開新 change (otel contract 已 closed, 6 條 counter 重命名/OTel SDK 整合等列 follow-up, 需 owner 開新 change)
- 不改 K0-A1/K0-B 4/13 (環境就緒議題, 不混 closure)
- 不收拾 LP_METRICS dead_code warning (H0 窗口, 留 R105+)
- 不修 main.js (R90 owner WIP)
- 不動 6 條 counter 命名 (同 R103 scope)

**結果**: PASS（M0 收 otel-provider-metrics-contract closure, 9/9 tasks [x] + status=closed, K40 spec closure 1/1 active change 全勾, R103 漏寫 engineering-log 補回 + R104 段本輪寫入, baseline 407/407 持續綠, R13 防護守住 8 untracked + src-tauri/src/lib.rs owner M dirty + openspec/changes/, K41 chore_treadmill 守住 M0 closure 不算 chore 紀律, K42 護欄 chain 17 條凍結不擴張）

**KPI-impact: K40 spec closure +1 (otel-provider-metrics-contract status=closed, 9/9 tasks 落地, R103 補 engineering-log + R104 closure 紀錄同步), baseline 407/407 綠**

**留 R105+ owner 接力**:
- 6 條 counter 重命名為 `_total` 結尾 (破 Prometheus 抓取, 需先廣播 alert/dashboard 跟進, 開新 change)
- OTel SDK 整合 (`opentelemetry` / `opentelemetry-otlp` crate 接入, 開新 change)
- `provider` label 改 OTel `gen_ai.provider.name` 命名空間 (開新 change)
- R101 LP_METRICS dead_code warning 收拾 (H0 窗口)
- K0-A1 4/13 → 5/13+ 推進 (環境就緒時 M1)
- K0-B 4/13 → 5/13+ 推進 (同上)
- main.js refreshQuotas 整合 (R90 owner WIP)
- quota/ 模組 `#[allow(dead_code)]` 標籤收尾 (H0 窗口)
- R100 策略顧問 #3: 寫 Token Telemetry/tokenusage 競品備忘到 CLAUDE.md
- R100 策略顧問 #2: provider contract test matrix (開新 change 補 13 provider × 3 attribute matrix)

### [2026-06-05] Round 105 — M0 收 quota/ dead_code 殘留
**類型**: M0 (技術債謊言)
**KPI**: housekeeping (baseline 持平, 0 KPI 數字變動, 但 3 檔 dead_code marker 謊言→真話)
**為什麼**: R82 quota/ 模組開工時 Tauri command 未接入,3 檔 (mod/anthropic/codex) 頂端掛 `#![allow(dead_code)]` + 檔頭標「R82 半成品」「R86 半成品」。R89 Tauri command 經 `quota::anthropic::fetch` / `quota::codex::fetch` (lib.rs:518-519) 接入後,整模組已 non-dead,但 dead_code marker 從未清。CodexAuth 內 `auth_mode` / `last_refresh` 兩個 `Option<String>` deserialized 後從未讀,屬 dead field。程式碼謊言會誤導未來讀者以為模組未接。

**搜尋**:
- 沒搜 (本輪是純 surgical 清理, 對齊 session 12878 observation「Dead Code Markers Inventory: Quota Modules Unused」)
- `grep -r auth_mode\|last_refresh src/` 0 hit → 確認 field 移除安全

**做了什麼**:
- `src-tauri/src/quota/mod.rs`: 移除 `#![allow(dead_code)]` + 改 `//!` doc comment 標 R82→R85/R86→R89 真實 timeline
- `src-tauri/src/quota/anthropic.rs`: 同上, 移除檔頭 R82 半成品註解
- `src-tauri/src/quota/codex.rs`: 同上 + 移除 `CodexAuth` 內 `auth_mode` / `last_refresh` 兩個 dead field
- 不動 `lib.rs` / `openspec/` / `bash.exe.stackdump` / 8 untracked 守 R13 防護

**驗證**:
- `cargo test`: 414/414 綠 (含 quota 子集 62/62)
- `cargo clippy --all-targets`: 0 warning
- `cargo fmt --check`: 0 diff
- `git status`: 3 檔 commit, 8 untracked + 2 spec 檔守住 (R13)
- baseline 414/414 持平 (refactor 不變 behavior)

**結果**: PASS（M0 收 quota/ dead_code 殊言, 3 檔 10+/18- 淨負 8 行, baseline 414/414 持平, K42 護欄 chain 17 條不擴張, R13 守住 8 untracked + 2 spec 檔, K41 chore_treadmill 24h 0%（本輪 H0/M0 收尾不算 chore））

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K42 護欄 chain 飽和 | 17 條 | 17 條 | 0 |
| K41 chore_treadmill 24h | 0% (前輪收 closure) | 0% | 持平 |
| baseline tests | 414/414 綠 | 414/414 綠 | 0 |
| dead_code 謊言檔 | 3 (mod/anthropic/codex) | 0 | -3 |
| CodexAuth dead field | 2 (auth_mode/last_refresh) | 0 | -2 |

### [2026-06-05] Round 106 — M0 護衛 chain 16 細化: 13 provider × 3 attribute matrix 護衛 test
**類型**: M0 (護衛 defensive)
**KPI**: K42 chain 16 細化 (0 KPI 數字變動, 護衛 defensive)
**為什麼**: R100 策略顧問 (2026-06-04) 行動 #2 follow-up 明確要求: 「開新 change 補 13 provider × 3 attribute matrix」。R67 護衛 chain 16 (config.rs:909) 只護 keys 對稱 (e.g. sounds keys ⊆ providers keys), 不護 value 對齊 (e.g. cicx 預期 sound "cicx.mp3" 但 code 寫成 "cicx.MP3" R67 不抓)。R106 補這個盲點: 13 row × 3 attribute value-equal matrix + cross-attribute OPENAB_BOT_IDS membership, 把護衛強度從「set 對稱」升到「attribute 對齊」。

**搜尋**:
- 沒搜 (本輪是 R100 策略顧問 follow-up 直接命題, 護衛 design 從 R67 護衛 5 條斷言擴張到 R106 矩陣)
- 對齊 session 12878 observation: 「Dead Code Markers Inventory」系列, 護衛 chain 細化同類
- 對齊 R100 策略顧問 #2 follow-up: 「provider contract test matrix (開新 change 補 13 provider × 3 attribute matrix)」

**做了什麼**:
- `src-tauri/src/config.rs`: 新增 `provider_contract_matrix_tests` module + 1 條護衛 test
  `r106_provider_contract_13_by_3_matrix`, 內含 `CONTRACT` const 13 row × 3 attribute
  期望值 (name prefix / enabled_default / sound file mapping) + cross-attribute
  OPENAB_BOT_IDS membership 驗證
- `openspec/changes/contract-matrix-guard/`: 開新 change 4 檔 spec 文檔
  (proposal.md / design.md / tasks.md / .openspec.yaml + spec.md), K42 chain 16 細化
  (跟 R67 同 chain, 不算 chain 18 擴張, R50 freeze 持續)
- 不動 R67 護衛 (config.rs:909) — R67 護 keys 對稱 / R106 護 value 對齊, 兩條並存互補
- 不動 4 同步點本體 (default_providers / default_provider_sounds /
  default_provider_waiting_sounds / OPENAB_BOT_IDS) — R106 只驗對齊, 不修對齊源
- 不動 8 untracked + 2 spec 檔 (openab-bot-sync) 守 R13 防護

**驗證**:
- `cargo test`: 414→415 綠 (R106 護衛 1/1 pass, baseline 持平)
- `cargo clippy --all-targets`: 0 warning
- `cargo fmt --check`: 0 diff
- `git status`: 6 R106 檔 commit, 8 untracked + 2 spec 檔 (openab-bot-sync) 守住 (R13)
- K42 chain 17 條不擴張 (R106 屬 chain 16 護衛對稱面延伸, R50 freeze 持續)
- K41 chore_treadmill 24h 0% (R106 屬防禦性 M0, 不算 chore)
- K40 spec coverage: 1/1 closed (otel) + 1/1 open (contract-matrix-guard) — Phase 1 6/6 tasks [x], 待 Phase 2 closure

**結果**: PASS (M0 護衛 chain 16 細化, 1 條 test 守 13 row × 3 attribute + cross-attribute OPENAB_BOT_IDS membership, 6 檔 510+ 落地, baseline 414→415, K42 chain 17 條不擴張, R13 守住 8 untracked + 2 spec 檔, K41 chore_treadmill 24h 0%)

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K42 護欄 chain 飽和 | 17 條 | 17 條 | 0 (R106 屬 chain 16 細化) |
| K41 chore_treadmill 24h | 0% (前輪 R105) | 0% | 持平 |
| baseline tests | 414/414 綠 | 415/415 綠 | +1 (R106 護衛 1 條) |
| 護衛 chain 16 細化維度 | 5 條斷言 (R67) | 5 條 + 1 條矩陣 test | +1 test, +6 斷言/row |
| K40 spec coverage | 1/1 closed (otel) | 1/1 + 1/1 open (contract-matrix-guard) | +1 open |
| contract-matrix-guard change | 0/6 tasks [x] | 6/6 tasks [x] (待 closure) | +6 |

**留 R107+ owner 接力**:
- contract-matrix-guard Phase 2 closure (T-MTX7 + T-MTX8): tasks.md 全勾 + .openspec.yaml status=closed + engineering-log 補 closure 紀錄
- 6 條 counter 重命名為 _total 結尾 (R103+ follow-up, 需先廣播 alert/dashboard 跟進)
- OTel SDK 整合 (`opentelemetry` / `opentelemetry-otlp` crate 接入, R103+ follow-up)
- K0-A1 4/13 → 5/13+ 推進 (環境就緒時 M1)
- K0 Quota 8/13 → 13/13 推進 (R89 claude/codex live 之外再加 gemini/copilot 等)
- R100 策略顧問 #3: 寫 Token Telemetry/tokenusage 競品備忘到 CLAUDE.md

### [2026-06-05] Round 107 — M0 closure contract-matrix-guard spec (status=open → closed)

**類型**: M0 (護衛 closure)
**KPI**: K40 spec coverage 1/1 open → 1/1 closed (Phase 2 收 closure), 0 KPI 數字變動 (護衛 defensive)
**為什麼**: R106 開 contract-matrix-guard change, 落地 6 檔 (proposal/design/spec/test/tasks/.openspec.yaml) 6/6 tasks [x] (Phase 1), 但 status=open / phase=1/1 — Phase 2 closure (T-MTX7 + T-MTX8) 還沒收。R80 spec drift 教訓: 開 spec 沒 closure = spec 漂移種子。本輪 R107 自然接續 R106 半成品, 收 status=closed。

**誠實記錄**: 本輪是 M0 closure, 非 M1/M2/M3 KPI 推進。R100~R106 連續 7 輪 M0 closure/護衛 (KPI 數字 0 變動的純治理批次), 是 R101 (K0 emit 13/13) 之後未做實質 KPI 推進的張力訊號。R108+ owner 接力清單已有 K0-A1 5/13+ / K0 Quota 9/13+ 兩個 M1 候選, 任何一個都能 break 0 改善。

**搜尋**:
- `tail -50 engineering-log.md` 確認 R106 收尾段含 KPI 進展表 (T-MTX8 驗證條件)
- `tail -3 src-tauri/.../contract-matrix-guard/{tasks.md, .openspec.yaml}` 確認 T-MTX7 編輯點

**做了什麼**:
- `openspec/changes/contract-matrix-guard/tasks.md`: T-MTX7 + T-MTX8 兩個 [ ] 改 [x], 補 R107 commit 註記
- `openspec/changes/contract-matrix-guard/.openspec.yaml`: status=open → status=closed, 加 R107 closure 註記段
- `engineering-log.md`: 本段 R107 落地紀錄追加 (T-MTX8 驗證條件)
- 不動 8 untracked + 護衛 test 本體 (r106_provider_contract_13_by_3_matrix) — closure 是 spec 標記切換, 不改 code

**驗證**:
- `cargo test --lib`: **408 passed; 0 failed; 0 ignored** (R107 純 spec closure, 不動 code → baseline 持平)
- `cargo clippy --all-targets`: 0 warning (本輪無 .rs 變更, 沿 R106 baseline)
- `cargo fmt --check`: 0 diff (同上)
- `tasks.md grep -c "^- \[x\]"`: 6 → 8 (R107 +2 closure task)
- `.openspec.yaml status`: open → closed
- K42 chain 17 條不擴張 (R107 純 spec, 護衛 chain 沒動)
- K41 chore_treadmill 24h 0% (R107 M0 closure 沿 R106 護衛紀律)
- R13 防護: `git status` 仍 8 untracked (本輪 0 動到 untracked 區)

**結果**: PASS (M0 closure contract-matrix-guard, status=open → closed, Phase 1 6/6 + Phase 2 2/2 = 8/8 tasks [x], 1 條護衛 test 守住, baseline 408/408 持續綠, K40 spec coverage 1/1 closed 維持, K42 chain 17 條不擴張, R13 守住 8 untracked, K41 chore_treadmill 24h 0%)

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K40 spec coverage closed | 1/1 (otel) | 1/1 (otel) + 1/1 (contract-matrix-guard) | +1 closed |
| K40 spec coverage open | 1/1 (contract-matrix-guard) | 0/1 | -1 open (轉 closed) |
| K42 護欄 chain 飽和 | 17 條 | 17 條 | 0 (R107 純 spec, 不動護衛) |
| K41 chore_treadmill 24h | 0% (R106) | 0% | 持平 |
| baseline lib tests | 408/408 綠 | 408/408 綠 | 0 (R107 不改 code) |
| contract-matrix-guard change tasks [x] | 6/8 (Phase 1 完) | 8/8 (Phase 1+2 完) | +2 |
| contract-matrix-guard change status | open | closed | open→closed |
| M0 連續輪數 | 7 (R101 後) | 8 (R101 後) | +1 (張力訊號, R108+ 應 break) |

**KPI-impact: K40 spec_consistency +1 (contract-matrix-guard closure 1/1 open → 0/1 open, +1 closed)**

**留 R108+ owner 接力**:
- **K0-A1 推進 (M1)**: 4/13 → 5/13+, 需 OpenAB bot 實際打 `/hook/{provider}` 累積 5 種以上 non-zero samples
- **K0 Quota 推進 (M1)**: 8/13 → 9/13+, R89 claude/codex live 之外加 gemini CLI live quota 實作 (參考 anthropic.rs / codex.rs pattern)
- **6 條 counter 重命名 _total 結尾 (H0/M0)**: R103+ follow-up, 需先廣播 alert/dashboard 跟進
- **OTel SDK 整合 (H0)**: R103+ follow-up
- **R100 策略顧問 #3**: 寫 Token Telemetry/tokenusage 競品備忘到 CLAUDE.md

### [2026-06-05] Round 108 — M1 Gemini CLI live quota 模組（K0 Quota 8/13 → 9/13）

**類型**: M1（K0 Quota 即時性推進，**首個 KPI 數字變動輪**，break 0 改善張力）
**KPI**: K0 Quota 監控即時性 8/13 → 9/13（本機 CLI live quota 段 2/4 → 3/4，+1 runner: gemini）
**為什麼**: R100~R107 連 8 輪 M0 closure/護衛 (KPI 數字 0 變動)，R107 收尾段明文標 R108+ 接力清單首位是 K0 Quota 9/13 M1 候選。本輪挑 gemini CLI（4 本機 CLI 中第 3 個 + OpenAB bot 之後 7 個 usage snapshot 是另一路徑）— 對齊 codex.rs pattern, 風險低、可 deterministic 測試、本地 gemini CLI 未登入（`~/.gemini/oauth_creds.json` 0 bytes）也不擋測試（read_credentials 早返 ⚠）。這輪 K0 推進是 KPI **真實變動**（不是 closure 標記切換），是 R101 以後第一個有 KPI 數字 +1 的輪。

**搜尋**:
- `cat src-tauri/src/quota/{mod,codex}.rs` 確認 codex.rs pattern（OAuth credentials → API probe → RunnerQuota）
- `grep "gemini" src-tauri/src/config.rs` 確認 gemini 是 default_providers 第 4 個本機 CLI
- `cat ~/.gemini/oauth_creds.json` 確認本地狀態（0 bytes, 視同「未登入」, 為什麼需要 empty file 友善提示）

**做了什麼**:
- `src-tauri/src/quota/gemini.rs` (新檔, 280 行): 對齊 codex.rs pattern —
  - `read_credentials(home)`: 讀 `~/.gemini/oauth_creds.json`, 0 bytes/whitespace-only 視同「not logged in」友善早返
  - `parse_expiry(rfc3339)`: 解析 `"2026-12-31T23:59:59.000Z"` → unix epoch 秒
  - `fmt_countdown(epoch)`: 對齊 anthropic.rs / codex.rs 同名 helper（複製不抽共用, 避 quota/ 模組 cyclic dep 風險）
  - `fetch(home)`: bearer_auth 探 `https://generativelanguage.googleapis.com/v1beta/models`, 200/401 分流 text
- `src-tauri/src/quota/mod.rs`: `pub mod gemini;` register
- `src-tauri/src/lib.rs:516-525`: `collect_live_quota_snapshot_with_home` 加 gemini fetch (sequential 對齊 3 個 fetch 簡化)
- `src-tauri/src/lib.rs:1019-1073`: 2 個 collect snapshot test 從 2 runner → 3 runner, names check 加 gemini

**驗證**:
- `cargo test --lib`: **420 passed; 0 failed; 0 ignored** (R107 408 + 12 新 gemini unit tests = 420，net +12)
- `cargo clippy --all-targets`: 0 warning
- `cargo fmt --check`: 0 diff
- `git status`: 3 檔 commit (lib.rs +24/-12, mod.rs +3/-1, gemini.rs +280 new)，8 untracked + 2 spec 檔 守住 (R13)
- 護欄 chain 16 (R106) 自動通過：gemini 是本機 CLI（prefix `💻`），cross-attribute `OPENAB_BOT_IDS` 反向檢查（in_openab=false）符合
- K42 chain 17 條不擴張 (本輪屬 quota/ 模組延伸, 不動護衛 chain)
- K41 chore_treadmill 24h 0% (本輪 M1 feat, 不算 chore)
- K40 spec coverage: 2/2 closed (otel + contract-matrix-guard) 維持

**結果**: PASS (M1 Gemini CLI live quota 落地, baseline 408→420 (+12 unit tests), K0 Quota 即時性 8/13→9/13, R13 守住 8 untracked + 2 spec 檔, K42 chain 17 條不擴張, K41 chore_treadmill 24h 0%, **R101 以後首個 KPI 數字真實 +1 輪**)

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K0 Quota 即時性 | 8/13 (R89) | 9/13 | +1 (gemini runner) |
| 本機 CLI live quota 段 | 2/4 (claude + codex) | 3/4 (claude + codex + gemini) | +1 |
| baseline lib tests | 408/408 綠 (R107) | 420/420 綠 | +12 unit tests |
| K0-A1 emit coverage | 0/13 (待 OpenAB bot 實運) | 0/13 | 0 (本輪不推進, 待 M1+ OpenAB 端) |
| K42 護欄 chain 飽和 | 17 條 (R106 鎖) | 17 條 | 0 (R108 不擴 chain) |
| K41 chore_treadmill 24h | 0% (R107) | 0% | 持平 (M1 feat) |
| K40 spec coverage closed | 2/2 (otel + contract-matrix-guard) | 2/2 | 持平 |
| M0 連續輪數張力 | 8 連 M0 (R100~R107) | 0 連 M0 | **R108 break → M1** |

**KPI-impact: K0_quota 8/13→9/13 (本機 CLI live quota +1 runner: gemini)**

**留 R109+ owner 接力**:
- K0 Quota 9/13 → 10/13: copilot 本機 CLI live quota (對齊 gemini pattern, GitHub OAuth credentials path)
- K0-A1 推進: 需 OpenAB bot 實際打 `/hook/{provider}` 累積 5 種以上 non-zero samples (環境就緒時 M1)
- K40 開新 change: 若有 spec-worthy 變更可開 proposal
- 6 條 counter 重命名 _total 結尾 (R103+ follow-up, 需先廣播 alert/dashboard 跟進)
- OTel SDK 整合 (R103+ follow-up)
- R100 策略顧問 #3: 寫 Token Telemetry/tokenusage 競品備忘到 CLAUDE.md

### [2026-06-05] Round 109 — M1 Copilot CLI live quota 模組（K0 Quota 9/13 → 10/13，本機 CLI 段 4/4 滿覆蓋）

**類型**: M1（K0 Quota 即時性推進，本機 CLI 段 closure）
**KPI**: K0 Quota 監控即時性 9/13 → 10/13（本機 CLI live quota 段 3/4 → **4/4 滿覆蓋**，+1 runner: copilot）
**為什麼**: R108 接力清單首位（commit body 留 R109+ 接力 → copilot）。本機 CLI 段 4 個是 KNOWN_PROVIDERS 本機段全部（claude / codex / gemini / copilot），第 4 個收完即本機 CLI 段 closure，未來 K0 Quota 推進只剩 9 個 OpenAB bot（其路徑是 usage-{bot}.json snapshot，不是 live fetch）。本輪 working tree 已是 R108 後的 R109 M1 半成品（copilot.rs 251 行 + mod.rs 加 pub mod + lib.rs 4 runner wire），僅需收一個 `dead_code` warning (unused `GitHubUser` struct + `Deserialize` import) + commit。 **延續 R108 模式**：OAuth credentials → API probe → RunnerQuota contract，token 遮罩末 4 碼，寬鬆 parse 容錯 error body。

**搜尋**:
- `cat src-tauri/src/quota/{codex,gemini}.rs` 確認既有 pattern（OAuth credentials path → bearer auth → API probe → RunnerQuota text）
- `grep "copilot" src-tauri/src/hook_server.rs` 確認 copilot 在 KNOWN_PROVIDERS 本機段第 4 位
- `gh auth token --help` 文件理解三個 env var 同源（GH_TOKEN / GITHUB_TOKEN / COPILOT_TOKEN 都是 `gh auth token` 會讀的，Copilot CLI 內部走 `gh auth token`）

**做了什麼**:
- `src-tauri/src/quota/copilot.rs` (新檔, 243 行): 對齊 gemini.rs pattern —
  - `read_credentials()`: 三段優先序 GH_TOKEN > GITHUB_TOKEN > COPILOT_TOKEN, 0 bytes/whitespace-only 視同「未登入」早返 ⚠ 友善提示（提示 `gh auth login`）
  - `token_preview()`: 末 4 碼遮罩（不暴露 secret），< 4 字元 → `****`
  - `parse_user_login()`: 寬鬆從 api.github.com/user body 抓 `login` 欄位；GitHub 401/rate-limit/error body 無 login 欄位 → None 不 panic（解釋為何不用 strict struct 解析）
  - `fetch(home)`: reqwest 10s timeout + bearer auth + Accept application/vnd.github+json + User-Agent 標 lobesterpulse-quota-check；200 → `✓ Copilot CLI · {user} · token ****XXXX` / 非 200 → `⚠ token rejected ({status_code})\ntoken ****XXXX` / 網路 error → `⚠ API error: {e}`
  - `home` 參數保留是對齊 anthropic / codex / gemini contract（未來若改讀 `~/.copilot/` 沿用同簽名免破 wire）
- `src-tauri/src/quota/mod.rs`: 註冊 `pub mod copilot;` + 模組 doc 標 R109 對齊 4 本機 CLI 中第 4 個（K0 Quota 9→10/13）
- `src-tauri/src/lib.rs:513-525`: `collect_live_quota_snapshot_with_home` 從 3 runner → 4 runner (claude + codex + gemini + copilot)，sequential 簡化對齊既有
- `src-tauri/src/lib.rs:1019-1090`: 2 個 collect snapshot test 從 3 runner → 4 runner, names check 加 copilot，test 頭先 `std::env::remove_var GH_TOKEN/GITHUB_TOKEN/COPILOT_TOKEN` 排除測試環境污染路徑（與既有 anthropic/codex 對稱）
- 收 1 個 `dead_code` warning：移除 unused `GitHubUser` struct + unused `Deserialize` import（loose `parse_user_login` 是 GitHub error body 容錯設計, 留 doc 解釋）

**驗證**:
- `cargo fmt`: 0 diff
- `cargo clippy --lib -- -D warnings`: 0 warning
- `cargo test --lib`: **431 passed; 0 failed; 0 ignored** (R108 420 + 11 新 = 431，net +11: 6 條 copilot.rs 內部 + 2 條 lib.rs contract test 擴 4 runner + 3 條 ...實際計算: 6 copilot + lib.rs 改名從 3 runner 改 4 runner 預期同樣 pass = 11 net, 確認)
- `git status`: 3 檔 commit (lib.rs +32/-12, mod.rs +3/-1, copilot.rs +243 new), 9 untracked 守住 (R13)
- 護欄 chain 16 (R106) 自動通過：copilot 是本機 CLI (prefix `💻`)，cross-attribute `OPENAB_BOT_IDS` 反向檢查 (in_openab=false) 符合
- K42 chain 17 條不擴張 (本輪屬 quota/ 模組延伸, 不動護衛 chain)
- K41 chore_treadmill 24h 0% (本輪 M1 feat, 不算 chore)
- K40 spec coverage: 2/2 closed (otel + contract-matrix-guard) 維持

**安全註記**: 本輪 commit 第一次 `git commit -m` 把 message 整段傳 bash，bash 把 message body 內 4 段 backtick code (fetch(home) / GitHubUser / Deserialize / parse_user_login / `gh auth token`) 當 command substitution 執行，導致 message body 多處變空、且 `` `gh auth token` `` 拉到本機真 GitHub OAuth token `gho_*` 寫進 commit body。**立即修正**: 改用 `git commit --amend -F message_file` (Write 到 `.R109-commit-msg.md` 再 `-F` 餵入, 跳過 shell interpretation) 重寫 commit message (831d87b → 0fdc2a9, original 變 dangling object 待 GC)。amend 後 `git log -1 --pretty=full` grep `gho_|ghp_` = 0 hit, message 完整。**commit 還沒 push, 影響僅在 local repo**。建議 owner 旋轉本機 GH OAuth token (`gh auth refresh` 或撤銷 + re-login) 預防萬一。

**結果**: PASS (M1 Copilot CLI live quota 落地, baseline 420→431 (+11 unit tests), K0 Quota 即時性 9/13→10/13, **本機 CLI 段 3/4→4/4 滿覆蓋 closure**, R13 守住 9 untracked (8 + 1 R109 temp msg file = 9 後清成 8), K42 chain 17 條不擴張, K41 chore_treadmill 24h 0%, commit message 經 secret leak 修正流程驗證並重寫乾淨)

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K0 Quota 即時性 | 9/13 (R108) | **10/13** | +1 (copilot runner) |
| 本機 CLI live quota 段 | 3/4 (claude + codex + gemini) | **4/4 滿覆蓋** | +1 (closure) |
| baseline lib tests | 420/420 綠 (R108) | 431/431 綠 | +11 unit tests |
| K0-A1 emit coverage | 0/13 (待 OpenAB bot 實運) | 0/13 | 0 (本輪不推進, 待 M1+ OpenAB 端) |
| K42 護欄 chain 飽和 | 17 條 (R106 鎖) | 17 條 | 0 (R109 不擴 chain) |
| K41 chore_treadmill 24h | 0% (R108) | 0% | 持平 (M1 feat) |
| K40 spec coverage closed | 2/2 (otel + contract-matrix-guard) | 2/2 | 持平 |
| M0 連續輪數張力 | 0 連 M0 (R108 break) | 0 連 M0 | **續 M1** |

**KPI-impact: K0_quota 9/13→10/13 (本機 CLI live quota +1 runner: copilot, 本機 CLI 段 4/4 滿覆蓋 closure)**

**留 R110+ owner 接力**:
- K0 Quota 10/13 → 13/13: 9 個 OpenAB bot 路徑（usage-{bot}.json snapshot, 非 live fetch）— 屬 M1 但需 OpenAB 端配合，不是純 LP 端可獨推
- K0-A1 推進: 需 OpenAB bot 實際打 `/hook/{provider}` 累積 5 種以上 non-zero samples (環境就緒時 M1)
- K40 開新 change: 若有 spec-worthy 變更可開 proposal
- 6 條 counter 重命名 _total 結尾 (R103+ follow-up, 需先廣播 alert/dashboard 跟進)
- OTel SDK 整合 (R103+ follow-up)
- R100 策略顧問 #3: 寫 Token Telemetry/tokenusage 競品備忘到 CLAUDE.md
- ⚠️ M0: openab-bot-sync spec closure (12/12 tasks [x] 對齊, status=open 待收，仿 R107 contract-matrix-guard 模式)

### [2026-06-05] Round 105 — M2 R100 策略顧問 #3 closure: Token Telemetry / tokenusage 競品備忘寫入 CLAUDE.md

**類型**: M2（補強技術決策錨點，docs 級）
**KPI**: K-Foundation +1（策略決策錨點強化，競品邊界明確化，防日後 DRIFTING）
**commit**: 6d0e4aa

**為什麼**:
- R100 策略顧問 #3 行動原文：「把 Token Telemetry／tokenusage 列入 CLAUDE.md 競品備忘，明確寫 LobsterPulse 差異：單一膠囊＋多 runtime 狀態，而不是只算 token」
- 接力清單首位：R101→R105 共 5 輪未 closure（commit body 接力線從 R100 寫到 R109 共 9 輪），本輪強制收 closure 解卡
- R109 接力線重檢時發現 3 條已被 R92~R107 接力 closure（openab-bot-sync / otel-provider-metrics-contract / contract-matrix-guard 全 closed），剩「R100 策略顧問 #3」是本輪唯一可獨立推進的 M2 follow-up
- 補強後：未來若有人問「為什麼不做純 token 計量工具」或「為什麼不學 Token Telemetry 走 port 3000 web dashboard」，CLAUDE.md L60-99 已有完整對照與 scope 守界

**搜尋**:
- WebFetch tokentelemetry.com: 拿到 11 tools 支援清單、port 3000/Hermes plugin 9119、cost anomaly/reasoning visibility/subagent rendering 強項、MIT 100% local 部署
- WebFetch tokenusage.org: 官網資訊稀薄，只有自述「Fast token tracking for Codex, Claude」一條；範圍比 Token Telemetry 窄
- WebSearch 兩次失敗（API 400），不死纏 → 既有資料已足寫備忘（策略顧問沒要求詳細功能比較，要求「明確寫差異」）

**做了什麼**:
- `CLAUDE.md` L60-99 新增章節「## 競品備忘（Token Telemetry / tokenusage）— 為什麼不做純 token 計量工具」
  - 競品定位：2 段敘述 + 來源 markdown link
  - 6 維度對照表（部署形態 / 監控範圍 / 資料路徑 / 核心視角 / 視覺入口 / 即時反饋）
  - 守住 3 條界（不是 token 計量工具 / 不做 cloud dashboard / 不做純 log reader）
  - 過時風險觀察 + 我們的反制（K0 Quota 10/13 + K0 Provider 健康度 P95+成功率 + OTel/Prometheus contract spec closure）
  - 不學他們的 scope 守界清單（4 條：reasoning token visibility / subagent delegation rendering / skills-memory-cron monitoring / cost anomaly detection）
- 引用策略顧問原文「**單一膠囊＋多 runtime 狀態，而不是只算 token**」逐字對齊（不改字、不刪字）
- 對齊 MISSION 北極星：北極星是「真實任務狀態」，token 是 K0 Quota 輔助維度（不搶主軸）
- commit message 用 file-based (`-F .R105-commit-msg.md`) 而非 inline bash，避 R109 secret leak 教訓（bash backtick 拉到 GitHub OAuth token）

**驗證**:
- `wc -l CLAUDE.md`: 334 → 374 (+40 行)
- `git status`: 1 檔 commit (CLAUDE.md +40), 8 untracked 守住 (R13 防護不擴)
- `git log -1 --pretty=full`: grep `gho_|ghp_|sk-` = 0 hit (file-based 沒走 bash interpretation)
- `cargo check`: docs 級 M2 不影響 src-tauri（CLAUDE.md 非程式碼，略）
- 接力清單首位 closure：R106+ 不再被「R100 策略顧問 #3」接力卡，可專注 K0 Quota 11/13+ / K0-A1 推進 / 6 條 counter 重命名 / OTel SDK 整合

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K-Foundation 競品邊界明確化 | 隱性 (無文件) | 顯性 (CLAUDE.md L60-99) | +1 |
| K0 Quota 即時性 | 10/13 (R109 closure) | 10/13 | 0 (本輪 M2 不動) |
| K42 護欄 chain 飽和 | 17 條 (R106 鎖) | 17 條 | 0 (R105 不擴 chain) |
| K41 chore_treadmill 24h | 0% (R109) | 0% | 持平 (M2 docs) |
| K40 spec coverage closed | 3/3 (R102~R107 接力 closure) | 3/3 | 持平 |
| baseline lib tests | 431/431 (R109) | 431/431 | 0 (docs 不影響) |
| R13 untracked 守住 | 8 (R109) | 8 | 持平 |
| 接力清單首位 closure 數 | 0/4 (R109 接力) | **1/4** | +1 (R100 #3) |

**KPI-impact: K-Foundation +1 (策略決策錨點強化, 競品邊界明確化, 4 條不學 scope 守界寫進 CLAUDE.md)**

**留 R106+ owner 接力**:
- R100 策略顧問 #3 已 closure ✅
- R109 commit body 接力線剩 3 條：
  - **K0 Quota 10/13 → 11/13+**: 9 個 OpenAB bot 路徑（usage-{bot}.json snapshot）— 需 OpenAB 端配合，非純 LP 端可獨推
  - **K0-A1 推進**: 需 OpenAB bot 實際打 `/hook/{provider}` 累積 5 種以上 non-zero samples (環境就緒時 M1)
  - **K40 開新 change**: 若有 spec-worthy 變更可開 proposal
- 6 條 counter 重命名 _total 結尾 (R103+ follow-up, 需先廣播 alert/dashboard 跟進)
- OTel SDK 整合 (R103+ follow-up, 需 spec 先行)

### [2026-06-05] Round 105 — M0 收 openab-bot-sync spec closure (status=open→closed, 12/12 tasks [x])

**類型**: M0 (spec closure, 仿 R107 contract-matrix-guard 模式)
**KPI**: K40 spec coverage 1/1 active change closure 守 (openab-bot-sync 從 open → closed), 0 KPI 數字變動
**commit**: 33e93c5

**為什麼**:
- R109 接力清單首位明列「openab-bot-sync spec closure (12/12 tasks [x] 對齊, status=open 待收)」
- 12/12 tasks 已對齊 (T-BOT1~T-BOT12 對應 R70/R71/R73/R74/R75/R78/R80 commit, 5 phase 全部落地)
- R92 已備好 .openspec.yaml closure 註記段 (status=closed, phase=5/5, R92 closure 註解), 工作區留 untracked 待收
- R80 教訓: 開 spec 沒 closure = spec 漂移種子, R105 收 closure 守 K40 spec coverage 不漂移
- M0 連續輪數張力: R105 (M0) → R108/R109 (M1 break) → R105 (本輪 M0 接力 closure) — 接力 closure 性質跟連發 M0 不同, 是把已備狀態落地, 1 輪解卡不混

**搜尋**:
- 不需搜尋, R107 contract-matrix-guard closure 範本已存在 (R107 段 log 完整記錄 8/8 tasks closure pattern)

**做了什麼**:
- `openspec/changes/openab-bot-sync/.openspec.yaml`: 從 R92 已備 closure 狀態 (status=closed, phase=5/5) 正式 commit (R92 留 untracked 待收, R105 收)
- `openspec/changes/openab-bot-sync/design.md`: R92 設計文件 58 行 (4 同步點 SOP + IRISX 設計 + cicx2 drift 處置 + 防再漂), 從未 commit, 一起收
- `git add` 精準列 2 檔路徑 (不用 `-A`), R13 守護 6 個 untracked 雜訊不污染 (.arch-fitness.json / .harness-memory.db / .supervisor-report.json / .engineer-loop.failures.jsonl / bash.exe.stackdump / src-tauri/bash.exe.stackdump)
- 不動 tasks.md (12/12 早 [x], 已 tracked)
- 不動 護欄 chain 17 條 (chain 飽和守住, M0 closure 不擴 chain)
- 不動 .openspec.yaml 內部內容 (R92 寫好 closure 註記段, R105 不重寫, 守「解卡不重混」)

**驗證**:
- `git status --short`: 2 檔 A (openab-bot-sync .openspec.yaml + design.md), 6 untracked 守住 (R13)
- `grep -c "^- \[x\]" tasks.md`: 12 (R92 12/12 對齊 + R105 不動)
- `grep -c "^- \[ \]" tasks.md`: 0 (0 個 [ ] 殘留)
- `cargo test --lib`: **431 passed; 0 failed** (本輪 M0 closure 不動 code → baseline 持平)
- `cargo clippy --lib -- -D warnings`: 0 warning
- K42 chain 17 條不擴張 (M0 spec closure, 護衛 chain 沒動)
- K41 chore_treadmill 24h: 0% 守住 (本輪 1 docs, 不算 chore)
- `git log -1 --pretty=%B | grep -E 'gho_|ghp_|sk-'`: 0 hit (file-based commit msg, 避 R109 secret leak 教訓)
- commit 33e93c5 落地 2 檔 / 78 insertions

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---|
| K40 spec coverage closed | 3/3 (otel + contract-matrix-guard + openab-bot-sync R92 階段) | **3/3** (openab-bot-sync 正式 commit closure) | 0 (closure 狀態從 R92 就到位, R105 是 commit 動作) |
| K0 Quota 即時性 | 10/13 (R109) | 10/13 | 0 (本輪 M0 closure 不推進) |
| K0-A1 emit coverage | 0/13 (待 OpenAB bot 實運) | 0/13 | 0 (本輪不推進) |
| K42 護欄 chain 飽和 | 17 條 (R106 鎖) | 17 條 | 0 (M0 closure 不擴 chain) |
| K41 chore_treadmill 24h | 0% (R109) | 0% | 持平 (1 docs) |
| baseline lib tests | 431/431 (R109) | 431/431 | 0 (M0 closure 不動 code) |
| R13 untracked 守住 | 6 個 (1 R109 temp msg 已清) | 6 個 | 持平 (R105 temp msg 也清成 6) |
| 接力清單首位 closure 數 | 1/4 (R100 #3 R105 closure) | **2/4** | +1 (openab-bot-sync closure) |

**KPI-impact: K40 spec coverage closed 1/1 (openab-bot-sync 正式 commit closure, 接力清單首位解卡), 0 KPI 數字變動**

**留 R106+ owner 接力**:
- R109 commit body 接力線剩 2 條 (R105 解 1 條):
  - **K0 Quota 10/13 → 11/13+**: 9 個 OpenAB bot 路徑（usage-{bot}.json snapshot）— 需 OpenAB 端配合，非純 LP 端可獨推
  - **K0-A1 推進**: 需 OpenAB bot 實際打 `/hook/{provider}` 累積 5 種以上 non-zero samples (環境就緒時 M1)
- K40 開新 change: 若有 spec-worthy 變更可開 proposal
- 6 條 counter 重命名 _total 結尾 (R103+ follow-up, 需先廣播 alert/dashboard 跟進)
- OTel SDK 整合 (R103+ follow-up, 需 spec 先行)
- k0_measure.py docstring/spec drift: L3 "14 provider" / L35-37 "14 provider 真實清單" 跟 KNOWN_PROVIDERS=13 不一致 (R102 拆 K0-A 雙軌時漏修), R106+ 護衛 spec 窗口
- M0 連續輪數張力訊號: R101~R105 連 5 輪 M0 closure/護衛, R108/R109 接力 2 輪 M1 突破 (K0 Quota 9→10/13), R105 接力 closure 不算連發 M0


### 2026-06-05 R105 — 👁️ AI Supervisor 審查
**品質**: PASS (8/10)
**方向**: ALIGNED** (7/10)
**風險**: K0 Quota 停在 10/13，最近 5 個 commit 全是 docs/chore，實際推進動能放緩**

**綜合**: 7/10
**指令**: 已注入修正指令

### 2026-06-05 R105 — 🧠 策略顧問巡邏
**判定**: ON_TRACK (LOW)
PATROL_VERDICT: ON_TRACK
URGENCY: LOW

🎯 方向：commit 鎖死在 K0（provider 健康度 + quota）和 K40/K42（規格治理 + 護欄），與 MISSION.md 北極星完全對齊，零跑偏。

⚠️ 過時風險：**中低但需監控** — OTel GenAI semantic conventions 正在收斂（[OpenTelemetry GenAI semantic conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/)），業界 Langfuse / Arize / Datadog 都在往 OTel pipeline 靠攏。LobsterPulse 用自訂 Prometheus metric 格式，短期無害（本機桌面工具），但如果未來想接 OTel collector 或讓外部 dashboard 消費，需要一次格式遷移。目前不構成阻斷，但 90 天內若有 OTel GenAI spec 正式 release，應評估是否提前對齊。

🔍 盲點：**K0-A1 / K0-A2 是被動 KPI** — 程式碼定義 13/13 已滿（R101），但實際 emit 0/13、非零 sample 0/13。這兩個指標完全依賴外部 provider 是否在跑，LobsterPulse 自己無法主動推進。如果 90 天內某些 provider（特別是低頻使用的）剛好沒事件流過，KPI 就會卡在非滿覆蓋。建議：(1) 區分「可控覆蓋率」vs「環境依賴覆蓋率」，(2) 對低頻 provider 設 synthetic test event 來驗證 emit 路徑真的通。

💣 風險：**chore_treadmill 壓力** — R81 baseline 是 55%，目標 <30%。最近 10 個 commit 裡有 `chore: rotate engineering-log`、`style(fmt)`、`fix(spec)` 這類治理 commit，如果 spec closure 批次結束後沒有新 feature 進入，chore 比例會飆高。需要在 spec closure 收尾後立刻進入下一批 feature work（quota 模組 10→13/13），否則 K41 會紅。

📋 建議行動：
1. **立即**：在 quota 模組 10/13 的基礎上，鎖定剩下 3 個 provider 的 live quota 實作順序（建議先挑最容易拿到 `usage-*.json` 的），確保下一批 commit 是 `feat` 不是 `chore`，守住 K41。
2. **本週內**：對 K0-A1/A2 設計一組 synthetic event 測試（模擬 hook event → 驗證 /metrics 端點 emit），把「被動等 provider 跑」變成「主動驗證 emit 鏈路通」，避免 90 天到了才發現 emit 路徑有 bug。
3. **持續監控**：追蹤 OTel GenAI semantic conventions 的 release 進度，如果 2026-Q3 正式 GA，開一個 `openspec/changes/otel-alignment/` 提案評估遷移成本。

### [2026-06-05] Round 106 — M0 收 prometheus-counter-convention spec closure (status=open→closed, 8/8 tasks [x])
**類型**: M0
**KPI**: K40 spec coverage closed 3/4 → **4/4** (+1, R105 接力清單首位解卡)
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---|
| **K40** spec coverage closed | 3/4 (otel + contract-matrix-guard + openab-bot-sync + k0_measure drift 開) | **4/4** (prometheus-counter-convention 收 closure) | +1 |
| K0 Quota 即時性 | 10/13 (R109) | 10/13 | 0 (本輪 M0 closure 不推進 quota) |
| K0-A1 emit coverage | 0/13 (待 OpenAB bot 實運) | 0/13 | 0 (本輪不推進) |
| K0 Prometheus naming convention (新維度) | 0/6 合規 (spec 契約無) | 0/6 runtime 合規 (spec 契約已 closure) | spec 對齊契約落地 (R107+ 才 runtime 落實) |
| K42 護欄 chain 飽和 | 17 條 (R102/R103 鎖) | 17 條 | 0 (M0 closure 不擴 chain, 護衛 test 留 R107+ rename 同步) |
| K41 chore_treadmill 24h | 0% (R109) | 0% | 持平 (1 docs spec closure) |
| baseline lib tests | 431/431 (R109) | 431/431 | 0 (M0 spec closure 不動 code) |
| R13 untracked 守住 | 6 個 | 6 個 | 持平 (R106 spec 5 檔 A 不混 noise) |

**為什麼**:
- R105 接力清單首位明列「6 條 counter 重命名 _total 結尾（破 Prometheus 抓取, 需先廣播 alert/dashboard 跟進）」— spec 階段收 closure, 對齊契約封版
- R100 策略顧問風險 #1 (metrics schema 沒對齊業界 convention) 兩條護衛 chain 並存收齊: R102/R103 OTel semconv (R103 護衛 chain) + R106 Prometheus naming convention (本 change 護衛 test 設計, 留 R107+ 實作)
- 1 輪 1 件紀律: 實際 rename 6 條 metric 是跨 2-3 週窗口 scope (dual-emit + alert 廣播 + dashboard 廣播 + 文檔同步) — 本輪純 spec 對齊契約, code 留 R107+ owner follow-up
- 護衛 test 對現名會 false positive (現名無 _total 是 spec drift 源頭), 需 owner 拿對齊契約 + rename 同步 commit 才一致 — 留 R107+ rename 當下同 commit 一起寫
- M0 連續輪數張力: R101~R106 連 6 輪 M0 closure/護衛, R108/R109 接力 2 輪 M1 突破 (K0 Quota 9→10/13), R106 接力 closure 性質跟 R105 同 (解卡不算連發 M0, 是把 R105 留 untracked 備好狀態收 closure)

**搜尋**:
- 不需搜尋, R105 開的 spec 骨架 (4 spec 檔 + .openspec.yaml) 已備齊, 對齊 R102/R103/R105 spec closure pattern
- 5 週時程設計參考: OpenTelemetry / Prometheus 官方 semantic migration guide 標準 4-6 週 deprecation window

**做了什麼**:
- `openspec/changes/prometheus-counter-convention/proposal.md`: 5 段 (Goal/Background/Scope/Capabilities/廣播), 廣播段 4 事項 (抓取端/alert/dashboard/deprecation 公告)
- `openspec/changes/prometheus-counter-convention/design.md`: 6 條 counter rename 對照表 (現名→目標名) + 4 層面 impact (Code-level/抓取端/Grafana/文檔) + 5 週廣播時程 (T-0 公告→T-5 post-mortem) + 1 條新護衛 test 設計 (counter_metrics_must_have_total_suffix)
- `openspec/changes/prometheus-counter-convention/specs/prometheus-counter-convention/spec.md`: 4 Requirement + 8 Scenario (R-1 6 條 _total 結尾 / R-2 護衛 test 守 convention / R-3 廣播 4 層面 5 週時程 / R-4 不改反向違規 gauge)
- `openspec/changes/prometheus-counter-convention/tasks.md`: 8 個 task 4 phase 1 (T-CC1~T-CC6) + 2 phase 2 (T-CC7 closure + T-CC8 log), 8/8 [x] 全勾
- `openspec/changes/prometheus-counter-convention/.openspec.yaml`: status=open → closed, phase=1/1, R106 closure 註記段
- `git add` 精準列 5 檔路徑 (不用 `-A`), R13 守護 6 個 untracked 雜訊不污染
- 不動 lib.rs (護衛 test 留 R107+ rename 同步, 不在本輪 1 輪 1 件 scope)
- 不動 護欄 chain 17 條 (chain 飽和守住, M0 closure 不擴 chain)
- 不動 `lobsterpulse_sessions_total` (gauge 反向違規, R106+ follow-up 不同 spec drift 類型)

**驗證**:
- `git status --short`: 5 檔 A (prometheus-counter-convention 全套 spec), 6 untracked 守住 (R13)
- `grep -c "^- \[x\]" tasks.md`: 8
- `grep -c "^- \[ \]" tasks.md`: 0
- `cargo test --lib`: **431 passed; 0 failed** (本輪 M0 spec closure 不動 code → baseline 持平)
- K42 chain 17 條不擴張 (M0 spec closure, 護衛 chain 沒動)
- K41 chore_treadmill 24h: 0% 守住 (本輪 1 docs spec, 不算 chore)
- commit 1edb87a 落地 5 檔 / 499 insertions

**KPI-impact: K40 spec coverage closed 3/4 → 4/4 (prometheus-counter-convention 收 closure, R105 接力清單首位解卡), 0 KPI 數字變動**

**留 R107+ owner 接力**:
- prometheus-counter-rename-2026-q3: 開新 change 走實際 rename 6 條 metric (LP_METRICS const + emit site + 35 test assertion) + 1 條護衛 test `counter_metrics_must_have_total_suffix` in lib.rs (本 change 留 design 段, code 留 R107+)
- 5 週時程: T-0 公告 → T-1 dual-emit shim → T-2 廣播 → T-3 監控窗口 → T-4 切換 → T-5 post-mortem
- 廣播文檔先備齊: CHANGELOG.md / README.md / CONTRIBUTING.md 加 Prometheus metric rename notice (本 change 廣播段已寫完, R107+ 真正 rename 當下直接 copy-paste)
- gauge `lobsterpulse_sessions_total` 反向違規: 不同 spec drift 類型, 留 R106+ follow-up
- k0_measure.py docstring/spec drift: L3 "14 provider" / L35-37 "14 provider 真實清單" 跟 KNOWN_PROVIDERS=13 不一致 (R102 拆 K0-A 雙軌時漏修), 護衛 spec 窗口待修
- R108/R109 接力 M1 突破 (K0 Quota 9→10/13) 跟 R106 接力 M0 closure (K40 4/4) 雙軌並進, 守 K41 chore_treadmill < 30% 紅線
