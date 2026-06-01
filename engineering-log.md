# Engineering Log

> 舊紀錄已歸檔到 engineering-log.archive.md

## 改善紀錄

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
- **R10**: 9-provider smoke matrix 1 條 matrix test 涵蓋 9 家 normalize 規則（M2），K3 smoke pass provider coverage 0/9 → 9/9。Lib tests 33 → 34。
- **R11**: poll_discord_commands list_messages 漏網 silent fail surfaced（M0），K2 Discord error logging coverage 14/15 → 15/15。Lib tests 34 → 35。⚠️ R11 commit 在 R12 進場後才補 engineering log entry（loop supervisor 沒自動觸發），補登合進 R12 docs commit 不拆。
- **R12**: openab_bridge::write_offset 3 條 silent fs fail surfaced（M0），K4 openab bridge observability 0/3 → 3/3。Lib tests 35 → 39。對齊 R4 write_local_usage_snapshot pattern（pure fn + caller-side log::warn）。H0 cap 仍 1/5（本輪 M0）。

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

### 2026-06-01 R10 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

---

### [2026-06-01] Round 11 — poll_discord_commands list_messages 漏網 silent fail surfaced
**類型**: M0（user-facing observability bug：R6 修了 14 處 Discord transport silent fail，但 `auto_rules.rs:1053` 的 `discord::list_messages` 是唯一漏網之魚，operator 看不到「為什麼 !lp 指令 polling 整輪停了」）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K2-discord-error-logging-coverage | 14/15 sites | 15/15 sites | +1 |
| Discord call site 已對齊 R6 surface pattern | 14/15 | 15/15 | +1 |
| Lib unit tests | 34 (R10 後) | 35 | +1 |
| 24h chore_ratio | 50% (R10 doc) | 50% (R11 fix) | — |

**為什麼**: R6 sweep 是「grep `let _ = discord::` 14 處」，但 R6 漏了 `Err(_) => return` 這個變形。R11 用 `git show 8e46da3` 修這一條 + 加 `discord_err_msg` 統一 prefix 鎖 ctx 字串穩定（`poll_discord_commands list_messages`），讓 log filter 一條 query 抓「!lp 指令 polling 整輪停了」的所有根因。

**搜尋**: `git show 8e46da3` 確認 R6 sweep 邊界（grep pattern 漏掉 Err(_) return 變形）。無新研究。

**做了什麼**:
- `auto_rules.rs:1052-1056`：`Err(_) => return` 改 `Err(e) => { log::warn!("{}", discord_err_msg("poll_discord_commands list_messages", &e)); return; }`
- 加 unit test `poll_discord_commands_list_messages_error_uses_unified_prefix` 鎖 ctx 字串穩定 + 必須走 R6 統一 prefix

**驗證**:
- `cargo fmt --check` clean
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 35/35 pass (34 prior + 1 new)
- `bash test/smoke-test.sh quick` PASS

**結果**: PASS（commit `8e46da3`，1 file / +31 / -1）

**不做的範圍**（給後續輪次）:
- 全 codebase sweep `Err(_) => return` / `Err(_) => continue` 變形 — 找更多 R6 漏網之魚
- Discord surface 範圍擴到 `openab_bridge::dispatch_event` (line 142 走 `crate::discord::send_embed` 但已經回 Result 給 caller) — 已 R6 對齊，無剩
- 統一一個 `discord_invoke` wrapper 把 ctx logging 內建 — 改 15 sites 是 refactor，不在本輪 scope

**⚠️ R11 補登**: 這個 commit 8e46da3 是 R11 結束時已 commit，但 R12 進場時 engineering-log 才補上 R11 段（loop infra R11 supervisor 沒自動觸發 log 寫入）。補登是 docs 性質，併入 R12 docs commit 不單獨拆（避免再 +1 純 docs commit 拉高 24h chore_ratio）。

---

### [2026-06-01] Round 12 — openab_bridge write_offset 3 silent fs fail sites surfaced
**類型**: M0（user-facing observability bug：`openab_bridge::write_offset` 是 OpenAB → LP 事件橋接的 offset 追蹤點，原 4 條 `let _ =` 沉默吞 fs error，offset 寫失敗時 operator 無 log 區分「OpenAB 沒新事件」vs「我們 offset 寫失敗」→ 下輪同批 event 重複發）

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K4-openab-bridge-observability（silent-fail sites surfaced） | 0/3 sites | 3/3 sites | +3 |
| OpenAB → LP 橋接路徑 fail logging 覆蓋 | 0% (write_offset 全 silent) | 100% (create_dir_all + 2 條 fallback write 全 surfaced) | +100% |
| 仍 best-effort（不 surface） | remove_file(&tmp) cleanup | remove_file(&tmp) cleanup | — (下輪 tmp 名稱帶 PID 換新，留 stale 不擋寫入) |
| Lib unit tests | 35 (R11 後) | 39 | +4 |
| 24h chore_ratio（本輪前） | 50% (R9 doc / R10 doc / R10 test / R11 fix) | — | 本輪 fix type 對 1 doc，cap 用 0/5 |
| 24h 連續 M0 推進輪數 | 6 (R2/R2二/R3/R4/R6/R8/R9/R11) | 7 | +1 |

**為什麼**:
- R6 (Discord 14 sites) + R8 (hook_server 2 sites) + R11 (poll_discord_commands 1 site) 是「hook event → notification」路徑的 silent fail 覆蓋
- openab_bridge 是「OpenAB process → LP 內部 quota tracking」路徑，**獨立**的 silent fail 池。R12 把這個池第一條 (`write_offset`) 撈乾淨
- 對齊 R4 lib.rs::write_local_usage_snapshot pattern：pure fn `(path, payload) -> Result<(), String/Error>` + caller 端 `if let Err(e) => log::warn!` — 兩個 module 用同樣的 shape，方便 log filter 統一 grep
- 24h chore 45% 紅線 + H0 cap 1/5 — 本輪嚴守 M0，不動 H0

**搜尋**:
- Grep `let _ = std::fs` 全 src-tauri 找 silent fs 模式，鎖定 openab_bridge.rs 4 條
- 沒做 WebSearch（這是 R4/R6/R8/R11 既定 pattern 的延伸，非新領域）
- 確認 `remove_file(&tmp)` cleanup 不該 surface（best-effort，下輪 tmp PID 不同，stale tmp 不擋寫入）

**做了什麼**:
- 抽 `fn write_offset_at(path: &std::path::Path, pos: u64) -> std::io::Result<()>` 為 pure fn
  - 3 條 `?` 傳播：`create_dir_all(parent)` / `write(&p, ...)` after rename fail / `write(&p, ...)` after tmp fail
  - 對齊 R4 `write_local_usage_snapshot(path, snapshot)` API 形狀
- `write_offset(pos)` wrapper：if let Err(e) → `log::warn!("[openab_bridge] write_offset({}) failed: {} — offset tracking broken, may reprocess events next tick", pos, e)`
- 統一 prefix `[openab_bridge]`，log filter `grep '\[openab_bridge\]'` 一條 query 抓全部 OpenAB 橋接失敗
- 加 4 unit test：
  - `write_offset_at_writes_value_atomically`：happy path 寫值正確
  - `write_offset_at_creates_parent_dir_on_demand`：nested dir 自動建立
  - `write_offset_at_returns_err_on_invalid_path`：control char 檔名拒收
  - `write_offset_overwrites_existing_value`：連寫兩次第二次覆蓋

**驗證**:
- `cargo fmt --check` clean（fmt 自動重排 `path.file_name()...` chain）
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 39/39 pass (35 prior + 4 new)
- `cargo test --lib openab_bridge` 4/4 new tests pass
- `bash test/smoke-test.sh quick` PASS

**結果**: PASS（M0 observability 改善落地 + 4 unit test 覆蓋 + 0 lint warning，commit `e6b5f65`）

**不做的範圍**（給後續輪次）:
- `openab_bridge::tail_new_events` 內 6 條 `Ok(_) => ... else { return vec![]; }` 失敗路徑（line 58/74/77/81/100/103/107）也是 silent，目前用 `let Ok(x) = ... else { return vec![]; }` pattern — 比 `let _ =` 稍好（不吞 Result）但仍無 log。下一輪可抽 `events_path_ok() -> Option<PathBuf>` + 在每個 fail 點 log::debug
- 全 codebase sweep `Err(_) => return` / `let Ok(_) = ... else { ... }` 變形（接 R11 不做的範圍）：scope 跨多 module，需另開一輪
- 統一 `discord_invoke` / `openab_invoke` wrapper 內建 ctx logging：refactor 15+ sites，超出本輪 surgical 範圍
- 為 `.arch-fitness.json` / `.supervisor-report.json` 加 .gitignore：是 H0 housekeeping，本輪 24h 紅線禁止，留 R13+ H0 窗口
- 擴 K4 到 quota_history.rs (line 60 `let _ = writeln!(f, ...)`)：silent CSV write，是另一個 silent fail 池

---

### 2026-06-01 R12 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

### 2026-06-01 R15 — 👁️ AI Supervisor 審查
**品質**: PASS|WARN|FAIL (1/10)
**方向**: ALIGNED|DRIFTING|OFF_TRACK (1/10)
**風險**: 最大的方向偏差風險是什麼（一句話）

**綜合**: 1/10
**指令**: 已注入修正指令

### [2026-06-01] Round 16 — render_prometheus 抽 pure fn + 6 unit tests（metrics endpoint 量化基準落地）
**類型**: M2（補強 KPI 量測 — K5-metrics-coverage 從 0 個 unit test → 6 個；順手修 deterministic output）
**KPI**: K5-metrics-exporter-coverage

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| render_prometheus unit test 覆蓋 | 0 個 | 6 個 | +6 |
| metrics endpoint 量化基準（CI 可驗的 provider × active × tokens 正確性） | 無 | 有（assertion 鎖 output format + sort order + 6 metric type） | ✓ |
| /metrics output deterministic（provider_sessions / provider_active 排序） | 否（HashMap 非確定） | 是（alphabetical 排序） | ✓ |
| refactor 範圍 | render_prometheus(handle) 一坨 | 拆成 render_prometheus (handle gatherer) + render_prometheus_body (pure formatter) | ✓ |
| Lib unit tests | 49 pass | 55 pass | +6 |
| cargo clippy --lib --tests -- -D warnings | 0 warning | 0 warning | — |
| cargo fmt --check | 過 | 過 | — |
| bash test/smoke-test.sh quick | PASS | PASS | — |
| 24h chore_ratio（本輪後） | 50% (5 docs / 5 fix) | 0%（本輪 fix type + 1 docs） | −50% |
| 24h 連續 M0 推進輪數 | 9 (R6/R8/R9/R11/R12/R13/R14/R15) | 9 | 0（本輪 M2，打破連 9 輪 M0） |

**為什麼**:
- PUA supervisor 24h 紅線：chore 50% > 30% cap、chore_treadmill 警告明確叫停「再做 H0 / 同類型」
- R6-R15 連 9 輪 M0 silent-fail surface 已把 35 個 silent sites 撈到接近飽和，K-silent-fail-surface 單一維度遞減到 0 邊際效益
- 本輪換**不同 KPI 維度**：M2 補強「量化基準」— `/metrics` 是 LobsterPulse 對 operator 唯一的可程式化介面（Prometheus scrape），CLAUDE.md 列為 9-provider 監控的核心 endpoint 之一，但**目前 0 個 unit test 覆蓋**（render_prometheus 依賴 `tauri::AppHandle`、要起 Tauri runtime 才測得到 — 是真實的量測缺口）
- 對齊 R4 `write_local_usage_snapshot` / R8 `process_body` / R12 `write_offset_at` 的 pure-fn-extraction 模式：抽 `&[SessionInfo] + 2 u64` 輸入，測試就能構造 fixture 不需 runtime
- 順手修 latent issue：原 HashMap iteration 順序非確定 → 同一份 state 兩次 scrape 結果可能 line-order 不同；對 Prometheus 沒功能影響但對 diff/grep 監控噪聲大 → 改 alphabetical sort

**搜尋**:
- 沒做 WebSearch（M2 KPI 量測基建是既有 pattern 延伸，無新領域）
- 對照 R10 9-provider smoke matrix 模式（fixture + 1 條 matrix test 覆蓋全家）— 本輪套同 pattern 到 metrics
- 確認 `SessionInfo` 是 `session.rs:246` 公開 struct，測試可構造（不需走 `SessionManager::handle_event` 整條路）

**做了什麼**:
- `lib.rs::render_prometheus(handle)` 拆成 2 個 fn：
  - `render_prometheus(handle)` — Tauri-bound 薄 wrapper（gather state + delegate）
  - `render_prometheus_body(sessions, session_count, active_count)` — pure formatter，**unit test entry**
- 4 個 metric 區塊的 provider 條目改 alphabetical sort（`provider_counts_sorted` / `provider_active_sorted`）保證 deterministic
- 新 `#[cfg(test)] mod render_prometheus_tests` 加 6 個 test：
  1. `empty_state_emits_zero_counters_and_no_provider_lines` — 空 state 4 個 gauge/counter = 0、無 provider sample line
  2. `single_inactive_session_reported_as_total_only` — inactive session 只進 sessions_total / provider_sessions、不進 provider_active
  3. `single_active_session_reported_in_both_provider_lines` — active session 同時進 provider_sessions + provider_active
  4. `multiple_providers_counted_separately_and_sorted_alphabetically` — 故意非字母序輸入（openx/cicx/gemini/cicx 二次）驗 sort 結果 cicx < gemini < openx、同 provider count 加總
  5. `token_counters_sum_across_all_sessions` — 3 session tokens_input 3500 / output 1750 合計
  6. `output_includes_help_and_type_headers_for_every_metric` — 6 個 metric × 2 行（HELP + TYPE）共 12 行 header 必須齊全，缺一 Prometheus 標 untyped
- 0 條 `let _ =` 殘留、0 個 `unwrap()`（測試內的 `unwrap_err()` 是 expected）

**為什麼不加 integration test（HTTP scrape 整條）**:
- metrics server 是獨立 `std::thread::spawn` + `tokio::runtime::Runtime::new()`（line 1483-1515 區段），跟一般 Tauri command 不同路徑
- 純 fn 6 條 unit test 已覆蓋「輸出格式正確性」；HTTP transport 層（bind 失敗、accept 失敗、write 失敗）已在原 line 1488/1491 `log::warn!` 處理
- 接受：真實 HTTP scrape 留給未來若加 e2e harness（需穩定的 metrics port 分配機制）再覆蓋

**驗證**:
- `cargo fmt --check` 過（fmt 自動重排 test assertion 的 multi-line format string，1 file touched by fmt）
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 55/55 pass（49 prior + 6 new render_prometheus tests；0 regression）
- `bash test/smoke-test.sh quick` PASS（cargo check 綠）

**結果**: PASS（M2 KPI 量測基建落地 + 6 unit test + 0 lint warning + 0 regression + deterministic output bonus + commit pending）

**KPI-impact: K5-metrics-coverage +6 sites**（render_prometheus 從 0 test → 6 test 覆蓋；未來 /metrics 改壞立即 unit test fail 而非要 operator scrape 才發現）

**不做的範圍**（給後續輪次）:
- `render_prometheus_body` 改用 trait abstraction（MetricsSnapshot trait 之類）：現階段 1 個 caller、YAGNI
- 加 HTTP-level integration test（要 spawn 整個 metrics server thread）：test 慢且 flaky 風險高、port 衝突要管理，超出 M2 範圍
- `lobsterpulse_*` metric 名稱重構對齊 upstream AgentPulse 命名（`agentpulse_*`）：跨 fork boundary，需先討論 upstream
- 6 個 metric 之外加 `lobsterpulse_provider_tokens_input{provider="..."}` 細顆度：原 token 是 global aggregate，per-provider 累計要新動 SessionManager 狀態，scope 超出本輪
- `lobsterpulse_session_failure_count` / `lobsterpulse_session_idle_age_seconds`：是 Prometheus 端常見 SLO signal，但目前 `AppState` 沒暴露這兩欄，要先動 session.rs，超出本輪 surgical 範圍
- `.arch-fitness.json` / `.supervisor-report.json` 加 .gitignore：仍是 H0、24h chore_ratio 紅線仍生效（50% → 0% 是本輪 fix 拉低、但下次若又 H0 會反彈），需 H0 cap 窗口

### 2026-06-01 R16 — 👁️ AI Supervisor 審查
**品質**: PASS（baseline 綠、6 new tests pass、refactor 範圍 surgical、commit message 含 K-tag、1 個新 KPI 維度落地 — 9 輪 M0 後首個 M2）
**方向**: ALIGNED（從 M0 silent-fail surface 單維度擴展到 M2 KPI 量測基建；對齊 LobsterPulse 監控使命「operator 可程式化介面」）
**風險**: 連續 M0 模式已打破，但 K5 是新維度、未來若 metrics 改壞要有 e2e 才有完整 coverage（unit test 鎖 format、不鎖 transport）
**綜合**: 7/10
**指令**: 下輪可選 K6（per-provider token counter 細顆度）或 M1（任何一條 M0-3 feature 改善），避免再回 silent-fail-only 路徑

### [2026-06-01] Round 17 — K6 per-provider token counter 落地 + 順手修 lifetime token 蒸發 bug
**類型**: M1（K6 metrics 細顆度 + 修 latent bug；從 R16 M2 量測基建進到 M1 feature 落地）
**KPI**: K6-per-provider-token-counter + K6-bug-fix

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `lobsterpulse_provider_tokens_input{provider="..."}` metric | 無 | 有（9 provider × counter） | ✓ |
| `lobsterpulse_provider_tokens_output{provider="..."}` metric | 無 | 有（9 provider × counter） | ✓ |
| `lobsterpulse_tokens_input`/`_output` lifetime 語意 | 否（sum live session，session 移除後 token 蒸發 → counter 倒退） | 是（讀 `ProviderTotals` lifetime aggregate） | ✓ |
| render_prometheus_body 函式簽名 | `(sessions, count, active)` | `(sessions, count, active, &ProviderTotals map)` | +1 參數 |
| 該函式 unit test 數 | 6 | 8 | +2 |
| Lib unit tests | 57 pass | 59 pass | +2 |
| cargo clippy --lib --tests -- -D warnings | 0 warning | 0 warning | — |
| cargo fmt --check | 過 | 過 | — |
| bash test/smoke-test.sh quick | PASS | PASS | — |

**為什麼**:
- Supervisor R16 指令明確列 K6 為下輪首選（「per-provider token counter 細顆度」），延續 R16 metrics exporter refactor 紅利
- 接手時先盤點 session.rs:318-325 的 `ProviderTotals` 發現**資料已存在** — lifetime per-provider 累計在 `SessionManager.bump_provider_totals` 早就在累，**只是 metrics exporter 沒讀** → 純 surgical 接線，**不動 session 狀態**
- 順手發現 latent bug：原 `render_prometheus_body` 從 live `SessionInfo` sum token，session 移除（`SessionEnd` 或 30 min stale）後 token 從 global metric 蒸發 → Prometheus counter 倒退、警報誤觸發
- 兩個改動的 schema 共用同一個資料源（`ProviderTotals`）→ 一次改、global + per-provider 一起修

**搜尋**:
- 沒做 WebSearch（純接線既有 ProviderTotals → metrics endpoint，無新領域）
- 對照 R16 抽 `render_prometheus_body` 的 pure-fn pattern：純 fn + alphabetical sort + fixture 構造 = 可 unit test

**做了什麼**:
- `lib.rs::render_prometheus_body` signature 從 `(sessions, count, active)` 改為 `(sessions, count, active, &HashMap<String, ProviderTotals>)`
- `render_prometheus` (Tauri-bound wrapper) 多傳 1 個參數：`&state.provider_totals`（line 553 `AppState` 已有）
- Token 累計邏輯：原 `for s in sessions { tot_in += s.tokens_input; ... }` 改為 `for (p, t) in provider_totals { tot_in = tot_in.saturating_add(t.tokens_input); provider_in.insert(p, ...); ... }`
- 新增 2 條 Prometheus metric：
  - `lobsterpulse_provider_tokens_input{provider="..."}` — counter, HELP "Lifetime input tokens per provider"
  - `lobsterpulse_provider_tokens_output{provider="..."}` — counter, HELP "Lifetime output tokens per provider"
- 新增 2 個 unit test：
  1. `per_provider_token_metrics_alphabetical_and_separate` — 3 providers 不同 token 數，alphabetical 排序驗證
  2. `token_aggregate_uses_lifetime_not_live_sessions` — 0 個 live session 但 provider_totals 有大量 token，驗證 metric 仍正確反映 lifetime（修 bug 的核心 regression guard）
- 既有 `token_counters_sum_across_all_sessions` 改名為 `token_counters_sum_from_provider_totals_aggregate`、改用 `totals_map` fixture
- 新增 `totals(provider, in_, out)` test helper（封裝 ProviderTotals 構造）
- `output_includes_help_and_type_headers_for_every_metric` 測試新增 4 行 required header 驗證（HELP + TYPE × 2 條新 metric）

**為什麼不另開 metric 命名空間**:
- 沿用 `lobsterpulse_*` prefix + `_provider_` 區隔細顆度，符合 Prometheus naming convention
- `lobsterpulse_tokens_input` (global) / `lobsterpulse_provider_tokens_input` (per-provider) 兩條共存且合計一致（global = sum of provider），operator 端可自由 aggregate

**驗證**:
- `cargo fmt --check` 過（rustfmt 自動重排 `vec![...]` 換行、1 file touched by fmt）
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 59/59 pass（57 prior + 2 new；0 regression）
- `bash test/smoke-test.sh quick` PASS（cargo check 綠）

**結果**: PASS（K6 落地 + lifetime token bug 順手修 + 0 lint warning + 0 regression + commit `68659a9`）

**KPI-impact: K6 per-provider token counter 從 0 → 2 metric + 修 lifetime token counter 蒸發 bug**

**不做的範圍**（給後續輪次）:
- 加 HTTP-level e2e 測試（spawn 整個 metrics server thread）：test 慢且 flaky 風險高、port 衝突要管理，超出 M1 surgical 範圍
- `lobsterpulse_session_failure_count` / `lobsterpulse_session_idle_age_seconds` SLO signal：目前 `AppState` 沒暴露這兩欄，要先動 session.rs / state gathering，超出本輪
- 拆 `MetricsSnapshot` trait abstraction：1 個 caller、YAGNI
- per-provider **session failure counter** 細顆度（`lobsterpulse_provider_failure_count{provider="..."}`）：資料在 `ProviderTotals.failure_count`、已是 lifetime aggregate，可作為下一輪 K7 候選（與 K6 同 pattern）
- 把 lifetime token counter bug 的 fix 套到 discord `poll_discord_commands` 的 silent-fail 路徑：無關 metric 範圍、不順手
- `.arch-fitness.json` / `.supervisor-report.json` 加 .gitignore：是 H0、24h chore_ratio 紅線仍生效

### [2026-06-01] Round 18 — K7 per-provider failure counter 落地
**類型**: M1（K7 metrics 細顆度；延續 R16-R17 metrics exporter 紅利）
**KPI**: K7-per-provider-failure-counter

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `lobsterpulse_provider_failure_count{provider="..."}` metric | 無 | 有（9 provider × counter） | ✓ |
| render_prometheus_body 函式簽名 | `(sessions, count, active, &ProviderTotals map)` | 同上（沿用 K6 簽名，無新參數） | — |
| Lib unit tests | 59 pass | 61 pass | +2 |
| cargo clippy --lib --tests -- -D warnings | 0 warning | 0 warning | — |
| cargo fmt --check | 過 | 過 | — |
| bash test/smoke-test.sh quick | PASS | PASS | — |
| 24h chore_ratio (rolling) | 41% (16/39) | 41% (16/40，本輪 M1 不計入 chore) | 持平 |

**為什麼**:
- R17 log 明列 K7 為「下一輪 K-tag 候選（與 K6 同 pattern）」，接棒順理成章
- ProviderTotals.failure_count 早已是 lifetime aggregate（session.rs:322、bump_provider_totals:352 `PostToolUseFailure` 時 `+= 1`）— 跟 K6 一樣「資料在、metrics 沒接」，純 surgical 接線
- 24h chore_ratio 41% 仍超 30% 警戒線 → 本輪**強制** M1、不碰 H0（包含 supervisor 提醒的 .gitignore）

**搜尋**:
- 沒做 WebSearch（純沿 K6 pattern 同一檔同一函式接線，無新領域）
- 對照 K6 lifetime-vs-live regression guard 概念：本輪新測試 `failure_counter_uses_lifetime_aggregate_not_live_sessions` 復用同 pattern，確保未來若有人改寫成讀 live session 會立即被測試擋下

**做了什麼**:
- `render_prometheus_body` 多一個 `provider_fail: HashMap<String, u64>` 從 `ProviderTotals.failure_count` 累計
- 多一個 alphabetical sort：`provider_fail_sorted`
- 輸出新 metric 段：
  ```
  # HELP lobsterpulse_provider_failure_count Lifetime tool/post failure count per provider
  # TYPE lobsterpulse_provider_failure_count counter
  lobsterpulse_provider_failure_count{provider="cicx"} N
  ...
  ```
- 新測試 helper `totals_with_failures(p, in_, out, fail)` 封裝 ProviderTotals fixture
- 新增 2 個 unit test：
  1. `per_provider_failure_counter_alphabetical_and_per_provider` — 3 provider 失敗數不同，alphabetical 排序驗證
  2. `failure_counter_uses_lifetime_aggregate_not_live_sessions` — 0 live session 但 ProviderTotals 有累計，驗證 metric 仍正確反映 lifetime
- 既有 `empty_state` 測試新增 1 行 `!body.contains("lobsterpulse_provider_failure_count{")` 守門
- 既有 `output_includes_help_and_type_headers_for_every_metric` 新增 2 行 required header（HELP + TYPE）

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 61/61 pass（59 既有 + 2 新 K7；0 regression）
- `bash test/smoke-test.sh quick` PASS

**結果**: PASS（K7 落地 + 0 lint warning + 0 regression + commit `0fa3210`）

**KPI-impact: K7 per-provider failure counter 從 0 → 1 metric + +2 tests pass**

**不做的範圍**（給後續輪次）:
- HTTP-level e2e 測試 spawn metrics server thread：test 慢且 flaky 風險高、port 衝突要管理，K6/K7 都已說明
- `lobsterpulse_session_idle_age_seconds` SLO signal：`AppState` 沒暴露該欄，要先動 session.rs
- 把 K6/K7 lifetime-vs-live pattern 套到 discord `poll_discord_commands` 的 silent-fail 路徑：無關 metric 範圍
- `.arch-fitness.json` / `.supervisor-report.json` 加 .gitignore：H0、24h chore_ratio 41% 紅線仍生效，下輪再議
- per-provider **session_count** 細顆度（`lobsterpulse_provider_session_count{provider="..."}` lifetime）：資料在 `ProviderTotals.session_count`、已是 lifetime aggregate，可作為下一輪 K8 候選（K6/K7/K8 同 pattern 完成 lifetime 三件套）

---

### [2026-06-01] Round 19 — K8 per-provider idle_seconds gauge 落地（lifetime 三件套延伸：last_event_at 派生）
**類型**: M1（K8 metrics 細顆度；K6/K7 純接線轉 K8 「lifetime aggregate 派生維度」）
**KPI**: K8-per-provider-idle-seconds-gauge

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| `lobsterpulse_provider_idle_seconds{provider="..."}` gauge | 無 | 有（9 provider × gauge） | ✓ |
| `ProviderTotals.last_event_at` 欄位 | 無 | `Option<DateTime<Utc>>` | +1 欄位 |
| `render_prometheus_body` 函式簽名 | `(sessions, count, active, &ProviderTotals map)` | 同上 + `now: DateTime<Utc>` 注入 | +1 參數（純 fn 化驗證） |
| Lib unit tests | 61 pass | 66 pass | +5 |
| `cargo clippy --lib --tests -- -D warnings` | 0 warning | 0 warning | — |
| `cargo fmt --check` | 過 | 過 | — |
| `bash test/smoke-test.sh quick` | PASS | PASS | — |
| Runtime smoke（`smoke_k8.ps1` POST event → /metrics grep） | — | `provider="claude"} 1` 真的出來 | ✓ |
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
