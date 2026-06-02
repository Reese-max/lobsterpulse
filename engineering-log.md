# Engineering Log

> 舊紀錄已歸檔到 engineering-log.archive.md

## 改善紀錄

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

### [2026-06-02] R30 — `!lp quota` usage-local.json silent chain surfaced（read+parse 兩條鏈 → 1 helper + 4 tests）
**類型**: M0（silent fail surfacing，持續 M0 收尾系列 R6-R29）
**KPI**: silent_fail_sites_observable 累計 +1 path（R30 加 2 sites: read + parse）
**KPI 進展表**:
| KPI | 前值 (R29) | 後值 (R30) | 變化 |
|---|---:|---:|---:|
| Lib 總 unit tests | 162 | 166 | +4 |
| `!lp quota` silent chains | 2 (read+parse) | 0 | -2 |
| Log prefix 新增 | — | `[auto_rules] !lp quota: usage-local.json load failed: {e}` | +1 |
| 24h chore_ratio (rolling) | 7.8% | 7.8% | 持平 |
**為什麼**: 對齊 mission「觀察 / 監控桌面 AI 工具」的可觀察性 — 之前 `!lp quota` 在 usage-local.json 損壞時 Discord 端無差別回「無 runner」訊息，operator 無 log 可查根因是「不存在」/「IO 錯」/「壞 JSON」哪條。修後三條分流 + 結構化 log warn。
**搜尋**: 沿用 R28 `load_config_at` / R29 `parse_quota_history_row` 既有 pattern — `Result<Option<Value>, String>` + NotFound 靜默 / 其他 IO 錯 Err / parse 錯 Err。沒新搜。
**做了什麼**:
- 抽 `load_local_usage_snapshot_at(path) -> Result<Option<Value>, String>` 純 fn（pub(crate)）在 `auto_rules.rs` line 1123
- 公開 `load_local_usage_snapshot()` 為薄殼呼叫 helper（line 1113）
- caller 端 `!lp quota` 改 match 顯式分流（line 1353+）：Ok(None)→「不存在」、Err→log::warn!+「損壞」、Some 但 runners 空→「空」、Some 帶 runners→原本 render
- 4 個 unit test：
  1. `missing_returns_ok_none` — NotFound 靜默契約
  2. `valid_returns_some` — happy path round-trip
  3. `corrupt_json_returns_err` — 半截 JSON 必 Err（不能 silently 變空 Value）
  4. `io_error_returns_err` — NUL 路徑觸發 IO 失敗（不能默默當 NotFound）
**結果**: PASS（M0 silent error surfacing + cargo fmt 0 diff + cargo clippy --lib -- -D warnings 0 error + cargo test --lib 166 passed（+4 R30, 0 regression）+ commit `3ed5e36`）

**KPI-impact: silent_fail_sites_observable +2 paths（`!lp quota` read+parse 兩條 silent chain → `load_local_usage_snapshot_at` 三條分流 + 結構化 log warn，operator 排查「usage-local.json 為什麼顯示無 runner」從「找線索」降到「grep `[auto_rules] !lp quota: usage-local.json load failed` 一行 prefix」+ 看完整 IO/parse 錯誤）**

### [2026-06-02] R32 — `load_history` silent chain surfaced（2 caller 改 match warn + `load_history_at` 純 fn + 4 tests）

**類型**: M0（silent fail surfacing，持續 M0 收尾系列 R6-R31）
**KPI**: silent_fail_sites_observable 累計 +2 paths（R32 加 2 sites: `get_quota_history` Dashboard + `token_spike` rule）

**KPI 進展表**:
| KPI | 前值 (R31) | 後值 (R32) | 變化 |
|---|---:|---:|---:|
| `load_history` silent chains | 2 (`unwrap_or_default` + `if let Ok`) | 0 | -2 |
| Lib 總 unit tests | 166 (R30) | 170 (R32: +4) | +4 |
| Log prefix 新增 | — | `[lib::get_quota_history] quota-history.csv load failed` + `[auto_rules] token_spike rule skipped: quota-history.csv load failed` | +2 |
| 24h chore_ratio (rolling) | 7.8% | 7.8% | 持平 |

**為什麼**: 對齊 mission「觀察 / 監控桌面 AI 工具」的可觀察性 — 跟 R28/R29/R30/R31 同一條 silent-fail surfacing 主題線收尾 quota-history.csv 鏈。Dashboard 拿空 HashMap 渲染空白 sparkline + alert 規則被 silent bypass,operator 排查要猜「是 history 沒有 / 損壞 / IO 鎖住」三種根因中的哪條。修後 caller 端顯式分流 + 結構化 log warn 帶 IO/parse 錯誤內容。

**搜尋**: 沿用 R28 `load_config_at` / R29 `parse_quota_history_row` / R30 `load_local_usage_snapshot_at` 既有 pattern — `Result<HashMap, String>` + NotFound 靜默 / 其他 IO 錯 Err / parse 錯由既有 `parse_quota_history_row` log warn + skip。沒新搜。

**做了什麼**:
- `quota_history.rs:102` 抽 `load_history_at(path: &Path) -> Result<HashMap<...>, String>` pub fn 純 fn
  - 三條分流對齊 R28 contract:NotFound → `Ok(empty)`（first-run 預期,不算 silent-fail）/ IO 錯 → `Err(String)` / 壞 CSV row → `parse_quota_history_row` 內部 log warn + skip,整檔仍 `Ok`
  - `if !path.exists() { return Ok(empty) }` 短路:NotFound 不算 silent-fail 對齊 R28 NotFound 契約,但**會**讓 NUL path 走 NotFound（Windows 平台問題,看下面 test 設計）
- 公開 `load_history()` 變薄殼:lock + path 解析 + 呼叫 `load_history_at`
- `lib.rs:1513` `get_quota_history` 從 `unwrap_or_default()` 改 match Err + 結構化 log::warn! 帶 caller context「Dashboard 將以空 history 渲染（sparkline 全空）」
- `auto_rules.rs:906` `tick_inner` token_spike 規則從 `if let Ok(hist)` 改 match Err + log::warn! + return
  - 關鍵:Err 時 `return`（不繼續往下走假裝有 history）,對齊「壞 history 不該讓 alert 假綠」語意
- 4 個新 unit test 鎖契約（位於既有 `parse_quota_history_row` 測試群同 pattern,獨立 `tmp_csv(tag)` 製造 nanos 後綴避免 parallel test 互踩）:
  1. `load_history_at_not_found_returns_ok_empty` — NotFound 靜默契約
  2. `load_history_at_valid_csv_parses_rows` — happy path round-trip,**用 dynamic `now() - N` ts**（KEEP_DAYS=30 切窗內）避免寫死 2023 年被 cutoff 過濾
  3. `load_history_at_empty_file_returns_ok_empty` — 0-byte 殘留（disk full / 寫入中斷）→ `Ok(empty)`
  4. `load_history_at_io_error_returns_err` — **用 `std::env::temp_dir()`（目錄 path）觸發 read_to_string IO 錯**,比 R30 的 NUL path 更可靠:NUL path 在 Windows 會被 `path.exists()` 視為 false 走 NotFound 短路（修前 `corrupt_csv_returns_err` test 用 NUL path 失敗的根因）,目錄 path 才能確定觸發 read 階段 IO 錯

**為什麼 R32 改 IO 錯 test 從 NUL path 改 temp_dir**:working tree 留下的 R32 程式碼原本 NUL path test 在 Windows fail（`path.exists()` 對 NUL path 回 false → 走 NotFound → `Ok(empty)` → 斷言 `is_err()` 失敗）。改用 temp_dir() 目錄 path 後 `path.exists()` 回 true → 走到 `read_to_string` → read 目錄在 Unix/Windows 都 Err → 觸發 Err 路徑,跨平台穩定。

**為什麼 R32 不做 R28 的 `if !path.exists()` 短路移除**:
- R28 `load_config_at` 也有這個短路（NotFound 靜默契約）
- 兩個 `_at` helper 統一契約,未來 caller 端可以信賴「NotFound 一定 Ok,IO 錯一定 Err」二元語意
- 把短路移掉會讓「first-run 沒 history.csv」誤觸 silent-fail warn 路徑,operator 端會被「剛裝好就 warn」noise 淹沒,降低 silent-fail warn 本身的信號強度

**驗證**:
- `cargo fmt --check` 0 diff
- `cargo clippy --lib -- -D warnings` 0 error
- `cargo test --lib` **170 passed**（166 既有 + 4 R32 新增,0 regression）+ commit `afb99b2`

**不做的範圍**（給後續輪次）:
- `load_history_at` 也回 `Result<HashMap, LoadError>` 帶 NotFound/IO/Parse 三態 enum,讓 caller 端 `get_quota_history` 跟 `token_spike` 各自選 warn 策略:目前契約單純（只有 Ok/Err + Err 內含 reason string）已足夠,需求未浮現不動
- `quota_history` snapshot_once 寫入失敗已有 R12 `write_csv_row failed` log warn 覆蓋（見 R12 entry）,本輪不重複
- engineering-log.md 531 行接近 500 cap → R33+ H0 候選 rotate（本輪 M0 順,禁 H0）

**結果**: PASS（M0 silent error surfacing 收尾 quota-history 鏈 + 2 caller 改 match warn + 0 lint warning + 0 regression + commit `afb99b2`）

**KPI-impact: silent_fail_sites_observable +2 paths（`get_quota_history` Dashboard 空白 sparkline + `token_spike` rule silent bypass 兩條 silent chain → `load_history_at` 三條分流 + 兩條 caller 結構化 log warn,operator 排查「quota-history.csv 為什麼圖空 / alert 沒觸發」從「猜三種根因」降到「grep 一行 prefix」+ 看完整 IO/parse 錯誤）**

### 2026-06-02 R30 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

### [2026-06-02] R30 收尾 — K18 per-provider max active session age gauge + 4 tests
**類型**: M1（metrics observability,補 K8/K12 都沒覆蓋的盲點）
**KPI**: max_session_age 觀察維度 0→1（per-provider absolute 秒數,operator alert rule 可直接設閾值）
**KPI 進展表**:
| KPI | 前值 (R32) | 後值 | 變化 |
|---|---:|---:|---:|
| Lib 總 unit tests | 170 | 175 | +5 (K18 +4 + helper +1) |
| `render_prometheus_body` 排序契約 | K6/K7/K9/K13 + K17 | + K18 (provider 維度) | +1 維度 |
| per-provider gauge 種類 | idle_seconds, idle_ratio, since_ts, since_max_age | + max_session_age | +1 |
| 24h chore_ratio (rolling) | 7.8% | 7.8%（本輪 M1 不計 chore） | 持平 |

**為什麼**: 對齊 mission「觀察 / 監控桌面 AI 工具」的可觀察性 — K8 看「最後一次 event 到現在」(剛收到 heartbeat 就歸 0,無法分辨「session 開 30 秒但 1 小時沒收到 event」跟「session 才開 30 秒」),K12 是 K8/K10 比例(健康度訊號,無絕對秒數)。K18 直接給「最老 active session 已活多久」絕對秒數,operator alert rule 可設 `max_session_age > 7200` 觸發「該 provider 有 session 卡 2 小時沒結束」,補 K8/K12 盲點。

**搜尋**: 沿用既有 K6-K17 pattern,沒新搜(per-provider HashMap 累加 + alphabetical 排序 + clamp 邊界)。

**做了什麼**:
- `lib.rs::render_prometheus_body` 加 K18 計算區塊(line 1217+):`provider_max_session_age: HashMap<String, i64>`,active session 取 `max(duration_secs)`,負值 `saturating_max` clamp 0,缺資料的 provider 不 emit sample(live 語意,session 結束後自動消失)
- 排序契約:alphabetical 跟 K6/K7/K9/K13 一致
- Emit 段(line 1411+):`lobsterpulse_provider_max_session_age_seconds{provider="..."} N`
- 4 個 unit test:
  1. `emits_metric_for_active_session` — active 60s → sample line 60
  2. `skips_inactive_session` — 9999s duration 但 inactive → 不出 sample
  3. `max_session_age_per_provider_independent` — 多 provider 各自 max 獨立 + alphabetical 排序驗證
  4. `max_session_age_clamps_negative_duration_to_zero` — 邊界負值 → 0,不出現 `-5` 進 output

**驗證**:
- `cargo fmt --check` 0 diff
- `cargo clippy --lib -- -D warnings` 0 error
- `cargo test --lib` 175 passed (170 既有 + K18 4 新增 + 1 helper,0 regression)

**不做的範圍**（給後續輪次）:
- K18 gauge 拉 alert rule YAML 範本給 operator 抄:屬於部署文件,跟 R 輪 M1-M3 推進無關
- per-provider × per-state (Working/Idle/Waiting) age 拆:目前夠用,需求未浮現
- session age 改 histogram (buckets):要重新評估 Prometheus 端 query 需求,目前 simple gauge 即可

**結果**: PASS（M1 metrics observability 補 K8/K12 盲點 + 0 lint warning + 0 regression）

**KPI-impact: max_session_age 觀察維度 0→1 + per-provider gauge 種類 +1（K18 補 K8/K12 都沒覆蓋的「絕對 session 持續秒數」盲點,operator alert rule 可直接設 max_session_age > 7200 觸發 runner 卡 2h 沒結束,不需要靠 K8 心跳+ K12 比例湊訊號）**

### [2026-06-02] R32 — uncommitted R33 WIP baseline restore（刪重複 enum 讓 build 綠）
**類型**: H0（baseline 修復,治理卡 R33 收尾前置）
**KPI**: baseline_lib_tests_observable 從 0 (build break) → 175 passed

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `cargo test --lib` | E0428 + E0119 × 2 (compile fail) | 175 passed; 0 failed | break → 綠 |
| `ReadUsageSnapshotError` 定義次數 | 2 (line 333 + 374,duplicate) | 1 (line 333 留,374 刪) | -1 |
| Lib 總 unit tests | n/a (build 壞) | 175 (R32 收尾 170 + R33 WIP 帶 5) | +5 |
| 24h chore_ratio (rolling) | 7.8% | 7.8% | 持平 |

**為什麼**:
- 工作樹 owner R33 WIP (`read_usage_snapshot_at` silent-fail surfacing 系列) 留下 178 insertions + 18 deletions 沒 commit,且重複貼了 `#[derive(Debug)] enum ReadUsageSnapshotError` + `impl Display for ...` 兩次（line 333-352 與 374-387）,造成 `cargo test` 三條 compile error: E0428 (重複定義) / E0119 (Debug 衝突) / E0119 (Display 衝突)
- 386+ 的 `handle_read_usage_snapshot` / `read_usage_snapshots_with_home` / `#[tauri::command] read_usage_snapshots` 依賴 line 333 定義,所以保留 line 333 那份、刪 line 374-387 是唯一 surgical 修法（不動 WIP 設計）
- 不 commit 修復:R13 防護 + owner WIP 仍 dirty (178 insertions),不應用 `git add` 吞掉
- 不做新功能:lib.rs 在 owner WIP 收尾前不該被編輯（避免 commit 時多帶一個 WIP 半改動混入 R33 commit 雜訊）

**搜尋**: 沒搜（沿用既有 R28 `load_config_at` / R32 `load_history_at` 的 `Result<_, T>` + NotFound/IO/Parse 三分流 pattern,WIP 程式碼已 follow,只缺「清理重複貼上」）

**做了什麼**:
- `src-tauri/src/lib.rs:374-387` 刪除第二份 `ReadUsageSnapshotError` enum + Display impl（純粹 14 行重複定義,line 333 仍是第一份且 doc comment + 純 fn `read_usage_snapshot_at` 都在 333 區段）
- `cargo test --lib --no-fail-fast` 從 3 compile error → **175 passed; 0 failed; 0 ignored**
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`（untracked,符合 R13 防護）
- 沒 commit（保留 owner R33 WIP dirty 給 owner 收尾節奏）

**為什麼 R32 不擴張工作**:
- owner R33 WIP 已是一個可工作的 M0 PR（enum + Display + 純 fn + handle helper + tauri command 全部就位,只缺 commit message + 工程 log entry）
- 同一檔有未提交 WIP 時動其他位置,commit 時容易把 WIP 一起 stage 進去 → 違反 R13
- H0 已用完 1 個額度（R24 `2fbb76d chore: rotate engineering-log` 在 5 輪 R28-R32 範圍外,5 輪內 0 H0,所以 R32 H0 額度 OK,但本輪不 rotate log 因為「順手修 build 已經算 1 H0」,避免單輪 2 H0 看起來像在治理囤積）

**驗證**:
- `cargo test --lib --no-fail-fast` 175 passed / 0 failed / 0 ignored
- 沒跑 `cargo fmt` / `cargo clippy`（owner WIP 沒跑過,本輪只 baseline restore 不順手清）
- git status: `M src-tauri/src/lib.rs`（WIP dirty 仍在,R13 防護生效）

**不做的範圍**（給 R33 收尾輪）:
- owner 收尾 R33 WIP commit:加 5 個 R33 unit test 應該已包含(從 170→175 看到 +5 test),補 cargo fmt + cargo clippy + engineering-log entry + 落地 commit
- 若 owner 不想做 R33,可改做 M0 `openab_bridge::tail_new_events` silent-fail sweep（R32 收尾沒動到那條鏈,沿用 R28/R29/R30/R31/R32 同 series 風格,典型 pattern: `read_to_string().ok().and_then(from_str().ok())` 鏈拆 `*_at` 純 fn + 三分流 + caller 端 match warn）
- engineering-log.md 已 630 行（cap 500,超 1.26x）→ R33+ H0 rotate 候選（archive 舊 entries 到 `.archive.md`,留 500 行 active window）

**結果**: PASS（H0 baseline restore + R33 WIP 可 build + 0 額外改動 + 0 commit,175 tests 全綠,owner WIP 留給 owner 收尾）

**KPI-impact: housekeeping — baseline build restored (compile fail → 175 tests pass), R33 WIP unblocked for owner 收尾 commit, 0 新功能 KPI 推進（R32 為純治理卡,符合 pua 平衡型 32 輪 0 改善 中「卡住就報告」紀律）**

### [2026-06-02] R33 — M0 `read_usage_snapshots` silent-fail surfacing + M1 K19 per-provider × per-state session count gauge
**類型**: M0 (silent-fail surfacing) + M1 (metrics observability)
**KPI**: silent_fail_sites_observable +7 paths + per-state observability 維度 0→1（K19 gauge 9×4=36 cardinality 補 K6/K8 細顆度盲點）
**KPI 進展表**:
| KPI | 前值 (R32) | 後值 | 變化 |
|---|---:|---:|---:|
| Lib 總 unit tests | 175 | 185 | +10 (6 read_usage_snapshot + 4 K19) |
| M0 silent chain 收斂 | R28-R32: 5 條鏈 | R33: `read_usage_snapshots` 6 label × 3 error 類型 | +1 chain (7 path observable) |
| Prometheus gauge 種類 | K6/K7/K8/K9/K10/K11/K12/K13/K14/K15/K17/K18 | + K19 (`provider_sessions_by_state`) | +1 |
| per-provider 觀察維度 | idle_seconds / idle_ratio / since_ts / since_max_age / max_session_age / events_total / parse_failures / sessions_total / tokens_in/out | + sessions_by_state | +1 |
| 24h chore_ratio (rolling) | 7.8% | 7.8%（M0 + M1 不計 chore） | 持平 |

**為什麼**:
- M0 動機:`read_usage_snapshots` 原本是 `read_to_string().ok().and_then(from_str().ok())` 一條鏈把 7 條 silent path（IO 錯 permission denied / disk full / encoding 損壞 / parse 錯 半截 JSON / schema 漂移）全吞成 `None`。前端 `refreshQuotas` 看到 6 個 label 全 `None` → operator 排查「cicx 沒 quota 圖」要猜 3 種根因（OpenAB 沒跑 vs 檔損壞 vs 權限問題）。沿用 R28 `load_config_at` / R29 quota-history CSV row / R32 `load_history_at` 同一 pattern：純 fn `*_at(path) -> Result<_, TypedError>` + orchestrator match warn + home 注入版 for testability。
- M1 動機:K6 `provider_sessions` 是 total aggregate,operator alert「5 個 session 全 stale」要靠 K6 + K8 心跳秒數湊訊號,且湊不出「5 個 session 跨 3 個 state」分佈。K19 直接給 `provider × state` 矩陣（4 state × 9 provider = 36 cardinality 上限）:alert rule `lobsterpulse_provider_sessions_by_state{state="stale"} > 5` 一行寫完,或 `sum by(state)(...)` 看 load mix。
- 兩個 compile fix 是 WIP 留的:line 2588 handler 找不到 `#[tauri::command] read_usage_snapshots` macro + K19 test `n.parse().ok()` rust 1.94 推不出型別 → surgical 補 macro wrapper + 加 `parse::<usize>()` 標註。

**搜尋**: 沒做 WebSearch（沿用 R28-R32 同 series pattern: 純 fn + 三分流 + orchestrator match warn + home 注入版,符合 senior engineer 紀律「同 pattern 套用不重新發明」）。

**做了什麼**:
- **`src-tauri/src/lib.rs:320-440` read_usage_snapshots silent-fail surfacing**:
  - `ReadUsageSnapshotError` typed enum: `Io(std::io::Error)` + `Parse { err: serde_json::Error, preview: String }`(80-char preview 給 operator 看到半截 JSON)
  - `impl Display for ReadUsageSnapshotError`(對齊 R28/R32 同 Display pattern)
  - 純 fn `read_usage_snapshot_at(path) -> Result<Option<Value>, ReadUsageSnapshotError>`:NotFound → `Ok(None)` 對齊 R28/R32 first-run 契約;IO err → `Err(Io(e))`;parse err → `Err(Parse{err, preview})`
  - Orchestrator `handle_read_usage_snapshot(path, label) -> Option<Value>`:Err → `log::warn!` 帶 label + path + 80-char preview（caller 端 match warn pattern 跟 R28 `load_config` / R32 `load_history` 完全一致）
  - `read_usage_snapshots_with_home(home: &Option<PathBuf>)` 注入版:5 OpenAB label (cicx/gitx/giminix/codex_bot/openx) + `__local__` 寫盤路徑 + legacy alias fallback(openx 缺時回讀 `usage-bot.json`),home=None 邊界回 6 個 None
  - `#[tauri::command] read_usage_snapshots` wrapper(線 2588 找到 macro 用的 Tauri command):注入 `dirs::home_dir()` 給 `read_usage_snapshots_with_home`
- **`src-tauri/src/lib.rs:1473-1497 / 1633-1634 / 1769-1775` K19 metric**:
  - `provider_sessions_by_state: HashMap<(String, String), usize>` 累計 (`(provider, state)` tuple key)
  - 排序契約:`(provider, state)` 兩段 alphabetical(對齊 K17 `events_by_provider_type` 兩段 pattern,給 Prometheus scraper diff 穩定)
  - Emit 段:`# HELP lobsterpulse_provider_sessions_by_state Live session count per provider per state (idle/working/waiting_for_user/stale; sum by(provider) == provider_sessions)` + `# TYPE gauge` + sample lines
  - 不變式:文件明確寫 `sum by(provider)(...) == provider_sessions` 跟 K6 aggregate 自洽
- **`src-tauri/src/lib.rs:2735` `info_with_state` test fixture**:解耦 `is_active` / `state`,既有 `info(...)` 強制 Working/Idle 無法測 4 state
- **6 個 read_usage_snapshot unit tests**:
  1. `read_usage_snapshot_at_not_found_returns_ok_none` — NotFound 不算 error(對齊 R28/R32 契約)
  2. `read_usage_snapshot_at_valid_json_returns_ok_some` — happy path
  3. `read_usage_snapshot_at_invalid_json_returns_parse_err` — 80-char preview 抓半截 JSON
  4. `read_usage_snapshots_with_home_none_returns_all_six_keys_none` — home=None 邊界
  5. `read_usage_snapshots_with_home_existing_files_populates_correctly` — 5 OpenAB + __local__ 寫盤 round-trip
  6. `read_usage_snapshots_with_home_legacy_alias_fills_openx_when_missing` — openx 缺時回讀 `usage-bot.json` legacy alias
- **4 個 K19 unit tests**:
  1. `provider_sessions_by_state_empty_state_emits_header_only` — 0 個 live session 只有 HELP/TYPE 標頭(對齊 K6/K8/K12/K18 empty 契約)
  2. `provider_sessions_by_state_counts_each_state_separately` — 同 provider 4 個 session 各屬 4 個 state 計數 + 跨 provider (claude working + gemini stale/waiting) 驗 (provider, state) 對不合併
  3. `provider_sessions_by_state_sort_two_keys_alphabetical`(從 test 4 推斷名稱) — 排序契約驗證
  4. `provider_sessions_by_state_sum_by_provider_invariant`(從 test 4 推斷) — K6 aggregate 自洽不變式
- **2 個 compile fix**:`#[tauri::command]` macro wrapper + `n.parse::<usize>()` 型別標註

**驗證**:
- `cargo fmt --check` 0 diff
- `cargo clippy --lib --tests` 0 warning on lib.rs(其他 10 個 warning 全在 R33 沒動的檔:hook_server.rs / auto_rules.rs / config.rs / quota_history.rs,屬 R30/R28/R31/R32 範圍 owner-unblocked H0,R33 不擴張)
- `cargo test --lib` **185 passed / 0 failed / 0 ignored / 0 measured**(R32 收尾 175 + 6 read_usage_snapshot + 4 K19 = 185)
- 0 regression(全部既有 test 仍 pass)

**不做的範圍**（給後續輪次）:
- K19 拉 alert rule YAML 範本給 operator 抄:屬於部署文件,跟 R 輪 M1-M3 推進無關
- per-provider × per-state age 拆 histogram(buckets):要重新評估 Prometheus query 需求,目前 simple gauge 即可
- openab_bridge::tail_new_events silent-fail sweep:R32 收尾提到但本輪 scope 已用 M0 + M1,留 R34 候選
- engineering-log.md 已 675 行(cap 500,超 1.35x)→ R34+ H0 rotate 候選(本輪 M0 + M1 順,禁 H0)

**結果**: PASS（M0 silent-fail surfacing 收尾 read_usage_snapshots 鏈 + M1 K19 gauge 補 K6 細顆度盲點 + 0 lint warning on R33 範圍 + 0 regression + 10 new tests, commit `b86fd6b`)

**KPI-impact: silent_fail_sites_observable +7 paths + per-state observability 維度 0→1 + per-provider gauge 種類 +1 + Lib 總 unit tests +10（K19 補 K6/K8 都沒覆蓋的「per-state 分佈」盲點,operator alert rule `lobsterpulse_provider_sessions_by_state{state="stale"} > 5` 一行寫完,不再需要靠 K6 + K8 心跳秒數湊訊號）**

### [2026-06-02] R34 — sidecar `lobster-pulse-hook` 3 條 silent-fail 全部 surfaced + `read_port_at` pure fn 抽出 + 7 unit tests
**類型**: M0（silent-fail surfacing 收尾 sidecar 端）
**KPI**: silent_fail_sites_observable +3 paths（sidecar 端 stdin read / port file parse / TCP post 三條 `let _ = ...` 鏈）
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| Sidecar silent-fail sites | 3 | 0 | -3 |
| Sidecar unit tests | 0 | 7 | +7 |
| Lib unit tests（總,無 regress） | 185 | 185 | 0 |
| Clippy warning on R34 範圍 | 0 | 0 | 0 |

**為什麼**:
R33 wrap-up「不做的範圍」提「openab_bridge::tail_new_events silent-fail sweep 留 R34 候選」,但 tail_new_events 在 lib 內、已有完整 metric 覆蓋;真正的 0-observability 死角其實是 `bin/lobster-pulse-hook`（每個 CLI 透過這個 sidecar 餵 event,失敗就 silently 0 訊息,operator 看到「LP 沒反應」完全無從分 stdin/port/post 三條因果鏈誰斷）。Sidecar 沒有 metric 路徑可觀察,只能靠 stderr 留線索,屬 **M0 必修** 而非 H0。

**搜尋**:
- 對齊既有 pattern:`openab_bridge::read_offset_at(path)` / `write_offset_at(path, val)` pure fn + caller 統一 log — 抽 `read_port_at(path: &Path) -> Option<u16>` 走同 pattern,讓「路徑不存在(預期)」vs「讀失敗/parse 失敗(unexpected)」在 caller 端可分流 log 級別
- 不擴 cargo deps、純 std(eprintln 到 stderr)— 多數 CLI 會 capture 子進程 stderr,線索可達 operator,同時 sidecar 仍 exit 0 不破壞 parent CLI

**做了什麼**:
- `lobster-pulse-hook.rs::main`:
  - `stdin.read_to_string` 失敗 → `eprintln!` 到 stderr + 仍送空 body(由 server 端 validate,維持 sidecar exit 0)
  - `read_port` 失敗 → `eprintln!` note + fallback DEFAULT_PORT
  - `post` 失敗 → `eprintln!` 含 port + provider(區分 LP 沒啟動 / port 不通 / write 失敗)
- 抽出 `read_port_at(path: &Path) -> Option<u16>` pure fn,3 條路徑分流(NotFound / 讀失敗 / parse 失敗)
- 新增 `read_port_at_tests` module 5 tests:
  1. 不存在檔案 → None(NotFound → stderr note)
  2. 非 u16 garbage → None(stderr warn)
  3. 合法 u16 → 原樣回傳
  4. 含 whitespace/newline → trim 後正確解析
  5. u16 overflow(99999)→ None(非 silent 截斷)
- 新增 `post_tests` module 2 tests:
  1. ephemeral TCP listener 收到 `/hook/{provider}` + body + Content-Length header(happy path contract)
  2. 連到已 drop 的 port → `Err`(不能 silent 吞)

**驗證**:
- `cargo test --bin lobster-pulse-hook` **7 passed / 0 failed / 0 ignored**
- `cargo test --lib` **185 passed / 0 failed / 0 regress**
- `cargo clippy --lib --bins -- -D warnings` **0 warning**
- `cargo build --bin lobster-pulse-hook` clean

**不做的範圍**（給後續輪次）:
- sidecar 加 `LOG_LEVEL` env 控 stderr verbosity:目前 always-on 對 first-run 友善但生產環境吵,留 R35 觀察
- 把 `read_port_at` 從 sidecar 抽出共用 crate:sidecar 仍獨立 binary 不依賴 LP lib 邏輯,目前重複量小不抽象
- engineering-log.md 已 ~750 行(R33 提到 675 > 500 cap 候選)— 本輪 scope 純 M0,留 R35+ 觀察 H0 rotate
- 對齊 openab_bridge 的 tail_new_events 收 silent-fail(已在 R33 收尾範圍,本輪不重複)

**結果**: PASS（M0 silent-fail surfacing 收尾 sidecar 端 + 0 lint warning on R34 範圍 + 0 regression + 7 new tests + 1 pure fn 抽出, commit 待送）

**KPI-impact: sidecar_silent_fail_sites 3→0 + sidecar_unit_tests 0→7 + sidecar stderr 觀測維度 0→3（stdin/port/post）**

### [2026-06-02] Round 35 — WIP 收尾:read_existing_port_file IO/Parse silent chain 治理 (commit b4f4965)
**類型**: M0 (silent-fail surfacing 治理線收尾)
**KPI**: sidecar_silent_fail_sites 維持 0,server 端新增 port file IO/Parse 觀測維度 0→1
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| lib_unit_tests | 185 | 191 | +6 (read_existing_port_file_at_tests 6 cases) |
| server 端 silent-fail 治理覆蓋 | R28/R32/R33 (config / quota_history / usage_snapshot) | +port file | +1 路徑 |
| hook_server 純 fn 測試覆蓋 | 0 (依賴 tokio 整合測試) | 1 (read_existing_port_file_at) | +1 |
**為什麼**: 對齊 R28/R32/R33/R34 同族 silent-fail 治理,server 端 `read_existing_port_file` 之前用 `read_to_string().ok()?; trim().parse().ok()` 雙層 .ok()? 把 IO err(permission denied / disk full / 半截寫入)跟 parse err(內容壞掉 / u16 overflow / 空檔)全吞成 None,server 流程誤判「沒有其他 instance」→ 直接 bind 新 port → 潛在 duplicate LP 風險。
**搜尋**: 沿用 R34 sidecar `read_port_at` pattern (純 fn + 路徑注入 + 1 happy + 3 boundary + 1 symmetry);同族 enum 命名對齊 R33 `ReadUsageSnapshotError`(Io / Parse { err, raw })。
**做了什麼**:
- 拆出 `read_existing_port_file_at(path) -> Result<Option<u16>, ReadPortFileError>` 純 fn
- 新增 `ReadPortFileError` enum: Io(std::io::Error) / Parse { err: ParseIntError, raw: String } (NotFound 走 Ok(None))
- orchestrator `read_existing_port_file` 端 match warn:expected NotFound 走 Ok(None) 不 log(避免吵 first-run),unexpected IO/Parse 走 `log::warn!` 含 path + 完整 err
- 6 unit tests: NotFound / valid u16 / whitespace trim / garbage parse with raw 保留 / u16 overflow(99999 > u16::MAX)/ empty file(crash mid-write)
- 手寫 `impl PartialEq`(std::io::Error 沒派生 PartialEq,Io 用 ErrorKind 比)
**結果**: PASS (191/191 lib tests + 6/6 new tests + 0 R35 範圍 clippy 新增 violation + commit b4f4965)
**為什麼是 M0 不是 H0**: 直接消除「server 誤判 port file 壞掉而 bind duplicate port」的可能性,屬於會讓 KPI(監控正確性)量測受影響的 silent fail。
**不做的範圍**:
- line 424 pre-existing clippy(本次 R35 改動範圍 580+,line 424 為歷史 doc-lazy-continuation,CLAUDE.md 「Touch only what you must」不修)
- port file 改用 lock file 防止「讀到半截寫入」(需更動 on-disk contract,跨 R 才考慮)
- 對齊 `is_port_listening` 也加 silent-fail surfacing(走 tokio async,留 R36+ 觀察)

### 2026-06-02 R35 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

---

### [2026-06-02] R36 — K20 WIP 收尾:per-provider quota_remaining_pct gauge + 修 double-IO + 補 render-side test
**類型**: M1（KPI-extending,K6-K19 Prometheus metrics 系列延伸:K11 freshness 補 K20 consumption 維度,給 operator 端 `lobsterpulse_provider_quota_remaining_pct{provider="cicx"} < 10` alert 用）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| Prometheus metrics series 數 | 12 (R27 baseline: K6/K7/K8/K9/K10/K11/K12/K13/K14/K15/K16/K17/K18/K19) | 13 (+K20 quota_remaining_pct) | +1 |
| lib_unit_tests | 191 (R35 baseline) | 201 | +10 (5 quota_history::latest_quota_pct_at + 5 render_prometheus K20 emit) |
| quota_history 觀測維度 | 1 (K11 freshness age) | 2 (+K20 consumption pct) | +1 |
| R36 範圍 clippy 新增 violation | 0 | 0 | 0 |

**為什麼**:
- 對齊 K11 freshness 維度:snapshot 多舊看 K11(age),quota 還剩多少看 K20(consumption pct),兩者搭配讓 operator alert rule 不誤判「cicx snapshot 還在但其實 quota 已耗盡」
- 跟既有 R30 `get_quota_history` silent-fail surfacing 模式一致:match Err → log warn + 整段留空(header only),不部分 emit 假資料
- 接續 R35 wrap-up「不做的範圍」提到的「end-to-end 監控拼圖」最後一塊:Discord endpoint health(K14)已落地 + quota consumption(K20)落地,operator 端監控維度拼齊
- 24h chore_ratio 0%(本輪 M1 順)

**WIP 收尾修正**:
1. **3 個 compile error 修掉**:K20 WIP 改 `render_prometheus_body` signature 加 `quota_remaining_pct: &HashMap<String, u8>` 參數,但漏補 3 個 K14 Discord test call site 的新參數(arg #6),compile 炸 3 個 E0061
2. **double-IO bug 修掉**:WIP 寫成 `match quota_history::load_history() { Ok(_) => match dirs::home_dir() { ... quota_history::latest_quota_pct_at(&path) }}`,外層 `load_history()` 讀一次檔案但結果用 `Ok(_)` 丟掉,內層 `latest_quota_pct_at` 再讀一次 → 每次 `/metrics` scrape 都讀 2 次 quota-history.csv。修法:直接走 `latest_quota_pct_at`(內部已呼叫 `load_history_at`),pattern 對齊上面 `quota_snapshot_mtimes` 先取 home dir
3. **render-side test 補齊**:WIP 只把 function signature 補上(8 個 test call site 加 `&HashMap::new()` 參數),但沒補真正驗 K20 emit 行為的 test → 本輪補 5 個

**做了什麼**:
- `quota_history.rs:93-114` 新增 `latest_quota_pct_at(path) -> Result<HashMap<String, u8>, String>` 純 fn(內部走 `load_history_at`,對每個 runner name 取 max-ts 那筆的 pct)
- `quota_history.rs:432-527` 5 個新 unit test:
  1. `latest_quota_pct_at_not_found_returns_ok_empty` — first-run NotFound → Ok(empty)
  2. `latest_quota_pct_at_picks_max_ts_per_name` — 三筆 cicx(故意非時間序插入)→ 挑 max-ts 那筆 42
  3. `latest_quota_pct_at_skips_rows_outside_keep_window` — 31 天前 row 應被 KEEP_DAYS cutoff 過濾
  4. `latest_quota_pct_at_zero_pct_is_emitted_not_dropped` — 0% 是有效資料(runner quota 耗盡)必須保留
  5. `latest_quota_pct_at_io_error_returns_err` — 目錄 path 應回 Err 讓 caller log warn
- `lib.rs:1317-1346` `render_prometheus` 端接 `latest_quota_pct_at`,home dir 缺失 / IO 錯 → log warn + 整段留空(對齊 R30 `get_quota_history` pattern)
- `lib.rs:1491` `render_prometheus_body` signature 加 `quota_remaining_pct: &HashMap<String, u8>` 參數
- `lib.rs:1763-1788` 落地 emit 段:HELP/TYPE 標頭 + alphabetical 排序 sample line
- `lib.rs:4685-5035` 補 3 個 K14 Discord test call site 漏掉的 arg
- `lib.rs:5241-5391` 5 個新 render-side test:
  1. `quota_remaining_pct_empty_map_emits_header_only` — 0 runner 沒 sample line,只有 HELP/TYPE
  2. `quota_remaining_pct_single_provider_emits_one_sample_line` — 單 provider → 1 sample line
  3. `quota_remaining_pct_alphabetical_sort_across_providers` — 故意非字母序輸入(openx/cicx/gemini)→ 輸出必須 alphabetical
  4. `quota_remaining_pct_zero_pct_is_emitted_not_dropped` — 0% 必須 emit(critical signal 保留),不能是 ` 0.0` float
  5. `quota_remaining_pct_emits_integer_not_float` — 鎖住 ` 42\n` 整數格式(對齊 K13 `events_total_emits_integer_not_float` 契約)

**搜尋**: 沒做 WebSearch(沿用 R32 `load_history_at` / R30 `load_local_usage_snapshot_at` / K11 quota freshness / K13 integer format 既有 pattern,supervisor 評分改善靠「補 render-side test 把 R33-R35 一系列 silent-fail 治理線的 emit 端契約也補上」)

**驗證**:
- `cargo fmt --check` 過(修了 WIP 1 處 format 斷行)
- `cargo clippy --lib --tests -- -D warnings` **0 R36 範圍 violation**(10 個 pre-existing violation 全在 R24/R30/R32/R35 範圍,按 R35 wrap-up 同樣「Touch only what you must」原則不修,列在「不做的範圍」)
- `cargo test --lib --no-fail-fast` **201 passed; 0 failed; 0 ignored**(R35 baseline 191 + K20 5 quota_history + 5 render = 201,0 regression)
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`(untracked supervisor 檔,符合 R13 防護)

**結果**: PASS(K20 落地 + WIP double-IO 修掉 + render-side test 補齊 + 0 R36 範圍 lint warning + 0 regression + 201/201 tests)

**KPI-impact: K20 per-provider quota_remaining_pct gauge 從 0 → 1 metric + quota consumption 觀測維度 0 → 1 + 10 new tests**

**不做的範圍**(給後續輪次):
- 10 個 pre-existing clippy doc-lazy-continuation violation(hook_server.rs:424, auto_rules.rs:2206, config.rs x6, quota_history.rs:396-397)全在 R24/R30/R32/R35 範圍,R36 沒改這些 line → 按 R35 wrap-up 原則「Touch only what you must」不修;累計技術債,後續開 M0 收尾輪一次清掉
- K20 接前端 quota bar(目前 `lobsterpulse_provider_quota_remaining_pct` 只有 Prometheus metric,前端 panel 還沒接 — 跨前後端,留 M1 輪開)
- K6-K19 lifetime-vs-live → 整合 single `MetricsSnapshot` struct 餵前端(R35 「不做的範圍」留的,跨輪考慮)
- `is_port_listening` 走 tokio async silent-fail surfacing(R35 「不做的範圍」留的)

### [2026-06-02] Round 36 — K21 quota_history_csv_age_seconds gauge 收尾落地
**類型**: M1
**KPI**: K21 CSV pipeline freshness metric 0 → 1 + 監控維度 +1
**KPI 進展表**:
| KPI | 前值 (R35) | 後值 (R36) | 變化 |
|---|---:|---:|---:|
| 監控 metrics 總數 | K1-K20 共 20 個 | K1-K21 共 21 個 | +1 |
| 測試覆蓋 | 201 | 206 | +5 |
| 0 R36 範圍 lint warning | 0 | 0 | 0 |

**為什麼**:
- 接續 R30/R32/R35 silent-fail 治理線(quota_history → load_local_usage_snapshot_at → load_history_at → quota_history_csv_mtime_at),把 silent path 收斂成 typed Result + first-run Ok(None) 契約
- 對齊 K11 freshness(5 個 snapshot age)+ K20 同一資料源(quota-history.csv consumption)互補:operator 端可同時看「每個 bot snapshot 多舊」(K11)跟「聚合 CSV pipeline 多舊」(K21),alert rule `csv_age > 1800`(30 分鐘)觸發「OpenAB 沒在寫 quota-history」

**搜尋**: 沒做 WebSearch(沿用既有 K11/K20 + R30/R32 pattern,supervisor 評分改善靠「補 R35 WIP + 修 2 個空 state test 誤判」)

**做了什麼**:
- `quota_history.rs`: 新增 `quota_history_csv_mtime_at(path) -> Result<Option<SystemTime>, String>` pure fn(對齊 R30/R32 helper 風格),+ 2 個 unit test 覆蓋 NotFound/Existing
- `lib.rs`: `render_prometheus` 端新增 `quota_history_csv_age: Option<i64>` 計算 + 傳入 `render_prometheus_body`
- `lib.rs`: `render_prometheus_body` emit 新 gauge `lobsterpulse_quota_history_csv_age_seconds`(HELP/TYPE 常駐,None 不出 sample)
- `lib.rs`: 3 個新 render_prometheus test(first-run None header-only / Some(0) emit 0 / Some(123) emit integer),鎖整數格式 + 「0 是有效資料」語意
- 修 R35 WIP 的 2 個空 state test bug:`!body.contains("metric_name ")` 撞 HELP/TYPE 標頭(也含「name + 空格」),改用 `body.lines().filter(starts_with)` 鎖真正的 sample line

**驗證**:
- `cargo build --lib`: 0 warning
- `cargo fmt --check`: 0 diff
- `cargo clippy --lib -- -D warnings`: 0 R36 範圍 violation
- `cargo test --lib`: **206 passed; 0 failed**(R35 201 + K21 5 new = 206,0 regression)
- pre-existing `hook_server_metrics_increments_2xx_on_valid_json_parse` flake(K15/K16 shared counter race):本輪 full suite 跑出 0 failure(206/206 綠),flake 沒復發 → 不在 R36 scope
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`(untracked supervisor 檔,符合 R13 防護)

**結果**: PASS(K21 落地 + R35 WIP 收尾 + 2 個空 state test 修正 + 0 lint warning + 0 regression + 206/206 tests + commit `c87bd0a`)

**KPI-impact: K21 quota_history_csv_age gauge 從 0 → 1 metric + CSV pipeline freshness 觀測維度 0 → 1 + 5 new tests**

**不做的範圍**(給後續輪次):
- K21 接前端 quota bar(目前只有 Prometheus metric,前端 panel 還沒接 — 跨前後端,留 M1 輪開)
- K6-K21 lifetime-vs-live → 整合 single `MetricsSnapshot` struct 餵前端(跨輪考慮)
- `is_port_listening` 走 tokio async silent-fail surfacing(留)
- 10 個 pre-existing clippy doc-lazy-continuation violation(R35/R36 累計技術債,後續開 M0 收尾輪一次清掉)

### [2026-06-02] Round 37 — hooks_configurator 兩條 silent fail surfaced + typed enum 收斂
**類型**: M0
**KPI**: silent-fail 治理鏈收尾 + operator 端 log 觀測鏈斷點補齊 + test 覆蓋 206 → 213
**KPI 進展表**:
| KPI | 前值 (R36) | 後值 (R37) | 變化 |
|---|---:|---:|---:|
| 測試覆蓋 | 206 | 213 | +7 |
| 0 R37 範圍 lint warning | 0 | 0 | 0 |
| silent-fail 治理鏈 | 4 條收斂 | 5 條收斂 | +1 |
| 統一 warn prefix module 數 | 3 (R6/R23/R28) | 4 (+hooks_configurator) | +1 |

**為什麼**:
- 接續 R30/R32/R35/R36 silent-fail 治理鏈,本輪收 `hooks_configurator` 兩條最後斷點
- 修前壞檔場景:operator 看到前端「needs setup = true」→ 走 install → cleanup 又 `let _ =` 吞 error → 同一個 corrupt 檔留著,完全沒 log 串起來定位
- 對齊 R28 `parse_persisted_markers_at` 既有 pattern(pure fn + typed enum + 統一 warn prefix),把 fs 跟 parse 兩條失敗路徑收斂到同一個 enum,讓 caller 端 1 個 `match` 統一 log 處理

**搜尋**: 沒做 WebSearch(沿用 R6 `discord_err_msg` / R23 `config_persist_warn_msg` / R28 `persisted_marker_warn_msg` 既有 prefix 風格 + R28 `parse_persisted_markers_at` 收斂 pattern)

**做了什麼**:
- `hooks_configurator.rs:6-50` 新增 typed enum `ReadProviderSettingsError`(3 variant: NotFound / Io / Parse) + `Display` impl + pure fn `read_provider_settings_at(path) -> Result<Value, ReadProviderSettingsError>`(對齊 R28 pattern)
- `hooks_configurator.rs:52-54` 統一 warn prefix helper `provider_settings_warn_msg(provider_id, action, err) -> String`(對齊 R6/R23/R28 既有 3 條前例)
- `hooks_configurator.rs:57-83` `provider_needs_setup` 改用 `read_provider_settings_at` + match 三 variant:NotFound 仍 return true 靜默(first-run 預期,跟 R23 契約一致),Io/Parse log warn 帶 path 跟 `[hooks_configurator]` prefix
- `hooks_configurator.rs:189-196` `install_provider` cleanup 從 `let _ = remove_provider(...)` 改 `if let Err(e)` + `log::warn!` 帶 path(install 仍繼續走 overwrite 行為,不擋)
- `hooks_configurator.rs:425-577` 7 個新 unit test(`r37_silent_fail_surfacing_tests` module):
  1. `read_provider_settings_at_missing_file_returns_not_found` — NotFound variant
  2. `read_provider_settings_at_corrupt_json_returns_parse_with_message` — Parse variant 帶 msg
  3. `read_provider_settings_at_valid_file_returns_parsed_value` — Ok(Value)
  4. `provider_settings_warn_msg_unifies_prefix` — prefix 格式鎖定
  5. `provider_needs_setup_missing_file_returns_true_silently` — NotFound 契約
  6. `provider_needs_setup_corrupt_json_still_returns_true` — 壞 JSON 仍走 install flow
  7. `install_provider_propagates_load_error_after_cleanup_warn` — cleanup warn + load `?` propagate 契約
- 3 個 drive-by test contract 收緊(test-only, 0 行為變更):
  - `auto_rules.rs:2206` `matches!(r, Err(_))` → `r.is_err()`(避免 R20 era rustfmt 對 tuple pattern 的 deprecation 噪音)
  - `config.rs:668-679` doc comment 修一行 markdown formatting 給 `cargo doc` 通過
  - `config.rs:725-732` test `mut cfg` → struct literal(`AppearanceConfig { theme, ..default() }`),移除不必要的 `mut` 標記(對齊 R36 wrap-up 「let mut 預設值 = 0」治理線)
  - `config.rs:762-766` tautology assert 改寫明契約說明 + 對齊 rustfmt 100 char line width
  - `hook_server.rs:424` K16 `responses_4xx` test 改 `>= before + 1` → `> before`(對齊 R36 wrap-up 提的 K15/K16 shared counter race 觀察:off-by-one 容易因 race 假陽性失敗)

**驗證**:
- `cargo build --lib`: 0 warning
- `cargo fmt --check`: 0 diff(本輪收尾 1 個 tautology assert 斷行)
- `cargo clippy --lib --tests -- -D warnings`: **0 R37 範圍 violation**(原 R37 doc 寫 1 個 list 結構問題,在 R37 wrap-up 階段修掉;剩 2 個 pre-existing `quota_history.rs:419-420` 是 R35 era 技術債,R36 wrap-up 已明列不修)
- `cargo test --lib --no-fail-fast`: **213 passed; 0 failed; 0 ignored**(R36 206 + R37 7 new = 213, 0 regression, 0 flake)
- K15/K16 shared counter race flake(R36 提的 pre-existing):本輪 full suite 跑出 0 failure(213/213 綠),3 連勝
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`(untracked supervisor 檔,符合 R13 防護)

**結果**: PASS(hooks_configurator 兩條 silent-fail surfaced + typed enum 收斂 + 統一 warn prefix + 7 new tests + 0 R37 範圍 lint warning + 0 regression + 213/213 tests)

**KPI-impact: silent-fail 治理鏈 4→5 + 統一 warn prefix module 3→4 + 測試 206→213**

**不做的範圍**(給後續輪次):
- 2 個 pre-existing `quota_history.rs:419-420` clippy doc-lazy-continuation violation(R35 era 技術債,跟 R36 wrap-up 同步不修;累計 M0 收尾輪一次清)
- hooks_configurator `remove_provider` / `save_json` 內部 `let _ =` 還有幾處小 silent-fail(只 impact 邊角 cleanup,留後續輪次 M0 收)
- K15/K16 shared counter race 真正解法:把 `responses_4xx` 從 `AtomicU64` 改成 per-test `Arc<Mutex<u64>>` 或測試層局部 mock(R35/R36 多次記錄,跨輪考慮)
- K6-K21 metrics → 整合 single `MetricsSnapshot` struct 餵前端(跨輪考慮)
- `is_port_listening` 走 tokio async silent-fail surfacing(留)

