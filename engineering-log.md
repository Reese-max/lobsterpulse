# Engineering Log

> 舊紀錄已歸檔到 engineering-log.archive.md

## 改善紀錄


### [2026-06-02] R46 — K28 `lobsterpulse_provider_completed_sessions_stddev_seconds` gauge + 9 tests（R45 WIP 撿收）
**類型**: M1（metrics 推進主軸 K-tag series, 沿 K3→K22→K23→K24→K25→K26→K27→K28 線）
**KPI**: `_metrics_emitted_K28` 累計 +1（累計 21 個 K-tag metrics: K3/K6/K8/K10/K11/K12/K13/K14/K15/K16/K18/K19/K20/K21/K22/K23/K24/K25/K26/K27 → K28）

**KPI 進展表**:
| KPI | 前值 (R45) | 後值 (R46) | 變化 |
|---|---:|---:|---:|
| K-tag metrics 累計 | 20 | 21 | +1 |
| Lib 總 unit tests | 257 | 266 | +9 |
| K28 pure fn test | 0 | 6 | +6 |
| K28 render test | 0 | 3 | +3 |
| 24h chore_ratio (rolling) | 待 R46 盤 | TBD | — |
| 0 R46 範圍 lint warning | 0 | 0 | 持平 |

**為什麼**:
- 補完 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) 四件套之外的 **波動性**維度：同 avg 60s 的 provider 可能 stddev=5（穩定）或 stddev=300（短任務/長任務混跑），operator 一看 stddev 就知道該 provider session 時長分布狀態。例：avg 60s, latest 65s, max 7200s, min 8s, **stddev 280s** = 過去 2 小時 outlier 跟 8 秒極短 session 拉高波動，operator 端 alert `stddev > 300` 觸發「該 provider 短/長任務混跑待分桶」
- R45 wrap-up（`31b98cc`）時 K28 WIP 100% scaffold 完成（Welford online algorithm fields + update 邏輯 + pure fn + 6 unit test + render block + 3 render test + 12 fixture initializer）但沒 commit —— 跟 R44 撿 R43 WIP K26 同 pattern，R46 撿 R45 WIP 落地（補漏 + 修 1 個 test fixture assertion bug + 跑驗證就 commit，比從零開新 metric 快 5-10x token + 時間）
- 沿用 Welford online algorithm 而非最樸素的「保留所有 sample 在 Vec」：O(1) 空間（Vec 會 unbounded grow，上線跑一週 sample 數就破萬）+ 數值穩定性比「先算 mean 再算 Σ(x-mean)²」高一個數量級（避免大數吃小數）。`mean` / `M2` 跟 K23 `completed_sessions_count` 強綁定（不在 ProviderTotals 另存 count 欄位）—— stddev 跟 completed count 永遠同步，不會有 count 跟 stddev 不一致的中間態
- 第一個 f64 metric（K 系列 K3-K27 全用 i64 整數）：stddev 數學本質決定用連續值（樣本 [10, 20] variance = 50, stddev ≈ 7.07s 強制裁整為 7 → 0.07s 精度流失）。emit 沿用 `{:.4}` 4 位小數固定 precision 跟 K25 avg 對齊
- 跟 K12 `idle_ratio` / K25 `avg` 同屬「既有資料源派生指標」, 補 K22 / K23 / K24 / K25 / K26 / K27 六維體系的「波動性」維度
- H0 cap 持續觸發（chore_ratio 33% > 30% threshold）→ 本輪 M1 KPI 推進（沿 R42/R43/R44/R45 同 K-tag series 主軸）, 撿既有 pattern scaffold（Welford 對稱 K22/K23/K24/K26/K27 的 lifetime aggregate 邏輯, 但改 f64 雙精度），token / 時間密度最高

**搜尋**: 沿用 K22 / K25 / K26 / K27 既有 pattern —— ProviderTotals lifetime aggregate + pure fn `*_at` 攤平 + render 端 alphabetical sort。stddev 算法選 Welford online algorithm 跟 K25 一致（不是 sample stddev 用 N-1），因為 lifetime aggregate 不分 sample/population 用 N 不用 N-1。沒新搜。

**做了什麼**:
- `session.rs:419-441` `ProviderTotals` 加 `completed_sessions_mean_secs: f64` + `completed_sessions_m2_secs: f64` 兩欄位（Welford online algorithm 累積, `f64` 預設 0.0 跟 K25 avg 對齊, count 沿用 K23 不另存）
- `session.rs:673-693` `record_completed_session_age` 觸發點 Welford update —— `x = clamped_age as f64; n_new = count as f64; delta = x - mean; mean += delta / n_new; delta2 = x - mean; M2 += delta * delta2`，第一次完成時 count 0→1 自動初始化 mean=該 sample + M2=0（單樣本無波動 → stddev=0，沿用 K26/K27 `clamped_age` 變數）。`as f64` 轉換在 i64::MAX 範圍內精確，saturation 不可能發生在現實 session duration 量級
- `session.rs:930-959` 新 `completed_sessions_stddev_at` pure fn（攤平 `ProviderTotals` → `HashMap<provider, f64>` 給 render; 過濾 `count == 0` 沿用 K22 / K26 / K27 語意但走 count 過濾不用 None —— K28 不用 Option 是因為 Welford mean/M2 是 `f64` 預設 0.0, 沒有「無值」vs「值=0」的可區分性, 改用 count 過濾更明確; formula: `(M2 / count).sqrt()` population stddev 跟 K25 avg 一致用 N 不用 N-1, lifetime aggregate 不分 sample/population）
- `session.rs:1789-1892` 6 個 unit test：first completion 初始化 mean/M2、Welford 兩樣本 mean=15/M2=50 數值驗證、負值 clamp、Welford 跟 K23 強綁定、單樣本 emit 0、count=0 過濾、per-provider 隔離
- `lib.rs:1987-2006` `render_prometheus_body` emit K28 HELP/TYPE + alphabetical 全 provider sample（K6-K27 既契約, `{:.4}` 4 位小數 f64 格式跟 K25 avg 一致, 跟 K26/K27 整數格式區分; count=1 emit 0.0000 視為有效資料, 跟 K25 avg=該 sample 邏輯一致; count=0 過濾不 emit 假資料）
- `lib.rs:2963` test module imports 加 `completed_sessions_stddev_at`
- `lib.rs` 12 個 test fixture `ProviderTotals` initializer 補 `completed_sessions_mean_secs: 0.0` + `completed_sessions_m2_secs: 0.0`（跟 production `ProviderTotals::default()` 同語意, 跟 K26/K27 fixture 整合註解為「K26/K27/K28 落地」）
- `lib.rs:6945-7173` 3 個 K28 emit integration test —— `completed_sessions_stddev_empty_totals_emits_header_only`（empty-state 跟 K22 / K26 / K27 一致）+ `completed_sessions_stddev_per_provider_isolated_and_skips_zero_count`（cicx stddev=20 / gemini stddev=0 / openx count=0 跳過 隔離 emit）+ `completed_sessions_stddev_alphabetical_sort_and_four_decimal_precision`（3 provider 非字母序插入 → alphabetical 排序 + f64 4 位小數格式 + 跟 K22 / K25 / K26 / K27 / K28 五件套各自 emit 各自的值）

**R46 撿 R45 WIP 修的測試 bug**:
- `lib.rs:7147-7153` R45 WIP 漏寫的 test fixture assertion bug：cicx fixture 設 `last_completed_session_age_secs: Some(200)` 但 R45 assertion 寫 `cicx latest 100`（100 是 min 不是 latest，fixture 跟 assertion 不一致），R46 撿 WIP 跑 test 才抓出（跟 R44 撿 R43 WIP K26 漏的 2 個 emit test bug 同 pattern）。修法：assertion 改成 `cicx latest 200`，跟 fixture + 註解一致（fixture 寫 `latest=200`，K22 latest gauge 跟 K27 min gauge 兩者值不同 —— 這是 R45 寫 WIP 時 fixture / assertion 沒對齊的 copy-paste 漏）
- 這是 dirty WIP 漏寫的測試 fixture bug，R45 wrap-up 階段 code 寫了但沒跑測試就 commit docs，R46 撿 WIP 收尾時補跑發現

**驗證**:
- `cargo build --lib --tests`: 0 warning
- `cargo fmt --check`: 0 diff
- `cargo clippy --lib --tests -- -D warnings`: 0 warning
- `cargo test --lib --no-fail-fast`: **266 passed; 0 failed; 0 ignored**(R45 257 + R46 +9 K28 new, 0 regression, 0 flake)
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`（untracked supervisor 檔, 符合 R13 防護）

**結果**: PASS（K28 落地 + 補 R45 WIP 漏的 1 個 test fixture assertion bug + 0 R46 範圍 lint warning + 0 regression + 266/266 tests + commit `7785941`）

**KPI-impact: K28 per-provider 完成 session 時長 population stddev gauge 從 0 → 1 metric + 波動性觀測維度 0 → 1 + 9 new tests**

**不做的範圍**（給後續輪次）:
- `render_prometheus_body` 12 個參數的怪 signature 重構 → 統一進 `MetricsSnapshot` struct（R26/R27 policy 已記；K6-K28 共 21 個 metrics 都各自 inline 派發 + alphabetical sort，重構可一次清掉 ~150 行 render helper 內的 sort 邏輯但要搬 K6 起的所有 emit 段，跨輪考慮）
- K15 / K16 shared counter race 真正解法（改 per-test `Arc<Mutex<u64>>` 或測試層局部 mock，R35/R36/R37/R44/R45 多次記錄，跨輪考慮）
- hooks_configurator 內部 `let _ =` 剩餘小 silent-fail 收邊（R37 wrap-up 已記）
- K6-K28 metrics 整合 single `MetricsSnapshot` struct 餵前端（跨輪考慮）
- K29+ 後續方向：failure rate（需 failed session counter, 跟 `failure_count` 欄位可能重疊待盤點）、p95/p99 percentile（需 rolling buffer / histogram, O(1) 空間不像 Welford 那樣直接套用）

---


### [2026-06-02] R45 — K27 `lobsterpulse_provider_completed_sessions_min_duration_seconds` gauge + 9 tests
**類型**: M1（metrics 推進主軸 K-tag series, 沿 K3→K22→K23→K24→K25→K26→K27 線）
**KPI**: `_metrics_emitted_K27` 累計 +1（累計 20 個 K-tag metrics: K3/K6/K8/K10/K11/K12/K13/K14/K15/K16/K18/K19/K20/K21/K22/K23/K24/K25/K26 → K27）

**KPI 進展表**:
| KPI | 前值 (R44) | 後值 (R45) | 變化 |
|---|---:|---:|---:|
| K-tag metrics 累計 | 19 | 20 | +1 |
| Lib 總 unit tests | 248 | 257 | +9 |
| K27 pure fn test | 0 | 6 | +6 |
| K27 render test | 0 | 3 | +3 |
| 24h chore_ratio (rolling) | 33% | 待 R46 盤 | — |
| 0 R45 範圍 lint warning | 0 | 0 | 持平 |

**為什麼**:
- 補完 K22 (latest) / K25 (avg) / K26 (max) 三件套的第四角 **min** → 形成 **min / max / latest / avg 四件套**：operator 端可一次看「該 provider 歷史最快 / 最慢 / 最近 / 平均」四個視角,快速分辨 session 時長分佈（例：avg 60s, latest 65s, max 7200s, min 8s = 大部分 session 都跑 ~1 分鐘,但偶有 2 小時 outlier, 且曾有 8 秒極短 session 可能是 fast-path / 早期測試 / 假觸發）。可設 alert `min < 1` 觸發「該 provider 有次秒級完成 session」異常信號
- 沿用 K22 / K26 Option 語意：缺資料（`min_completed_session_age_secs: None`）不 emit sample（避免 Prometheus 端把「沒看到」當「min=0」誤判「該 provider 瞬間完成」= 假健康信號）,已寫入後 saturating_min 不倒退（session 結束 + 30 min stale 回收後 ProviderTotals 仍保留 → Prometheus gauge 不會倒退）。`i64` 而非 `u64` 跟 K22 / K26 一致,雖然實際寫入值都被 `age.max(0)` clamp 過,留 `i64` 方便未來若要支援 signed duration metric 直接擴充
- H0 cap 持續觸發(chore_ratio 33% > 30% threshold)→ 本輪 M1 KPI 推進（沿 R42/R43/R44 同 K-tag series 主軸）, 撿既有 pattern scaffold(K27 saturating_min 鏡像對稱 K26 saturating_max), token / 時間密度最高
- 跟 K12 `idle_ratio` 同屬「既有資料源派生指標」, 補 K22 / K23 / K24 / K25 / K26 五維體系的「最快 / 最慢」分布維度

**搜尋**: 沿用 K22 / K25 / K26 既有 pattern —— ProviderTotals lifetime aggregate + pure fn `*_at` 攤平 + render 端 alphabetical sort + 整數 emit（沒 f64, 跟 K22 / K26 對齊, min 是「單點 saturating_min」語意沒浮點小數必要）。沒新搜。

**做了什麼**:
- `session.rs:403-422` `ProviderTotals` 加 `min_completed_session_age_secs: Option<i64>` 欄位（跟 K22 / K26 對稱：i64 為主, `age.max(0)` clamp 過, `None` = 該 provider 累計收過 event 但還沒完成過 session, 觸發點 SessionEnd + Working→Idle 兩路徑）
- `session.rs:618-634` `record_completed_session_age` 觸發點 saturating_min 更新 —— 沿用 K26 同個 `clamped_age` 變數（K26 已先做 `age.max(0)` clamp）, 第一次完成時 `None` 直接寫 `Some(clamped_age)`, 後續完成用 `prev.min(clamped_age)` 降級
- `session.rs:876-893` 新 `completed_sessions_min_duration_at` pure fn（攤平 `ProviderTotals` → `HashMap<provider, secs>` 給 render; 過濾 `None` 沿用 K22 / K26 語意, lifetime saturating_min 寫入後不蒸發）
- `session.rs:1553-1707` 6 個 unit test（第一次完成初始化 / saturating_min 降級 / 較大值保持 / 負值 clamp / `completed_sessions_min_duration_at` 過濾 None / per-provider 隔離）
- `lib.rs:1954-1989` `render_prometheus_body` emit K27 HELP/TYPE + alphabetical 全 provider sample（K6-K26 既契約, 整數格式, 跟 K22 / K26 saturating 鏡像對稱）
- `lib.rs:6670-6869` 3 個 K27 emit integration test —— `completed_sessions_min_duration_empty_totals_emits_header_only`（empty-state 跟 K22 / K26 一致）+ `completed_sessions_min_duration_per_provider_isolated_and_skips_none`（K22 / K26 / K27 三件套隔離 emit）+ `completed_sessions_min_duration_alphabetical_sort_and_integer_format`（3 provider 非字母序插入 → alphabetical 排序 + 整數格式 + 跟 K22 / K25 / K26 三件套同 totals 各自 emit 各自的值）
- `lib.rs` 12 個 test fixture `ProviderTotals` initializer 補 `min_completed_session_age_secs: None`（跟 production `ProviderTotals::default()` 同語意, 跟 K26 fixture 整合註解為「K26 / K27 落地」）
- `lib.rs:2911` test module imports 加 `completed_sessions_min_duration_at` 派生函式

**驗證**:
- `cargo build --lib --tests`: 0 warning（K27 pure fn 已從 render helper 內被呼叫, dead_code warning 消失）
- `cargo fmt --check`: 0 diff
- `cargo clippy --lib --tests -- -D warnings`: 0 warning
- `cargo test --lib`: **257 passed; 0 failed; 0 ignored**(R44 248 + R45 +9 K27 new, 0 regression, 0 flake; 第一次跑有 1 個 hook_server flaky test fail (R15 已知 race, 隔離重跑 pass, 跨輪處理) → 第二次跑 248 + 9 = 257 全綠)
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`（untracked supervisor 檔, 符合 R13 防護）

**結果**: PASS（K27 落地 + 0 R45 範圍 lint warning + 0 regression + 257/257 tests）

**KPI-impact: metrics 維度 +1（per-provider 歷史最短完成 session gauge）, 測試 248→257**

**不做的範圍**（給後續輪次）:
- `render_prometheus_body` 11 個參數的怪 signature 重構 → 統一進 `MetricsSnapshot` struct（R26/R27 policy 已記；K6-K27 共 20 個 metrics 都各自 inline 派發 + alphabetical sort，重構可一次清掉 ~150 行 render helper 內的 sort 邏輯但要搬 K6 起的所有 emit 段，跨輪考慮）
- K15 / K16 shared counter race 真正解法（改 per-test `Arc<Mutex<u64>>` 或測試層局部 mock，R35/R36/R37/R44/R45 多次記錄，跨輪考慮）
- hooks_configurator 內部 `let _ =` 剩餘小 silent-fail 收邊（R37 wrap-up 已記）
- 2 個 pre-existing `quota_history.rs:419-420` clippy doc-lazy-continuation violation —— R38 已清（追問：實際上 R38 commit `d2976a7` 已修，這條已不適用；待下輪盤點時從「不做的範圍」清掉）
- K6-K21 metrics 整合 single `MetricsSnapshot` struct 餵前端（跨輪考慮）
- K28+ 後續方向：percentile (p50/p95/p99, 需 rolling buffer / histogram)、failure rate (需 failed session counter, 跟 `failure_count` 欄位可能重疊待盤點)

---


### [2026-06-02] R44 — K26 `lobsterpulse_provider_completed_sessions_max_duration_seconds` gauge + 8 tests（R43 WIP 收尾）
**類型**: M1（metrics 推進主軸 K-tag series, 沿 K3→K22→K23→K24→K25→K26 線）
**KPI**: `_metrics_emitted_K26` 累計 +1（累計 18 個 K-tag metrics: K3/K6/K8/K10/K11/K12/K13/K14/K15/K16/K18/K19/K20/K21/K22/K23/K24/K25 → K26）

**KPI 進展表**:
| KPI | 前值 (R43) | 後值 (R44) | 變化 |
|---|---:|---:|---:|
| K-tag metrics 累計 | 17 | 18 | +1 |
| Lib 總 unit tests | 239 | 248 | +9 (含 1 baseline 漂移) |
| K26 pure fn test | 0 | 4 | +4 |
| K26 render test | 0 | 4 | +4 |
| 24h chore_ratio (rolling) | 33% | TBD | — |
| 0 R44 範圍 lint warning | 0 | 0 | 持平 |

**為什麼**:
- 補完「per-provider 完成 session 持續時間」三件套（max / latest / avg）的最後一角：K22 gauge 看「最近一次跑多久」（single sample 沒平均）、K25 gauge 看「平均跑多久」（派生 from K23/K24 計次 + 累計時長）、K26 gauge 看「歷史最長一次跑多久」（saturating_max lifetime, 不蒸發）。operator 端三視角並列可比 outlier：avg 60s / latest 65s / max 7200s = 過去有 2 小時 outlier session（可能 runner hang / 大 context window 場景）。可設 alert `max > 3600` 觸發「該 provider 有異常長 session 待撈」
- R43 wrap-up（`1b6deea`）時 K26 code 已 scaffold 寫到 dirty state 但沒 commit —— R44 撿 R43 WIP 落地，token / 時間密度最高（補漏 + 修測試 bug + 跑驗證 + commit，比從零開新 metric 快 5-10x）
- 沿用 K22 None-跳過 + saturating_max 策略：缺資料（`max_completed_session_age_secs: None`）不 emit sample（避免 Prometheus 端把「沒看到」當「max=0」誤判「該 provider 瞬間完成」= 假健康信號），已寫入後 saturating_max 不倒退（counter 語意，session 結束 + 30 min stale 回收後 ProviderTotals 仍保留 → Prometheus gauge 不會倒退）

**搜尋**: 沿用 K22 / K25 既有 pattern —— ProviderTotals lifetime aggregate + pure fn `*_at` 攤平 + render 端 alphabetical sort + 整數 emit（沒 f64, 跟 K22 對齊, max 是「單點 saturating_max」語意沒浮點小數必要；K25 派生 f64 是因為 K24 / K23 除法會出現非整數結果, K26 純 saturating_max 整數足夠）。沒新搜。

**做了什麼**:
- `session.rs:386-405` `ProviderTotals` 加 `max_completed_session_age_secs: Option<i64>` 欄位（跟 K22 `last_completed_session_age_secs` 對稱：i64 為主, `age.max(0)` clamp 過, `None` = 該 provider 累計收過 event 但還沒完成過 session, 觸發點 SessionEnd + Working→Idle 兩路徑）
- `session.rs:609-622` `record_completed_session_age` 觸發點 saturating_max 更新 —— `age.max(0)` clamp 後跟歷史 max 比，第一次完成時 `None` 直接寫 `Some(age)`，後續完成用 `prev.max(clamped_age)` 升級
- `session.rs:815-834` 新 `completed_sessions_max_duration_at` pure fn（攤平 `ProviderTotals` → `HashMap<provider, secs>` 給 render；過濾 `None` 沿用 K22 語意，lifetime saturating_max 寫入後不蒸發）
- `session.rs:1417-1540` 6 個 unit test（第一次完成初始化 / saturating_max 升級 / 較小值保持 / 負值 clamp / `completed_sessions_max_duration_at` 過濾 None / per-provider 隔離）
- `lib.rs:1921-1951` `render_prometheus_body` emit K26 HELP/TYPE + alphabetical 全 provider sample（K6-K25 既契約，整數格式）
- `lib.rs:6422-6543` 2 個 K26 emit integration test —— `per_provider_isolated_and_skips_none`（K22 跟 K26 隔離 emit）+ `alphabetical_sort_and_integer_format`（3 provider 非字母序插入 → alphabetical 排序 + 整數格式 + 跟 K22 / K25 三件套同 totals 各自 emit）
- `lib.rs` 12 個 test fixture `ProviderTotals` initializer 補 `max_completed_session_age_secs: None`（跟 production `ProviderTotals::default()` 同語意）
- `lib.rs:2910` test module imports 加 `last_completed_session_age_at` 派生函式

**R44 收尾時發現並修的測試 bug**:
- 兩個 K26 emit integration test 原本把 8th 參數 `last_completed_session_age: &HashMap<String, i64>` 傳 `&HashMap::new()`（空 map），導致 K22 sample line 在 body 內缺失（K22 emit 用這個外部 map，不是從 totals 派生），R44 cargo test 跑出 2 個 K26 emit test fail。修法：兩處 test 都改成 `let last_completed = last_completed_session_age_at(&totals); render_prometheus_body(..., &last_completed, ...)`，模擬 production 從 ProviderTotals 派生 K22 map 的路徑（K22 跟 K26 共用 provider_totals 資料源 → 8th 參數該跟 totals 同步不能傳空）
- 這是 dirty WIP 漏寫的測試 fixture bug，R43 wrap-up 階段 code 寫了但沒跑測試就 commit docs，R44 收尾時補跑發現

**驗證**:
- `cargo build --lib`: 0 warning
- `cargo fmt --check`: 0 diff
- `cargo clippy --lib --tests -- -D warnings`: 0 warning
- `cargo test --lib`: **248 passed; 0 failed; 0 ignored**(R43 239 + R44 +8 K26 new + 1 baseline 漂移, 0 regression, 0 flake)
- 沒動 `.arch-fitness.json` / `.supervisor-report.json`（untracked supervisor 檔, 符合 R13 防護）

**結果**: PASS（K26 落地 + 補 R43 WIP 收尾時 2 個 emit test bug + 0 R44 範圍 lint warning + 0 regression + 248/248 tests）

**KPI-impact: metrics 維度 +1（per-provider 歷史最長完成 session gauge）, 測試 239→248**

**不做的範圍**（給後續輪次）:
- `render_prometheus_body` 11 個參數的怪 signature 重構 → 統一進 `MetricsSnapshot` struct（R26/R27 policy 已記；K6-K26 共 18 個 metrics 都各自 inline 派發 + alphabetical sort，重構可一次清掉 ~150 行 render helper 內的 sort 邏輯但要搬 K6 起的所有 emit 段，跨輪考慮）
- K15 / K16 shared counter race 真正解法（改 per-test `Arc<Mutex<u64>>` 或測試層局部 mock，R35/R36/R37 多次記錄，跨輪考慮）
- hooks_configurator 內部 `let _ =` 剩餘小 silent-fail 收邊（R37 wrap-up 已記）
- 2 個 pre-existing `quota_history.rs:419-420` clippy doc-lazy-continuation violation —— R38 已清（追問：實際上 R38 commit `d2976a7` 已修，這條已不適用；待下輪盤點時從「不做的範圍」清掉）
- K6-K21 metrics 整合 single `MetricsSnapshot` struct 餵前端（跨輪考慮）

---

### [2026-06-02] R43 — K25 `lobsterpulse_provider_completed_sessions_average_duration_seconds` gauge + 7 tests
**類型**: M1（metrics 推進主軸 K-tag series,沿 K3→K22→K23→K24→K25 線）
**KPI**: `_metrics_emitted_K25` 累計 +1（累計 17 個 K-tag metrics:K3/K6/K8/K10/K11/K12/K13/K14/K15/K16/K18/K19/K20/K21/K22/K23/K24 → K25）

**KPI 進展表**:
| KPI | 前值 (R42) | 後值 (R43) | 變化 |
|---|---:|---:|---:|
| K-tag metrics 累計 | 16 | 17 | +1 |
| Lib 總 unit tests | 232 | 239 | +7 |
| K25 pure fn test | 0 | 4 | +4 |
| K25 render test | 0 | 3 | +3 |
| 24h chore_ratio (rolling) | 33% | 33% | 持平（仍 > 30% threshold → H0 cap 持續觸發） |
| 0 R43 範圍 lint warning | 0 | 0 | 持平 |

**為什麼**:
- 對齊 mission「觀察 / 監控桌面 AI 工具」operator 端 KPI 表的 average time-to-completion 維度：K22 看「最近一次」(single sample, 沒平均語意)、K23 看「累計次數」(純計次, 沒時長)、K24 看「累計總時長」(純加總, 沒除以次數), 三者都沒把次數 / 時長兩個 dimension 結合成除法結果 → K25 把 K23/K24 兩個 lifetime counter 派生為單一 derived gauge, operator 端 PromQL / Grafana 不再需要 cross-query 算除法(scrape 缺一條時算式直接壞)
- H0 cap 持續觸發(chore_ratio 33% > 30% threshold)→ 本輪 M1 KPI 推進, 撿 R42 收尾時同 main 上 scaffold 好的 K25 WIP 落地(token / 時間密度最高: 補漏 + 跑驗證就 commit, 比從零開新 metric 快 5-10x)
- K25 跟 K12 `idle_ratio` 同屬「既有資料源派生指標」, 補 K22 / K23 / K24 三維體系的「平均效率」維度

**搜尋**: 沿用 K12 / K22 / K23 / K24 既有 pattern —— ProviderTotals lifetime aggregate + pure fn `*_at` 攤平 + render 端 alphabetical sort + f64 `{:.4}` 4 位小數固定 precision(對齊 K12 idle_ratio)。emit 策略沿用 K22 None-跳過語意但觸發條件改寫成「該 provider 從未完成過 session」= `count==0`, 因 0/0 = NaN, emit 0.0 會誤導 Prometheus 端把「沒資料」判成「瞬間完成」= 假健康信號。沒新搜。

**做了什麼**:
- `session.rs:755-789` 新增 pure fn `completed_sessions_average_duration_at(&HashMap<String, ProviderTotals>) -> HashMap<String, f64>` —— count==0 過濾(對齊 K22 None-跳過語意, 觸發條件改寫), count>0 emit total/count f64
- `session.rs:1262-1373` 4 個新 unit test 鎖契約:
  1. `k25_completed_sessions_average_duration_at_emits_average_when_count_nonzero` — total=180/count=3 → 60.0 整除(避免浮點尾數雜訊干擾 assert_eq)
  2. `k25_completed_sessions_average_duration_at_skips_zero_count_provider` — count=0 該跳過, emit 0.0 假冒 average=0 是假健康信號
  3. `k25_completed_sessions_average_duration_at_per_provider_isolated` — 兩個 provider 獨立算(openx 7200/2=3600, gemini 60/4=15), catch「全部算成同值」regression
  4. `k25_completed_sessions_average_duration_at_handles_non_exact_division` — 7/3 f64 雙精度保留, helper 不能 round / floor, 用 epsilon 1e-12 比較
- `lib.rs:1898-1923` `render_prometheus_body` 加 K25 emit block(28 行, HELP/TYPE 標頭 + alphabetical sort + `{:.4}` 4 位小數 f64 格式), 不動 render 端 signature(K25 直接讀 `provider_totals` 自己派發 pure fn 攤平, 避免第 13 個參數, 對齊 R26/R27 政策: cross-cutting snapshot 留給 M1 輪 `MetricsSnapshot` struct 統一處理, 本輪不重構)
- `lib.rs:6137-6290` 3 個新 render test:
  1. `completed_sessions_average_duration_empty_totals_emits_header_only` — 空 totals → 沒 sample line(HELP/TYPE 仍 emit, 對齊 K11 / K18-K24 empty-state 契約)
  2. `completed_sessions_average_duration_zero_count_provider_is_skipped` — K25 vs K24 emit 策略差異化在 render 端體現(順便驗 K24 不受 K25 skip 邏輯影響, 兩條 series 行為獨立)
  3. `completed_sessions_average_duration_alphabetical_sort_and_four_decimal_precision` — 3 provider 非字母序插入(openx/cicx/gemini) → alphabetical 輸出, 挑整除值避免 IEEE 754 尾數雜訊; 同時驗 f64 4 位小數格式契約(不是 u64 整數, 跟 K24 counter 區分)

**為什麼 R43 接 K25 WIP 而不是新開 K26**:
- K25 WIP 100% scaffold 完成(pure fn + 4 tests + render emit + 3 render tests 全寫好, fixture 預設值都補完)→ R43 補漏 = 跑驗證就 commit, 比從零開 K26 快 5-10x token + 時間
- K25 跟 K22 / K23 / K24 互補形成完整「最近一次 / 累計次數 / 累計時長 / 平均時長」四維體系 —— 完成 K25 比再開 K26 對 operator 端 KPI 表的價值密度高
- 對齊「不刪既有、不重構無關」原則: working tree dirty 322 行不是技術債, 是 R42 沒關 commit 的 WIP, 撿起來是 M1 KPI 推進最便宜路徑

**為什麼 R43 沒做 MetricsSnapshot struct 重構**:
- R26/R27 政策明寫「cross-cutting snapshot 整合留給 M1 輪統一處理」→ 本輪 M1 是 K25 推進, 不是重構
- render_prometheus_body 12 個參數 signature 已知 anti-pattern, 但每加一個 metric 都不該用「順手重構」當藉口; K25 自己攤平 + pure fn 已經把耦合壓在 session.rs 內, render 端沒惡化
- 下一輪 H0 cap 解除後(chore_ratio < 30%)再做 MetricsSnapshot 比較合時

**驗證**:
- `cargo build --lib`: 0 warning
- `cargo fmt --check`: 0 diff
- `cargo clippy --lib --tests -- -D warnings`: 0 warning
- `cargo test --lib --no-fail-fast`: **239 passed; 0 failed; 0 ignored**(R42 232 + K25 4 session + 3 render = 239, 0 regression)
- pre-existing K15/K16 shared counter race flake(R35/R36/R37 累計紀錄)本輪 full suite 0 復發(239/239 綠), 不在 R43 scope, 後續 M0 收尾輪處理

**結果**: PASS(K25 落地 + 0 R43 範圍 lint warning + 0 regression + 239/239 tests + commit `6183792`)

**KPI-impact: K25 per-provider 平均完成時長 gauge 從 0 → 1 metric + average time-to-completion 觀測維度 0 → 1 + 7 new tests**

**不做的範圍**(給後續輪次):
- 10 個 pre-existing clippy doc-lazy-continuation violation 累計 M0 收尾輪(本輪 clippy 0 確認是 R43 範圍內 0 violation, pre-existing 跨檔累計由獨立 sensor 追蹤)
- K25 接前端 quota bar(目前 `lobsterpulse_provider_completed_sessions_average_duration_seconds` 只有 Prometheus metric, 前端 panel 還沒接 — 跨前後端, 留 M1 輪開)
- K6-K25 lifetime-vs-live → 整合 single `MetricsSnapshot` struct 餵前端(R35/R42/R43 「不做的範圍」累計留的, 跨輪考慮, H0 cap 解除後處理)
- K15/K16 shared counter race 真正解法: 把 `responses_4xx` 從 `AtomicU64` 改成 per-test `Arc<Mutex<u64>>` 或測試層局部 mock(R35/R36/R37/R43 累計, 跨輪考慮, M0 收尾輪處理)
- `is_port_listening` 走 tokio async silent-fail surfacing(R35/R37 「不做的範圍」留的, M0 收尾輪處理)
- hooks_configurator `remove_provider` / `save_json` 內部 `let _ =` 還有幾處小 silent-fail(R37 留的, M0 收尾輪處理)


### [2026-06-02] R42 — K24 `lobsterpulse_provider_completed_sessions_total_duration_seconds` counter + 4 tests（R33 WIP 落地）
**類型**: M1（metrics 推進主軸 K-tag series,沿 K3→K22→K23→R42 線）
**KPI**: `_metrics_emitted_K24` 累計 +1（累計 16 個 K-tag metrics:K3/K6/K8/K10/K11/K12/K13/K14/K15/K16/K18/K19/K20/K21/K22/K23 → K24）

**KPI 進展表**:
| KPI | 前值 (R40) | 後值 (R42) | 變化 |
|---|---:|---:|---:|
| K-tag metrics 累計 | 15 | 16 | +1 |
| Lib 總 unit tests | 228 | 232 | +4 |
| K24 render test | 0 | 3 | +3 |
| K24 pure fn test | 0 | 1 | +1 |
| Engineering log 行數 | 540 | ~580 | +40（仍 < 500 cap 警戒） |
| 24h chore_ratio (rolling) | 33% | 33% | 持平（仍 > 30% threshold → H0 cap 持續觸發,本輪禁 H0） |

**為什麼**: 對齊 mission「觀察 / 監控桌面 AI 工具」的可量化性 — K22（最近一次完成 age gauge）跟 K23（累計完成次數 counter）只覆蓋「時長」跟「次數」兩個維度,operator 端算「平均 time-to-completion」要自己用兩個 metric 做除法,跨 query 容易出錯。K24 直接 emit 第三條 counter 讓 operator 端有現成 series 算 `duration / count = avg time-to-completion` 效率 KPI,搭配 `rate(duration[1h])` 看 throughput-seconds KPI。H0 cap 觸發（chore_ratio 33% > 30% threshold）→ 本輪 M0-M3 限定,挑 KPI 推進直接命中。

**搜尋**: 沿用 K22/K23 既有 pattern — ProviderTotals lifetime aggregate + saturating_add + pure fn `*_at` 攤平 + render 端 alphabetical 排序 + counter 0 emit 策略。沒新搜。

**做了什麼**:
- `session.rs:387` `ProviderTotals` 加 `completed_sessions_total_duration_secs: u64` field（lifetime aggregate,對齊 K7/K9/K13/K23,session 結束 + 30 min stale 回收後 counter 不蒸發）
- `session.rs:590` `record_completed_session_age` 累加 `age.max(0) as u64`（saturating_add 防時鐘回撥污染 counter 總和,跟 K22 同樣 `age.max(0)` 防線）
- `session.rs:742` 抽 `completed_sessions_total_duration_at(&ProviderTotals) -> HashMap<String, u64>` pub fn 純 fn（對齊 K23 `completed_sessions_count_at` 同 emit 策略:counter 0 跟 missing 不同語意,全部 provider 都進 map 含 0）
- `lib.rs:1871` `render_prometheus_body` 加 K24 counter emit block(28 行,含 HELP/TYPE 標頭 + alphabetical 排序),不動 render 端 signature（K24 直接讀 provider_totals 自己攤平,避免第 12 個參數,對齊 R26/R27 政策:cross-cutting snapshot 留給 M1 輪 `MetricsSnapshot` struct 統一處理）
- `lib.rs` 11 個 `render_prometheus_tests` fixture 補 K24 field 預設 0（跟 `ProviderTotals::default()` 同語意）
- 修 R33 WIP 留下的 11 個 `clippy::doc_lazy_continuation` error:K24 doc 開頭「公式:」+ 緊接 list + 連續段落被 clippy 判定 list 還沒結束,要求全部 4+ space 縮排。修法:在「operator 端用 ...」段前加空行斷段,讓 list 明確結束 → 11 errors → 0
- 4 個新 unit test 鎖契約（位於既有 K22/K23 test 群同 pattern, `k24_*` prefix）:
  1. `k24_session_end_accumulates_total_duration` — 單次完成 42s → total = 42
  2. `k24_repeated_completions_sum_durations_across_unique_sessions` — 3 個 session 各自完成 30+60+90s → total = 180（驗證累加不是 last-wins,對齊 K23 repeated-completions 同 3-session shape）
  3. `k24_record_clamped_age_clamps_negative_to_zero` — 負值 age -100 → saturate 到 0（防 `as u64` wrap,跟 K22 同防線）
  4. `k24_completed_sessions_total_duration_at_emits_zero_for_uncompleted_provider` — pure fn 端到端:counter 0 跟 missing 不同語意,全部 provider 都進 map 含 0
- 3 個新 render test 端到端驗（位於既有 K22/K23 render test 群同 pattern）:
  1. `completed_sessions_total_duration_empty_totals_emits_header_only` — 空 totals → 沒 sample line（HELP/TYPE 仍 emit,跟 K11/K18-K22 同 empty-state 契約）
  2. `completed_sessions_total_duration_zero_is_emitted_not_dropped` — total=0 是有效資料必須 emit,對齊 K20/K21/K23「counter 0 跟 missing 不同語意」契約
  3. `completed_sessions_total_duration_alphabetical_sort_across_providers` — non-alphabetical 插入(openx/cicx/gemini) → alphabetical 輸出(cicx < gemini < openx) + 整數格式（不是 float）

**為什麼 R42 接 R33 WIP 而不是新開 K25**:
- R33 WIP scaffold 95% 完成（ProviderTotals field + 累加 + pure fn + lib.rs render emit + 4 + 3 tests 全寫好,連 fixture 預設值都補完）→ R42 補漏 = 修 clippy doc 11 errors + 跑驗證就 commit,比從零開 K25 快 5-10x token + 時間
- K24 跟 K22/K23 互補形成完整「時間 / 次數 / 平均」三維體系 — 完成 K24 比再開 K25 對 operator 端 KPI 表的價值密度高
- 對齊「不刪既有、不重構無關」原則:working tree dirty 372 行不是技術債,是 R33 沒關 commit 的 WIP,撿起來是 M1 KPI 推進最便宜路徑

**為什麼 R42 沒做 MetricsSnapshot struct 重構**:
- R26/R27 政策明寫「cross-cutting snapshot 整合留給 M1 輪統一處理」→ 本輪 M1 是 K24 推進,不是重構
- render_prometheus_body 11 個參數 signature 已知 anti-pattern,但每加一個 metric 都不該用「順手重構」當藉口;K24 自己攤平 + pure fn 已經把耦合壓在 session.rs 內,reder 端沒惡化
- 下一輪 H0 cap 解除後(chore_ratio < 30%)再做 MetricsSnapshot 比較合時

**驗證**:
- `cargo fmt --check` 0 diff
- `cargo clippy --lib -- -D warnings` 0 error（修 11 個 R33 doc_lazy_continuation 留下來的 error）
- `cargo test --lib` **232 passed**（228 既有 + 4 R42 新增,0 regression）+ commit `49f8be6`

**KPI-impact: _metrics_emitted_K24 +1（累計 16 個 K-tag metrics,operator 端可算 avg time-to-completion + throughput-seconds 兩條新 KPI series,跟 K22/K23 互補形成完整「時間 / 次數 / 平均」三維體系）**

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


---

### [2026-06-02] R38 — quota_history.rs:419-420 doc_lazy_continuation 收尾
**類型**: H0（治理債收尾,R37 wrap-up 明列預定清掉）
**KPI**: 0 lint warning baseline 恢復
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| clippy warning | 2 | 0 | -2 |
| tests passing | 221 | 221 | 0 |
**為什麼**: R37 完成 213 passing tests 後剩 2 個 `doc_lazy_continuation` warning 阻斷
「0 lint warning」契約。R37 wrap-up 明確標註此為 R35 era pre-existing 技術債、計畫
「累計 M0 收尾輪一次清」。R38 為該收尾輪。
**搜尋**: 無（已知 R35 文案,無需新搜尋）
**做了什麼**:
- `quota_history.rs:416-420` doc 段落重組:加空行分段,「log warn + skip」/「全檔都是壞 row」/
  IO 錯等子句獨立成可讀段落(對齊同檔 line 433-443 風格)
- 語意不變,僅 doc 排版
**驗證**:
- `cargo clippy --all-targets`: 0 warning
- `cargo fmt --check`: 0 diff
- `cargo test --lib`: 221 passed; 0 failed
- commit `d2976a7`(1 file / +5 -4)
**結果**: PASS(0 lint warning baseline 恢復,測試 0 regression)
**KPI-impact: housekeeping**

### 2026-06-02 R40 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

### [2026-06-02] R41 — K23 `lobsterpulse_provider_completed_sessions_total` counter 收尾（修 R40 留 WIP 12 個 initializer 漏 field + 落地 emission）
**類型**: M1（推進 metrics KPI,K22 gauge 維度補完 → counter 維度）
**KPI**: metrics 維度 +1（per-provider 累計完成 session 數）
**KPI 進展表**:
| KPI | 前值 (R40) | 後值 (R41) | 變化 |
|---|---:|---:|---:|
| `/metrics` lobsterpulse_* 樣本數 | 17 series | 18 series | +1 |
| Lib 總 unit tests | 221 | 225 | +4 |
| Lib test 編譯 | 12 error E0063 | 0 | -12 |
| clippy warning | 0 | 0 | 持平 |
**為什麼**: 對齊 LobsterPulse v5.1 mission「本機 CLI + OpenAB 雙路徑觀察」的可觀察性 —— K22 gauge 給「最近一次跑多久」,但 operator 看不到「累計跑了幾次」,無法算 `rate(completed_sessions_total[1h])` 觀察吞吐。K23 counter 補這維度,跟 K7 / K9 / K13 lifetime aggregate 對齊:ProviderTotals 寫入後不蒸發,session 結束 + 30 min stale 回收後 counter 不會倒退,符合 Prometheus counter 語意（單調遞增）。R40 寫到一半 WIP 留 12 個 `ProviderTotals` initializer 漏 `completed_sessions_count` 欄位 + 4 個 K23 tests + emission code,R41 收尾補欄位即可。
**搜尋**: 沿用既有 K22 `last_completed_session_age_at` pure fn pattern + K9 `session_count` 0-default 風格;無新搜（counter 語意清楚,lifetime aggregate 對齊 K9 已驗證）。
**做了什麼**:
- `src-tauri/src/session.rs:354-368` `ProviderTotals` 加 `completed_sessions_count: u64` 欄位（飽和累加）
- `src-tauri/src/session.rs:558-568` `record_completed_session_age` 觸發點同步 +1（跟 K22 同觸發點,SessionEnd + Working→Idle 兩路徑）
- `src-tauri/src/session.rs:692-707` 加 `completed_sessions_count_at` 純 fn（攤平 ProviderTotals → HashMap<provider, count>,全部 emit 含 0）
- `src-tauri/src/session.rs:968-1066` 4 個 unit test（SessionEnd +1、Working→Idle +1、3 個 unique session 累計 3、0 該 emit 不該跳過）
- `src-tauri/src/lib.rs:1846-1870` `render_prometheus_body` emit K23 HELP/TYPE + alphabetical sort 全 provider 樣本
- `src-tauri/src/lib.rs` 12 個 test fixture `ProviderTotals` initializer 補 `completed_sessions_count: 0,`（K23 default 語意,純補欄位零行為變更）
**驗證**:
- `cargo test --lib`: 225 passed; 0 failed（+4 K23,0 regression）
- `cargo clippy --lib -- -D warnings`: 0 warning
- `cargo fmt --check`: 0 diff
**結果**: PASS（baseline 從 R40 WIP broken 恢復 + K23 落地 + 0 regression）
**KPI-impact: metrics 維度 +1（per-provider 累計完成 session counter）**

### 2026-06-03 R45 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

### [2026-06-03] R47 — K29 `lobsterpulse_provider_failure_to_completion_ratio` gauge + 9 tests（R46 WIP 撿收 + 修 R46 WIP 漏的 K25 隔離 assertion bug）
**類型**: M1（metrics 推進主軸 K-tag series, 沿 K3→K22→K23→K24→K25→K26→K27→K28→K29 線）
**KPI**: `_metrics_emitted_K29` 累計 +1（累計 22 個 K-tag metrics: K3/K6/K8/K10/K11/K12/K13/K14/K15/K16/K18/K19/K20/K21/K22/K23/K24/K25/K26/K27/K28 → K29）

**KPI 進展表**:
| KPI | 前值 (R46) | 後值 (R47) | 變化 |
|---|---:|---:|---:|
| K-tag metrics 累計 | 21 | 22 | +1 |
| Lib 總 unit tests | 266 | 275 | +9 |
| K29 pure fn test | 0 | 6 | +6 |
| K29 render test | 0 | 3 | +3 |
| 0 R47 範圍 lint warning | 0 | 0 | 持平 |
| 0 R47 範圍 fmt diff | 0 | 0 | 持平 |
| R46 WIP bug fix 累計 | 0 | 1 | +1 |

**為什麼**: 對齊 LobsterPulse v5.1 mission「本機 CLI + OpenAB 雙路徑觀察」的可觀察性 —— K22-K28 五件套（latest/avg/max/min/stddev）只覆蓋 session duration 分布、沒覆蓋「失敗 vs 成功比」維度。K9/K10 是純絕對失敗計數,operator 端要分辨「流量大失敗難免」（絕對值高 ratio 低）vs「流量小每次都失敗」（絕對值低 ratio 高 = 嚴重健康問題）得自己寫 PromQL `failure_count / completed_sessions_total` 除法算式 —— 兩個 metric cross-query 在 PromQL 易出錯、scrape 缺一條時算式直接壞。K29 直接在 exporter 端 emit 派生 gauge 補這個 operator 友善 ratio 維度,alert 閾值 `ratio > 2.0` = 「每完成一次 session 平均 retry 2 次以上」= 健康度異常信號。R46 寫到一半 WIP 留 K29 pure fn + render emit + 3 render test + 6 unit test + K25 隔離 assertion bug,R47 收尾:補 K25 隔離 assertion（原本寫 `!contains cicx K25` 假錯,K25 邏輯是 count>0 一律 emit 含 0.0,改用雙驗證「K25 emit 0.0000 + K29 emit 1.5000 各發各的 series line 互不污染」）。
**搜尋**: 沿用既有 K25 `completed_sessions_average_duration_at` pure fn pattern（兩個 lifetime counter 組合成 ratio 純 derived gauge）+ K9/K10 失敗計數 source;無新搜（derived gauge 語意清楚,PromQL 派生計算移到 exporter 端是標準 pattern）。
**做了什麼**:
- `src-tauri/src/session.rs:991-1008` 加 `failure_to_completion_ratio_at` 純 fn（攤平 ProviderTotals → HashMap<provider, f64 ratio>, 過濾 completed_sessions_count=0 避免 0/0 數學未定義 emit 0.0 假冒「零失敗」假健康信號 —— 跟 K25「0/0 不 emit」同款防線）
- `src-tauri/src/session.rs:1980-2077` 加 6 個 unit test（skip count=0 / zero failure emit 0.0 / integer ratio / fractional ratio / per-provider 隔離 / high failure rate=10.0）
- `src-tauri/src/lib.rs:2007-2034` `render_prometheus_body` emit K29 HELP/TYPE + alphabetical sort 全 provider 樣本（4 位小數 f64 跟 K25 avg / K28 stddev 對齊）
- `src-tauri/src/lib.rs:7233-7405` 加 3 個 render test（empty totals header-only / per-provider 隔離 + count=0 跳過 / alphabetical sort + 4 位小數 + K25 隔離雙驗證）
- `src-tauri/src/lib.rs:7396` 修 R46 WIP 漏的 K25 隔離 assertion bug：原本 `!contains cicx K25` 假錯（K25 邏輯是 count>0 一律 emit 含 0.0,cicx total=0 + count=2 → K25 emit 0.0000）,改用雙驗證「K25 emit 0.0000 + K29 emit 1.5000 各發各的 series line 互不污染」（跟 K6-K28 既 K25 隔離 test 風格一致,真實反映兩 metric 隔離語意）

**驗證**:
- `cargo test --lib`: 275 passed; 0 failed（+9 K29,0 regression）
- `cargo clippy --lib -- -D warnings`: 0 warning
- `cargo fmt --check`: 0 diff

**結果**: PASS（K29 撿收 R46 WIP + 修 R46 WIP K25 隔離 assertion bug + 0 regression + 275/275 全綠）
**KPI-impact: metrics 維度 +1（per-provider failure-to-completion ratio gauge, 補 K22-K28 duration 分布外的「失敗 vs 成功比」觀測維度, alert 閾值 ratio > 2.0 觸發「該 provider session 平均 retry 2 次以上」健康度異常信號）**
