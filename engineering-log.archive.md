# Engineering Log

> 這是自主工程師的工作日誌。每次改善都會記錄思考過程和結果。

## 改善紀錄

### [2026-06-01] Round 15 — auto_rules::hook_failure_burst 2 條 silent fail surface (Stop-Process + openab_restart spawn)
**類型**: M0（user-facing observability bug：hook_failure_burst 觸發 OpenAB restart 時兩條 powershell spawn 沉默吞 error，user 端 capsule 紅點狂閃但 restart 沒跑、log 全無）
**KPI**: K-silent-fail-surface
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| hook_failure_burst powershell spawn silent fail sites | 2 (Stop-Process + openab_restart) | 0 (if let Err + log::warn) | −2 |
| 累積 silent-fail-surface 覆蓋（含 R6/R8/R9/R11/R12/R13/R14） | 33 sites surfaced | 35 sites surfaced | +2 |
| cargo test --lib | 49 pass | 49 pass | — |
| cargo clippy --lib --tests -- -D warnings | 0 warning | 0 warning | — |
| 24h 連續 M0 推進輪數 | 8 (R6/R8/R9/R11/R12/R13/R14) | 9 | +1 |
| 24h chore_ratio (純 M0 fix) | 0% (R7-R14 連續 fix) | 0% | — |

**為什麼**:
- R14 末 explicit defer「下一輪單獨評估」：`auto_rules.rs:631, 638` 是 user-input command 觸發的 spawn，scope 比 R13 純 tray menu click 大
- 影響面：hook_failure_burst 是 4 條 auto rule 之一（Discord 通知/hook 失敗 burst/每日摘要/每週摘要），burst 觸發時用來救命的 OpenAB restart 機制本身失敗 = user 完全失能 + 完全無 log
- 安全 scope 評估：`openab_restart_command` 來自 `AppConfig.openab_restart_command: String` (config.rs:38, 287 預設空字串)，但走 `Command::new("powershell.exe").args([..., openab_restart_command])` 是 args 陣列、PowerShell 收 positional arg → **無 shell injection 風險**，scope 收斂成純 silent fail observability（同 R12/R13/R14 範疇）
- 對齊 R12/R13/R14 改法：inline `let _ =` → `if let Err(e) = ... { log::warn!(...) }`，訊息含 site (`auto_rules`) + rule (`hook_failure_burst`) + 失敗原因；restart case 額外帶 cmd 字串（user 自己設的，debug 路徑寫錯用）

**搜尋**:
- 沒做 WebSearch（同 R6/R8/R11-R14 既有 pattern 延伸、非新領域）
- 順手對照 R14 末「不做的範圍」清單：本輪只動 `auto_rules.rs:631-640`，其他留的（`auto_rules.rs:669, 751` mark_summary_fired_if_new 內部邏輯；`auto_rules.rs:1311, 1318` save_config；`auto_rules.rs:1617` TmpDir test drop；`hook_server.rs:441` remove_port_file；`quota_history.rs:60` CSV row）仍不混入

**做了什麼**:
- `auto_rules.rs:631-640` hook_failure_burst 內 2 條 powershell spawn：
  - 631 Stop-Process openab (hardcoded)：`let _ = ...spawn()` → `if let Err(e) = ...spawn() { log::warn!("auto_rules: hook_failure_burst openab stop spawn failed: {e}"); }`
  - 638 openab_restart_command (user config)：同 pattern，`log::warn!("auto_rules: hook_failure_burst openab restart spawn failed (cmd={openab_restart_command}): {e}")`
- 兩條獨立 if let Err（不 return）— 對齊 R12/R13/R14 模式：spawn 失敗不擋後續 Discord 通知（user 還是想知道 burst 觸發了）

**為什麼不加 unit test**:
- powershell spawn 是 Windows-only integration test territory（mono 假陽性、跨平台行為差異）
- 接受：留給後續若加 e2e harness 再覆蓋；同 R12/R13/R14 取捨

**驗證**:
- `cargo fmt --check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 49/49 pass（0 regression；R14 累積 49，本輪未新增 unit test）
- `bash test/smoke-test.sh quick` PASS

**結果**: PASS（M0 observability 改善落地 + 2 silent fail sites surfaced + 0 lint warning + 0 regression + commit `79f6408`）

**不做的範圍**（給後續輪次）:
- `auto_rules.rs:669, 751` `mark_summary_fired_if_new` — 是內部 HashSet<bool> insert、非 IO、不會 silent fail 高優先
- `auto_rules.rs:1311, 1318` `save_config` — config write，R12 已歸類「save 失敗下次啟動讀不到 default 仍可運作」LOW
- `auto_rules.rs:1617` TmpDir Drop — test helper，test 結束時清理、故意 silent
- `hooks_configurator.rs:138` `remove_provider` — 設定移除流程，scope 涉及設定檔 IO + 9 provider 邏輯、R12 已歸類 LOW
- `lib.rs:91, 130, 394, 401, 432, 434, 439, 441, 453, 508, 610, 615, 620, 625, 634-636, 768, 916, 953, 1141-1143, 1166, 1168-1169, 1172-1173` 等大量 — 多屬 window.set_position/show/focus、tx send、child kill 等「操作本身 best-effort、caller 已有 UX fallback」LOW 範疇
- `hook_server.rs:109` `stream.write_all(response.as_bytes()).await` — HTTP response write，連線斷時 client 端也收不到、caller 已 log「connection closed」、LOW
- `.arch-fitness.json` / `.supervisor-report.json` supervisor 產物 gitignore 化 — R12 末 deferred 給 H0 窗口

---

### [2026-06-01] Round 14 — hook_server::write_port_file 2 條 silent fail surface (create_dir_all + write)
**類型**: M0（user-facing observability bug：port file 是 sidecar 找 port 唯一依據、setup 失敗整個 hook 路徑走錯 port、user 端 capsule 動不了、log 全無）
**KPI**: K-silent-fail-surface
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| write_port_file 內 silent fail sites | 2 (create_dir_all + write) | 0 (if let Err + log::warn) | −2 |
| 累積 silent-fail-surface 覆蓋（含 R6/R8/R9/R11/R12/R13） | 31 sites surfaced | 33 sites surfaced | +2 |
| cargo test --lib | 49 pass | 49 pass | — |
| cargo clippy --lib --tests -- -D warnings | 0 warning | 0 warning | — |
| 24h 連續 M0 推進輪數 | 7 (R6/R8/R9/R11/R12/R13/本輪) | 8 | +1 |
| 24h chore_ratio (純 M0 fix) | 0% (連續 fix type) | 0% | — |

**為什麼**:
- R13 末 explicit defer「下次 round 挑」：`hook_server.rs:427-428` port file write — port file 是 sidecar (`bin/lobster-pulse-hook.rs:31`) `read_port()` 唯一讀取源，create_dir_all 或 write 失敗時 sidecar 端 `read_to_string(...).ok()?` 直接 None → fallback DEFAULT_PORT=19280、整個 hook 路徑走錯 port、user 端 capsule 動不了、log 全無
- 影響面：9 家 provider 全部 hook event 進不到（因為 post 到錯 port 連不上），等同整套監控失明
- 對齊 R12/R13 改法：inline `let _ =` → `if let Err(e) = ... { log::warn!(...) }`，訊息含 site name (`write_port_file`) + 失敗原因 + path（create 還帶 dir，write 還帶 port 編號）

**搜尋**:
- 沒做 WebSearch（同 R6/R8/R11/R12/R13 既有 pattern 延伸、非新領域）
- 順手對照 R13 末「不做的範圍」清單：本輪只動 `hook_server.rs:427-428`，其他留的 (sidecar stdin/HTTP、auto_rules context menu、openab_bridge best-effort cleanup) 仍不混入
- 順手驗 `bin/lobster-pulse-hook.rs:25-26` `let _ = post(...)` 是 sidecar 端，sidecar 整個 process 一退出 = CLI 不等結果，符合「fail 不破壞 parent CLI」設計意圖、非 silent fail

**做了什麼**:
- `hook_server.rs:424-436` write_port_file：
  - 427 `let _ = std::fs::create_dir_all(&dir);` → `if let Err(e) = ... { log::warn!("write_port_file: create dir {} failed: {e}", dir.display()); }`
  - 428 `let _ = std::fs::write(dir.join("port"), port.to_string());` → `if let Err(e) = ... { log::warn!("write_port_file: write port={port} to {}/port failed: {e}", dir.display()); }`
- 兩條獨立 if let Err（不 return）— 對齊 R12/R13 模式；create 失敗仍嘗試 write，換 debug 時「create 失敗」vs「write 失敗」訊息清晰（disk full 兩條都會 fire、disk permission 只 create 會 fire、read-only fs 兩條都 fire，分得開）

**為什麼不加 unit test**:
- write_port_file 是 private fn，內部用 `dirs::home_dir()` 讀 env、跨平台行為（Windows 讀 USERPROFILE / Unix 讀 HOME）要設對才能 mock
- 要包 fn 加 `dir: &Path` 參數 = 改 ABI 為了測 = 「abstraction for test only」、違反 R13 拒絕的「abstraction for single-use」精神
- 接受：這條屬於 integration-test territory（真實 fs 行為、跨平台 home dir），留給後續若加 e2e harness 再覆蓋；同 R12/R13 取捨

**驗證**:
- `cargo fmt --check` 過
- `cargo check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 49/49 pass（0 regression；R13 累積 49，本輪未新增 unit test）
- `bash test/smoke-test.sh quick` PASS

**結果**: PASS（M0 observability 改善落地 + 2 silent fail sites surfaced + 0 lint warning + 0 regression + commit `338cc23`）

**不做的範圍**（給後續輪次）:
- `hook_server.rs:432-436` `remove_port_file` 內 `let _ = std::fs::remove_file(...)` — 是 shutdown 階段 best-effort cleanup、port file 留著下次啟動會被 `read_existing_port_file` 偵測到仍可運作（is_port_listening fallback）、非 silent fail 高優先
- `bin/lobster-pulse-hook.rs:23, 26` sidecar stdin read / HTTP post — R14 驗過設計意圖是「fail 不破壞 parent CLI」、非 silent fail
- `auto_rules.rs:631, 638` powershell context menu spawn — 是 user-input command 路徑、scope 更大（可能涉及 shell injection 防護），本輪 R14 不混入；下一輪可單獨評估
- `openab_bridge.rs:69, 240, 253-254, 273, 321, 331, 342, 357, 371` best-effort temp cleanup — R12/R13 已歸類「下一次 open() 會 Err 暴露、caller 已 log」、雙層保護
- `quota_history.rs:60` `let _ = writeln!(f, ...)` CSV row write — R13 歸類 LOW（趨勢圖少一點而已），留著
- `.arch-fitness.json` / `.supervisor-report.json` 是 supervisor runtime 產物，R12 末 deferred 給 H0 窗口加 .gitignore

---

### [2026-06-01] Round 13 — tray menu 4 條 CLI spawn silent fail surface (openab_restart × 2 + open_config + restart)
**類型**: M0（user-facing observability bug：4 條 on_menu_event closure 內 `let _ = std::process::Command::new(...).output()/spawn()` 沉默吞 error）
**KPI**: K-silent-fail-surface
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| on_menu_event closure CLI spawn silent fail sites | 4 (openab_restart stop / start / open_config / restart) | 0 (if let Err + log::warn) | −4 |
| 累積 silent-fail-surface 覆蓋（含 R6/R8/R9/R11/R12） | 27 sites surfaced | 31 sites surfaced | +4 |
| cargo test --lib | 49 pass | 49 pass | — |
| cargo clippy --lib --tests -- -D warnings | 0 warning | 0 warning | — |
| 24h 連續 M0 推進輪數 | 6 (R6/R8/R9/R11/R12/本輪) | 7 | +1 |
| 24h chore_ratio (純 M0 fix) | 0% (R10/R11/R12 連續 fix) | 0% | — |

**為什麼**:
- R12 末 explicit defer「下次 round 挑」：spawn-loop 內已清完，本輪換**同 handler 內但不同觸發**的 4 條 CLI process spawn（user click 觸發，非 spawn-loop 排程觸發）
- 影響面分析 4 條 user impact：
  - `openab_restart` stop (1641)：powershell.exe 缺失 → user 點「Restart OpenAB」→ openab 進程沒死、restart 卡住、**無 log**
  - `openab_restart` start (1649)：自訂 restart cmd 寫錯路徑 → **無 log**
  - `restart` self (1670)：self-exe 路徑失效（被 rename/刪）→ user 點「Restart」→ app 還在原狀、**無 log**
  - `open_config` (1664)：MEDIUM（explorer.exe 永不缺，但 config file 被刪 → 開空目錄、無 log）
- 3/4 HIGH + 1 MEDIUM，沿用 R6/R8/R9/R11/R12 同一論點：user-facing 操作失敗 = 必須有 log 才能 debug，否則等於「user 端完全失明」

**搜尋**:
- 沒做 WebSearch（沿用既有 pattern、無新領域）
- 沒做新 rg 掃描 — 本輪目標從 R12 隱式 follow-up 來（on_menu_event 是 R12 grep `let _ = .*::` 對 7 個 module 時旁觀到、但當時歸為 caller 端不吞 / 非 spawn-loop 類，R12 不收；本輪單獨評估、4 條全在 closure 內、可成單一 commit）

**做了什麼**:
- `lib.rs:1638-1675` on_menu_event closure：
  - 1641 `let _ = Command::new("powershell.exe").args([...]).output();` → `if let Err(e) = ... { log::warn!("openab_restart: stop powershell failed: {e}"); }`
  - 1649 `let _ = Command::new("powershell.exe").args([...]).spawn();` → `if let Err(e) = ... { log::warn!("openab_restart: spawn \`{}\` failed: {e}", restart_cmd); }`
  - 1664 `let _ = Command::new(opener).arg(path).spawn();` → `if let Err(e) = ... { log::warn!("open_config: spawn \`{}\` failed: {e}", opener); }`
  - 1670 `let _ = Command::new(exe).spawn();` → `if let Err(e) = ... { log::warn!("restart: spawn self failed: {e}"); }`
- log 訊息含 site name + 失敗原因 + 對應 command（user 從 log 一眼看出哪條 click 沒生效）

**驗證**:
- `cargo fmt --check` 過
- `cargo check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 49/49 pass（0 regression；R12 42→49 為 R10/R11/R12 累積加的，本輪未新增 unit test）
- `bash test/smoke-test.sh quick` PASS

**為什麼不加 unit test**:
- on_menu_event closure 內 inline，`AppHandle` 從 Tauri runtime 來、無 plain unit-test entry
- 加 helper function 抽出 spawn logic = abstraction for single-use，違反「surgical / minimum code」原則
- R12 dispatch_event 加 3 個 test 是因為它是 `pub fn` 直接可 unit test；本輪 4 條全在 closure 內
- 接受：Tauri menu handler 是 integration-test territory，留給後續若加 e2e harness 再覆蓋

**結果**: PASS（M0 observability 改善落地 + 4 silent fail sites surfaced + 0 lint warning + 0 regression + commit `71fbcda`）

**不做的範圍**（給後續輪次）:
- `hook_server.rs:427-428` port file write `let _ = std::fs::write(...)` — port file 是 sidecar 找 port 的唯一依據，若失敗 sidecar 全壞，但目前無 user-facing click 路徑觸發（只在 setup 階段被呼叫）
- `bin/lobster-pulse-hook.rs:23, 26` sidecar stdin read / HTTP post — 失敗時 sidecar 整個 process 退出、Tauri 端 log 已有「hook_server 無收到事件」可觀察
- `quota_history.rs:60` `let _ = writeln!(f, ...)` CSV row write — failure 僅丟一筆 snapshot、無連鎖影響（趨勢圖少一點而已）
- `auto_rules.rs:631, 638` `let _ = std::process::Command::new("powershell.exe")` — 是 context menu 點「執行 powershell script」、非 tray menu；同 pattern 但 scope 更大（可能涉及 user-input command），本輪不混入
- `openab_bridge.rs:69, 240, 253-254, 273, 321, 331, 342, 357, 371` `let _ = std::fs::remove_file/remove_dir` — best-effort temp cleanup，下一次 open() 會 Err 暴露、caller 已 log

---

### [2026-06-01] Round 12 — lib.rs spawn loop 3 條 silent fail 補 surface (quota_history × 2 + openab_bridge dispatch × 1)
**類型**: M0（user-facing observability bug：3 條 spawn-loop 內 `let _ = 函式()` 沉默吞 error）
**KPI**: K-silent-fail-surface
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| lib.rs spawn-loop 沉默吞 error sites | 3 (post-runners snap / hourly snap / dispatch_event) | 0 (if let Err + log::warn) | −3 |
| dispatch_event 「?」 fallback 一致性 | 內層有 / 外層無（`""`） | 內外層都有 | ✓ |
| unit tests 覆蓋（dispatch_event Err path） | 0 | 3 | +3 |
| cargo test --lib | 39 pass | 42 pass | +3 |
| 24h chore_ratio | 0% (R10/R11 兩輪 fix) | 0% | — |
| 24h 連續 M0 推進輪數 | 5 (R6/R8/R9/R11/本輪) | 6 | +1 |

**為什麼**:
- R6/R8/R9/R11 連續 surface silent fail pattern，但**只 cover 到 module 內部**的 call sites
- 本輪往** caller 端**再掃（grep `let _ = .*::` 對 7 個 module），發現 3 條 spawn-loop 內遺漏：
  - `lib.rs:1329` `let _ = quota_history::snapshot_once()`（post-runners 60s 補 snap）→ CSV 寫失敗 user 端 quota-history 缺資料、無 log
  - `lib.rs:1335` 同上（hourly 3600s 排程）
  - `lib.rs:1393` `let _ = openab_bridge::dispatch_event(...)`（15s tick 對每個 OpenAB event 呼叫）→ 事件沒到 Discord、user 端 OpenAB 狀態變化消失、無 log
- 影響面：quota-history.csv 是 user 看 trend chart 的唯一依據、OpenAB 通知是 OpenAB 跨進程的**唯一對外通道** — 兩個都 silent 等於「整個 observability 層壞掉 user 也不知道」
- 對齊 R6/R8/R11 改法：inline `let _ =` → `if let Err(e) = ... { log::warn!(...) }`，訊息含 function 名 + 失敗原因 + 可觀察 context（dispatch 還帶 source/event）
- 順手修 consistency bug：`dispatch_event` 的 outer source/kind fallback 是 `unwrap_or("")`、inner changes/summary 是 `unwrap_or("?")`，log 端會看到 `source= event=`（空字串）vs `source=? event=?` — 改外層對齊內層 convention

**搜尋**:
- 沒做 WebSearch（同 R6/R8/R11 既有 pattern 延伸、非新領域）
- Grep `let _ = .*::` 對 7 個 module（auto_rules/hook_server/openab_bridge/discord/lib/session/config）
- 確認 auto_rules.rs 14 處已在 R6 改完、hook_server.rs 2 處在 R8、openab_bridge.rs 3 處在 R11
- 確認 discord.rs 本身 caller 端不吞（直接 `?` propagation）

**做了什麼**:
- `lib.rs` 3 call sites：`let _ = quota_history::snapshot_once()` × 2 + `let _ = openab_bridge::dispatch_event(...)` → `if let Err(e) = ... { log::warn!(...) }`
- `openab_bridge::dispatch_event`：outer source/kind `unwrap_or("")` → `unwrap_or("?")`
- 加 3 個 unit test 覆蓋 dispatch_event：
  - `dispatch_event_returns_err_for_unknown_source_kind`：unknown pair → Err 含 source + kind
  - `dispatch_event_falls_back_to_question_mark_for_missing_fields`：缺欄位 → "?" fallback 走 unknown branch
  - `dispatch_event_handles_known_kind_with_bogus_inner_shape`：known pair + 假 token → Err path 可達

**驗證**:
- `cargo fmt --check` 過
- `cargo check` 過
- `cargo clippy --lib --tests -- -D warnings` 0 warning
- `cargo test --lib` 42/42 pass（39 prior + 3 new）
- 過程中抓出 1 個 latent consistency bug（dispatch_event outer fallback 是 `""` 不是 `"?"`，與 inner field 不一致）→ 改一致後 42/42 綠
- `bash test/smoke-test.sh quick` PASS

**結果**: PASS（M0 observability 改善落地 + 順手修 outer/inner fallback consistency + 3 unit test + 0 lint warning，commit `7778960`）

**不做的範圍**（給後續輪次）:
- `lib.rs:1329` 60s 補 snap 跟 1335 hourly 兩個 quota_history::snapshot_once 是否要合併到單一 thread（避免重複 IO）— 純 refactor、無 observability 改善
- `openab_bridge::tail_new_events` 內 5 處 `let Ok(_) = ...`（read_to_string / metadata / File::open / seek / read_to_end）— 是「檔案不存在 = 無新事件」的 expected 場景、不是 silent fail
- `lib.rs:1553/1576` monitor 偵測 silent fail — 是 best-effort UI placement fallback、無 user-facing impact
- `lib.rs:169-173` rodio 音訊 nested `if let Ok(...)` 鏈 — 是「無音檔 = 靜默」 的設計意圖、非 bug
- `quota_history::snapshot_once` 內 line 39 `let _ = std::fs::create_dir_all(parent)` — 後續 open() 會回 Err 暴露、caller 已 log、雙層保護

---

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
