# Engineering Log

> 這是自主工程師的工作日誌。每次改善都會記錄思考過程和結果。

## 改善紀錄

### [2026-06-01] Round 9 — hook_failure_burst 5 條 false-positive 命中改 silent → log::debug surface
**類型**: M0（user-facing observability bug：Rule 3 hook_failure_burst 對 5 條已知誤報 pattern 命中即 silent continue，真實 hook 失敗若撞子字串會被一起吃掉且無 log）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K-hook-failure-observability：false-positive 命中可觀察性 | 0/5 sites (silent) | 5/5 sites (log::debug) | +5 |
| false-positive pattern 命名清楚度 | inline 字串、無名 | 5 個具名 const | ✓ |
| latent bug：後段 pattern 永遠 unreachable | 是（`ripgrep` 永遠先吃） | 否（specific-first 排序） | ✓ |
| Lib unit tests | 30 | 33 | +3 |
| 24h chore_ratio（chore type 無 K-tag） | 58% (R8 觸發紅線) | — | 本輪 fix type，cap 用 0/5 |
| 24h 連續 M0 推進輪數 | 4 (R2/R2二/R3/R4/R6/R8) | 5 | +1 |

**為什麼**:
- R6 修 Discord 14 sites transport-layer silent fail、R8 修 hook_server 2 sites event-entry silent fail — 都是 surface-fail pattern
- R3 留下的「不做的範圍」清單 #1 就是 Rule 3 中文硬碼 filter：當時歸 noise reduction / 非 user-facing 阻斷
- 本輪重評估：filter 行為不變（命中即丟避免誤報）≠ silent。silent 的問題是「為什麼這次失敗沒被算」沒 log 可查 → 對齊 R6/R8，命中時 log::debug 留 pattern name + provider + 80 byte err preview，RUST_LOG=debug 才會看到，user 平時零噪聲
- 順手發現 latent bug：原 inline `.contains() ||` 順序下，`ripgrep` 永遠先吃「取代 find/grep」組合案例，後 2 條 pattern 永遠 unreachable。改 const 排序 specific-first 後 log pattern name 更精準，filter 行為不變（命中即丟）

**搜尋**:
- R3 / R5 log 「不做的範圍」段找候選
- Grep 5 條 inline `err_s.contains(` 鎖定位置
- 沒做 WebSearch（這是既有程式碼 pattern 重構，不是新領域）

**做了什麼**:
- 加 module-level const `HOOK_FAILURE_FALSE_POSITIVE_PATTERNS: &[(&str, &str)]`（name + needle pair）
- 加 helper `fn hook_failure_false_positive(err: &str) -> Option<&'static str>`（純函式，回第一個命中的 pattern name）
- `tick_inner` Rule 3 區段：5 條 inline `.contains() ||` → `if let Some(pattern) = hook_failure_false_positive(err_s) { log::debug!(...); continue; }`
- 加 3 個 unit test：
  - `hook_false_positive_matches_each_known_pattern`：5 條 pattern 各自命中驗
  - `hook_false_positive_returns_none_for_legit_errors`：空字串、ENOENT、permission、裸 "rg"、單獨 "取代" 都回 None
  - `hook_false_positive_pattern_names_are_stable`：const 順序鎖定（log 端能依賴 name 而非 index）

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 33/33 pass（30 prior + 3 new）
- `bash test/smoke-test.sh quick` PASS
- 過程中抓出 1 個 latent test failure（`ripgrep` pattern 太廣導致後 2 條 unreachable）→ 改 const 排序後 33/33 綠

**結果**: PASS（M0 observability 改善落地 + 順手修 latent 排序 bug + 3 unit test + 0 lint warning，commit `9a14387`）

**不做的範圍**（給後續輪次）:
- Rule 3 的 5 條 pattern 改為 user-configurable（config 暴露）：超出本輪 surgical 範圍
- 加 log::debug 對 `quota_low` / `token_spike` 等其他 rule 的命中也 surface：scope 漸進、未有 user report
- `daily_summary_hour == now_local.hour()` 整點 + 15s tick 精度（R3 提）：1 分鐘內容忍，未有 user report
- `last_summary_date` 持久化（R3 提）：R5 觀察時已驗證 wired up（lib.rs:1341 呼叫 `load_persisted_summary_markers`、auto_rules.rs:698/779 呼叫 `persist_summary_markers`），非遺留

---

### [2026-06-01] Round 3 — daily/weekly summary 純 toast 模式瘋狂重發
**類型**: M0（user-facing 阻斷 bug：純 toast 模式開 daily/weekly 會被 15s tick 反覆推）
**KPI**: K-summary-dedup-correctness
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| daily_summary 純 toast 模式 1 小時內 fire 次數 | 240 (15s × 60) | 1 | −239 |
| weekly_summary 純 toast 模式 1 小時內 fire 次數 | 240 (15s × 60) | 1 | −239 |
| send_embed 失敗時是否 unblock dedup | 是（會反覆重試轟炸） | 否 | ✓ |
| 24h chore_ratio | 0% | 0% | — |

**為什麼**:
R1/R2 結論「LobsterPulse 端無/已修 M0」偏重 user panic / dev workflow break。
本輪往**功能性 silent failure** 方向重掃 1087 行 `auto_rules.rs`，找到一個 user-facing 阻斷 bug：

`auto_rules.rs` Rule 4/5（daily/weekly summary）：
```rust
if should_fire {
    // ... collect data + send_toast ...
    if discord::send_embed(...).is_ok() {     // ← 問題在這
        state.lock().unwrap().last_summary_date = today;
    }
}
```

dedup marker 只在 **Discord send 成功** 後才寫。純 toast 模式（無 Discord 通道）
→ `is_ok()` 永遠 false → `last_summary_date` 永遠是空字串 → 下一次 tick 通過 `should_fire` 檢查
→ **整天反覆發 toast**（tick = 15s，每小時 240 次，summary_hour 整點一小時 240 個 toast 通知）。

影響面：
- `daily_summary_enabled` / `weekly_summary_enabled` 預設 off，但用戶主動開 + 只用 toast 通道 → 中招
- 與 R2 修的「Discord silent fail」是**同方向 root cause**：原本用 send 結果決定 dedup，邏輯反了
- chore_treadmill 紅線要求 M0-M3，本 bug 完美符合 M0 定義（user-facing 阻斷）

**搜尋**:
- 重掃 `auto_rules.rs` 全 1087 行找 `let _ = ... = is_ok() { state.lock() = }` 同類模式
- 確認 Rule 1/2/3/6 用 `dedup_gate` 正確（決定要 fire 時就 set）→ Rule 4/5 是 outlier
- 確認 `quota_history.rs` 的 `extract_min_percent` 與這無關

**做了什麼**:
- Rule 4 (daily_summary) line 519-575：dedup marker 從「send_embed 成功後」移至「should_fire 通過後立即」
- Rule 5 (weekly_summary) line 591-643：同樣改法
- 抽 helper `mark_summary_fired_if_new(&mut AutoRuleState, SummaryMarker, &str) -> bool`
  + enum `SummaryMarker { Daily, Weekly }` 統一兩條規則的 dedup 語意
- Discord send 改為 `let _ = ...`（best-effort，失敗不再 unblock dedup 避免失敗重試轟炸）
- 加 3 個 unit test 覆蓋：
  - `summary_dedup_marks_before_send_pure_toast_mode`（核心 regression：模擬純 toast 模式 tick 第二次）
  - `weekly_dedup_marks_before_send_pure_toast_mode`（同上、weekly 變體）
  - `daily_and_weekly_markers_are_independent`（daily 標過不影響 weekly）

**驗證**:
- `cargo check --quiet` → Finished 0 errors ✓
- `cargo clippy -- -D warnings` → 無 warning ✓
- `cargo test --lib auto_rules::tests` → **6 passed; 0 failed**（含 3 個新增）✓
- `bash test/smoke-test.sh quick` → PASS ✓

**結果**: PASS（M0 bug 修復落地 + 3 個 unit test 覆蓋）

**不做的範圍**（記錄給後續輪次）:
- Rule 3 (hook_failure_burst) 的中文硬碼 filter（`"請用"`/`"ripgrep"`/`"rg.exe"`/`"取代 find"`/`"取代 grep"`）：
  屬 noise reduction 而非 critical functionality，且非 user-facing 阻斷，留觀
- 11+ 處 `let _ = discord::...` 吞 error：R2 提的 caller side 改善，是 noise 而非 silent failure
- `daily_summary_hour == now_local.hour()` 的整點 + 15s tick 精準度（可能在 9:00:00 沒 tick、9:00:15 補發）：
  屬 best-effort 推送容忍範圍，1 分鐘內可接受
- `last_summary_date` 沒持久化（重啟會 reset）：目前是 in-memory `Arc<Mutex>`、不算阻斷 bug



### [2026-06-01] Round 2 — Discord HTTP 4xx silent fail → surfaced
**類型**: M0（user-facing 阻斷 bug）
**KPI**: K-discord-err-visibility
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| discord-err-visibility | silent (curl 無 -f) | surfaced (exit 22 = HTTP err) | ✓ |
| yes/no 投票可用性 | broken on 429/400 emoji | working + 顯式 error | ✓ |
| 「測試連線」按鈕錯誤訊息 | `no message id: {…}` | `curl exit 22: …` | ✓ |

**為什麼**:
- R1 結論「LobsterPulse 端無 M0」是「已建立功能無 panic」層級
- 本輪重看 LobsterPulse-specific 新程式碼（`openab_bridge.rs` / `discord.rs`），找出**功能性 silent failure**：
  - `discord.rs::curl()` 沒加 `-f` flag → HTTP 4xx/5xx curl 仍 exit 0
  - `add_reaction` 對 rate-limit (429) / invalid emoji (400) 回 `Ok(())` 而非 `Err`
  - 所有 caller 都 `let _ =` 吞 error → 投票 ✅❌ 在 429 觸發時整個決策流永遠卡住
  - `send_message` 走 `parse_msg_id` 會回「no message id: {…}」誤導 user，實際是 401
- 影響面：自動投票決策 / 確認按鈕 / OpenAB 轉發 / 「測試連線」UX 全中
- chore_treadmill 紅線要求本輪必 M0-M3，本 bug 完美符合 M0 定義

**搜尋**:
- 查 `.lp-notify.health` = HTTP 401 確認 notify daemon 憑證壞（loop infra，非 LP 代碼）
- grep `discord::` / `openab_bridge::` 所有 call site 確認影響範圍
- 確認 Discord REST API 對 204/2xx 不受 `-f` 影響（不會誤報 success）

**做了什麼**:
- `src-tauri/src/discord.rs::curl()` args array 加 `-f` flag（一行）
- 副作用：所有 Discord function 對 HTTP 4xx/5xx 統一回 `Err("curl exit 22: …")`
- 204 No Content（部分 Discord endpoint）不受影響，繼續 exit 0

**驗證**:
- `cargo check` → Finished 1.79s ✓
- `bash test/smoke-test.sh` → PASS (cargo check 綠) ✓
- 未做整合測試（需真 Discord token）；curl 行為變更是文件化的，無破壞性風險

**結果**: PASS（M0 bug 修復落地）

**不做的範圍**（記錄給後續輪次）:
- 跨平台 `curl.exe` → `curl` 命名：CLAUDE.md 仍標 Windows-first，本輪最小修
- `let _ =` 改為 `if let Err(e) = log::error!(...)`：caller side 改善是另議題，需逐個 audit auto_rules.rs call site
- Unit test 對 curl mock 需引入 wiremock / mockall，超出本輪最小修
- `.lp-notify.health` HTTP 401 = loop notify daemon 憑證壞，非 LP 代碼，本輪不動

---

### [2026-06-01] Round 2 (二) — fork dev tooling rename 補完
**類型**: M0（dev workflow bug）
**KPI**: dev tooling 與 Cargo.toml binary 對齊
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| shell + CI 與 binary 名一致性 | 0/6 | 6/6 | +6 |
| smoke-test.sh quick | PASS | PASS | — |
| 24h chore_ratio | 0% | 0% | — |

**為什麼**:
R1 結論「LobsterPulse 端無 M0」是「無 panic / build error」層級。R2 我從 R1 觀察的 H0 候選中挑選並**重新評估嚴重性**：
- `src-tauri/Cargo.toml` line 2: `name = "lobster-pulse"`
- `src-tauri/src/bin/lobster-pulse-hook.rs` (sidecar 實檔)
- 實際 build 產物（已 ls 確認）: `target/release/lobster-pulse.exe` + `lobster-pulse-hook.exe`

→ 對 fork 開發者：`pkill -9 -x agent-pulse` 永遠 no-op、`[ -f target/release/agent-pulse ]` 永遠 false、`./reload.sh` 直接 `Error: no binary found`、CI artifact zip 名字錯。

R1 把它歸 H0 是誤判 — 這是真實的 dev workflow break，升級為 M0。

**搜尋**: 無（從 R1 結論直接推導、避免重複盤點）

**做了什麼**:
- 改 4 shell: `agent-pulse` → `lobster-pulse`（含 comments，pkill + binary path）
- 改 2 CI: `build.yml` artifact name + 4 paths、`release.yml` comment + zip name + 4 binary refs
- 全部改完 `agent-pulse` 在 `**/*.{sh,yml}` 0 殘留

**驗證**:
- `bash -n` 4 shell 語法 OK
- `python yaml.safe_load` 2 workflow 解析 OK
- `bash test/smoke-test.sh quick` 仍 PASS

**結果**: PASS（commit `cb2edca`，6 files / +23 / -23）

**不做的範圍**:
- `package.json` version 0.2.2 → 0.5.4 對齊（R1 H0 #1）：純格式對齊、無功能差異
- upstream 13 個 commits 的 backport 評估：跨 fork boundary、需單獨 round 做
- `.gitignore` 的 `.spectra/` 線：本輪發現仍 dirty 但非我本輪改的（R1 設置 loop 時加的、commit `d6d0eb3` 沒包它）→ 維持原樣觀察，loop 可能依賴這行未提交狀態

---

### [2026-06-01] Round 1 — BACKLOG ↔ Codebase 對齊盤點
**類型**: 不適用（盤點輪，無程式碼變更）
**KPI**: N/A（無法推進）
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| (無可推進 KPI) | — | — | — |

**為什麼**:
本輪做「對齊 Mission + 盤點現狀」而不直接動工，原因：

1. **Spectra 變更與 codebase 錯位**：
   - 作用中 change = `openclaw-self-evolution`（21 tasks / 6 done）
   - 剩餘 tasks 包含「SQLite FTS5 索引 agent 對話」「DSPy + GEPA 安裝」「搜尋 API `/evolution/search`」「失敗率高的 skills 自動進化」等
   - 上述所有 tasks 屬於 **agent runtime / openclaw 系統** 範疇
   - 當前 codebase = LobsterPulse（Tauri 2.x 桌面監控 app，無 SQLite/rusqlite/sqlx 依賴、無 agent runtime 程式碼）
   - 兩者交集 = 0。前 6 個 done tasks 也不在本目錄可見（openspec/changes/openclaw-self-evolution/ 不存在、specs/ 為空），推測由外部 Spectra daemon 追蹤

2. **Baseline 確認綠**：
   - `cargo check`（src-tauri）→ 0.77s finished ✓
   - `bash test/smoke-test.sh` → PASS ✓

3. **LobsterPulse 端無 M0**：
   - 無 test 失敗
   - 無 panic / build error
   - `.lp-notify.health` 顯示 broken（HTTP 401）但那是 loop 通知 daemon 憑證問題，**非 LobsterPulse 程式碼問題**
   - Cargo.toml 0.5.4 / tauri.conf.json 0.5.4 / package.json 0.2.2 三者版本不同步，但屬 H0 housekeeping，且 24h chore_ratio = 0（本 loop 才啟動），按「反 Pattern 黑名單『對齊格式』」原則，本輪不動

**搜尋**: 無（本輪不做改動，不需搜尋）

**做了什麼**:
- 盤點目錄結構、git 狀態、Cargo deps、openspec 內容
- 確認上下游分支（`upstream/main` 領先 `lobsterpulse/main` 13 個 non-merge commits，但這些是 upstream 路線圖，不屬本 loop 推進範圍）
- 確認 baseline gate（cargo check + smoke test）

**結果**: PASS（無改動 / 文件化卡點）

---

### [2026-06-01] Round 4 — session_idle 純 toast 模式永久 spam 修掉
**類型**: M0（user-facing 阻斷 bug：純 toast 模式 session 閒置 30 分鐘後每 5 分鐘永久重發 toast）
**KPI**: K-session_idle_toast_spam
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| session_idle 純 toast 模式永久重發（30min 後） | 每 5min 一次，永久 | 1 次 / idle 週期 | ✓ |
| 24h chore_ratio（本輪後） | 0% | 0% | — |
| cargo test --lib | 15 pass | 18 pass | +3 |

**為什麼**:
R3 修了 Rule 4/5 (daily/weekly summary) 純 toast 模式 dedup bug 後，往同方向再掃
Rule 2 (session_idle) 找有沒有相同 pattern。發現：
- `cfg.dedup_window_secs = 300s`（5min）
- `cfg.session_idle_trigger_secs = 1800s`（30min）
- Rule 2 純 toast 模式：30min 觸發後，5min dedup 窗一過就再 fire 一次。
  由於純 toast 模式無 Discord reaction → 無 `pending_confirm` 紀錄 → 沒有任何
  機制告訴「這個 session 已經通知過了」。
- 永久 spam 直到 session 變 active 或被 kill。

這跟 R3 修的 Rule 4/5 bug 結構同形：dedup 錨點用 `now_secs()`，對「事件週期」
太短。修法對齊：dedup 錨點改成「事件本身的週期」。

**搜尋**: 沒做（這是直接同 pattern 延伸，不是新領域探索）。

**做了什麼**:
- `AutoRuleState` 新增 `last_session_idle_event_ts: HashMap<String, i64>`
- 新增 `should_notify_session_idle()` helper：
  * 以 `session.last_event_time` epoch_secs 為錨點
  * 同 `(sid, last_event_ts)` 跳過（同一個 idle 週期）
  * `last_event_ts` 推進（session 變 active）→ 失配 → 允許下輪 idle 再 fire
  * `> 256` 筆時 lazy GC 64 筆最舊（防外部 session 移除後殘留撐大 map）
- Rule 2 loop 把 `dedup_gate` 換成 `should_notify_session_idle`
- 3 個 unit test 覆蓋：同週期去重 / 跨週期放行 / 不同 session 獨立 / lazy GC
- 後續在 `run_local_usage_runners` 抽 `write_local_usage_snapshot()` helper：
  * 原 inline 寫入路徑對 direct write fallback 是 silent fail（`let _ = ...`），
    60s loop 下會讓膠囊 quota 卡舊值且 user 不知是 OpenAB 沒更新還是 LP 自己寫失敗
  * 改用 `Result<(), String>` + 兩層 `log::error!`（atomic 失敗 + direct 失敗）
  * 2 個 unit test 覆蓋：happy path JSON 合法 / 不存在目錄必回 Err 且 error chain 含「atomic」

**驗證**:
- `cargo test --lib` → 20/20 pass（3 + 2 新增）
- `cargo clippy --lib --tests -- -D warnings` → 0 警告
- `cargo fmt --check` → 過

**結果**: PASS（M0 bug 修復落地 + 5 個 unit test 覆蓋 + 0 lint warning）

---

### [2026-06-01] Round 5 — restore baseline green by .gitignore-ing auto-dev harness runtime
**類型**: H0（housekeeping，解決「baseline 不綠」這個 P0 級治理債）
**KPI**: K-baseline-clarity
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `git status --short` 髒檔行數 | 16 | 0 | −16 |
| `git ls-files --others --exclude-standard` 未忽略 untracked | 16 | 0 | −16 |
| `git check-ignore` 對 16 髒檔命中率 | 0/16 | 7/7（已抽樣驗證；其餘同 pattern） | +7 |
| `cargo test --lib` | 24 pass | 24 pass | — |
| 24h chore_ratio | 0% | 33% | +33%（本輪 H0 觸發，4 輪內首次） |

**為什麼**:
本輪進場時 baseline = 16 髒檔，workflow 要求「baseline 綠才動工」，先停下查性質：
- 全部 `??`（untracked），無 ` M` / ` M ` 殘留
- 全部是 auto-dev engineer-loop + harness + lp-notify daemon 的 runtime state
  - `.engineer-loop.pid`（內容 "42904"）= loop PID lock
  - `.engineer-loop.state.json` = loop state
  - `.harness-*.json` / `.harness-*.state` = harness sensor 報告（R4 剛跑完）
  - `.lp-notify*` + `.lp-notify-cache/` = notify daemon jsonl / DLQ / health
  - `.project.lock`（"engineer-loop"）= project lock marker
  - `.spectra.yaml` = spectra app config（全 commented defaults）
  - `runs/` = 5 個 per-round run JSON（R1-R5，1.8-4.7KB / 個）
- R4 commit `a6e6169` 是 session_idle 純 toast 修，**乾淨**。這 16 檔不是 R4 殘留，是跨多輪累積的 harness 噪聲。
- Loop 正在跑 Round 5：`runs/5.json` 顯示 `status: "running"`、`started_at: 2026-06-01T11:55:59`、plan phase 11:56:02 完成。**不能刪任何檔案**，會 crash loop 與丟 state。
- R2 二已在「不做的範圍」記過同樣觀察，當時 loop 狀態不確定所以延後；現在 loop 仍活，唯一安全動作 = declarative .gitignore。

H0 cap 檢查：24h chore_ratio 前 = 0%（R1-R4 全 M0 或 inventory），本輪 H0 後 = 33%（= 1 H0 / 3 commits，R2 / R3 / R4 / 本輪共 4 個 commit，chore 1）。Cap = 5 輪 1 個 H0，**合規**。

**搜尋**: 無（這是已知 deferred 議題、loop 狀態已查清楚、不需新研究）。

**做了什麼**:
- `.gitignore` 補 8 個 pattern，分 4 區塊 + 註解：
  - engineer-loop runtime：`.engineer-loop.pid`、`.engineer-loop.state.json`
  - harness sensors：`.harness-*.json`、`.harness-*.state`
  - lp-notify daemon：`.lp-notify*`、`.lp-notify-cache/`
  - project lock + spectra config：`.project.lock`、`.spectra.yaml`
  - per-round runs：`runs/`
- 共 +20 / -0 lines，surgical
- 不刪任何既有檔案（loop 正在寫）

**驗證**:
- `git check-ignore -v` 對 7 個樣本檔（涵蓋每個 pattern） 7/7 命中正確行
- `git status --short` 從 16 行 → 0 行（commit 後空）
- `git ls-files --others --exclude-standard` = 0（無遺漏）
- `cargo check --quiet` → 0 errors ✓
- `cargo test --lib` → 24 passed; 0 failed ✓

**結果**: PASS（baseline 從 16 髒檔 → 0 髒檔；commit `0101e62`、1 file / +20 / -0）

**不做的範圍**（給後續輪次）:
- M0-3 程式碼改動：「一輪一件事」原則；本輪先還 baseline，Round 6 再挑 M0 bug 修。候選已備（見 R4 觀察 #3 + R2/R3 「不做的範圍」累積）：
  - `let _ = discord::...` 11+ 處 caller side 改善（R2 提）
  - Rule 3 hook_failure_burst 中文硬碼 filter（R3 提）
  - `last_summary_date` 持久化到磁碟（R3 提，目前 in-memory）
  - `daily_summary_hour == now_local.hour()` 整點 + 15s tick 精度（R3 提）
- `package.json` 0.2.2 → 0.5.4 對齊（R1 / R2 二 提）：仍屬 H0、且無功能差異，待後續 H0 窗口
- `dev.sh` / `build.sh` process name `agent-pulse` → `lobster-pulse`（R4 提）：H0、跨 shell 改動，待後續
- upstream 13 commits backport 評估（R4 提）：跨 fork boundary，需單獨 round
- 刪除這 16 個 runtime 檔：loop 正在跑，動 state = crash；本輪絕不做
- `.lp-notify.health` HTTP 401 修：loop infra 問題、非本目錄程式碼（R1/R2/R4 已標），不歸 LP 端

---

## 觀察事項（給後續輪次）

1. **Spectra change `openclaw-self-evolution` 對 LobsterPulse 不可執行**（記憶 10335 確認）：
   - 21 個 task 涉及 SQLite FTS5 對話索引 / DSPy+GEPA 反思進化 / skill genesis hook — 全是 OpenAB agent runtime 的概念
   - 本 repo 是 `lobsterpulse`（Tauri v2 桌面膠囊）+ AgentPulse fork 程式碼，沒有 OpenAB runtime、沒有 agent 對話 log 可索引
   - R4 結尾的決策：把這 change 視為**跨 repo 誤派**，不在本 loop 推進。後續若要繼續，需 user 端把 change 移到 OpenAB repo（或關閉本 change、在 OpenAB repo 重開）
2. **若 Spectra 變更其實是 meta-task**（指 loop 自身的演化）：本 loop 還沒產生對話 log（無 .jsonl 對話記錄，只有 burn.log + notify jsonl），FTS5 索引無對象可索引。
3. **LobsterPulse 端可考慮的 H0 候選**（不在本輪做）：
   - `package.json` version 0.2.2 → 0.5.4 對齊 Cargo.toml / tauri.conf.json
   - `dev.sh` / `build.sh` 仍叫 `agent-pulse`（upstream 進程名），應改 `lobster-pulse`（fork 已改名）
   - upstream 13 個 commits 中 `aa388d9`（Drop spurious completion-signal mappings）和 `96f8c05`（fall back to cargo-tauri when npm tauri CLI absent）可能是 backport 候選
4. **Notify 401**：loop 通知 daemon 憑證壞，影響 loop → user 的通知。屬 loop infra 修，非本目錄 code。建議 user 端查 webhook/bot token。

## 決策記錄

- **R1**: 不硬幹。寫 log、標出卡點、等下一輪看 Spectra / loop 是否有 openclaw 對應的程式碼到位。
- **R2 / R2 二**: Discord HTTP 4xx silent fail → surfaced（M0）；fork dev tooling rename 補完（M0），並把 `.gitignore` `.spectra/` 議題 deferred 標記。
- **R3**: daily/weekly summary 純 toast 模式 dedup 提前 set（M0）。
- **R4**: session_idle 純 toast 永久 spam 修掉（M0）；抽出 `write_local_usage_snapshot` helper 修 direct write silent fail（M0 連帶）。
- **R5**: baseline 16 髒檔 → 0 髒檔（declarative .gitignore，H0）。H0 cap 0/5 → 1/5 啟用。M0-3 程式碼改動 deferred 給 R6，候選清單在 R5 log 「不做的範圍」段。
- **R6**: Discord 14 處 fire-and-forget silent fail 全面 surfaced（M0），K2 0/14 → 14/14。
- **R7**: 本 loop 漏記（supervisor 標 1 輪無改善）。M0-3 未推進。
- **R8**: hook_server 2 處 silent fail surfaced + process_body 抽 pure fn + 5 unit tests（M0），K3 0/2 → 2/2。Lib tests 25 → 30。chore treadmill 紅線觸發（58% > 50% cap），本輪嚴守 M0-3、H0 cap 仍 1/5。
- **R9**: hook_failure_burst 5 條 false-positive pattern 命中改 silent → log::debug surfaced（M0），K-hook-failure-observability 0/5 → 5/5。順手修 latent bug：原 inline `.contains() ||` 順序下「ripgrep」會先吃「取代 find/grep」組合案例，後 2 條 unreachable → const 改 specific-first 排序。Lib tests 30 → 33。Chore treadmill 紅線未消（本輪 fix type），H0 cap 仍 1/5。

### 2026-06-01 R5 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

### [2026-06-01] Round 6 — Discord 14 處 silent error swallowing 全面 surfaced
**類型**: M0（user-facing observability bug：Discord 4xx/5xx 在 14 處 fire-and-forget 通知點被吞 → operator 看不到 quota_low / session_idle / hook_failure_burst / daily- weekly summary / token_spike / discord_kill_cmd 任何一條推不出去）
**KPI**: K2-discord-delivery-observability（0/14 surfaced → 14/14 surfaced）

**為什麼**: R2 修了 curl `-f` 讓 transport 不 silent，但呼叫端仍 `let _ = discord::xxx(...)` 吞 error → log 系統完全沒有軌跡。Discord outage 無法與「無事可報」區分。修這個對齊 MISSION「事件流可靠 + 錯誤可見」。

**搜尋**: 直接 Grep `let _ = discord::` 14 處全在 `auto_rules.rs`（驗證 R5 觀察 10355），無需 WebSearch。

**做了什麼**:
- 加 `pub(crate) fn discord_err_msg(ctx, err) -> String` 統一 prefix `[auto_rules] discord <ctx> failed: <err>`（一個 log filter 抓全部 14 類）
- 14 sites 改 `if let Err(e) = ... { log::warn!("{}", discord_err_msg(ctx, &e)) }`，ctx 含 rule + id（quota_low name+pct、session_idle sid_short + kind、hook_failure_burst provider、token_spike name+pct、discord_kill_cmd sid+reaction、daily/weekly summary 用 rule name）
- 加 unit test `discord_err_msg_unifies_prefix` 鎖 format + 中文 ctx 不被 trim/lower
- 0 個 `let _ = discord::` 殘留（1 個是 doc comment 範例）

**驗證**:
- `cargo test --lib` 25/25 pass（24 prior + 1 new）
- `cargo build --lib` clean
- `cargo clippy --lib -- -D warnings` clean
- commit `da71ddc`：1 file +129 / -25

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K2 discord error logging 覆蓋 | 0/14 sites | 14/14 sites | +14 |
| Lib unit tests | 24 | 25 | +1 |

**結果**: PASS

### 2026-06-01 R8 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

### [2026-06-01] Round 8 — hook_server 2 處 silent fail surfaced + 5 unit tests
**類型**: M0（user-facing observability bug：9 個 provider 全部事件入口 hook_server::handle_client 對 JSON parse 失敗 + tx.send 失敗皆 silently dropped，operator 無 log 可查「事件送達失敗」）

**KPI**: K3-hook-event-observability（0/2 sites surfaced → 2/2 sites surfaced）

**為什麼**: R6 修了 Discord 14 處 transport-layer silent fail，但 hook_server 自身才是 9 providers 全部事件的入口。`Err(_) => 400` 直接吞 parse error、`let _ = tx.send(event)` 直接吞 channel send error——operator 看到的現象是「capsule 不動 / 名單沒新事件」，但 log 系統完全沒線索區分「無事件」 vs 「事件被吞」。對齊 R6 模式：fail 路徑 surfaced via log::warn，body 截 200 byte preview 避免 log 爆。

**搜尋**: Grep `let _ =` hook_server.rs 鎖定 2 條 fail 路徑（JSON parse 400 + tx.send 丟 event）。process_body 抽 pure function 方便 unit test 鎖 normalize + session_id default + failed status promote 行為。

**做了什麼**:
- 抽 `fn process_body(body, provider) -> Result<HookEvent, ()>` 為 pure function（從 handle_client 內聯展開）
- `Err(())` arm 改 `log::warn!("[hook_server] JSON parse failed for provider={} body={}", provider, preview)` + 200 byte body preview（`from_utf8_lossy` 處理非 UTF-8）
- `tx.send` 失敗改 `log::warn!("[hook_server] tx.send failed (receiver dropped) for provider={}", provider)`
- 加 5 個 unit test 鎖 process_body 行為：valid parse、invalid json → Err、session_id missing → default、snake_case → PascalCase、failed PostToolUse → PostToolUseFailure

**驗證**:
- `cargo test --lib` 30/30 pass（25 prior + 5 new process_body tests）
- `cargo fmt --check` clean
- `cargo clippy --lib -- -D warnings` clean
- commit `61de03a`：1 file +71 / -15

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K3 hook event logging 覆蓋 | 0/2 sites | 2/2 sites | +2 |
| Lib unit tests | 25 | 30 | +5 |
| 24h chore_ratio | 58% (10/17) | — | （R8 不做 H0，cap 用 0/5） |

**結果**: PASS

---

### [2026-06-01] Round 10 — 9-provider smoke matrix for K3 baseline
**類型**: M2（補強 KPI 量測 — K3 smoke pass 從「人腦記住 9 條路徑」變成「1 條 test 跑過即覆蓋全部」）
**KPI**: K3-smoke-pass-provider-coverage

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K3 smoke pass provider coverage（test 內可量測） | 0/9 | 9/9 | +9 |
| 9-provider event flow 改壞立即 fail 並指出哪家 | 否（要 user 端 curl 9 條才發現） | 是（fixture 內 `assert_eq!` message 自帶 provider 名） | ✓ |
| 加 provider 強迫同步加 fixture | 無 enforce | `assert_eq!(fixtures.len(), 9)` 漏加直接 fail | ✓ |
| Lib unit tests | 33 (R9 後) | 34 | +1 |
| 24h chore_ratio（本輪後） | 61% pure（R10 進場時 harness 量） | 0%（R10 是 test type） | — |
| 24h 連續 M0-3 推進輪數 | 5 (R2/R2二/R3/R4/R6/R8/R9) | 6 | +1 |

**為什麼**:
- R5 retrospective 把 K3 smoke pass 點名為 tracked KPI，但 0/9 是 ad-hoc 記憶「我手動 curl 過 9 條 endpoint」，改壞任何一家 normalize/alias 規則要在 user 端才暴露
- R8 修 `hook_server.rs` 抽 `process_body` 為 pure fn 時，已開出 2 條 unit test（session_id alias + openx legacy alias）— 本輪把同一條「完整路徑走完 9 家」的概念擴大成 1 條 matrix test
- 進場時 `git status` 顯示 `src-tauri/src/hook_server.rs` 已有 uncommitted diff（120 行 = 這條 test 的 fixture）— 是 R8 / R9 期間留下未 commit 的草稿，cargo test 已驗過 1/1 pass，本輪 commit 它

**搜尋**:
- 沒做 WebSearch（這是既有 test pattern 擴展，不是新領域）
- 讀 `engineering-log.md` R5 段確認 K3 定義；R8 段確認 `process_body` 已是 pure fn 可直接構造 fixture 餵入
- grep `parse_provider` / `process_body` call site 確認 matrix 覆蓋 9 家、沒有第 10 家被遺漏

**做了什麼**:
- `hook_server.rs::tests` 加 1 條 `#[test] fn smoke_test_all_9_providers_event_flow()`（120 行）
  - `struct Fixture { http, body, expected_provider, expected_event, field_check }` 9 個 fixture
  - 4 本機 CLI（claude/codex/copilot/gemini）走 PascalCase / kebab-case / camelCase / Gemini 自創 event name 四種 normalize 規則
  - 5 OpenAB（cicx/gitx/giminix/codex_bot/openx）走 OpenAB native event name（`tool_call` / `token_update` / `thinking_delta` / `post_tool_use`）+ `tool_status=failed` 升級成 `PostToolUseFailure` + `/hook/bot` legacy alias rewrite 成 `openx`
  - 對 5 家用 `Box<dyn Fn(&HookEvent)>` 做 field alias 驗（`sessionId` / `toolName` / `inputTokens` 等 alias 是否落對欄位）
- self-enforce：`assert_eq!(fixtures.len(), 9, "smoke matrix 必須 9 個 provider，加 provider 就要加 fixture")` — 漏加直接 fail
- assertion message 自帶 provider 名：壞了 trace 立即指出「fixture #3 (gemini): event_name normalize 錯誤」

**驗證**:
- `cargo test --lib` → **34/34 pass**（30 prior + 3 R9 + 1 new）✓
- `cargo test --lib hook_server::tests::smoke_test_all_9_providers_event_flow -- --nocapture` → **1/1 pass** ✓
- `cargo clippy --lib --tests -- -D warnings` → **0 warning** ✓
- 9 條 fixture 全綠：claude/codex/copilot/gemini/cicx/gitx/giminix/codex_bot/openx

**結果**: PASS（M2 KPI 量測基準落地，commit `2ef56bd`，1 file / +120 lines）

**不做的範圍**（給後續輪次）:
- 把 `test/smoke-test.sh` 從 `cargo check` 升級到 `cargo test --lib` 跑 smoke matrix：超 180s timeout，目前 `cargo check` 是合理的 fast gate；matrix 留給 nightly / pre-release gate
- 把 smoke matrix 拆成 9 條獨立 test：拆了反而失去「1 條跑完 = 9 家全綠」的可讀性（kpi 進度表的「9/9」要逐條數），目前 1 條 matrix 是對 KPI 報表友善的形狀
- 把 K3 smoke pass 接到 `harness-reflection-kpi.json`：`kpi_rounds` 3/5 (60%) < 80% target，是 R5 提的 follow-up，本輪 M2 是把量測本體做出來，自動化是另一段工程
- 加負向 fixture（如「`/hook/foo` 應回 Err」）：matrix 設計是「9 條合法路徑全綠」，負向是 `process_body` 自己的單測範圍（已覆蓋），不重複堆
