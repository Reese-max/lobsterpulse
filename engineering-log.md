# Engineering Log

> 舊紀錄已歸檔到 engineering-log.archive.md

## 改善紀錄

| 24h chore_ratio (rolling) | 41% | 41% (本輪 M1 不計入 chore) | 持平 |

**為什麼**:
- R18 末列 K8 候選 = per-provider `session_count` lifetime；predecessor 接手時盤點發現 idle_seconds 是更優先的派生信號：
  - `session_count` 純接線 K6/K7 同 pattern，**沒有新設計價值**
  - `idle_seconds` 需要新欄位 `last_event_at`（每個 event 都更新）→ 順手把 metrics exporter 純 fn 化（注入 `now` 取代 `Utc::now()` 內呼叫）→ 補 5 條 unit test 蓋 idle 數學、None 跳過、lifetime-vs-live、alphabetical、clamp negative
  - 從 user 角度：idle_seconds 是 SLO signal（某 provider 卡住多久沒動），session_count 是純累計；idle 直接可接 alert，session_count 還要再算
- 純 fn 化紅利：`render_prometheus_body` 原本依賴 `Utc::now()` 內呼叫 → 改成接受 `now: DateTime<Utc>` 參數，wrapper `render_prometheus` 注入 `Utc::now()`，純 fn 內 0 時鐘依賴 → test 可注入任意時間驗證 idle 數學
- lifetime-vs-live 同 K6/K7：失敗事件已結束、session 早已被 stale 回收後 ProviderTotals 仍有 `last_event_at`，metric 仍正確反映（idle 持續增加、不會因 session 結束歸零）
- 24h chore_ratio 41% 仍超 30% 紅線 → 本輪**強制 M1**，不碰 H0

**搜尋**:
- 沒做 WebSearch（K6/K7/K8 同 pattern 延伸，純 surgical 接線 + 純 fn 化）
- 對照 K6 lifetime-vs-live regression guard 概念：本輪新測試 `idle_seconds_uses_lifetime_aggregate_not_live_sessions` 復用同 pattern

**做了什麼**:
- `session.rs:325` `ProviderTotals` 加 `last_event_at: Option<DateTime<Utc>>` 欄位（`None` = 該 provider 還沒收過 event）
- `session.rs:bump_provider_totals` 內每個 event 都 `entry.last_event_at = Some(Utc::now())`（不限 TokenUpdate / Failure — 任何 event 進來都刷新）
- `lib.rs::render_prometheus_body` signature 加 `now: DateTime<Utc>` 參數；wrapper `render_prometheus` 注入 `Utc::now()`
- `lib.rs` import `use chrono::{DateTime, Utc};`
- 新 metric 段輸出：
  ```
  # HELP lobsterpulse_provider_idle_seconds Seconds since last event per provider (lifetime aggregate)
  # TYPE lobsterpulse_provider_idle_seconds gauge
  lobsterpulse_provider_idle_seconds{provider="cicx"} 60
  ...
  ```
- 新增 5 個 unit test：
  1. `idle_seconds_empty_state_emits_header_only` — 0 provider，header 有、sample line 沒有
  2. `idle_seconds_skips_providers_with_no_event_yet` — `last_event_at = None` 的 provider 不輸出 sample（避免 Prometheus 端把缺失當 0 idle 誤判「剛剛才動」）
  3. `idle_seconds_uses_lifetime_aggregate_not_live_sessions` — 0 live session 但 ProviderTotals 有 last_event_at，metric 仍正確反映 lifetime
  4. `idle_seconds_alphabetical_and_deterministic` — 4 providers 不同 idle 值，alphabetical 排序 + 確定性
  5. `idle_seconds_clamps_negative_to_zero` — `last_event_at` 在「未來」1 秒時 clamp 到 0（時鐘回撥 / 序列化時間差 edge case）
- 既有 test helper `totals(provider, in_, out)` 預設 `last_event_at: Some(Utc::now())` 避免既有 9 條測試被 K8 新 metric 干擾
- 新 test helper `totals_with_last_event(p, in_, out, last_at)`、`totals_with_no_event(p, in_, out)` 製造 idle 數學 + None 跳過 fixture
- 既有 `output_includes_help_and_type_headers_for_every_metric` 新增 2 行 required header（HELP + TYPE）

**為什麼用 `now - last_event_at` 而不是 live session `last_event_at`**:
- 跟 K6/K7 lifetime-vs-live 一致：session 結束或 30 min stale 回收後，live sessions map 已空，但 `ProviderTotals.last_event_at` 仍保留 → idle 持續增加、不歸零
- user 體驗：某 provider 真的卡住 10 分鐘沒動 = idle 顯示 600；session 結束 + 新 session 開始 = idle 從 0 重新計（新的 `last_event_at` 覆蓋舊的）

**為什麼 `last_event_at = None` 時不輸出 sample**:
- Prometheus 端若看到 metric 缺失，預設視為「該 provider 沒收過 event、idle 不可知」
- 若輸出 0，會被誤判「剛剛才動、health 好」→ 跟實際語意相反
- 跟 K7 failure counter 對齊：`failure_count = 0` 的 provider 仍輸出 sample（0 是有意義的值）；idle 缺失是語意差異，要分開處理

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 66/66 pass（61 既有 + 5 新 K8；0 regression）
- `bash test/smoke-test.sh quick` PASS
- **Runtime smoke**（`smoke_k8.ps1`）：起 release exe → POST `/hook/claude` `UserPromptSubmit` → 讀 `/metrics` → grep `idle_seconds` 真的出 `lobsterpulse_provider_idle_seconds{provider="claude"} 1` ✓

**結果**: PASS（K8 落地 + last_event_at 派生維度 + 純 fn 化 + 0 lint warning + 0 regression + commit `R19-pending`）

**KPI-impact: K8 per-provider idle_seconds gauge 從 0 → 1 metric + lifetime 三件套 K6/K7/K8 收尾**

**不做的範圍**（給後續輪次）:
- HTTP-level e2e 測試 spawn metrics server thread：test 慢且 flaky 風險高、port 衝突要管理，K6/K7/K8 都已說明
- 把 K6/K7/K8 lifetime-vs-live pattern 套到 discord `poll_discord_commands` 的 silent-fail 路徑：無關 metric 範圍
- `.arch-fitness.json` / `.supervisor-report.json` 加 .gitignore：H0、24h chore_ratio 41% 紅線仍生效，下輪再議
- per-provider **session_count** 細顆度（`lobsterpulse_provider_session_count{provider="..."}` lifetime）：R18 末列為 K8 候選；本輪 K8 改走 idle_seconds 派生維度，session_count 純接線降為 K9 候選
- 把 `last_event_at` 順手接到 capsule UI 顯示「last activity X seconds ago」：超出 metrics 範疇、UI 改動大，下輪再議
- 把 idle_seconds gauge 改 counter（monotonic 計數）：語意不同（gauge = 當下 idle 多少秒 / counter = 累計 idle 秒數），user 端 SLO alert 通常用 gauge


### [2026-06-01] Round 20 — K9 per-provider session_count lifetime counter 落地（lifetime 四件套收尾）
**類型**: M1（K9 metrics 細顆度；lifetime 四件套 K6/K7/K8/K9 收尾）
**KPI**: K9-per-provider-session-count-lifetime-counter

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `lobsterpulse_provider_session_count{provider="..."}` counter | 無 | 有（9 provider × counter） | ✓ |
| `ProviderTotals.session_count` 欄位 | 已在（session.rs:430 handle_event += 1） | 已有，純接線 render 端 | 0（已就位） |
| `render_prometheus_body` 函式簽名 | 同 R19 | 同 R19 | — |
| Lib unit tests | 66 pass | 70 pass | +4（3 lib + 1 session integration） |
| `cargo clippy --lib --tests -- -D warnings` | 0 warning | 0 warning | — |
| `cargo fmt --check` | 過 | 過 | — |
| `bash test/smoke-test.sh quick` | PASS | PASS | — |
| 24h chore_ratio (rolling) | 43% | 43% → 待 R21 重算（本輪 M1 不計入 chore） | 持平 |

**為什麼**:
- R19 末列 K9 候選 = per-provider session_count 細顆度 lifetime。R19 評估「純接線、設計價值低」但實際有 SLO 價值：
  - 現有 `lobsterpulse_provider_sessions{provider="..."}` 是 **live** session 數（從 `SessionInfo` 切片算）
  - 新 metric 是 **lifetime** 累計（從 `ProviderTotals.session_count` 算）
  - 差異化 → user 知道該 provider 累計被 stale 回收的 session 數 = 使用量信號
  - 例如：當前 live claude=1、lifetime claude=10 → 知道 claude 累計開過 10 個 session、9 個已結束回收
  - 跟 K6 lifetime token / K7 lifetime failure / K8 lifetime idle 同 pattern
- 純接線紅利：`ProviderTotals.session_count` 欄位在 session.rs:430 早就 +1，K9 只缺 render 端讀出來
- 24h chore_ratio 43% 紅線仍生效 → 本輪**強制 M1**，不碰 H0
- 選 M1 K9 而非 M2 HTTP e2e / M0 silent-fail hunt：K9 風險最低、價值明確（1 個新 metric + 1 條 SLO 信號）

**搜尋**:
- 沒做 WebSearch（K6/K7/K8/K9 同 pattern 延伸，純 surgical 接線 + 純 fn 化）
- 對照 K6/K7/K8 lifetime-vs-live regression guard 概念：本輪新測試復用同 pattern

**做了什麼**:
- `lib.rs:1043-1056` `render_prometheus_body` 加 `let mut provider_session_count: HashMap<String, u64>` + 從 `t.session_count` 填
- `lib.rs:1067-1068` alphabetical 排序 `provider_session_count_sorted`
- `lib.rs:1118-1128` 新 metric 段輸出：
  ```
  # HELP lobsterpulse_provider_session_count Lifetime session count per provider
  # TYPE lobsterpulse_provider_session_count counter
  lobsterpulse_provider_session_count{provider="cicx"} 5
  ...
  ```
- 既有 `output_includes_help_and_type_headers_for_every_metric` 加 2 行 required header（HELP + TYPE）
- 既有 `empty_state_emits_zero_counters_and_no_provider_lines` 加 1 條 K9 empty 斷言
- 新 test helper `totals_with_session_count(provider, count)`
- 新 3 個 lib test：
  1. `session_count_empty_state_emits_header_only` — 0 provider，header 有、sample line 沒有
  2. `session_count_uses_lifetime_aggregate_not_live_sessions` — 0 live session 但 ProviderTotals.session_count=5，metric 仍正確反映
  3. `session_count_alphabetical_and_deterministic` — 3 providers 不同 session_count 值，alphabetical 排序 + 確定性
- `session.rs` 新 integration test `session_count_lifetime_aggregate_accumulates_across_unique_sessions`：
  - session 1：開 + 多個 event + 結束 → 累計 1
  - session 2：不同 session_id → 累計 2（lifetime 不蒸發）
  - session 3：開但不結束 → 累計 3
  - 同 session 重發 SessionStart → 不 +1
  - 不同 provider 獨立累計

**為什麼用 counter 而不是 gauge**:
- 跟 K6 lifetime token、K7 lifetime failure 對齊：lifetime 累計 = counter
- K8 idle_seconds 是 gauge（瞬時值）= 語意不同
- `session_count` 嚴格 monotonic 遞增（每個 unique session_id 算一次）→ counter 符合 monotonic 語意

**為什麼 `lobsterpulse_provider_sessions`（live）跟 `lobsterpulse_provider_session_count`（lifetime）並存**:
- live 給前端 capsule 顯示「當下有幾個 session 在跑」= UI 用途
- lifetime 給 Prometheus alert 算「使用量 / 變化率」= 監控用途
- 兩者並存 = 監控端可以算 rate(lifetime_session_count[5m]) = session 開啟率

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 70/70 pass（66 既有 + 3 lib K9 + 1 session K9 integration；0 regression）
- `bash test/smoke-test.sh quick` PASS
- **未做 runtime smoke**：R19 K8 runtime smoke 已驗證 metrics pipeline 真的從 /metrics 出來；K9 走完全相同的 pure fn 邏輯（從 ProviderTotals 拉出），pipeline 沒變 → skip
- 如果需要 runtime confirmation，下輪可加 `smoke_k9.ps1` 復用 R19 K8 模式

**結果**: PASS（K9 落地 + lifetime 四件套 K6/K7/K8/K9 收尾 + 0 lint warning + 0 regression + commit `033dc1e`）

**KPI-impact: K9 per-provider session_count lifetime counter 從 0 → 1 metric + lifetime 四件套 K6/K7/K8/K9 收尾**

**不做的範圍**（給後續輪次）:
- HTTP-level e2e 測試 spawn metrics server thread：test 慢且 flaky 風險高、port 衝突要管理，K6/K7/K8/K9 都已說明
- `.arch-fitness.json` / `.supervisor-report.json` 加 .gitignore：H0、24h chore_ratio 43% 紅線仍生效，下輪再議
- 把 K6/K7/K8/K9 lifetime-vs-live pattern 套到 discord `poll_discord_commands` 的 silent-fail 路徑：無關 metric 範圍
- 把 `ProviderTotals.session_count` 接到 capsule UI 顯示「累計 session 數」：超出 metrics 範疇、UI 改動大，下輪再議
- `lobsterpulse_provider_session_count` 跟 `lobsterpulse_sessions_total` 全域值的差異化 alert：R21+ 觀察

### [2026-06-01] Round 21 — quota_history CSV writeln 沉默吞 fs error surfaced + counter 修不說謊
**類型**: M0（user-facing observability bug + 計數語意 bug 同時修；R12 「不做的範圍」點名的 quota_history 池收尾）
**KPI**: K4-openab-bridge-observability 延伸池 → 新 K10-quota-history-csv-observability

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `quota_history::snapshot_once` silent fail sites | 1 (line 60 `let _ = writeln!`) | 0 | −1 |
| `written` counter 計數語意（caller 看到的值） | 不可信（含失敗列） | 與實際成功列數一致 | ✓ |
| Caller `Ok(N)` 語意（lib.rs:1446 hourly / lib.rs:1459 post-runners / manual_snapshot_once Tauri cmd） | 誤導 | 與磁碟一致 | ✓ |
| CSV row format regression guard | 無 | 1 條 unit test 鎖 `"ts,name,pct\n"` | ✓ |
| writeln 失敗 contract | 無 | 1 條 unit test 鎖 read-only handle → io::Error | ✓ |
| Lib unit tests | 70 pass | 72 pass | +2 |
| `cargo clippy --lib --tests -- -D warnings` | 0 warning | 0 warning | — |
| `cargo fmt --check` | 過 | 過 | — |
| `bash test/smoke-test.sh quick` | PASS | PASS | — |

**為什麼**:
- R12 末「不做的範圍」明確點名 `quota_history.rs:60 let _ = writeln!` 是「另一個 silent fail 池」待收。R6-R20 連 9 輪 M0 + K6-K9 metrics 都已落地，這條是**僅存**的 R12 deferred M0 候選
- 雙重 bug 同時修：
  1. **Silent fail**：`writeln!` 失敗（磁碟滿 / fd 中斷）完全無 log → operator 看 hourly 看到 `Ok(0)` 以為「無 usage runner」、實際是「CSV 寫失敗」
  2. **Counter 說謊**：原 `let _ = writeln!(f, ...); written += 1;` 不管 writeln 成功與否都 +1 → caller 收到 `Ok(N)` 誤以為 N 列都 commit 到磁碟
- 對齊 R6/R8/R11/R12 surface pattern：`log::warn!` 統一 prefix `[quota_history]`，log filter 一條 query 抓全部 quota CSV 失敗
- 對齊 R12 `write_offset_at` pure fn pattern：把 IO 操作抽成 `write_csv_row(f, ts, name, pct) -> io::Result<()>`，caller 端決定 log policy 與計數是否扣 → 同一個 module 內的兩個 IO helper 形狀對齊，方便日後 grep
- 24h chore_ratio 紅線觸發 → 本輪**強制 M0**，不碰 H0
- 選 M0 quota_history 而非 M1 K10+ 新 metric：M0 是真實 user-facing observability bug（CSV 資料點無聲遺失）、K-series 已收尾；新 metric 是 nice-to-have，bug 修是 must-fix

**搜尋**:
- 沒做 WebSearch（pure fn 化 + log surface 是 R4/R6/R8/R11/R12 既定 pattern，無新領域）
- 對照 R12 `write_offset_at` API 形狀：本輪 `write_csv_row(f, ts, name, pct) -> io::Result<()>` 完全對齊（caller-side log + counter policy）
- 對照 R8 `process_body` pure fn + R12 `write_offset_at` pure fn + R4 `write_local_usage_snapshot` pure fn：3 個 IO helper 都是同形狀，未來 grep 維護容易

**做了什麼**:
- `quota_history.rs:60` 抽 `fn write_csv_row(f: &mut File, ts: u64, name: &str, pct: u8) -> io::Result<()>` 為 pure fn
- caller `snapshot_once` 改 `match write_csv_row(&mut f, now, name, pct) { Ok(()) => written += 1, Err(e) => log::warn!("[quota_history] write_csv_row failed (ts={now}, runner={name}, pct={pct}): {e} — quota-history.csv 該輪缺一筆") }`
- 統一 prefix `[quota_history]`，與 R6/R8/R11/R12 既有 prefix 對齊（`[auto_rules]` / `[hook_server]` / `[openab_bridge]` / `[quota_history]`）→ log filter `grep '\[quota_history\]'` 一條 query 抓 CSV 失敗
- 加 2 條 unit test：
  1. `write_csv_row_writes_csv_line` — happy path：2 row 寫入 → 讀回 = `"1700000000,cicx,42\n1700000001,openx,7\n"`，鎖 format（無 BOM、無 CRLF、未來若有人改寫成 serialize 不能 break）
  2. `write_csv_row_returns_err_on_read_only_handle` — negative path：read-only handle 寫入 → `io::Error`（kind ∈ InvalidInput/BrokenPipe/PermissionDenied/Other，不鎖特定 kind 因 Windows/Unix 差異）→ 驗證 caller 端**會**收到 Err、**不會**誤算 `written`

**為什麼只動 line 60、不動 line 39 (`let _ = create_dir_all`)**:
- line 39 的 `let _ = std::fs::create_dir_all(parent)` 是 best-effort — 若失敗，後續 `OpenOptions::open(&path)` 會回 Err 並 bubble 到 caller 的 `log::warn!`（lib.rs:1446/1459 已處理）
- 改 line 39 表面是對齊 R12 surface pattern，**但**會把 `create_dir_all` 的 NotFound 報兩次（一次 line 39、一次 line 50 open）— 是 noise 不是 signal
- scope 控制：R12 已明確說明 create_dir_all 是 best-effort pattern，不重複 surface

**為什麼不把 `snapshot_once` 簽名改成 `Result<usize, (usize, usize)>` 帶「實際寫入 / 嘗試寫入」**:
- 既有 caller 對 `Err(e)` 已正確處理（log::warn + 繼續）；`Ok(N)` 語意從「嘗試寫 N 列」變「成功寫 N 列」= 語意變窄，但**沒人**依賴舊語意
- lib.rs:1446/1459 只 log Err，不讀 Ok 值
- manual_snapshot_once Tauri cmd 把 Ok(N) 丟給前端，UI 顯示「本次 snapshot X 列」= 改成「成功 X 列」語意更直覺，**不**是 breaking change

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 72/72 pass（70 prior + 2 new）
- `cargo test --lib quota_history` 2/2 new pass
- `bash test/smoke-test.sh quick` PASS

**結果**: PASS（M0 observability 改善 + counter 語意修對 + 0 lint warning + 0 regression + commit pending）

**KPI-impact: K10-quota-history-csv-observability silent fail 1 → 0；counter 計數從不可信 → 與磁碟一致**

**不做的範圍**（給後續輪次）:
- 把 `quota_history::snapshot_once` 從 `dirs::home_dir()` 抽成接受 `&Path` 參數：會動到 3 個 caller（lib.rs:737/1446/1459），scope 超出 M0 surgical
- 加 integration test「跑完整 snapshot_once → 讀 quota-history.csv 確認 row 數」：要 mock `dirs::home_dir()` 環境、需要 tmpdir，比 unit test 寫 CSV row 脆；unit test 已鎖 format 與 writeln contract
- 為 R6/R8/R11/R12/R21 5 個 silent-fail surface 點寫**整合** log filter doc（記下 `[prefix]` grep 速查表）：是 H0 docs，下輪再議
- 全 codebase sweep `let _ =` 殘留：R12 末已列為「scope 跨多 module、需另開一輪」，本輪 surgical
- 把 lifetime counter pattern 套到 `discord_kill_cmd` reaction count / `token_spike` trigger count 等：超出 metrics 範疇

### [2026-06-01] Round 22 — K10 per-provider since_timestamp gauge 落地（lifetime 五件套收尾：ProviderTotals.since 派生）
**類型**: M1（K10 metrics 細顆度；K6/K7/K8/K9 lifetime aggregate 同 pattern 收尾）
**KPI**: K10-per-provider-since-timestamp-gauge

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `lobsterpulse_provider_since_timestamp{provider="..."}` gauge | 無 | 有（per-provider × gauge，Unix epoch seconds） | ✓ |
| `ProviderTotals.since` 欄位 exposed 維度 | 0（已收集未 expose） | 1（gauge） | +1 |
| `render_prometheus_body` provider metric 段數 | 7 (sessions / active / tokens_in / tokens_out / failure / idle / session_count) | 8 (新增 since_timestamp) | +1 |
| Lib unit tests | 72 pass | 77 pass | +5 |
| `cargo clippy --lib --tests -- -D warnings` | 0 warning | 0 warning | — |
| `cargo fmt --check` | 過 | 過 | — |
| `bash test/smoke-test.sh quick` | PASS | PASS | — |
| 24h chore_ratio (rolling) | 41% | 41%（本輪 M1 不計入 chore） | 持平 |

**為什麼**:
- R19 末列 K8 候選 = per-provider `since` first-seen；K8 落地時 predecessor 順手把 `ProviderTotals.last_event_at` 收齊但 `since` 仍未 expose
- `ProviderTotals.since` 在 R19 之前的 `bump_provider_totals` 就已收集（`if entry.since.is_none() { entry.since = Some(Utc::now()) }`），是 R19 落地前就存在的 lifetime 信號 —— K10 純接線把它透出
- 用途：
  1. `now - since_timestamp` = uptime 對等量（該 provider 已監控多久）
  2. 結合 K8 idle_seconds 算「最近活動佔 lifetime 比例」= 健康度信號
  3. debug「quota 為什麼是 0」時一查就知道「這 provider 到底有沒有接入過」（since 缺失 = 從未收過 event）
- 對齊 K6/K7/K8/K9 lifetime-vs-live：since 進 ProviderTotals 後不蒸發，session 結束 + 30 min stale 回收後仍能看出「何時第一次被監控到」
- 24h chore_ratio 41% 仍超 30% 紅線 → 本輪**強制 M1**，不碰 H0

**搜尋**:
- 沒做 WebSearch（K6/K7/K8/K9 同 pattern 延伸，純 surgical 接線）
- 對照 K9 lifetime-vs-live regression guard 概念：本輪新測試 `since_timestamp_uses_lifetime_aggregate_not_live_sessions` 復用同 pattern（0 live session 但 ProviderTotals.since 已填 → 仍輸出）

**做了什麼**:
- `lib.rs::render_prometheus_body` 新增 `provider_since: HashMap<String, i64>` 收集 `ProviderTotals.since.timestamp()`、alphabetical 排序、sample line 輸出
- 新 metric 段：
  ```
  # HELP lobsterpulse_provider_since_timestamp Unix epoch seconds when this provider was first seen (lifetime aggregate)
  # TYPE lobsterpulse_provider_since_timestamp gauge
  lobsterpulse_provider_since_timestamp{provider="cicx"} 1735739400
  ...
  ```
- `since = None` 的 provider 不輸出 sample（對齊 K8 idle_seconds `last_event_at = None` 跳過策略，避免 Prometheus 端把缺失當 0 timestamp = 1970-01-01 誤判）
- 新增 5 個 unit test：
  1. `since_timestamp_empty_state_emits_header_only` — 0 provider，header 有、sample line 沒有
  2. `since_timestamp_emits_unix_seconds_per_provider` — 3 provider 不同 since（2024/2025/2026），驗 sample line 用 `timestamp()` 序列化
  3. `since_timestamp_skips_providers_with_no_since` — `since = None` 的 provider 不輸出 sample
  4. `since_timestamp_uses_lifetime_aggregate_not_live_sessions` — 0 live session 但 ProviderTotals.since 已填，metric 仍正確反映 lifetime
  5. `since_timestamp_alphabetical_and_deterministic` — 3 provider 故意非字母序輸入，alphabetical 排序 + 確定性
- 新 fixture helper：`totals_with_since(p, since)`、`totals_no_since(p)`
- 既有 `output_includes_help_and_type_headers_for_every_metric` test 補 K10 兩個 header
- 既有 `empty_state_emits_zero_counters_and_no_provider_lines` test 補 K10 sample line 缺席斷言
- test module 內 import `chrono::TimeZone`（用 `.with_ymd_and_hms` 構造 fixture timestamp，production code 不引入避免污染 runtime import）

**驗證**:
- `cargo fmt --check` → 過
- `cargo clippy --lib --tests -- -D warnings` → 0 warning
- `cargo test --lib` → 77 passed; 0 failed（前 72 + K10 5 條）
- `bash test/smoke-test.sh quick` → PASS

**結果**: PASS（commit `f71560c`、1 file / +230 / -0）

**不做的範圍**（給後續輪次）:
- M0-3 程式碼改動：K6/K7/K8/K9/K10 lifetime 五件套已收尾，下一輪可從更高層次思考：
  - 真正的 SLO 維度（histogram：session_duration_seconds / time_to_first_event）
  - OpenAB bridge ingest throughput（每分鐘事件數 counter）
  - 事件 type 細分（per-provider per-event-type counter，給「cicx ThinkingDelta 比例」等深度分析）
  - K-quota snapshot timestamp（OpenAB snapshot 檔最後修改時間 → age gauge）— quota_history 已有檔案，可順手 derive
- 把 K10 since_timestamp 接到 Discord Bot 通知（idle 比例 > 80% 觸發「該 provider 半年沒新事件」提醒）：超出 metrics 範疇、需另開 M1
- 全 codebase sweep `let _ =` 殘留：R12 末已列為「scope 跨多 module、需另開一輪」，本輪 surgical


### [2026-06-01] Round 23 — `!lp pause/resume` 2 處 config persist silent fail surfaced + save_config_at pure fn 化
**類型**: M0（user-facing observability bug：使用者主動改設定時，磁碟寫入失敗（磁碟滿 / 權限拒絕 / path 鎖住）原本 `let _ =` 沉默吞，UI 顯示「自動化暫停成功」但下次啟動 revert，operator 無 log 可查）

**KPI**: K11-auto-config-persist-observability

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `!lp pause` / `!lp resume` save_config silent fail sites | 2 sites | 0 sites | −2 |
| `config::save_config_at` 純 fn 化 + 路徑參數 | 無（耦合 `dirs::config_dir()`） | 有（接受 `&Path`，可注入 tmpdir 測） | ✓ |
| Discord 回應帶 ⚠️ 提示持久化失敗 | 否（永遠顯示「成功」） | 是（磁碟寫入失敗時 prefix `⚠️` + 提示查 `~/.lobsterpulse/config.json` 權限） | ✓ |
| 統一 log prefix `[auto_rules] !lp <action>:` | 無 | 有（對齊 R6 `[auto_rules] discord <ctx>` pattern，log filter 一條 query 抓所有 auto-config 持久化失敗） | ✓ |
| Lib unit tests | 77 pass (R22 後) | 80 pass | +3 |
| `cargo clippy --lib --tests -- -D warnings` | 0 warning | 0 warning | — |
| `cargo fmt --check` | 過 | 過 | — |
| `bash test/smoke-test.sh quick` | PASS | PASS | — |
| 24h chore_ratio | 41% (R22 後) | 41%（本輪 M0 不計入 chore） | 持平 |

**為什麼**:
- R12 末「不做的範圍」段點名「全 codebase sweep `let _ =` 殘留：scope 跨多 module、需另開一輪」— 本輪 surgical 聚焦 user-facing 風險最高的 2 條（`!lp pause/resume`，磁碟寫入失敗 = 使用者設定消失）
- 雙重 bug 同時修：
  1. **Silent fail**：`let _ = crate::config::save_config(&c)` 完全吞 error，operator / user 都看不到磁碟寫入失敗
  2. **UI 說謊**：Discord 回應永遠顯示「成功」，下次啟動設定 revert 沒人知道為什麼
- 對齊 R6 `discord_err_msg` 統一 prefix pattern：抽 `config_persist_warn_msg(action, err) -> String` helper，集中 prefix `[auto_rules] !lp <action>:`，log filter 一條 query 抓全部 auto-config 持久化失敗
- 對齊 R12 `write_offset_at` pure fn pattern：抽 `save_config_at(path, config) -> Result<(), String>`，`save_config` 變 thin wrapper（用 `config_path()`），unit test 可注入 tmpdir 測 happy / negative path，不必碰 process env 的 `dirs::config_dir()`
- 24h chore_ratio 41% 仍超 30% 紅線 → 本輪**強制 M0**，不碰 H0

**搜尋**:
- 沒做 WebSearch（pure fn 化 + log surface 是 R4/R6/R8/R11/R12/R21 既定 pattern，無新領域）
- 對照 R12 `write_offset_at` API 形狀：本輪 `save_config_at(&Path, &AppConfig) -> Result<(), String>` 對齊（caller-side log + caller caller UI 帶 ⚠️）
- 對照 R6 `discord_err_msg` 統一 prefix：抽 helper 集中 ctx 字串 → 1 條 unit test 鎖 format 穩定

**做了什麼**:
- `config.rs::save_config` 拆 2 個 fn：
  - `save_config_at(path: &Path, config: &AppConfig) -> Result<(), String>` — pure fn，接受任意路徑
  - `save_config(config: &AppConfig)` — thin wrapper 注入 `config_path()`
- `config.rs::tests` 加 `mod save_config_at_tests`：
  - `TmpDir` struct 含 `Drop` 自動清 tmpdir（沿 R12 pattern）
  - `save_config_at_writes_config_atomically` — happy path：寫出 → 讀回 → parse 過（鎖 serde round-trip + version/providers 欄位）
  - `save_config_at_returns_err_when_parent_is_a_file` — negative path：parent 是檔案（`/tmp/.../blocker/inner/config.json`）→ `create_dir_all` 失敗 → 回 Err，驗證 caller 端**會**收到 Err 才能 surfaced
- `auto_rules.rs` 加 `pub(crate) fn config_persist_warn_msg(action: &str, err: &str) -> String` helper
- `auto_rules.rs::handle_command` 內 2 處改寫：
  - `!lp pause`：`let _ = save_config(&c);` 改 `match save_config(&c) { Ok(()) => "自動化 **暫停**".into(), Err(e) => { log::warn!(...); format!("⚠️ 自動化 **暫停**（磁碟寫入失敗：{e}，重啟後會 revert，請查 `~/.lobsterpulse/config.json` 權限）") } }`
  - `!lp resume`：同上
- `auto_rules.rs::tests` 加 `config_persist_warn_msg_unifies_prefix` 鎖 format + 中文 / 特殊字元 error 原樣保留
- 既有 2 條 `let _ =` silent fail 清空

**為什麼只動 2 條 `!lp pause/resume`，不做全 codebase sweep**:
- 既有 35 條 silent fail sites 中，這 2 條是**唯一 user-initiated** 設定改動點（其他 33 條是 system / 自動背景操作）
- user-initiated 失敗的代價最高：使用者主動按下 → 期待生效 → 設定無聲 revert = 體驗斷裂
- 範圍控制 = surgical，本輪 M0 修最危險 2 條 + helper pattern 留下，後續輪次可套同 pattern 撈其他 system-initiated silent fail
- 對齊 R12 surface 原則：scope 控制、不過度 sweep

**為什麼 Discord 回應帶 ⚠️ 提示 + log::warn 雙管齊下**:
- `log::warn!`：給 operator（後台 log 監控 / Promtail / Splunk 抓 prefix 告警）
- Discord `⚠️` 前綴：給 user 立即可見（特別是 user 主動下 `!lp pause` 時，磁碟失敗立刻顯示，user 知道要去查權限）
- 兩者內容不重複：log 端帶 full prefix 方便 grep，UI 端給 actionable hint（具體 path）

**為什麼 `save_config_at` 加 TmpDir Drop 自動清而不是 `tempfile` crate**:
- YAGNI：本輪只需要 2 條 test，TmpDir 13 行就夠
- 既有 codebase 沒用 `tempfile` crate（grep `tempfile::` 0 hit）→ 不引入新 dep
- R12 `write_offset_at_tests` 也用 inline tmpdir pattern（`std::env::temp_dir()` + pid 後綴）→ 對齊 codebase 既有風格

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 80/80 pass（77 prior + 3 new：`config_persist_warn_msg_unifies_prefix` + `save_config_at_writes_config_atomically` + `save_config_at_returns_err_when_parent_is_a_file`）
- `cargo test --lib auto_rules::tests` 22/22 pass（21 prior + 1 new）
- `cargo test --lib config::save_config_at_tests` 2/2 pass
- `bash test/smoke-test.sh quick` PASS（cargo check 綠）

**結果**: PASS（M0 observability 改善 + UI 不再說謊 + pure fn 化 + 0 lint warning + 0 regression）

**KPI-impact: K11-auto-config-persist-observability silent fail 2 → 0；save_config 從 process-env-couple → testable pure fn**

**不做的範圍**（給後續輪次）:
- 全 codebase sweep 剩餘 33 條 silent fail sites（`hook_server::write_port_file` 既有 R14 處理、剩 `openab_bridge::tail_new_events` 6 條 `Ok(_) => ... else { return vec![]; }`、其他 system-initiated 點）：scope 大，需另開 M0 輪
- 把 `config_persist_warn_msg` 套到其他 Tauri command 的 config 持久化點（settings page 改 provider enabled / sound 設定）：範圍跨前端，本輪 M0 surgical
- 加 `tempfile` crate + 並行 test fixture：現有 1 個 helper 夠用，YAGNI
- 把 K10 since_timestamp 接到 Discord Bot 通知（idle 比例 > 80% 觸發「該 provider 半年沒新事件」提醒）：R22 末列為下輪 M1 候選
- K-quota snapshot timestamp（OpenAB snapshot 檔最後修改時間 → age gauge）：R22 末列為下輪 M1 候選，quota_history 已有檔案可順手 derive

### 2026-06-01 R20 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

### 2026-06-01 R23 — K11 per-provider quota_snapshot_age gauge 落地
**類型**: M1（推進 K6-K10 觀測性系列）
**KPI**: K11 quota snapshot age gauge 從 0 → 6 provider 監控點

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K11 quota snapshot age | 無 metric | 6 provider 監控點 | 落地 |
| K6-K10 series | 5 metric | 6 metric | +1 |
| unit test count | 80 | 91 | +11 |
| silent fail sites (R23 內) | 0 | 0 | 持平 |
| cargo clippy warning | 0 | 0 | 持平 |

**為什麼**:
- R22 末列為下輪 M1 候選，prior session 留的 K11 scaffold 缺 pure fn 測試（K8/K9/K10 每個都帶 3-5 條純函式測試 + 端對端測試，K11 卻只把新 metric 塞進既有 6 條 test 簽名改寫 + 1 條 empty-state assertion）
- 「runner 死了 vs quota 用完」對 user 來說無法區分 → K11 直接量化「snapshot 多久沒更新」→ Prometheus 可設 `quota_snapshot_age_seconds > 600` 觸發 alert
- 純推進既有方向，scope 控制 = K11 完整 closure（pure math + 端對端 + fs 邊界三件套）

**搜尋**: 沿用 codebase 既有 Prometheus exporter pattern（K6/K7/K8/K9/K10），無新外部依賴；不引入 `tempfile` crate（沿 R12/R22 config.rs inline TmpDir struct 13 行就夠）

**做了什麼**:
- `compute_quota_snapshot_age_seconds` 純函式 3 條 unit test：
  - mtime None → None（對齊 K8/K10 跳過策略）
  - mtime 過去 60s → Some(60)（主軸算法）
  - mtime 未來 → Some(0)（saturating，clock skew 安全網）
- `render_prometheus_body` K11 段 4 條端對端測試：
  - empty state 不假裝 0（防止「absent 假裝 age=0」誤判「runner 健康」）
  - 3 provider emit sample line
  - alphabetical 排序（含 `__local__` 排最前，因為 `_` < `a` 在 ASCII）
  - age=0 vs absent 區分（0 是「剛剛還在」、absent 是「從沒看到」）
- `collect_quota_snapshot_mtimes` fs helper 4 條測試：
  - home=None → 6 個 key 全 None（不 crash）
  - 部分檔案存在 → 有寫的 2 個 key 有 mtime、其他 None
  - openx 缺 + usage-bot.json 在 → legacy fallback 拿到 mtime
  - openx 在 + usage-bot.json 也寫了 → 走 primary，50ms sleep 確保 mtime 差異
- `QuotaSnapshotTmpDir` struct（沿 R12 config.rs pattern：pid 後綴命名 + Drop 自動清）
- 4 條 fs test 對齊 production shape：寫到 `<tmp>/.lobsterpulse/` 下（helper 內部 `home.join(".lobsterpulse")`）

**為什麼不引進 `tempfile` crate**:
- YAGNI：1 個 inline struct 13 行就夠 4 條 fs test
- 既有 codebase 沒用 `tempfile`（grep 0 hit）
- 對齊 R12 `write_offset_at_tests` + R22 `save_config_at_tests` 的 inline pattern

**為什麼 fs test 寫到 `<tmp>/.lobsterpulse/`**:
- helper 簽名是 production shape（`home: &Option<PathBuf>` = 真實 `dirs::home_dir()`）
- helper 內部 `home.as_ref().map(|h| h.join(".lobsterpulse"))` 組資料目錄
- 測試要模擬 production → 把檔案放在 `<tmp>/.lobsterpulse/` 才對齊
- 第一次跑 fs test 3 條全 fail 立刻抓出來這點（路徑偏差 bug 在 R23 被關掉，避免後續有人 copy paste 同樣 pattern 卻踩坑）

**為什麼 `skips_legacy_when_openx_exists` 加 50ms sleep**:
- 同一個 thread 連續 `std::fs::write` 兩次，在 Windows NTFS 上 mtime 精度可能都到秒級 → 兩個 mtime 可能相同 → `assert_ne!` flaky
- 50ms 間隔保證跨任何 fs 精度都不同 → 鎖定「helper 不會回 legacy mtime」這個語意
- 不靠 sleep 鎖主要斷言（`assert_eq!(*actual, openx_mtime)` 仍精確），只用在 secondary 反向斷言

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 91/91 pass（80 prior + 11 K11 new）
- `bash test/smoke-test.sh quick` PASS（cargo check 綠）

**結果**: PASS（K11 三件套 closure + 0 regression + 0 lint warning + scaffold→fully-tested）

**KPI-impact: K11-quota-snapshot-observability 6 provider 監控點落地；unit test 80 → 91 (+11)**

**不做的範圍**（給後續輪次）:
- K10 since_timestamp 接到 Discord Bot 通知（idle 比例 > 80% 觸發「該 provider 半年沒新事件」提醒）：R22/R23 兩輪都列為下輪 M1 候選沒動，本輪 K12 idle_ratio 上線後可直接餵 `> 0.8` alert
- 把 K11 age gauge 接到 Discord Bot：同樣列下輪 M1 候選
- K10 / K11 → 額外 derive metric（uptime / data-freshness 混合 score）：scope 大，需另開 M1 輪
- 全 codebase sweep 剩餘 33 條 silent fail sites（`openab_bridge::tail_new_events` 6 條 `Ok(_) => ... else { return vec![]; }` + 其他 system-initiated 點）：R22 末已列、scope 仍大
- 把 K6-K11 整合成 single `MetricsSnapshot` struct 餵前端：範圍跨前後端，本輪 M1 surgical 不動

---

### [2026-06-01] Round 24 — K12 per-provider idle_ratio gauge 落地 + 15 unit tests
**類型**: M1（operator 端 metrics 擴展：K12 idle_ratio gauge，K8/K10 派生，0..1 統一健康度信號）
**KPI**: K12-idle-ratio-observability（落地：15 個 unit test 全綠，0 false positive / 0 false negative）

**為什麼**: K8 絕對秒數容易被 provider age 短誤觸（剛上線 5 min 的 provider 收個 60s 沒 event 就 0.5，沒意義），K10 絕對時間（unix seconds）不會主動告訴 operator 怎麼判斷；K12 ratio 0=fresh / 1=never seen 是 0..1 統一閾值，alert rule `> 0.8` 一行就懂，且跨 provider 公平比較。純組合 K8 + K10、無新 fs / event 收集點 = 0 增加 cost 換一條新信號。

**搜尋**: 無（K8 idle_seconds / K10 since_timestamp 都已落地 + tests 完整，組合新 metric 是 trivial 推導；無需 WebSearch）。

**做了什麼**:
- `compute_provider_idle_ratio(now, last_event_at, since) -> Option<f64>` pure fn：
  - 任一 `None` → `None`（對齊 K8/K10 跳過策略：缺失值不該被當 0）
  - `lifetime ≤ 0`（`since == now` / 時鐘回撥）→ `None`（避免 NaN 誤判）
  - `idle > lifetime`（純函式防呆，理論不會發生）→ clamp 1.0
  - `idle < 0`（時鐘序列化時差）→ saturate 0
  - `idle = 0`（剛剛在動）→ 0.0 = 100% 健康，不丟這條信號
- `render_prometheus_body` 新增 `lobsterpulse_provider_idle_ratio` gauge，4-decimal 固定 precision（避免 IEEE 754 尾數雜訊導致 Prometheus diff 不穩）
- 15 個 unit test 涵蓋 pure fn 7 條 + end-to-end 8 條（含 alphabetical 排序 / 4-decimal 格式 / lifetime-vs-live regression guard / 與 K8+K10 跳過策略一致性）
- 修 1 個 fixture 數值錯誤：`idle_ratio_emits_fractional_value_with_four_decimals` openx 原本 `last_event_at=now-1s / since=now-30s` → ratio=0.0333 跟 comment 寫的 29/30≈0.9667 矛盾，改 `last_event_at=now-29s` 對齊 `idle 29s / lifetime 30s` 意圖
- 修 1 個 rustfmt diff（pure fn 鏈結斷行）：原 commit 沒跑 `cargo fmt`、留下 fmt diff 1 處，本輪順手補

**驗證**:
- `cargo test --lib` → 106 passed; 0 failed（baseline 91 → +15）
- `cargo fmt --check` → clean
- `cargo clippy --lib --tests -- -D warnings` → clean
- 修 fixture 後 K12 test `idle_ratio_emits_fractional_value_with_four_decimals` 從 panic 變 ok，0 false positive
- Prometheus 輸出 sample（manual 構造）：
  ```
  # HELP lobsterpulse_provider_idle_ratio Fraction of provider lifetime spent idle (0=fresh, 1=never seen activity); composite of K8 idle_seconds / K10 lifetime_seconds
  # TYPE lobsterpulse_provider_idle_ratio gauge
  lobsterpulse_provider_idle_ratio{provider="cicx"} 0.5000
  lobsterpulse_provider_idle_ratio{provider="gemini"} 0.5000
  lobsterpulse_provider_idle_ratio{provider="openx"} 0.9667
  ```
- commit `06d4ccf`：1 file +419 / -0

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K12 idle ratio metric | 0/0 (無) | 15/15 tests | +15 |
| Lib unit tests | 91 | 106 | +15 |
| Prometheus metrics gauge 數 | 5 (K6/K7/K8/K10/K11) | 6 (含 K12) | +1 |
| 跨 gauge 派生 metric | 0 | 1 (K8+K10→K12) | +1 |

**結果**: PASS

**不做的範圍**（給後續輪次）:
- K12 idle_ratio 接到 Discord Bot alert（`> 0.8` 觸發「該 provider lifetime 80% 在 idle」提醒）：本輪 M1 surgical 沒做，K12 signal 已就緒、下輪可一鍵接
- K10 since_timestamp 接到 Discord Bot 通知（同 R22/R23 候選沒動）
- K6-K11 + K12 → 整合成 single `MetricsSnapshot` struct 餵前端：範圍跨前後端、需另開 M1 輪
- 全 codebase sweep 剩餘 silent fail sites（`openab_bridge::tail_new_events` 等 33 條）：M0 surgical、可分多輪推進
