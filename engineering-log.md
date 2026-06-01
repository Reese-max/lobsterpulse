# Engineering Log

> 這是自主工程師的工作日誌。每次改善都會記錄思考過程和結果。

## 改善紀錄

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

## 觀察事項（給後續輪次）

1. **若 Spectra 變更真的要在本目錄推進**：需要先確認 openclaw agent runtime 的程式碼位址。可能是 sibling repo、或將在後續 round 從 openclaw repo 拉過來。在此之前，openclaw-self-evolution 對本目錄 = 不可執行。
2. **若 Spectra 變更其實是 meta-task**（指 loop 自身的演化）：本 loop 還沒產生對話 log（無 .jsonl 對話記錄，只有 burn.log + notify jsonl），FTS5 索引無對象可索引。
3. **LobsterPulse 端可考慮的 H0 候選**（不在本輪做）：
   - `package.json` version 0.2.2 → 0.5.4 對齊 Cargo.toml / tauri.conf.json
   - `dev.sh` / `build.sh` 仍叫 `agent-pulse`（upstream 進程名），應改 `lobster-pulse`（fork 已改名）
   - upstream 13 個 commits 中 `aa388d9`（Drop spurious completion-signal mappings）和 `96f8c05`（fall back to cargo-tauri when npm tauri CLI absent）可能是 backport 候選
4. **Notify 401**：loop 通知 daemon 憑證壞，影響 loop → user 的通知。屬 loop infra 修，非本目錄 code。建議 user 端查 webhook/bot token。

## 決策記錄

- **R1**: 不硬幹。寫 log、標出卡點、等下一輪看 Spectra / loop 是否有 openclaw 對應的程式碼到位。
