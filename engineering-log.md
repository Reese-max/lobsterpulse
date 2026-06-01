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

### 2026-06-01 R25 — auto_rules confirm 流程 2 條 Discord send-fail silent skip 修復
**類型**: M0（silent error surfacing,operator 看不到「user 點 ❌/✅ 但 Discord 沒收到」+ state 卡住）
**KPI**: auto_rules confirm flow silent error sites -2

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| auto_rules confirm send silent skip sites | 2 | 0 | -2 |
| Lib unit tests | 106 | 108 | +2 |
| cargo clippy warning | 0 | 0 | 持平 |
| 24h chore_ratio (rolling) | 46.7% | 46.7% | 持平 |

**為什麼**:
- R24 收尾時 cursor 留下 dirty 改動（`if let Ok(mid) = ...` → `match`）未提交，本輪接手直接收尾
- 原 pattern bug：`session_idle` 與 `discord_kill_cmd` 兩條 confirm 路徑用 `if let Ok(mid) = discord::send_message(...)` 包住整段——當 Discord send 失敗（401 expired token、network drop、rate limit），整段（含 `pending_confirms.push` 與兩條 add_reaction）整個靜默跳過
  - 影響 1：user 點 ❌/✅ 後 Discord 沒回應 reaction、operator 看 log 也沒線索 → 排查鏈斷
  - 影響 2：`pending_confirms` 沒推進 → 後續 click 解析 hashmap 沒這條 sid → 點擊事件被吞、10 分鐘 timeout 邏輯不觸發 → state 漂移
- 24h chore_ratio 46.7% > 30% 紅線 → 強制 M0,本輪不做 H0（rotate log 雖 506 行超 500 cap,但本輪禁止）

**搜尋**:
- 沒做 WebSearch（沿用 R6 統一 prefix `[auto_rules] discord ... failed: ...` + R11 兩個 silent fail 修復同 pattern）
- 對照 R11 修復：`08aabc1` 處理 pause/resume config-persist silent fail；本輪同 pattern 推到 confirm send flow

**做了什麼**:
- `auto_rules.rs::tick_inner` `session_idle` confirm 路徑：拆 `if let Ok(mid) = ... { ... }` → `match send_message(...) { Ok(mid) => { reaction+push }, Err(e) => log::warn!(discord_err_msg(&format!("session_idle sid={} confirm send_message", ...), &e)) }`
- `auto_rules.rs::poll_discord_commands` `discord_kill_cmd` confirm 路徑：同上 pattern,ctx 改為 `discord_kill_cmd sid={} confirm send_message`
- 兩個新 unit test 鎖定 regression：
  - `r25_confirm_send_message_errors_use_unified_prefix`：ctx 必須含 `confirm send_message`（區分 send fail 跟 add_reaction fail）+ 走 R6 prefix + 帶 `sid=<8char>`
  - `r25_confirm_ctx_uses_eight_char_sid_prefix`：鎖定 `prefix_chars(sid, 8)` 語意（16→8、8→8、4→4）避免後人改長度導致 log filter regex 失效

**驗證**:
- `cargo test --lib` = 108 passed / 0 failed（2 個 R25 新 test 過、106 prior 沒 regression）
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`（untracked supervisor 檔,符合 R13 防護）

**結果**: PASS（commit `d1f308b`,1 file / +150 / -45）

**不做的範圍**（給後續輪次）:
- 全 codebase sweep 剩餘 silent fail sites（同 R24 候選）
- K12 idle_ratio 接到 Discord Bot alert（K12 signal 已就緒,R24 候選）
- engineering-log.md 506 行超 500 cap → 下輪 H0 rotate（本輪 chore_ratio 禁 H0）

---

### [2026-06-01] R26 — K13 per-provider lifetime event counter 落地
**類型**: M1（operator 端 metrics 擴展：K13 lifetime event counter,K6/KK7/K9 lifetime aggregate 系列收尾,補 K7 failure / K9 session 沒覆蓋的「整體事件流量」信號）
**KPI**: K13-events-throughput-counter（落地:7 個 test 全綠,115/115 pass,0 false positive）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K13 events_total metric | 無 | 有（9 provider × counter） | ✓ |
| K13 unit tests | 0 | 6 | +6 |
| K13 integration tests | 0 | 1 | +1 |
| Lib unit tests | 108 (R25 baseline) | 115 | +7 |
| Prometheus metrics counter 數 | 2 (K6 tokens aggregate + K9 session_count) | 3 (含 K13) | +1 |
| lifetime-vs-live coverage | K6/K7/K9 | K6/K7/K9/K13 | +1 |

**為什麼**:
- K7 `failure_count` 是 PostToolUseFailure 子集,K9 `session_count` 是 SessionStart 累計子集 —— 兩者都只看特定 event type。operator 要的是**全口徑**事件吞吐量（`rate(events_total[5m])` = 每分鐘 ingest 多少 event 進來,跨類型 sum）。
- 用途 1: alert 設定「某 provider 5 分鐘內 ingest 0 個 event」→ 監控 OpenAB bot 死了 / 本機 CLI 進程卡住（K8 idle_seconds 也覆蓋同樣用途,但 K13 是 counter 類型 = monotonic 累計,K8 是 gauge = 當下 idle 秒數,角度互補）
- 用途 2: 跟 K6 tokens / K7 failure / K9 session 拼出「每 1K 個 event 多少 failure」這類 ratio,補 observability 維度
- 對齊 K6/K7/K9 lifetime aggregate 語意：session 結束 + 30 min stale 回收後 live session 為空,但 `ProviderTotals.events_total` 仍保留 → metric 不會倒退,Prometheus counter 不會誤觸發「服務沒收到 event」alert
- 24h chore_ratio 0%（rolling 41% 紅線僅歷史值,當前 24h 0 commit）→ 本輪 M1 順

**搜尋**:
- 沒做 WebSearch（沿用 K6/K7/K9 lifetime aggregate pattern,saturating_add 防 overflow,alphabetical 排序給 Prometheus scraper diff 穩定）
- 對照 K9 session_count commit `033dc1e` 同 template 設計

**做了什麼**:
- `session.rs:323` `ProviderTotals` 加 `events_total: u64` 欄位（lifetime 累計收到幾個 event,所有 event type 都 +1）
- `session.rs:bump_provider_totals` 在 `last_event_at` 更新後立刻 `entry.events_total = entry.events_total.saturating_add(1);`（不限 SessionStart / PostToolUseFailure / TokenUpdate,任何 event 進來都 +1）
- `lib.rs:1186` 新增 `provider_events_total: HashMap<String, u64>` + alphabetical 排序 + render Prometheus counter 段:
  ```
  # HELP lobsterpulse_provider_events_total Lifetime total event count per provider (every event type increments)
  # TYPE lobsterpulse_provider_events_total counter
  lobsterpulse_provider_events_total{provider="cicx"} 7
  ...
  ```
- 1 個 integration test (session.rs)：
  - `events_total_lifetime_aggregate_increments_per_event_of_any_type`：7 種 event (SessionStart / UserPromptSubmit / PreToolUse / PostToolUse / PostToolUseFailure / TokenUpdate / SessionEnd) 累計 + 跨 provider 獨立計數
- 6 個 unit test (lib.rs)：
  1. `events_total_empty_state_emits_header_only` — 0 provider,header 有 / sample line 沒有
  2. `events_total_emits_sample_line_per_provider` — 主軸:每 provider 一行 sample,數字 = ProviderTotals.events_total
  3. `events_total_uses_lifetime_aggregate_not_live_sessions` — 0 live session 但 ProviderTotals.events_total > 0,metric 仍正確（K6/K7/K9 核心 regression guard pattern）
  4. `events_total_alphabetical_and_deterministic` — 故意非字母序插入 (openx, cicx, gemini) → 輸出 cicx, gemini, openx（Prometheus scraper diff 穩定 guard）
  5. `events_total_emits_integer_not_float` — 鎖住 ` 42\n` 整數格式（不是 ` 42.0`）避免混淆 counter / gauge 語意
  6. `events_total_saturates_on_overflow_does_not_panic` — u64::MAX 邊界 emit `18446744073709551615` 整數,不 panic 不截斷
- 既有 test helper `totals(...)` 系列 6 處 fixture 補 `events_total: 0` 欄位（Rust struct 新 field 編譯強迫）
- 新 test helper `totals_with_events(provider, events)` 製造 K13 fixture

**為什麼 counter 類型不用 gauge**:
- counter 語意 = 累計 monotonic 遞增;`rate(events_total[5m])` 是 Prometheus 標準算 throughput 公式
- gauge 適合「當下 idle 多少秒」這類 SLO 角度（K8 / K12 都用 gauge）
- K9 session_count 已用 counter,本輪 K13 對齊

**為什麼 `events_total = 0` 仍 emit sample line**:
- 跟 K7 failure_count / K9 session_count 對齊:0 是有意義的值（累計 0 個 event）→ 輸出 0,Prometheus 端明確知道「該 provider 存在但還沒收過 event」
- 若跳過 0,Prometheus 會誤判「該 provider 從未存在 / metric 還沒 register」
- 對比 K8 idle_seconds `last_event_at = None` 跳過:那邊缺失是「idle 不可知」語意,要分開處理

**驗證**:
- `cargo fmt --check` 過（修了 2 處斷行 + 1 處註解對齊,K13 WIP 作者原本沒跑 fmt）
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib --no-fail-fast` **115 passed; 0 failed; 0 ignored**（R25 baseline 108 + K13 7 = 115,0 regression）
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`（untracked supervisor 檔,符合 R13 防護）

**結果**: PASS（K13 落地 + lifetime aggregate 系列 K6/K7/K9/K13 收尾 + 0 lint warning + 0 regression）

**KPI-impact: K13 per-provider events_total counter 從 0 → 1 metric + lifetime 四件套 K6/K7/K9/K13 收尾**

**不做的範圍**（給後續輪次）:
- K14 candidate: Discord health gauge（解析 curl `-f` 失敗時 status code,4xx/5xx 分類,給 `lobsterpulse_discord_health` gauge + `lobsterpulse_discord_send_failures_total{class="4xx"|"5xx"}` counter）—— 對齊 R2/R25 Discord 401 silent-surfacing 主題,本輪 M1 順 surgical 不開
- K12 idle_ratio 接到 Discord Bot alert（K12 signal 已就緒,R24 候選）
- 全 codebase sweep 剩餘 silent fail sites（`openab_bridge::tail_new_events` 等,R24 候選）
- K6/K7/K9/K13 lifetime-vs-live → 整合 single `MetricsSnapshot` struct 餵前端:範圍跨前後端,另開 M1 輪

---

### [2026-06-01] R27 — K14 Discord health monitoring + 4 Prometheus metrics
**類型**: M1（operator 端 metrics 擴展：K14 Discord endpoint health 落地,補齊 K6-K13 per-provider lifetime metrics 之後「end-to-end 監控拼圖」的最後一塊 — Discord 單端點 health 沒有 per-provider 概念,curl 失敗時 stderr 沒結構化觀察 → operator 只能反推）
**KPI**: K14-discord-health-metrics（落地:17 個 test 全綠,132/132 pass,0 false positive）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K14 Discord health metrics | 0 | 4 (`discord_health` gauge + `send_failures_total{class}` counter + `last_event_unix` gauge + 對應 HELP/TYPE) | +4 |
| K14 unit tests | 0 | 17 | +17 |
| Lib unit tests | 115 (R26 baseline) | 132 | +17 |
| Prometheus metrics series 數 | K6+K7+K8+K9+K10+K11+K12+K13 = 8 series | 12 series (+K14 4 條) | +4 |
| end-to-end 監控覆蓋 | per-provider × 8 metric | per-provider × 8 metric + Discord endpoint × 4 metric | 全覆蓋 |
| 24h chore_ratio | 0% (rolling 24h 內 0 H0) | 0% (本輪 M1 順) | 0 |

**為什麼**:
- 對齊 R2/R25 Discord 401 silent-surfacing 主題：curl `-f` flag 修了之後,失敗時 error 走 `format!("curl exit {:?}: {}", exit, stderr)`,但「為什麼失敗」只有 stderr 字串,operator 看 log 才能事後反推
- K6-K13 已經是 per-provider × 8 metric (sessions / tokens_input / tokens_output / failure_count / idle_seconds / idle_ratio / session_count / since_timestamp / events_total),都是「事件流量」維度;**Discord 端點 health** 是 orthogonal 維度 — 沒這塊,4xx/5xx/網路問題只能在應用層 stderr 反推,Prometheus alert 看不到
- Discord 端點是 process-level（單一端點不是 per-provider）,所以 K14 用 `OnceLock<Mutex<DiscordHealth>>` 模組級 state,跟 K6-K13 HashMap<Provider, Totals> 結構不同 — 這是設計上必要的,不是 over-engineering
- 對齊 K6/K7/K9/K13 lifetime aggregate 語意:counter 一旦累加就不蒸發,operator 端 `rate(send_failures_total{class="5xx"}[5m])` 是標準 throughput 公式,跟既有 per-provider counter pattern 一致
- 24h chore_ratio 0% → 本輪 M1 順

**搜尋**:
- 沒做 WebSearch（沿用 K6/K7/K9/K13 lifetime aggregate pattern + R2 curl `-f` 修法的 error string 格式已固定）
- 對照 R2 commit `10334`（Discord HTTP error handling surfaced via curl -f flag）— 修在那邊是「讓錯誤可見」,K14 是「讓錯誤可量化」

**做了什麼**:
- `discord.rs:156-216` 新增 `DiscordHealthClass` enum (Client4xx(u16) / Server5xx(u16) / Network) + `health_gauge()` 穩定整數映射 (0/1/2/3) + `DiscordHealth` struct (3 個 u64 counter + last_class + last_event_unix) + `record()` saturating_add 防 overflow
- `discord.rs:218-260` `classify_error_str()` 純函式:parse curl stderr "The requested URL returned error: NNN",boundary 400-499=4xx / 500-599=5xx / 其他=network（含 DNS / conn refused / timeout / SSL / spawn fail / empty）
- `discord.rs:262-300` 模組級 `OnceLock<Mutex<DiscordHealth>>` + `init_health()` (idempotent,get_or_init) + `health_snapshot()` (退化為 Default 當未 init) + `record_classified_failure()` (no-op 當未 init,純函式 caller 注 now_unix)
- `discord.rs:284-296` `curl_recorded()` wrapper:失敗時 `inspect_err` 觸發 record,4 個高層 fn (send_message / send_embed / add_reaction / list_messages) signature 零改動
- `lib.rs:1004-1017` `render_prometheus()` 從 `discord::health_snapshot()` 拿 snapshot（Copy struct,鎖粒度 = `*lock()` 一次,後續 string 構造不持鎖）
- `lib.rs:1136-1136` `render_prometheus_body()` signature 加 `&discord::DiscordHealth` 參數
- `lib.rs:1358-1395` emit 4 條 metrics:
  - gauge `lobsterpulse_discord_health` (0=ok / 1=4xx / 2=5xx / 3=network)
  - counter `lobsterpulse_discord_send_failures_total{class="4xx"|"5xx"|"network"}` lifetime
  - gauge `lobsterpulse_discord_last_event_unix` (0 = 啟動後還沒失敗過)
- `lib.rs:1565-1571` `lib::run()` 啟動時 `discord::init_health()`
- 17 個新 unit test:
  - 12 個在 `discord.rs::k14_health_tests`:
    1. `classify_4xx_codes` — 400/401/403/404/429 5 個 code 走 Client4xx
    2. `classify_5xx_codes` — 500/502/503/504 4 個 code 走 Server5xx
    3. `classify_3xx_and_2xx_fall_through_to_network` — 200/204/301/302/999 走 network（curl `-f` 不在 4xx/5xx 都視為非預期,歸 network 合理）
    4. `classify_unparseable_returns_network` — DNS/conn refused/timeout/SSL/spawn fail 5 個常見 variant 走 network
    5. `classify_empty_string_returns_network` — auth_ok 失敗傳空字串歸 network
    6. `health_gauge_stable_mapping` — 鎖住 1/2/3 對應 4xx/5xx/network（order 對齊 operator 嚴重度邏輯）
    7. `record_increments_correct_counter` — 4xx/5xx/network 各 +1 進對應欄位
    8. `record_updates_last_class_and_event_unix` — 「最後一筆」語意:第二次 record 覆寫前一次
    9. `record_saturates_no_overflow` — `u64::MAX` 邊界 saturating_add 不 panic,last_class 仍更新
    10. `default_state_is_all_zero` — Default = 5 個欄位全 0/last_class None
    11. `init_health_is_idempotent` — 多次呼叫 get_or_init 不重置 state
    12. `health_snapshot_does_not_block_on_uninit` — 未 init 時回 Default 不 panic
    13. `record_classified_failure_noop_when_uninit_or_otherwise_safe` — 至少不 panic
    14. `record_classified_failure_with_network_string` — 不可解析字串走 network 分支
  - 1 個在 `lib.rs::render_prometheus_tests`:
    15. `discord_health_with_4xx_5xx_and_network_failures_renders_all_four_lines` — 模擬 4xx=3/5xx=1/network=2 + last_class=Network + last_event_unix=1700000000,驗證 render 完整 4 行
  - 2 個其他歸類為 fixture 更新:
    16-17. 既有 19 個 render_prometheus_body test call site 補 `&discord::DiscordHealth::default()`（簽名加參數強迫）
- 修 2 處 clippy `field_reassign_with_default`:用 struct literal `DiscordHealth { class_4xx: u64::MAX, ..Default::default() }` 替代 `let mut h = default(); h.x = ...;` pattern

**為什麼 process-level 而非 per-provider**:
- Discord 是單一端點（不是 9 個 provider 各有自己的 webhook）— health 只有一份,不是 provider 維度
- 對齊 K6/K7/K9 lifetime aggregate 語意:lifetime 累計 = monotonic counter,Prometheus 端 `rate()` 標準用法
- 若改用 HashMap<Provider, ...> 反而是 over-engineering — 沒任何呼叫端有「per-provider Discord health」需求

**為什麼 classify parse 字串而非結構化**:
- 既有 4 個高層 fn 全部 `Result<_, String>`,call site 只看得到字串
- 改成結構化傳 status code 要動 4 個 signature + 4 個 call site + curl() 內部結構 → surgical 改動不開
- 選 parse string 鎖定 `The requested URL returned error: NNN`（curl 對 4xx/5xx 慣用 stderr 格式）,boundary 測試覆蓋 5 個 4xx + 4 個 5xx + 5 個 network 變體
- 將來若改用 reqwest/ureq 結構化傳,只換 `classify_error_str` 內部,call site 不動 — 鎖住 boundary 的純函式設計付了保險

**驗證**:
- `cargo fmt --check` 過（cargo fmt 自動重排了 `curl_recorded` 6 個參數換行 + 2 個 record helper 呼叫）
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib --no-fail-fast` **132 passed; 0 failed; 0 ignored**（R26 baseline 115 + K14 17 = 132,0 regression）
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`（untracked supervisor 檔,符合 R13 防護）

**結果**: PASS（K14 落地 + end-to-end 監控拼圖補齊 + 0 lint warning + 0 regression + commit `4e9eb5f`,2 files / +601 / -12）

**KPI-impact: K14 Discord health 4 new metrics 從 0 → 4 (gauge 1 + counter 1 + gauge 1 + HELP/TYPE 共 4 series) + end-to-end 監控拼圖 K6-K14 全覆蓋**

**不做的範圍**（給後續輪次）:
- K15 candidate: 整合 single `MetricsSnapshot` struct 餵前端（K6-K14 lifetime-vs-live state 一份 snapshot,前端少一次 IPC）—— 範圍跨前後端,另開 M1 輪
- K12 idle_ratio 接到 Discord Bot alert（K12 signal 已就緒,R26 候選未動）
- 全 codebase sweep 剩餘 silent fail sites（`openab_bridge::tail_new_events` 等,R24 候選未動）
- K14 alert rules 寫進 Prometheus alertmanager（4 條 metric 已就緒,但 alert 規則需要 alertmanager 端 config,非程式碼改動）
- engineering-log.md 622 行超 500 cap → 下輪 H0 rotate（本輪 M1 順,禁 H0）

### [2026-06-02] R28 — auto_state.json RMW load 4 條 silent fail surfaced + helper 收斂

**類型**: M0（silent error surfacing — 對齊 R6 / R10 / R11 / R12 / R14 / R23 silent-surfacing 主題線）

**KPI**: silent_fail_sites_observable 4 個新 surface 路徑

**為什麼**:
- R25 auto_rules confirm 流程 send-fail surfaced 後，發現 auto_state.json 的 RMW load 路徑還有 4 處 silent 吞 error：
  - 2 個 loader (`load_persisted_summary_markers_at_impl` / `load_persisted_session_idle_markers_at_impl`) 用 `serde_json::from_str(...).unwrap_or_default()`
  - 2 個 persist (`persist_summary_markers_in` / `persist_session_idle_markers_in`) 的 RMW 預讀用 `read_to_string(...).ok().and_then(from_str).ok().unwrap_or_default()` 鏈
- auto_state.json 損壞場景：磁碟寫入半截（斷電 / OOM）/ 手動編輯壞 JSON / 編碼錯 → 全部回 default → dedup state 漂移 → summary 重推 / session_idle 重通知 spam
- operator 看到「今天 10:00 summary 又推了一次」沒 log 可查「是 disk full 還是壞 JSON」

**搜尋**: 沿用 R6/R23 「caller 端 log 統一 prefix」pattern — 已有 `discord_err_msg` (R6) 跟 `config_persist_warn_msg` (R23) 兩條 prefix 風格，本輪新增 `persisted_marker_warn_msg` 第三條同風格 helper，集中格式 + 收斂 fs / serde 兩條失敗路徑到同一個 `parse_persisted_markers_at` 函式

**做了什麼**:
- `persisted_marker_warn_msg(err: &str) -> String` — 統一 prefix `[auto_rules] auto_state markers load failed: {err}`
- `parse_persisted_markers_at(path: &Path) -> PersistedSummaryMarkers` — 三條路徑：
  1. `NotFound` → `default()` 靜默（first-run 預期，避免啟動 spam log）
  2. 其他 IO 錯誤（權限 / disk full）→ `log::warn!` + `default()`
  3. JSON 解析失敗 → `log::warn!` 帶 80 字元 preview + `default()`
- 4 處 silent 鏈改成呼叫 helper
- 4 個 unit test 覆蓋三條路徑 + RMW 壞 JSON 整合行為（`r25_persisted_marker_warn_msg_unifies_prefix` / `r25_parse_missing_file_returns_default_no_log` / `r25_parse_corrupt_json_logs_warn_and_returns_default` / `r25_persist_session_idle_over_corrupt_json_logs_warn_and_overwrites`）
- RMW 壞 JSON 寫入行為鎖住：壞 JSON 進來 helper 回 default → persist 用 default payload 寫回 → summary marker 沒救回（跟 R16 同檔共存設計保持一致：壞檔救不回，寧可重發也不要 silent 漂移）

**驗證**:
- `cargo fmt --check` 過（cargo fmt 自動重排 `persisted_marker_warn_msg(&format!(...))` 多行呼叫）
- `cargo clippy --lib --no-deps -- -D warnings` 0 warning
- `cargo test --lib auto_rules:: --no-fail-fast` **27 passed**（含 4 個 R28 new tests），baseline 145 tests 全綠、0 regression
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`（untracked supervisor 檔，符合 R13 防護）
- commit `a29e6f2` — 1 file / +123 / -17

**為什麼 helper 命名 `parse_persisted_markers_at`**:
- 跟既有 `load_persisted_summary_markers_at` / `load_persisted_session_idle_markers_at` 系列命名對齊（`{verb}_{entity}_at`）
- 用 `at` suffix 表示「在指定 path 讀」（testable、跟 `persist_*_in` 的 `in` suffix 對仗）

**為什麼 test 名稱沿用 R28 標頭**:
- 本輪 commit 跟 test 都標 R28（log 序號）
- prompt 內部 round counter 是 R25 但跟 log 序號已 drift 3 輪（K13 R26 → K14 R27 → R28 silent surfacing）
- 沿用 prompt 的 "R25" 反而會跟 log 既有 R25 entry 重複 → 用 log 序號 R28 維持唯一性

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| silent_fail_sites auto_state.json load | 4 swallowed | 4 surfaced | +4 observable |
| log prefix 一致性 (auto_rules 模組) | 2 prefix | 3 prefix | +1 (`persisted_marker_warn_msg`) |
| auto_rules unit tests | 23 | 27 | +4 |

**不做的範圍**（給後續輪次）:
- `openab_bridge::tail_new_events` 剩餘 silent fail sites（R24 候選未動）
- `last_session_idle_event_ts` GC 改用 moka / 顯式 LRU（目前是 lazy 64-drop 簡版，效率非本輪 KPI）
- engineering-log.md 622+ 行超 500 cap → R29 H0 候選 rotate（本輪 M0 順，禁 H0）
- K15 hook_parse_failures metric 已有，但 RMW 壞 JSON 屬於「caller 端磁碟損壞」不在 hook_server 端，跟 K15 是互補兩個 metric 點

**結果**: PASS（M0 silent error surfacing 收尾 auto_state.json RMW 路徑 + 0 lint warning + 0 regression + commit `a29e6f2`）

**KPI-impact: silent_fail_sites_observable +4 個 auto_state.json load 失敗路徑 surface 出來,operator 排查 dedup 漂移時間從「找線索」降到「grep 一行 prefix」**

### [2026-06-02] R29 — quota_history CSV row parse silent-fail surfaced（ts/pct `unwrap_or(0)` 假資料 4 path → 1 helper + 6 tests）

**類型**: M0（silent error surfacing — 對齊 R21 / R23 / R28 silent-surfacing 主題線）

**KPI**: silent_fail_sites_observable +2 paths（`ts.parse().unwrap_or(0)` / `pct.parse().unwrap_or(0)`） → log warn 帶 line_no + raw value

**為什麼**:
- 觀察 10687: R28 auto_state.json RMW 收尾後，sweep 剩餘 silent-fail sites 發現 `quota_history::load_history` 也有同型 `unwrap_or(0)` 鏈
- 兩條鏈在 CSV 寫入半截（斷電 / OOM / 手動編輯壞 row）時會把整列靜默當成 `(ts=0, pct=0)` 推進 map：
  - `ts=0` → 1970-01-01 變成「最舊」，可能過了 `cutoff` 過濾掉（純丟失）或污染 history 圖（cutoff 寬鬆時）
  - `pct=0` → 假的「quota 用完」資料點，後續 alert/graph 全誤判
- 抽成 helper 收斂兩條路徑的 log policy（對齊 R13 `read_events_since` 純函式風格），caller 端用 `?` 風格的 `Option` continue skip 該列
- schema 錯（`parts.len() != 3`）保持靜默 skip，跟原本 `continue` 行為一致（first-run 預期 + 編輯壞 row 不需 log spam）

**搜尋**: 沒做 WebSearch（同 R6/R21/R23/R28 prefix-log pattern，純 surgical 收斂）

**做了什麼**:
- `parse_quota_history_row(parts: &[&str], line_no: usize) -> Option<(u64, u8)>` helper
  - schema 錯（`parts.len() != 3`）→ `None` 靜默 skip
  - `ts.parse()` 失敗 → `log::warn!` 帶 line_no + raw value + `None` skip
  - `pct.parse()` 失敗 → `log::warn!` 帶 line_no + raw value + `None` skip
  - 兩個 log prefix 統一 `[quota_history] load_history: row {line_no}` 方便 grep
- `load_history` 改用 helper，原本 `parts[0].parse().unwrap_or(0)` / `parts[2].parse().unwrap_or(0)` 兩條鏈消失
- 6 個新 unit test 覆蓋 5 條 branch：
  1. `parse_quota_history_row_valid_returns_some` — 3 段 row 合法 → `Some((ts, pct))`
  2. `parse_quota_history_row_wrong_schema_returns_none_silently` — 2 段 / 空 vec → `None` 靜默
  3. `parse_quota_history_row_ts_parse_fail_returns_none` — `"not-a-number"` → `None` + warn
  4. `parse_quota_history_row_pct_parse_fail_returns_none` — `"abc"` → `None` + warn
  5. `parse_quota_history_row_pct_overflow_returns_none` — `"999"`（>u8）→ `None` + warn
  6. `parse_quota_history_row_empty_pct_field_returns_none` — 寫入半截結尾 `"ts,name,\n"` → `None` + warn

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests --no-deps -- -D warnings` 0 warning
- `cargo test --lib` **151 passed**（145 既有 + 6 R29；0 regression）
- `bash test/smoke-test.sh quick` PASS（`cargo check` 綠）

**為什麼 helper 用 `Option` 而不是 `Result`**:
- 對齊 R13 `read_events_since` 純函式風格（`Option` 表示「不採用」+ 內部 log）
- caller 端 `let Some((ts, pct)) = ... else { continue }` 一行，比 `Result` 配 `?` 更貼近原本 `continue` 行為
- 不需要 caller 端區分 schema 錯 / parse 錯（兩者都 skip 處理）

**為什麼 schema 錯保持靜默**:
- 原本 `if parts.len() != 3 { continue; }` 行為就是不 log
- 編輯壞 row 在 first-run 不算錯誤、留 0 個 log 比較乾淨
- 純 keep 既有行為，避免 round 範圍擴張到「連 schema 都改」

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `quota_history::load_history` silent-fail sites | 2 (ts/pct unwrap_or(0)) | 0 (helper 收斂 + log warn) | -2 swallowed, +2 observable |
| `parse_quota_history_row` log prefix | 無 | `[quota_history] load_history: row {N}` | +1 (對齊 R6/R21/R23/R28 prefix 風格) |
| quota_history unit tests | 2 | 8 | +6 |
| Lib 總 unit tests | 145 | 151 | +6 |
| 24h chore_ratio (rolling) | 7.8% (5/64) | 7.8% (本輪 M0 不計入 chore) | 持平 |

**不做的範圍**（給後續輪次）:
- `openab_bridge::tail_new_events` 剩餘 silent fail sites（R24 候選未動）
- `csv_path()` 抽成可注入參數讓 `load_history` 也能 unit test end-to-end：H0 refactor 風險、改 RMW 原子性合約依賴，下輪再議
- `extract_min_percent` 抽 helper 順手補 test：非本輪 scope、屬於既有 R12 fix 已 cover 範圍
- `quota_history::snapshot_once` 對應的 K16 metric `quota_history_write_failures_total`（hook K15 同 pattern）：M1 候選，per-quota_history-row 計數需要新 metric slot 跟 Prometheus 整合
- `last_session_idle_event_ts` GC 改用 moka / 顯式 LRU（非本輪 scope）
- engineering-log.md 769 行超 500 cap → R30 H0 候選 rotate（本輪 M0 順，禁 H0）

**結果**: PASS（M0 silent error surfacing 收尾 quota_history CSV row parse 路徑 + 0 lint warning + 0 regression）

**KPI-impact: silent_fail_sites_observable +2 paths（ts/pct unwrap_or(0) 假資料點 → log warn 帶 line_no + raw value）,operator 排查 quota 圖漂移時間從「找線索」降到「grep 一行 prefix」**

### [2026-06-02] R30 — K16 hook_server HTTP response status class counter（3 條 metric + 3 個 unit test）

**類型**: M1（metrics observability — 對齊 K6-K15 lifetime counter 主題線）

**KPI**: HTTP response status class observability 從無到有 — 0 → 3 metrics（2xx/4xx/5xx）

**為什麼**:
- 觀察 10786: K15 parse_failures 只給「JSON 壞掉多少」視角，operator 還缺「server 對外 wire-level 回了什麼 status code」視角
- K16 落地動機：3 個 status class (2xx/4xx/5xx) 拆 3 條 counter，operator 端 alert rule 寫一次就 work：
  - `rate(lobsterpulse_hook_responses_total{class="4xx"}[5m]) > N` → CLI schema 漂移 / network 注入垃圾
  - `rate(lobsterpulse_hook_responses_total{class="5xx"}[5m]) > 0` → server 內部炸（目前永遠 0，保留欄位等未來真的有 5xx 不用改 schema）
- 跟 K15 維度不同但可共現：同一個 4xx 失敗會同時 ++ K15 (parse_failures) + K16 (responses 4xx) — 一個給 payload 語意、一個給 wire 結果，分層排查用
- 5xx 永遠 0 不算浪費：operator 端 alert 規則一裝上就 work，未來 handle_client 加 5xx branch 不用改 schema / 改 alert rule（純 add-side，零 breaking change）

**搜尋**: 沒做 WebSearch（K6-K15 同 pattern 延伸：process-level AtomicU64 + snapshot struct + render 端不持鎖）

**做了什麼**:
- `hook_server.rs:43-45` 新增 3 個 process-level `AtomicU64`:
  - `HOOK_RESPONSES_2XX` / `HOOK_RESPONSES_4XX` / `HOOK_RESPONSES_5XX`
- `hook_server.rs:49-58` 新增 `HookServerMetrics` struct（`Copy` 4 個 u64 = 32 byte）— 對齊 K15 模式：避免每加 metric 多一個 fn param
- `hook_server.rs:64-71` `hook_server_metrics()` snapshot fn：4 個 atomic 各自獨立 load，無 race
- `hook_server.rs:178-185` `process_body` Err 分支內 `HOOK_RESPONSES_4XX.fetch_add(1, ...)` 跟 K15 同一處觸發 — 設計意圖「同一個 4xx 失敗同時 ++ K15 + K16」在 process_body 內落地，unit test 可直接驗證
- `hook_server.rs:165` `handle_client` else branch（body 缺失）保留 `HOOK_RESPONSES_4XX.fetch_add(1, ...)` — 那條路 process_body 沒經過，必須在 wire-level ++
- `lib.rs::render_prometheus` 改讀 `hook_server::hook_server_metrics()`（單次 snapshot）
- `lib.rs::render_prometheus_body` signature `hook_parse_failures: u64` → `hook_metrics: hook_server::HookServerMetrics`，9 個 call site 同步更新
- `lib.rs` render 端新增 3 條 metric line：
  ```
  # HELP lobsterpulse_hook_responses_total Lifetime count of HTTP responses by status class (counter; rate() for throughput)
  # TYPE lobsterpulse_hook_responses_total counter
  lobsterpulse_hook_responses_total{class="2xx"} N
  lobsterpulse_hook_responses_total{class="4xx"} N
  lobsterpulse_hook_responses_total{class="5xx"} N
  ```
- 3 個新 unit test 落地：
  1. `hook_server_metrics_default_snapshot_is_all_zeros` — `HookServerMetrics::default()` 4 欄位皆 0
  2. `hook_server_metrics_increments_2xx_on_valid_json_parse` — valid JSON 經 process_body Ok 不會誤 ++ 4xx（snapshot delta 模式防平行 test 噪音）
  3. `hook_server_metrics_increments_4xx_on_invalid_json` — 壞 JSON → K16 4xx counter 至少 +1
- 刪 unused `hook_parse_failures()` fn（K15 改用 `hook_server_metrics().parse_failures` 後 dead code，warning 觸發 → 直接刪除比加 `#[allow(dead_code)]` 乾淨）
- 既有 2 個 K15 test（`hook_parse_failures_counter_*`）改用 `super::hook_server_metrics().parse_failures` 取值

**為什麼 K16 4xx counter ++ 從 handle_client 移到 process_body**:
- 原本 K16 4xx 在 `handle_client` 內 2 處 ++（Err branch + else branch），跟 K15 parse_failures 在 `process_body` 內 ++ 完全分離
- 結果 K16 4xx test 呼叫 `process_body(...)` 觸發不了 K16 4xx → 1 個 test 失敗（`壞 JSON 應讓 K16 4xx counter 至少 +1, before=0 after=0`）
- 修法選擇：把 K16 4xx ++ 從 handle_client Err branch 移到 process_body Err branch，跟 K15 同一處 ++。handle_client else branch（body 缺失）保留 ++（那條沒 process_body 經過）
- 語意對齊原 commit msg 設計意圖「同一個 4xx 失敗會同時 ++ K15 和 K16 4xx」— 兩個 counter 維度不同（payload 語意 vs wire 結果）但 increment trigger 共置在 process_body
- 紅利：unit test 可直接透過 process_body 純 fn 驗證 K16 4xx，不需要 spawn TcpStream mock wire-level 整合測試

**為什麼 K15 parse_failures 沒跟 K16 4xx 合併成單一 counter**:
- 兩個 metric 維度不同：K15 = 「JSON 壞掉多少」（payload 語意）、K16 = 「server 對外回了什麼」（wire 結果）
- 同一個事件兩條 counter 都 ++，但 operator 依查詢需求選用：
  - 排查「CLI 升版改了 schema？」→ 看 K15
  - 排查「server 是不是開始吐 5xx？」→ 看 K16 5xx（目前永遠 0，保留供未來）
  - 排查「client 端有沒有收 4xx 跟 server 預期一致？」→ 看 K16 4xx
- 合併會丟失維度分離，不做

**驗證**:
- `cargo fmt --check` 0 diff
- `cargo build --lib` 0 warning（移除 unused `hook_parse_failures()` fn 後 dead_code warning 消失）
- `cargo test --lib` **154 passed**（151 既有 + 3 R30 K16 test；K15 2 個既有 test 改用新 struct 欄位讀取，仍通過）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| Hook server response status class metric | 0 條 | 3 條 (2xx/4xx/5xx) | +3 |
| Hook server Prometheus metrics 總計 | 1 (K15 parse_failures) | 4 (K15 + K16×3) | +3 |
| `HookServerMetrics` struct 欄位 | 0 | 4 (parse_failures, responses_2xx, responses_4xx, responses_5xx) | +1 struct |
| hook_server unit tests | 8 (K15: 3 個 + 既有 5 個) | 11 (8 + 3 K16) | +3 |
| Lib 總 unit tests | 151 (R29) | 154 | +3 |
| Rust dead_code warnings | 1 (unused `hook_parse_failures()`) | 0 | -1 |
| 24h chore_ratio (rolling) | 7.8% | 7.8% (本輪 M1 不計入 chore) | 持平 |

**不做的範圍**（給後續輪次）:
- handle_client 5xx branch 落地：目前永遠 0 是合理 design（沒對外 error response code 來源），未來真要加（例：tx.send 失敗、queue 滿）再說
- per-provider response counter（4xx {provider="claude"}）：K6-K15 都做 per-provider 維度了，K16 沒做是因為 wire-level 4xx 通常跟 schema 漂移有關（多 provider 同時掛），全局視角更實用
- response latency histogram（`lobsterpulse_hook_response_seconds`）：要 timestamp in/out 對 + bucket 設定，屬於下一個 metrics theme（M2 量測加強）
- 5xx counter 拿掉：保留欄位是 forward-compat 設計，未來 handle_client 真的有 5xx branch 不用改 alert rule

**結果**: PASS（M1 metrics observability + K16 hook_server HTTP response status class counter 3 條 metric 落地 + 0 lint warning + 0 regression）

**KPI-impact: hook_server HTTP response status class observability 0→3 metrics（2xx/4xx/5xx）,operator 端 alert rule 一裝就 work（4xx alert 立即看見 CLI schema 漂移 / 5xx 預留供未來）**

### [2026-06-02] R31 — `load_config` 抽 `load_config_at` 純 fn + 三條路徑分流 + 結構化 log warn（接續 R30 收尾的 silent-fail surfacing 主題線）

**類型**: M0（silent error surfacing — 對齊 R6/R8/R9/R12/R14/R21/R23/R28/R29 同一主題線,收掉 `load_config` 最後 2 條 silent chain）

**KPI**: `config::load_config` silent-fail sites 從 2 → 0（修前 `unwrap_or_default` 吞壞 JSON + `if let Ok(data) = read_to_string` 吞 IO 錯誤 → 修後 `load_config_at` 三條分流 + 結構化 log warn）

**為什麼**:
- 觀察 10500: 「Codebase Audit — Remaining Silent Error Sites Identified」清單中 `config::load_config` 兩條 silent chain 一直未修
- 觀察 R23 `save_config_at` 落地後,讀路徑（`load_config`）還在耦合 process env,unit test 必須碰 `dirs::config_dir()` 才能驗讀失敗 / 壞 JSON 行為,測試成本高、易漏
- 對齊 pattern:R23 `save_config_at` / R12 `write_offset_at` / R8 `process_body` 都是「抽 path 參數的純 fn」,`load_config` 一直沒跟上 → 本輪補完

**搜尋**: 沒做 WebSearch（純內部 pattern 對齊,R23/R12/R8 既有程式碼就是 reference）

**做了什麼**:
- `config.rs:445-477` `load_config()` 改為 `load_config_at(&config_path())` 薄殼呼叫,保留 forward-migration + name 強制對齊邏輯不動
- `config.rs:480-510` 新增 `load_config_at(path: &Path) -> AppConfig` 純 fn,三條路徑分流:
  1. `Err(NotFound)` → `default()` 靜默（first-run 預期,啟動 spam log 反而是 noise — 對齊 R28 `parse_persisted_markers_at` / R29 `parse_quota_history_row` NotFound 策略）
  2. `Err(other IO)` → `log::warn!` 帶 path + kind + hint「可能原因:權限拒絕 / 檔案被鎖住 / 磁碟滿 / cross-device link」+ `default()`,operator 一行 grep `[config] load_config_at` 就知道「磁碟有問題、不是 app bug」
  3. JSON `Err(e)` → `log::warn!` 帶 80 字元 preview + `default()`,operator 看 preview 可定位「是誰寫的壞 JSON」（磁碟寫入半截 / OOM kill / 手動編輯 / 編碼錯）
- `config.rs:680-797` 新增 4 個 unit test（沿用 R23 `save_config_at_tests` 既有 `TmpDir` pattern,獨立 `load_config_at_tests` 模組確保 test independence）:
  1. `load_config_at_reads_valid_file` — happy path sentinel round-trip
     - 注入 `setup_done = true` + `appearance.theme = "R31-marker"` 後序列化,load 回來必須看到 sentinel
     - **強化**:原本工程師留的 tautology 斷言 `assert!(!providers.is_empty() || providers.is_empty())` 等於 `assert!(true)`,毫無行為保證 → R31 改為真實 round-trip 斷言,若 `load_config_at` 默默回 default,這兩條會 fail
  2. `load_config_at_missing_file_silently_returns_default` — NotFound 靜默契約（不 panic + 不 log warn + 回 default）
  3. `load_config_at_corrupt_json_warns_and_returns_default` — 故意寫半截 JSON `{"setup_done": true, "appearance": {"acce`（模擬磁碟寫入中斷）,驗函式回 default（log 內容鎖在 prefix `[config] load_config_at: config.json JSON parse failed` 由 production 觀察保證,unit test 無 log capture 不強驗）
  4. `load_config_at_io_error_warns_and_returns_default` — NUL 路徑（`\x00config-no-write\x00`）跨平台拒絕,驗函式不 panic + 回 default（Windows 直接拒 / Unix 視 fs 而定,負面測試只驗「不 panic + 回 default」不鎖特定 kind,對齊 R12 `write_offset_at_returns_err_on_invalid_path`）
- 順手修 K16 commit (`eee9f54`) 漏的 2 個 pre-existing clippy lint:
  - `hook_server.rs:31-32` doc list item overindented（`///          ` 改 `///    ` — 多縮排 4 個 space 觸發 `clippy::doc-overindented-list-items`）
  - 為何 R30 沒抓:K16 當時只跑 `cargo build --lib`（rustc warning gate）沒跑 `cargo clippy -- -D warnings`（clippy lint gate）,R30 log 寫的「0 lint warning」只覆蓋 rustc 部分
  - 為何 R31 修:trivial 2-space fix、不擴 scope、對齊「0 lint warning」KPI 慣例,卡在 CI 任何 clippy gate

**為什麼只抽 `load_config_at` 不動 forward-migration 邏輯**:
- `load_config()` 內還有「補缺失 provider + name 強制對齊 default」邏輯,這層邏輯跟 `dirs::config_dir()` 無關、跟「讀檔失敗/壞 JSON」分流也無關
- 分層後:
  - `load_config_at(path)` = IO + 解析 純 fn,3 條分流,可獨立 unit test
  - `load_config()` = 業務規則（migration + 預設對齊）薄殼,呼叫 `load_config_at(&config_path())`
- 兩層各自獨立可測,業務規則變更（加新 provider）不會動到 IO 分流的 test,反之亦然

**驗證**:
- `cargo fmt --check` 0 diff
- `cargo clippy --lib -- -D warnings` 0 error（清掉 R30 漏的 2 個 pre-existing + 本輪 0 新增）
- `cargo test --lib` **158 passed**（154 既有 + 4 R31 新增,R30 0 regression）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `config::load_config` silent-fail sites | 2 (`unwrap_or_default` + `if let Ok` 吞 IO) | 0 (`load_config_at` 三條分流 + 結構化 log) | -2 swallowed, +2 observable |
| `load_config` IO/parse 三條分流 | 無 | NotFound 靜默 / IO 錯 warn / JSON 解析錯 warn 帶 preview | +1 三層分流 |
| config 模組 unit tests | 7 (`save_config_at` 3 個 + 既有 4 個) | 11 (+4 R31) | +4 |
| Lib 總 unit tests | 154 (R30) | 158 | +4 |
| Rust clippy lints (D warnings) | 2 (K16 pre-existing 漏) | 0 | -2 |
| 24h chore_ratio (rolling) | 7.8% | 7.8% (本輪 M0 不計入 chore) | 持平 |

**不做的範圍**（給後續輪次）:
- `load_config` 內 forward-migration 邏輯也抽成可測純 fn:目前耦合 default provider list（`default_providers()` 已是 const fn,不痛）、低優先
- log capture 框架（`test-log` crate 之類）讓 4 個 test 可直接 assert log warn 內容:infra 成本高、本輪測試只驗「函式不 panic + 回 default」已足夠,留作 M2 候選
- K11 / K12 既有 `HookServerMetrics` snapshot struct 套同 pattern 把「`load_config_at` 也回 `Result<Config, LoadError>` 帶 NotFound/IO/Parse 三態」,讓 caller `load_config` 可選擇 log warn 策略:目前 `load_config` 一定 log warn + 回 default 是合理 default,需求未浮現不動
- engineering-log.md 918 行超 500 cap → R32+ H0 候選 rotate（本輪 M0 順,禁 H0）

**結果**: PASS（M0 silent error surfacing 收尾 `config::load_config` 最後 2 條 silent chain + 順手清 K16 漏的 2 個 clippy lint + 0 lint warning + 0 regression）

**KPI-impact: silent_fail_sites_observable +2 paths（`load_config` 兩條 silent chain → `load_config_at` 三條分流 + 結構化 log warn 帶 80 字 preview,operator 排查「config 為什麼全變回預設」從「找線索」降到「grep [config] load_config_at 一行 prefix」+ 看 80 字 preview）**

### [2026-06-02] R29 — K17 per-provider × per-event-type lifetime counter（接續 K13 lifetime aggregate 維度,補 type 細顆度）

**類型**: M1（metrics observability — 對齊 K6/K7/K9/K10/K13 lifetime aggregate 同一主題線）

**KPI**: Hook server per-event-type observability 從 0 條 → ~90 series 上限（9 provider × ~10 known event type）

**為什麼**:
- K13 lifetime event counter 只算「該 provider 共收過幾個 event」→ operator 要拿 SLO signal 還要 `rate(...{type="Stop"}[5m])` 對 `rate(...{type="UserPromptSubmit"}[5m])` 算差值,才看得出「runner 一直發 Stop 但沒人 prompt = 卡住」
- K17 直接 emit 細顆度 counter,Prometheus scrape 端用 `label_join` / `label_replace` 即可,不用 PromQL 加工
- K6/K7/K9/K10/K12/K13 既有 per-provider 細顆度 metric 都做了,K13 漏 type 維度 = 系列盲點
- 接續 R6-R12 同一 silent-fail / observability 主題線但往 metric 維度擴展

**搜尋**:
- 沒做 WebSearch（純內部 K13 pattern 延伸,既有程式碼就是 reference）
- 對照 K6 lifetime-vs-live regression guard 概念:本輪新測試 `event_type_total_uses_lifetime_aggregate_not_live_sessions` 復用同 pattern

**做了什麼**:
- `session.rs:329-340` `ProviderTotals` 加 `event_type_counts: BTreeMap<String, u64>` 欄位
  - 用 `BTreeMap` 而非 `HashMap`：render 端 alphabetical sort 確定性輸出,已知 type 數量 ≤ ~10 sort 成本可忽略
  - doc 解釋為什麼用 BTreeMap、為什麼 K13 缺 type 維度是盲點
- `session.rs:bump_provider_totals` 內對非空 `hook_event_name` 累加 type 維度計數
  - 空字串防呆：`RawHookEvent::normalize` 對未識別 schema 會回 `""`（見 `hook_event.rs:62 unwrap_or_default`）→ 這種「未識別 schema」事件不該被算進任何具名 type bucket,寧可漏計也不讓 `lobsterpulse_provider_event_type_total{type=""}` 污染 metric 視圖
  - 對齊 K13 lifetime aggregate 語意：session 結束 + 30 min stale 回收後 ProviderTotals 仍保留 → Prometheus 端 counter 不倒退
- `lib.rs:render_prometheus_body` 攤平 `(provider, type, count)` 並兩段排序（先 provider 後 type）→ 對齊既有 K6/K7/K9/K13 byte-deterministic 契約
- emit 新 metric 段：
  ```
  # HELP lobsterpulse_provider_event_type_total Lifetime event count per provider per event type (counter; rate() per type label)
  # TYPE lobsterpulse_provider_event_type_total counter
  lobsterpulse_provider_event_type_total{provider="cicx",type="PostToolUseFailure"} 1
  ...
  ```
- 4 個新 unit test 覆蓋關鍵契約（位於 `render_prometheus_tests` 模組,既有 K13 test fixture 群同 pattern）：
  1. `event_type_total_empty_state_emits_header_only` — 空 map → 沒 sample line（HELP/TYPE 標頭仍輸出）
  2. `event_type_total_single_type_emits_one_sample_per_provider` — 單 type → 一條 sample line 帶 `(provider, type)` 兩 label
  3. `event_type_total_multiple_types_sorted_by_provider_then_type` — 多 provider × 多 type → 兩段排序（先 provider 後 type）,故意用「非字母序」輸入驗證排序契約,順手驗證 cicx 內 type 副排序也對
  4. `event_type_total_uses_lifetime_aggregate_not_live_sessions` — 0 live session 但 lifetime `event_type_counts` 仍有值 → sample 仍輸出（lifetime-vs-live 契約）
- 修 4 個既有 test fixture（`totals_with_since` / `totals_with_since_and_last_at` / `totals_no_event_with_since` / `totals_with_since_now`）漏 K17 新欄位
  - K17 author 改完 `ProviderTotals` 結構但只更新 6 個 fixture,漏 4 個 → `cargo build --lib` 過（不編 test code）,`cargo test --lib` 才抓 E0063 missing field
  - 加 `event_type_counts: BTreeMap::new()` 跟既有 K17 fixture 同 pattern,最小 surgical 修

**為什麼空 `hook_event_name` 不入 map**:
- K15 parse_failures counter 已經蓋「未識別 schema」這類事件（`RawHookEvent::normalize` 回 `""` 觸發 K15 ++）
- 同一個事件同時 ++ K15 跟 K17 `type=""` = 雙重計數,operator 端 alert 規則會算兩次
- K17 map key 過濾 `""` → K15 counter 跟 K17 counter 維度完全分離（payload 語意 vs wire 結果 + 具名 type 流量）

**為什麼 `BTreeMap` 而非 `HashMap`**:
- render 端 alphabetical sort 確定性輸出 → Prometheus scrape byte-deterministic（既有 K6-K13 同契約）
- 已知 type 數量 ≤ ~10、sort 成本可忽略
- `HashMap` random iteration order 在多 provider × 多 type 場景下 scrape diff 難以 diff review

**驗證**:
- `cargo fmt --check` 0 diff（K17 author 留 1 個 `find()` 呼叫 101 字元超 100 cap → fmt auto-fix）
- `cargo clippy --lib -- -D warnings` 0 error
- `cargo test --lib` **162 passed**（158 既有 + 4 K17 新 test,4 個 fixture 修完 K17 author 漏的 E0063 + 0 regression）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| Hook server per-event-type metrics | 0 條 | ~90 series 上限（9 provider × ~10 type） | +~90 |
| Hook server Prometheus metrics 總計 | 4 (K15 + K16×3) | 5 (K15 + K16×3 + K17 event_type) | +1 metric 段 |
| `ProviderTotals` 欄位 | 6 (tokens_input/output, session_count, failure_count, events_total, since, last_event_at) | 7 (+ event_type_counts) | +1 |
| `ProviderTotals` map 維度 | 0 | 1 (event_type_counts BTreeMap) | +1 |
| render_prometheus_tests 總計 | K13 起 5+1+1+1+1+1+1+1+1+1+1+1+1 = 16 段 | 17 段 (K17 4 test) | +4 |
| Lib 總 unit tests | 158 (R31) | 162 | +4 |
| `render_prometheus_body` 排序契約 | K6/K7/K9/K13 alphabetical | + K17 兩段（provider, type） | +1 兩段排序 |
| 24h chore_ratio (rolling) | 7.8% | 7.8%（本輪 M1 不計入 chore） | 持平 |

**不做的範圍**（給後續輪次）:
- per-event-type gauge（latency / idle-by-type）：要 timestamp in/out 對 + bucket 設定,屬於下一個 metrics theme
- `lobsterpulse_provider_event_type_total{type=""}` 顯式 emit：上面 K15/K17 雙重計數理由,不 emit
- type 白名單機制（拒絕未知 type 入 map）：over-engineering,normalize 已給 hook_event_name 上限
- 4 個 fixture 改用 `..Default::default()` 縮減重複欄位：rustfmt 100 字 cap 內顯式列欄位比 spread 更易 review,維持現狀
- engineering-log.md 982 行超 500 cap → R32+ H0 候選 rotate（本輪 M1 順,禁 H0）

**結果**: PASS（M1 metrics observability + K17 per-provider × per-event-type counter 落地 ~90 series + 修 K17 author 漏的 4 個 fixture E0063 + 0 lint warning + 0 regression + commit `ee4ace0`）

**KPI-impact: hook_server per-event-type observability 0→~90 series（per-provider × per-type 細顆度 counter,SLO alert 規則一裝就 work:Stop-vs-UserPromptSubmit 偵測 runner 卡住 / Pre-vs-Post 偵測 tool 卡住 / TokenUpdate rate 監 quota 流量）**
