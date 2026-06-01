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
