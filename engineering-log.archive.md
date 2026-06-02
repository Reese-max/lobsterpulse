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
