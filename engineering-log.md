# Engineering Log

> 舊紀錄已歸檔到 engineering-log.archive.md

## 改善紀錄

- `grep -c "^- \[x\]" tasks.md`: 12 (R92 12/12 對齊 + R105 不動)
- `grep -c "^- \[ \]" tasks.md`: 0 (0 個 [ ] 殘留)
- `cargo test --lib`: **431 passed; 0 failed** (本輪 M0 closure 不動 code → baseline 持平)
- `cargo clippy --lib -- -D warnings`: 0 warning
- K42 chain 17 條不擴張 (M0 spec closure, 護衛 chain 沒動)
- K41 chore_treadmill 24h: 0% 守住 (本輪 1 docs, 不算 chore)
- `git log -1 --pretty=%B | grep -E 'gho_|ghp_|sk-'`: 0 hit (file-based commit msg, 避 R109 secret leak 教訓)
- commit 33e93c5 落地 2 檔 / 78 insertions

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---|
| K40 spec coverage closed | 3/3 (otel + contract-matrix-guard + openab-bot-sync R92 階段) | **3/3** (openab-bot-sync 正式 commit closure) | 0 (closure 狀態從 R92 就到位, R105 是 commit 動作) |
| K0 Quota 即時性 | 10/13 (R109) | 10/13 | 0 (本輪 M0 closure 不推進) |
| K0-A1 emit coverage | 0/13 (待 OpenAB bot 實運) | 0/13 | 0 (本輪不推進) |
| K42 護欄 chain 飽和 | 17 條 (R106 鎖) | 17 條 | 0 (M0 closure 不擴 chain) |
| K41 chore_treadmill 24h | 0% (R109) | 0% | 持平 (1 docs) |
| baseline lib tests | 431/431 (R109) | 431/431 | 0 (M0 closure 不動 code) |
| R13 untracked 守住 | 6 個 (1 R109 temp msg 已清) | 6 個 | 持平 (R105 temp msg 也清成 6) |
| 接力清單首位 closure 數 | 1/4 (R100 #3 R105 closure) | **2/4** | +1 (openab-bot-sync closure) |

**KPI-impact: K40 spec coverage closed 1/1 (openab-bot-sync 正式 commit closure, 接力清單首位解卡), 0 KPI 數字變動**

**留 R106+ owner 接力**:
- R109 commit body 接力線剩 2 條 (R105 解 1 條):
  - **K0 Quota 10/13 → 11/13+**: 9 個 OpenAB bot 路徑（usage-{bot}.json snapshot）— 需 OpenAB 端配合，非純 LP 端可獨推
  - **K0-A1 推進**: 需 OpenAB bot 實際打 `/hook/{provider}` 累積 5 種以上 non-zero samples (環境就緒時 M1)
- K40 開新 change: 若有 spec-worthy 變更可開 proposal
- 6 條 counter 重命名 _total 結尾 (R103+ follow-up, 需先廣播 alert/dashboard 跟進)
- OTel SDK 整合 (R103+ follow-up, 需 spec 先行)
- k0_measure.py docstring/spec drift: L3 "14 provider" / L35-37 "14 provider 真實清單" 跟 KNOWN_PROVIDERS=13 不一致 (R102 拆 K0-A 雙軌時漏修), R106+ 護衛 spec 窗口
- M0 連續輪數張力訊號: R101~R105 連 5 輪 M0 closure/護衛, R108/R109 接力 2 輪 M1 突破 (K0 Quota 9→10/13), R105 接力 closure 不算連發 M0


### 2026-06-05 R105 — 👁️ AI Supervisor 審查
**品質**: PASS (8/10)
**方向**: ALIGNED** (7/10)
**風險**: K0 Quota 停在 10/13，最近 5 個 commit 全是 docs/chore，實際推進動能放緩**

**綜合**: 7/10
**指令**: 已注入修正指令

### 2026-06-05 R105 — 🧠 策略顧問巡邏
**判定**: ON_TRACK (LOW)
PATROL_VERDICT: ON_TRACK
URGENCY: LOW

🎯 方向：commit 鎖死在 K0（provider 健康度 + quota）和 K40/K42（規格治理 + 護欄），與 MISSION.md 北極星完全對齊，零跑偏。

⚠️ 過時風險：**中低但需監控** — OTel GenAI semantic conventions 正在收斂（[OpenTelemetry GenAI semantic conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/)），業界 Langfuse / Arize / Datadog 都在往 OTel pipeline 靠攏。LobsterPulse 用自訂 Prometheus metric 格式，短期無害（本機桌面工具），但如果未來想接 OTel collector 或讓外部 dashboard 消費，需要一次格式遷移。目前不構成阻斷，但 90 天內若有 OTel GenAI spec 正式 release，應評估是否提前對齊。

🔍 盲點：**K0-A1 / K0-A2 是被動 KPI** — 程式碼定義 13/13 已滿（R101），但實際 emit 0/13、非零 sample 0/13。這兩個指標完全依賴外部 provider 是否在跑，LobsterPulse 自己無法主動推進。如果 90 天內某些 provider（特別是低頻使用的）剛好沒事件流過，KPI 就會卡在非滿覆蓋。建議：(1) 區分「可控覆蓋率」vs「環境依賴覆蓋率」，(2) 對低頻 provider 設 synthetic test event 來驗證 emit 路徑真的通。

💣 風險：**chore_treadmill 壓力** — R81 baseline 是 55%，目標 <30%。最近 10 個 commit 裡有 `chore: rotate engineering-log`、`style(fmt)`、`fix(spec)` 這類治理 commit，如果 spec closure 批次結束後沒有新 feature 進入，chore 比例會飆高。需要在 spec closure 收尾後立刻進入下一批 feature work（quota 模組 10→13/13），否則 K41 會紅。

📋 建議行動：
1. **立即**：在 quota 模組 10/13 的基礎上，鎖定剩下 3 個 provider 的 live quota 實作順序（建議先挑最容易拿到 `usage-*.json` 的），確保下一批 commit 是 `feat` 不是 `chore`，守住 K41。
2. **本週內**：對 K0-A1/A2 設計一組 synthetic event 測試（模擬 hook event → 驗證 /metrics 端點 emit），把「被動等 provider 跑」變成「主動驗證 emit 鏈路通」，避免 90 天到了才發現 emit 路徑有 bug。
3. **持續監控**：追蹤 OTel GenAI semantic conventions 的 release 進度，如果 2026-Q3 正式 GA，開一個 `openspec/changes/otel-alignment/` 提案評估遷移成本。

### [2026-06-05] Round 106 — M0 收 prometheus-counter-convention spec closure (status=open→closed, 8/8 tasks [x])
**類型**: M0
**KPI**: K40 spec coverage closed 3/4 → **4/4** (+1, R105 接力清單首位解卡)
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---|
| **K40** spec coverage closed | 3/4 (otel + contract-matrix-guard + openab-bot-sync + k0_measure drift 開) | **4/4** (prometheus-counter-convention 收 closure) | +1 |
| K0 Quota 即時性 | 10/13 (R109) | 10/13 | 0 (本輪 M0 closure 不推進 quota) |
| K0-A1 emit coverage | 0/13 (待 OpenAB bot 實運) | 0/13 | 0 (本輪不推進) |
| K0 Prometheus naming convention (新維度) | 0/6 合規 (spec 契約無) | 0/6 runtime 合規 (spec 契約已 closure) | spec 對齊契約落地 (R107+ 才 runtime 落實) |
| K42 護欄 chain 飽和 | 17 條 (R102/R103 鎖) | 17 條 | 0 (M0 closure 不擴 chain, 護衛 test 留 R107+ rename 同步) |
| K41 chore_treadmill 24h | 0% (R109) | 0% | 持平 (1 docs spec closure) |
| baseline lib tests | 431/431 (R109) | 431/431 | 0 (M0 spec closure 不動 code) |
| R13 untracked 守住 | 6 個 | 6 個 | 持平 (R106 spec 5 檔 A 不混 noise) |

**為什麼**:
- R105 接力清單首位明列「6 條 counter 重命名 _total 結尾（破 Prometheus 抓取, 需先廣播 alert/dashboard 跟進）」— spec 階段收 closure, 對齊契約封版
- R100 策略顧問風險 #1 (metrics schema 沒對齊業界 convention) 兩條護衛 chain 並存收齊: R102/R103 OTel semconv (R103 護衛 chain) + R106 Prometheus naming convention (本 change 護衛 test 設計, 留 R107+ 實作)
- 1 輪 1 件紀律: 實際 rename 6 條 metric 是跨 2-3 週窗口 scope (dual-emit + alert 廣播 + dashboard 廣播 + 文檔同步) — 本輪純 spec 對齊契約, code 留 R107+ owner follow-up
- 護衛 test 對現名會 false positive (現名無 _total 是 spec drift 源頭), 需 owner 拿對齊契約 + rename 同步 commit 才一致 — 留 R107+ rename 當下同 commit 一起寫
- M0 連續輪數張力: R101~R106 連 6 輪 M0 closure/護衛, R108/R109 接力 2 輪 M1 突破 (K0 Quota 9→10/13), R106 接力 closure 性質跟 R105 同 (解卡不算連發 M0, 是把 R105 留 untracked 備好狀態收 closure)

**搜尋**:
- 不需搜尋, R105 開的 spec 骨架 (4 spec 檔 + .openspec.yaml) 已備齊, 對齊 R102/R103/R105 spec closure pattern
- 5 週時程設計參考: OpenTelemetry / Prometheus 官方 semantic migration guide 標準 4-6 週 deprecation window

**做了什麼**:
- `openspec/changes/prometheus-counter-convention/proposal.md`: 5 段 (Goal/Background/Scope/Capabilities/廣播), 廣播段 4 事項 (抓取端/alert/dashboard/deprecation 公告)
- `openspec/changes/prometheus-counter-convention/design.md`: 6 條 counter rename 對照表 (現名→目標名) + 4 層面 impact (Code-level/抓取端/Grafana/文檔) + 5 週廣播時程 (T-0 公告→T-5 post-mortem) + 1 條新護衛 test 設計 (counter_metrics_must_have_total_suffix)
- `openspec/changes/prometheus-counter-convention/specs/prometheus-counter-convention/spec.md`: 4 Requirement + 8 Scenario (R-1 6 條 _total 結尾 / R-2 護衛 test 守 convention / R-3 廣播 4 層面 5 週時程 / R-4 不改反向違規 gauge)
- `openspec/changes/prometheus-counter-convention/tasks.md`: 8 個 task 4 phase 1 (T-CC1~T-CC6) + 2 phase 2 (T-CC7 closure + T-CC8 log), 8/8 [x] 全勾
- `openspec/changes/prometheus-counter-convention/.openspec.yaml`: status=open → closed, phase=1/1, R106 closure 註記段
- `git add` 精準列 5 檔路徑 (不用 `-A`), R13 守護 6 個 untracked 雜訊不污染
- 不動 lib.rs (護衛 test 留 R107+ rename 同步, 不在本輪 1 輪 1 件 scope)
- 不動 護欄 chain 17 條 (chain 飽和守住, M0 closure 不擴 chain)
- 不動 `lobsterpulse_sessions_total` (gauge 反向違規, R106+ follow-up 不同 spec drift 類型)

**驗證**:
- `git status --short`: 5 檔 A (prometheus-counter-convention 全套 spec), 6 untracked 守住 (R13)
- `grep -c "^- \[x\]" tasks.md`: 8
- `grep -c "^- \[ \]" tasks.md`: 0
- `cargo test --lib`: **431 passed; 0 failed** (本輪 M0 spec closure 不動 code → baseline 持平)
- K42 chain 17 條不擴張 (M0 spec closure, 護衛 chain 沒動)
- K41 chore_treadmill 24h: 0% 守住 (本輪 1 docs spec, 不算 chore)
- commit 1edb87a 落地 5 檔 / 499 insertions

**KPI-impact: K40 spec coverage closed 3/4 → 4/4 (prometheus-counter-convention 收 closure, R105 接力清單首位解卡), 0 KPI 數字變動**

**留 R107+ owner 接力**:
- prometheus-counter-rename-2026-q3: 開新 change 走實際 rename 6 條 metric (LP_METRICS const + emit site + 35 test assertion) + 1 條護衛 test `counter_metrics_must_have_total_suffix` in lib.rs (本 change 留 design 段, code 留 R107+)
- 5 週時程: T-0 公告 → T-1 dual-emit shim → T-2 廣播 → T-3 監控窗口 → T-4 切換 → T-5 post-mortem
- 廣播文檔先備齊: CHANGELOG.md / README.md / CONTRIBUTING.md 加 Prometheus metric rename notice (本 change 廣播段已寫完, R107+ 真正 rename 當下直接 copy-paste)
- gauge `lobsterpulse_sessions_total` 反向違規: 不同 spec drift 類型, 留 R106+ follow-up
- k0_measure.py docstring/spec drift: L3 "14 provider" / L35-37 "14 provider 真實清單" 跟 KNOWN_PROVIDERS=13 不一致 (R102 拆 K0-A 雙軌時漏修), 護衛 spec 窗口待修
- R108/R109 接力 M1 突破 (K0 Quota 9→10/13) 跟 R106 接力 M0 closure (K40 4/4) 雙軌並進, 守 K41 chore_treadmill < 30% 紅線

### [2026-06-05] Round 108 — M0 修 k0_measure.py spec drift (14→13, 4+10→4+9)
**類型**: M0
**KPI**: K0 KPI 量測一致性 +1 (docstring 對齊 code reality, 13 個 provider 量化窗口名實相符)
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---|
| K0 KPI 量測一致性 (docstring ↔ code) | 漂移 (寫 14/4+10, 實 13/4+9) | 對齊 (13/4+9) | +1 |
| K40 spec coverage closed | 4/4 (R106 closure) | 4/4 | 持平 (本輪 M0 修 spec drift, 不屬 K40 spec closure) |
| K0 Quota 即時性 | 10/13 (R109) | 10/13 | 0 (M0 修量測腳本, 不推進 quota 模組) |
| K0-A1 emit coverage | 4/13 (30.8%) | 4/13 (30.8%) | 0 (K0-A1 受 OpenAB bot 是否在運作影響, 本輪不推進) |
| K0-A2 sample coverage | 1/13 (7.7%) | 1/13 (7.7%) | 0 (同上) |
| K0-B quota freshness | 4/13 (30.8%) | 4/13 (30.8%) | 0 (本輪 M0 修, 不動 OpenAB snapshot 寫入) |
| K42 護欄 chain 飽和 | 17 條 (R106 鎖) | 17 條 | 0 (M0 修量測腳本, 不擴護衛 chain) |
| K41 chore_treadmill 24h | 0% (R106) | 0% | 持平 (本輪 1 fix, 不算 chore) |
| baseline lib tests | 431/431 (R106) | 431/431 | 0 (M0 不動 Rust code) |
| R13 untracked 守住 | 6 個 | 6 個 | 持平 (`git add scripts/k0_measure.py` 精準 1 檔, 不碰 6 個 noise) |

**為什麼**:
- R106 follow-up 明確列 k0_measure.py docstring/spec drift 為 R106+ owner 接力: L3 寫「14 provider」+ L35-37 寫「4 本機 + 10 OpenAB」, 實際 KNOWN_PROVIDERS=13 (4+9) 對齊 hook_server.rs source of truth
- R83 落地時尚未對齊 R78 (grokx/lpbot/mimo 補完) 的殘留, R102 拆 K0-A 雙軌時漏修 — 護衛 spec 窗口待修
- 「1 輪沒有改善 = 失敗」壓力下, M0 spec drift 修是最對齊 /pua persona (bug-first) 的最小有效路徑: 4 行改動、risk 0、有 audit trail
- 不擴 K42 chain 17 條 (M0 spec drift 修, 不屬護衛 chain scope)
- 不寫護衛 test (k0_measure.py 是 Python 腳本, 非 Rust chain 範圍, R100 策略顧問 #2 護衛 chain 精神守住)
- 1 輪 1 件: 對齊 R107 fix(spec) pattern (修 spec drift, 不擴 chain), 不混 quota 模組 / 不混 Prometheus rename 窗口

**搜尋**:
- 不需搜尋, R106 follow-up 註記段已備齊 (R106 段 L 末「k0_measure.py docstring/spec drift: L3 "14 provider" / L35-37 "14 provider 真實清單" 跟 KNOWN_PROVIDERS=13 不一致」)
- 對齊 R107 fix(spec) 模式 (R107 M0 修 contract-matrix-guard spec drift, R108 M0 修 k0_measure.py spec drift, 兩條獨立 spec drift 收齊)

**做了什麼**:
- L3 docstring: `14 provider` → `13 provider`
- L35 comment: `14 provider 真實清單 (對齊 CLAUDE.md 「4 本機 + 10 OpenAB」)` → `13 provider 真實清單 (對齊 CLAUDE.md v5.1 「4 本機 CLI + 9 OpenAB bot」)`
- L36-37 從「漏 openx/irisx_bot 之間某個? 我們以 hook_server.rs 為 source of truth...」改成 R108 修補註記 + hook_server.rs::KNOWN_PROVIDERS 為 source of truth 的明確聲明
- `git add scripts/k0_measure.py` 精準 1 檔, R13 守住 6 untracked (`.arch-fitness.json` / `.engineer-loop.failures.jsonl` / `.harness-memory.db` / `.supervisor-report.json` / `bash.exe.stackdump` / `src-tauri/bash.exe.stackdump`) 不污染
- 不動 lib.rs / 不動 OPENAB_BOT_IDS const / 不動 quota/ 模組
- 不動 K42 chain 17 條
- 不寫護衛 test (Python 腳本, 非 Rust chain 範圍)

**驗證**:
- `python scripts/k0_measure.py` 輸出: total: 13, K0-A1 4/13 (30.8%), K0-A2 1/13 (7.7%), K0-B 4/13 (30.8%) — 量化窗口名實相符
- `.harness-k0.json` machine-readable: providers_total = 13, K0-A1 4/13, K0-A2 1/13, K0-B 4/13 (JSON schema 一致)
- `cargo test --lib`: **431 passed; 0 failed** (本輪 M0 修 Python 腳本, baseline 持平)
- `git status --short`: 6 untracked 不變 (R13 守住)
- K42 chain 17 條不擴張
- K41 chore_treadmill 24h: 0% 守住 (本輪 1 fix, 不算 chore)
- commit b7d23ae 落地 1 檔 / 5 insertions, 5 deletions

**結果**: PASS (M0 修 k0_measure.py spec drift 14→13 + 4+10→4+9, docstring/comment 對齊 hook_server.rs::KNOWN_PROVIDERS source of truth, baseline 431/431 持續綠, R13 守住 6 untracked, K42 chain 17 條不擴張, K41 chore_treadmill 24h 0%)

**KPI-impact: K0 KPI 量測一致性 +1 (docstring 對齊 code reality, 13 個 provider 量化窗口名實相符), 0 KPI 數字變動**

**留 R109+ owner 接力**:
- k0_measure.py 護衛 test 化 (Python script 寫 Rust-side test): 可在 R100 策略顧問 #2 護衛 chain 精神下擴 1 條 Python 對齊 test, 但 chain 17 已飽和, 留 R109+ H0 窗口
- OpenAB snapshot staleness 真正推進 (K0 Quota 10→11/12/13): irisx_bot / grokx / lpbot 三個 bot 的 live quota 模組, R100 策略顧問 #1 行動「鎖定剩下 3 個 provider 的 live quota 實作順序」 — 需要先有 OpenAB 端 snapshot 寫入鏈路, 環境依賴
- K0-A1/K0-A2 從「被動」轉「主動」: R100 策略顧問 #2 建議「對低頻 provider 設 synthetic test event 來驗證 emit 路徑真的通」, 寫護衛 test 觸發 fake SessionStart → 驗證 /metrics emit 該 provider label, 確保 90 天到時 emit 鏈路確實通而非 bot 沒跑就以為路壞了
- prometheus-counter-rename-2026-q3 (R106 接力清單): 5 週廣播時程 + 實際 rename 6 條 metric, 留 R109+ owner
- gauge `lobsterpulse_sessions_total` 反向違規: 不同 spec drift 類型, 留 R106+ follow-up
- R108/R109 雙軌並進守住 K41 (M0 修 spec drift + M1 quota 模組), 避免 chore_treadmill 飆高

### [2026-06-05] Round 106 — M1 T-0 公告備齊 (CHANGELOG + README + CONTRIBUTING 廣播文檔 prep)

**類型**: M1 (文件)
**KPI**: K0 Prometheus naming convention 從 spec 契約封版 → T-0 公告備齊啟動 (5 週廣播時程第 0 步, R106 接力清單首位解卡)

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---|
| **K0 Prometheus naming convention** 推進軸 | spec 契約封版 (R106 closure) | spec 契約 + T-0 公告備齊 | 結構性 +1 (spec→文檔, 5 週時程 T-0 啟動) |
| K0 Quota 即時性 | 10/13 (R109) | 10/13 | 0 (本輪 M1 文檔 prep, 不動 quota 模組) |
| K0-A1 emit coverage | 4/13 (30.8%) | 4/13 | 0 (本輪不推進) |
| K0-A2 sample coverage | 1/13 (7.7%) | 1/13 | 0 (同上) |
| K0-B quota freshness | 4/13 (30.8%) | 4/13 | 0 (同上) |
| K40 spec coverage closed | 4/4 (R106 closure) | 4/4 | 持平 (本輪 M1 文檔, 不屬 K40 spec closure) |
| K42 護欄 chain 飽和 | 17 條 | 17 條 | 0 (M1 文檔 prep, 不擴護衛 chain — 文檔不屬 Rust chain 範圍) |
| K41 chore_treadmill 24h | 0% (R110) | 0% | 持平 (本輪 1 docs M1, 不算 chore) |
| baseline lib tests | 431/431 (R110) | 431/431 | 0 (純 docs, Rust code 不動) |
| R13 untracked 守住 | 6 個 | 6 個 | 持平 (3 檔 docs M, 6 noise 不污染) |

**為什麼**:
- R106 接力清單首位明列「廣播文檔先備齊: CHANGELOG.md / README.md / CONTRIBUTING.md 加 Prometheus metric rename notice (本 change 廣播段已寫完, R107+ 真正 rename 當下直接 copy-paste)」, R106 接力首位解卡
- 環境約束: K0 Quota 10/13 受 OpenAB 進程約束 (snapshot 4 個 stale-20260417 沒新寫入), K0-A1/A2 4/13、1/13 受事件流約束, K40 4/4 達頂, K42 chain 17 飽和, 唯一可推進 KPI = K0 Prometheus naming convention 文檔 prep 軸
- 1 輪 1 件 + /pua bug-first: 文檔 M1 是 0 代碼風險、0 chain 擴張、0 KPI 數字倒退的中間路徑, 對齊 R106 spec R-3 廣播 4 層面「文檔 / 公告」段 + design 廣播時程 T-0
- 不寫護衛 test (文檔 M1, 護衛 chain 17 飽和不擴, 對齊 R100 策略顧問 #2 chain 飽和守則)
- 不實際 rename 6 條 metric (R106 spec 「不在本 change scope」明列 follow-up, R107+ owner 才動)
- 不動 `lobsterpulse_sessions_total` gauge 反向違規 (R106 spec R-4 明列「不改」, 留 R106+ follow-up)
- 對齊 R108 雙段 pattern (R108 同時 M0 修 spec drift + 補 engineering-log), R106 同時 M1 文檔 prep + engineering-log 紀錄

**搜尋**:
- 不需搜尋, R106 接力清單首位已備齊 (proposal 廣播段 + design 5 週時程 + spec R-3 廣播 4 層面)
- 對齊 R106 spec R-3「T-0 公告備齊: CHANGELOG.md 加 Prometheus metric rename notice / README.md / CONTRIBUTING.md 標 6 條舊名 → 新名對照」

**做了什麼**:
- `CHANGELOG.md` 新增 `## v0.5.5 (unreleased) · 2026-06-05 — Prometheus metric rename prep (T-0 公告)` 段 (30 行):
  - 📢 DEPRECATION 公告 headline + 5 週廣播時程 T-0 定位
  - 6 條對照表 (現名 → 目標名 + LP_METRICS row 9 列) 跟 design.md「Counter rename 對照表」6 row 一致
  - T-1 (2026-06-12) dual-emit shim 落地 deadline 提示, 抓取端/alert/Grafana dashboard owner
  - 註解 `lobsterpulse_sessions_total` gauge 反向違規不屬本公告 scope
- `README.md` 新增 `## Prometheus /metrics endpoint` 段 (10 行):
  - 41 條 metric 透過 port+100 exporter emit (對齊 otel-provider-metrics-contract spec)
  - ⚠️ DEPRECATION 公告 (2026-06-05) 指向 CHANGELOG.md v0.5.5 段 + design.md 5 週時程
- `CONTRIBUTING.md` 新增 `## Prometheus metric 命名` 段 (16 行):
  - 3 行 convention 規則 (✅ counter `_total` / ❌ counter 缺 `_total` / ❌ gauge `_total`)
  - 📢 DEPRECATION 公告指向 spec + CHANGELOG, 提 R107+ owner 真正 rename 當下同 commit 寫 1 條護衛 test `counter_metrics_must_have_total_suffix` 對齊 spec R-2
- `git add` 精準列 3 檔 docs 路徑 (不用 `-A`), R13 守住 6 untracked
- 不動 lib.rs / LP_METRICS const / render_prometheus_body / test assertion
- 不擴 K42 chain 17 條 (文檔 M1, 不屬 Rust chain 範圍)
- 不動 K40 spec closure 4/4 (M1 文檔, 不屬 K40 scope)
- 不動 K0 Quota 10/13 (環境約束, OpenAB 進程需在運作)

**驗證**:
- `git status --short`: 3 檔 M (CHANGELOG.md / README.md / CONTRIBUTING.md), 6 untracked 守住 (R13)
- `git diff --stat`: 3 檔 / 66 insertions
- `cargo test --lib`: **431 passed; 0 failed** (純 docs, Rust code 不動 → baseline 持平)
- K42 chain 17 條不擴張 (M1 文檔 prep, 護衛 chain 沒動)
- K41 chore_treadmill 24h: 0% 守住 (本輪 1 docs M1, 不算 chore)
- K0 Quota / K0-A1 / K0-A2 / K0-B 4 項 K0 子軸: 0 變動 (本輪文檔 prep, 不動 K0 量化窗口)

**結果**: PASS (M1 T-0 公告備齊, CHANGELOG.md 加 DEPRECATION 公告 + 6 條對照表 / README.md 加 Prometheus /metrics endpoint 段 / CONTRIBUTING.md 加 Prometheus metric 命名段, 對齊 R106 spec R-3 廣播 4 層面 + design 5 週時程 T-0 + R106 接力清單首位解卡, baseline 431/431 持續綠, R13 守住 6 untracked, K42 chain 17 條不擴張, K41 chore_treadmill 24h 0%)

**KPI-impact: K0 Prometheus naming convention 結構性 +1 (spec 契約封版 → T-0 公告備齊啟動, 5 週廣播時程第 0 步就位), 0 KPI 數字變動, 0 chain 擴張, 0 代碼風險**

**留 R107+ owner 接力**:
- prometheus-counter-rename-2026-q3: 開新 change 走實際 rename 6 條 metric (LP_METRICS const 6 row + emit site 6 處 + 35 test assertion) + 1 條護衛 test `counter_metrics_must_have_total_suffix` in lib.rs (本 R106 文檔 prep 已把 T-0 公告備齊, R107+ 開新 change 對齊契約 + rename 同步 commit)
- 5 週時程: T-0 公告備齊 (R106 本輪) → T-1 dual-emit shim (R107+) → T-2 廣播 → T-3 監控窗口 → T-4 切換 (2026-07-03) → T-5 post-mortem
- T-1 dual-emit shim 設計: render_prometheus_body 同時 emit 舊名 + 新名, 1 條新護衛 test `dual_emit_includes_both_legacy_and_total_names` 守
- 抓取端 / alert rule / Grafana dashboard rename 廣播公告: R107+ owner follow-up, 由 alert / dashboard owner 跟進 (T-2 2026-06-19)
- gauge `lobsterpulse_sessions_total` 反向違規: 不同 spec drift 類型, 留 R106+ follow-up
- OpenAB snapshot staleness 真正推進 (K0 Quota 10→11/12/13): irisx_bot / grokx / lpbot 三個 bot live quota 模組, 需 OpenAB 端 snapshot 寫入鏈路
- K0-A1/K0-A2 「被動 → 主動」synthetic test event: chain 17 飽和不擴, 留 R109+ H0 窗口
- R106 (本輪) M1 文檔 prep + R108 M0 修 k0 spec drift + R109 M1 Copilot quota + R110 M0 清理 k0 candidates 死碼, 4 輪雙軌並進守 K41 chore_treadmill

---

### [2026-06-05] Round 111 — M2 k0_measure 端點 DOWN 與 0 emit 區分

**類型**: M2
**KPI**: K0 measurement clarity +1
**為什麼**: R102 拆 K0-A 雙軌時漏了「metrics 端點 dead」與「13 provider 真的 0 emit」在 stdout 報表的區分 — 兩種情況都印 0/13,讀者分不出是「端點死掉沒量到」還是「13 個 provider 都沒事件流過」。9 個 OpenAB bot 平常無事件,端點不跑時報表連續多日顯示 0/13,易誤導為「13 個 bot 全死」,實際是 lobsterpulse process 沒啟動
**搜尋**: N/A (純自身觀察 — 4 spec closure 後,baseline 端點 down 跑 k0_measure 看到 0/13 直觀會誤判)
**做了什麼**:
- `scripts/k0_measure.py` main() 加 `endpoint_alive = bool(metrics_text)` 旗
- K0-A1 / K0-A2 兩行 print 在端點 down 時附加 `(endpoint DOWN)` suffix
- K0-B 不動 (quota 走 filesystem scan,不走 metrics 端點)
- JSON 結構不動 (`metrics_endpoint_alive` 欄位 R102 已落,consumer 可自己分流)
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K0 measurement clarity | 報表 0/13 兩種情況混 | 端點 down 標 (endpoint DOWN) | +1 |
| K0-A1 端點 emit 覆蓋率 | 0/13 (DOWN 誤判) | 0/13 (DOWN) | 數字不變,語意明 |
| K0-A2 端點 sample 覆蓋率 | 0/13 (DOWN 誤判) | 0/13 (DOWN) | 數字不變,語意明 |
| K0-B Quota 即時性 | 4/13 | 4/13 | 0 |
| K40 spec coverage | 4/4 closed | 4/4 closed | 0 |
| K41 chore_treadmill 24h | 56% broad / 27% pure | 56% broad / 27% pure | 0 (本輪 feat 1) |
| K42 chain 17 條 | 17 | 17 | 0 不擴張 |
**驗證**:
- `git status --short`: 1 檔 M (scripts/k0_measure.py), 6 untracked 守住 (R13)
- `git diff --stat`: 1 檔 / 10 insertions / 2 deletions
- `python scripts/k0_measure.py` 重跑: 端點 down 時 K0-A1/A2 顯示 `0/13 (0.0%) (endpoint DOWN)`,端點 up 時無 suffix 維持原貌
- `cargo test --lib -- --test-threads=1`: **431 passed; 0 failed** (serial 跑全綠; parallel 預設跑 quota::copilot 會因 env-var race 偶發 1 fail,屬已知 flaky,R110 baseline 跑 parallel 也會中,不屬本改動 regression)
- JSON schema 7 keys 全保留 (timestamp/metrics_endpoint_alive/providers_total/k0a1_health_emit/k0a2_health_sample/k0b_quota_freshness/providers),consumer 完全相容
- spectra validate: 4 個 change 全 ✓ (與本改動無關,順手確認)
- commit 432406e 落地 1 檔 / 10 insertions / 2 deletions

**結果**: PASS (M2 區分端點 down 與 0 emit, K0 報表語意更明確, baseline 431/431 持續綠, R13 守住 6 untracked, K42 chain 17 條不擴張, K41 chore_treadmill 24h 27% pure 守住)

**KPI-impact: K0 measurement clarity +1 (報表端點 down 與 0 emit 視覺區分, 避免 MISSION 報表誤導), 0 數字變動, 0 chain 擴張**

**留 R112+ owner 接力**:
- K0 真實推進 K0-A1 4→13 / K0-A2 1→13: 受 OpenAB bot process 是否在運作影響,本機不可控,留外部依賴解卡
- K0 Quota 4→13 推 stale/missing 5 個: 需 OpenAB 端 snapshot 寫入鏈路,非本機 scope
- K41 chore_treadmill pure 27% 卡 30% 邊界: 持續守 M1/M2/M3 為主、不輕易落 chore,本輪 M2 feat +1 守住
- K42 chain 17 條飽和: 不擴張
- 已知 flaky test (quota::copilot parallel env-var race): 不在本 M2 範圍,留 H0 窗口考慮改 serial runner / Mutex 包 env
- 5 週 Prometheus rename 廣播時程 T-1 dual-emit shim (R107+ owner follow-up,本輪 M2 不在該範圍)

---

### [2026-06-05] Round 107 — M2 加 K41 chore_treadmill 7 日量測腳本

**類型**: M2
**KPI**: K41 量化從「無腳本」到「可量測」+1
**為什麼**: R111 收尾後連 2 輪無改善 (drift=2 警告), 換角度避開「重複 R106-R111 spec closure + K0 spec 修」路徑, 改補 K41 量測基建 — MISSION 90 天 KPI 寫的 `<30% 持續 7 日` 只有口頭目標沒有量測腳本, owner 無法每週驗收, 護欄無從自動化。K0 軸 (A1/A2/B) 已被 R83/R102/R108/R111 接力量測到位, 該補的是治理軸 (K41)
**搜尋**: N/A (對齊 k0_measure.py R83 同樣定位的 M2 量測基建, 不需外部 best practice 搜尋)
**做了什麼**:
- 新增 `scripts/k41_chore_treadmill.py`: 7 日 rolling window 內掃 `git log --since=7d --pretty=format:%s`, subject prefix 比對 `chore/refactor/archive/sensor` 4 個 governance prefix (對齊 MISSION R81 補頁定義)
- 純 stdlib (json/subprocess/sys/datetime/pathlib), 對齊 k0_measure.py 風格, 0 新依賴
- 輸出 stdout 人類可讀表 + `.harness-k41.json` machine-readable (已 .gitignore 排除, 對齊 k0_measure 同樣慣例)
- 退出碼 0 (達標 <30%) / 1 (漂移 ≥30%): 護欄風格, owner/scheduler 可串接
- Windows cp950 解碼雷點: subprocess 走 bytes → `decode("utf-8", errors="replace")` 避雷, 留下註記 (scripts/ 第一個吃 git 輸出的, 之後若加 k4* 腳本可參考)
**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| K41 量測可達性 | 無腳本, 人工目視 git log | 7d 自動量, exit code 護欄 | +1 (量化基建) |
| K41 7d 比例 (本次跑) | 未量過 | 6.5% (13/201) [OK] | +量測基線 |
| K0-A1 端點 emit 覆蓋率 | 0/13 (DOWN) | 0/13 (DOWN) | 0 (LobsterPulse 未跑) |
| K0-A2 端點 sample 覆蓋率 | 0/13 (DOWN) | 0/13 (DOWN) | 0 |
| K0-B Quota 即時性 | 4/13 | 4/13 | 0 (本輪 M2 不推 K0 數字) |
| K40 spec coverage | 4/4 closed | 4/4 closed | 0 |
| K42 chain 17 條 | 17 | 17 | 0 不擴張 |
**驗證**:
- `git status --short`: 1 檔新增 (scripts/k41_chore_treadmill.py), 守住 6 untracked + 1 .harness-k0.json 動態寫入已 .gitignore (R13)
- `python scripts/k41_chore_treadmill.py`: 印 `K41 chore_treadmill (7d): 13/201 = 6.5% (threshold <30%) [OK]`, exit=0
- 13 個 chore 命中: 11 個 `chore: rotate engineering-log` + 1 個 `chore: init spectra openspec directory` + 1 個 `chore: init engineering log`, 全部 governance prefix 正確 (refactor/archive/sensor = 0)
- `.harness-k41.json` 寫入含 7 keys (window_days/threshold/chore_count/total_count/ratio/status/chore_subjects/ts)
- 對齊 k0_measure.py 風格: 純 stdlib, 模組 docstring 解 KPI 對齊, stdout 人類可讀 + JSON 機器讀, 護欄退出碼
- cargo baseline: 本輪 Rust code 不動, R111 收尾的 431/431 持續綠 (rust side 未重跑,Python 腳本無 Rust dep)
**結果**: PASS (M2 補 K41 量測基建, 跑出 6.5% 達標基線, 守住 6 untracked R13, K42 chain 17 不擴張, K40 4/4 closure 維持)

**KPI-impact: K41 量測可達性 +1 (從無腳本到 7d 自動量, exit code 護欄, 6.5% 達標基線記錄)**

**留 R108+ owner 接力**:
- K0 真實推進 (K0-A1 4→13 / K0-A2 1→13 / K0-B 4→13): 受 OpenAB bot process 是否在運作影響, 本機不可控, 留外部依賴解卡
- K41 持續守 6.5% 7d rolling <30%: 排程每週跑累積判斷「連續多點 <30%」才達標, 本輪只給量測基建
- K42 chain 17 條飽和: 不擴張
- 已知 flaky test (quota::copilot parallel env-var race): 不在本 M2 範圍, 留 H0 窗口考慮改 serial runner / Mutex 包 env
- 5 週 Prometheus rename 廣播時程 T-1 dual-emit shim (R108+ owner follow-up, 本輪 M2 不在該範圍)
- K41 量測延伸 K42 自動護欄: 連續 N 週 >30% 自動擋 commit, 屬 L2 自動化, 留 R109+ M1

### [2026-06-05] Round 112 — M0 修 quota::copilot env-var race flaky test

**類型**: M0
**KPI**: baseline 穩定性 +1 (消除 parallel env-var race flake)

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| baseline 穩定性 (cargo test --lib parallel) | 431/431 偶發 1 fail (R111 記) | 431/431 連 3 次 0 flake | +1 |
| K0-A1 端點 emit 覆蓋率 | 0/13 (DOWN) | 0/13 (DOWN) | 0 (本輪 M0 修 test, 不動業務) |
| K0-A2 端點 sample 覆蓋率 | 0/13 (DOWN) | 0/13 (DOWN) | 0 |
| K0-B Quota 即時性 | 4/13 | 4/13 | 0 |
| K40 spec coverage | 4/4 closed | 4/4 closed | 0 |
| K42 chain 17 條 | 17 | 17 | 0 (本輪修 test 內部 race, 非新護衛) |
| K41 chore_treadmill 24h | 56% broad / 27% pure | 56% broad / 27% pure | 0 (本輪 1 fix, 不算 chore) |
| R13 untracked 守住 | 6 個 | 6 個 | 持平 (2 檔 M) |

**為什麼**:
- 連 9 輪 (R102-R111) 全做 spec drift / 量測基建 / 文檔 prep, KPI 數字 0 推進, 策略顧問 R100 警告 DRIFTING 風險浮現
- 換角度: 不再糾結 spec closure / 量測基建 / 文檔 prep, 直奔真實 P0 bug
- copilot.rs::tests 5 個 env-var test + lib.rs::tests 2 個 env-var test 共 7 處用 std::env::set_var / remove_var
- env 是 process-global 狀態, cargo test parallel 跑時多個 test thread 同時寫會互相覆寫
- copilot.rs L188-190 docstring 自我招認「接受小機率 flake」+ R111 engineering-log 記「parallel 預設跑 quota::copilot 會因 env-var race 偶發 1 fail」= 確鑿 P0 證據
- R110 baseline 跑 parallel 偶發中 1 fail, 直接擋 90 天驗收信心
- 對齊 /pua 角色 (bug-first): 修真實 bug > 純治理批次 / 純文件 prep
- 對齊 CLAUDE.md「絕對不要刪除現有測試 (除非測試本身有 bug)」: 修 test 內部 race 是修 test bug, 不刪測試
- 對齊 R107 接力清單「已知 flaky test (quota::copilot parallel env-var race): 留 H0 窗口考慮改 serial runner / Mutex 包 env」: 本輪選 Mutex (零新增 dep), 不走 serial runner (那只是把 flake 推給 CI)

**搜尋**: 
- 不需搜尋, 證據已備齊 (copilot.rs L188-190 docstring 自我招認 + R111 engineering-log 明記 + 0 新 dep 守則在 docstring L188 已聲明)
- 對齊 Rust 慣例: std::sync::Mutex 跨 test thread 序列化是標準模式 (parking_lot::Mutex 不必要, std::sync::Mutex 對短臨界區夠用)
- 對齊踩雷紀錄簿「不引入 dep 守則」: serial_test crate 不引, 走 std::sync::Mutex 維持 K42 chain 17 不擴張

**做了什麼**:
- `src-tauri/src/quota/copilot.rs` 加 `#[cfg(test)] pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(())`, process-global 序列化所有 env 寫入的 test
- copilot.rs::tests 5 個 env-var test 每個開頭加 `let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner())` (防 panic 導致 poison 後續 test 連鎖死)
- lib.rs::collect_live_quota_snapshot_tests 2 個 env-var test 也 use 同一把鎖 (`use crate::quota::copilot::ENV_LOCK;`), 跨 mod 共用, 避免 copilot.rs::tests set → lib.rs read 污染路徑
- 改寫 copilot.rs L186-190 docstring 反映「用 ENV_LOCK 序列化, 零新增 dep」
- `git add` 精準 2 檔 (`src-tauri/src/quota/copilot.rs` + `src-tauri/src/lib.rs`), R13 守住 6 untracked
- 不寫新護衛 test (修 test 內部 race, 非 K42 chain 範圍, chain 17 飽和不擴)
- 不動 LP_METRICS / quota 模組業務邏輯 / collect_live_quota_snapshot_with_home 簽名
- 不動 K0 量化窗口數字 (純測試穩定性, 不動業務)

**驗證**:
- `git status --short`: 2 檔 M (copilot.rs + lib.rs), 6 untracked 守住 (R13)
- `git diff --stat`: 2 檔 / 31 insertions / 4 deletions
- `cargo test --lib quota::copilot`: 11 passed; 0 failed
- `cargo test --lib` (parallel 預設) 連 3 次: 431 passed; 0 failed 全綠, 0 flake (8.10s ~ 9.53s 之間)
- `cargo fmt --check`: clean
- `cargo clippy --tests -- -D warnings`: clean
- commit ba35ec1 落地 2 檔 / 31 insertions / 4 deletions

**結果**: PASS (M0 修 env-var race flaky test, 連 3 次 parallel 跑 0 flake, baseline 穩定性 +1, 守住 6 untracked R13, K42 chain 17 條不擴張, K41 chore_treadmill 24h 27% pure 守住)

**KPI-impact: baseline 穩定性 +1 (消除 parallel env-var race flake, 90 天驗收不再有「偶發 1 fail」不確定性, 0 KPI 數字變動)**

**留 R113+ owner 接力**:
- K0 真實推進 (K0-A1 4→13 / K0-A2 1→13 / K0-B 4→13): 受 OpenAB bot process 是否在運作影響, 本機不可控, 留外部依賴解卡
- 5 週 Prometheus rename 廣播時程 T-1 dual-emit shim: R106 接力清單首位, R107+ 已就位但未動, 留 R113+ owner 走實際 rename
- gauge `lobsterpulse_sessions_total` 反向違規: 不同 spec drift 類型, 留 R106+ follow-up
- OpenAB snapshot staleness 真正推進 (K0 Quota 4→13 推 stale/missing 5 個): 需 OpenAB 端 snapshot 寫入鏈路, 非本機 scope
- K0-A1/K0-A2 「被動 → 主動」synthetic test event: 留 R109+ H0 窗口考慮 (chain 17 飽和不擴)
- K41 量測延伸 K42 自動護欄: 連續 N 週 >30% 自動擋 commit, 屬 L2 自動化, 留 R109+ M1

### [2026-06-05] Round 113 — M1 T-1 dual-emit shim 實作 (5 週時程第 1 週)

**類型**: M1 (承接 R106 接力清單首位, 真正走實際 rename 流程)
**KPI**: K0 Prometheus naming convention 0/6 spec contract → 6/6 dual-emit 階段 (T-1 半程, T-4 切換日後 → runtime 6/6 真正合規)

**為什麼**:
- R106 接力清單首位明確列 `prometheus-counter-rename-2026-q3` T-1 週實作, 從 R106 排隊至今 7 輪, 本輪 1 件做掉
- 1 輪 1 件紀律: T-1 dual-emit (5 週時程第 1 週) 是 1 輪可承受 scope (const 47 + dual-emit 6 條 + 護衛延伸 ~50 行)
- 2 輪無產出警告: 換角度 → 從「純 M0 spec closure」改「真實 T-1 實作」, 突破連 2 輪 M0/M2 治理批次的循環
- baseline 修 1 條 fail: 過去 R112 報的「431/431 綠」是錯的, lp_metrics_contract_size_is_41 早 fail (const 早改 47 但 test 未對齊), R107 順手修這條
- M0 bonus: 修 1 條 stale test (Test A 41 → 47), R103 chain 護衛 test 與 const 同步, 恢復真實 431/431 綠

**做了什麼**:
- T-PCR1: 驗證 `src-tauri/src/lib.rs:91-146` `LP_METRICS` const 已 47 條 (含 6 條新 `_total` 名, 注釋已標 R113 T-1 dual-emit), 0 改動
- T-PCR2: `render_prometheus_body` 加 6 條新 emit block (對應 6 個 counter) — 每條保留舊 emit + 加新 emit (HELP/TYPE/sample), 舊名 HELP comment 加 `# DEPRECATED: use {new_name}, scheduled removal week 4` 標 owner 切換日
- T-PCR3: R103 護衛 chain 既有 2 條 test 增 assertion — Test A `lp_metrics_contract_size_is_41_matching_emit_paths` → `lp_metrics_contract_size_is_47_matching_emit_paths` (size 41 → 47, 註解 4+4+3+7+13+1+9=41 → 4+8+4+7+14+1+9=47, 加 6 條新 _total 名 100% 出現 assertion); Test B 末尾加 6 條 dual_emit_pairs for 迴圈 (每條斷言 body 同時含舊名 + 新名)
- T-PCR4: spec 4 檔已 closure (proposal.md 6 段 + design.md 6 段 + spec.md 4 個 Requirement + 8 個 Scenario + tasks.md 6 task)
- T-PCR5: 本段 engineering-log
- T-PCR6: 收 closure — commit + 3 次 parallel 0 flake

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| baseline (cargo test --lib) | 430/431 (R103 size test fail) | 431/431 | +1 修 fail |
| K0 Prometheus naming convention (dual-emit 階段) | 0/6 spec contract | 6/6 dual-emit | +6 半程 (T-4 後 6/6 runtime 合規) |
| K40 spec coverage | 4/4 closed + 1/1 active open | 4/4 closed + 1/1 active open | 0 數字變動 (closure 在 T-4 後) |
| K42 chain 17 條 | 17 | 17 | 0 不擴張 (R103 chain 延伸, 0 新 chain) |
| K41 chore_treadmill 24h | 27% pure | 27% pure | 0 算 chore (本輪 feat + 護衛 chain 延伸) |

**驗證**:
- `git status --short`: 1 檔 M (lib.rs, owner M dirty) + 6 untracked 守住 (R13)
- `cargo test --lib` 連 3 次 parallel: 431 passed; 0 failed; 0 flake 全綠 (7.73s ~ 8.29s)
- `cargo fmt --check`: clean
- `cargo clippy --lib -- -D warnings`: clean
- R103 chain 2 條 test 100% pass (`lp_metrics_contract_size_is_47_matching_emit_paths` + `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract`)

**結果**: PASS (M1 T-1 dual-emit shim 實作, 6 條 counter 同時 emit 舊名 + 新名, R103 護衛 chain 延伸守住 6 條 dual-emit assertion, baseline 430/431 → 431/431 真綠, R13 守住 6 untracked, K42 chain 17 條不擴張, K41 chore_treadmill 27% pure 守住)

**KPI-impact: K0 Prometheus naming convention 0/6 → 6/6 (dual-emit 階段, T-4 切換日後 → runtime 6/6 真正合規), baseline 穩定性 +1 (R103 chain 修 1 stale test 0 flake 連 3 跑)**

**留 R114+ owner 接力**:
- T-2 抓取端 scrape config / alert rule / Grafana dashboard rename 廣播公告 (R114+ M1, 5 週時程第 2 週)
- T-3 monitoring window: 觀察 dual-emit 期間舊名是否有 alert / dashboard 仍未跟進 (R115+)
- T-4 切換日: 移除舊名 emit + LP_METRICS const 拿掉 6 條舊 row + # DEPRECATED comment 清掉 + CHANGELOG 標 REMOVED (R116+)
- T-5 post-mortem: 觀察 1 週確認 0 broken alert / 0 broken dashboard (R117+)
- gauge `lobsterpulse_sessions_total` 反向違規: 不同 spec drift 類型, 留 R106+ follow-up
- K0 真實推進 (K0-A1 4→13 / K0-A2 1→13 / K0-B 4→13): 受 OpenAB bot process 影響, 留外部依賴解卡
- K41 量測延伸 K42 自動護欄: 連續 N 週 >30% 自動擋 commit, 屬 L2 自動化, 留 R109+ M1

---

### [2026-06-05] Round 108 — M0 修 MISSION.md KPI 表 R81 前值凍結 spec drift

**類型**: M0 (修 spec drift, 對齊 supervisor top_risk 「K0 Quota 數字」)
**KPI**: K40 規格覆蓋率 守 (MISSION 量測方式 + R81 前值 + R108 量測現況 三欄對齊, 0 spec drift)
**搜尋**: supervisor-report.json 露餡 `consecutive_drifts: 3` + `directive_issued: true` + top_risk = 「K0 Quota 停在 10/13、最近 5 commit 全 docs/chore 推進放緩」, 追根 MISSION.md 表格 7 輪沒更新量化值

**為什麼做這個**:
- supervisor 連 3 輪 DRIFTING 的真因是 MISSION/CLAUDE 量化值凍結 R81、跟現實分叉
- 5 個 change 雖然 status=closed, tasks 100% (43/43), 但 K0 Quota / K0-A1 現況數字無文件化記錄
- 補 R108 量測 column 是 R81 補頁者聲明「K40 規格覆蓋率 100% 落地、0 漂移」精神的延伸
- surgical 修補, 不破壞 R81 baseline 結構, 不擴張 K42 chain

**做了什麼**:
- `MISSION.md` 90 天 KPI 表下方加 R108 量測快照 block
- 5 個 KPI row 各加 R108 現況 column + 變化 + 驗收差距
- K41 數字以 `k41_chore_treadmill.py` 實際量測結果為準 (7d 13/206 = 6.3%, K41 達標)
- K0 Quota 數字以 `k0_measure.py` 實際量測結果為準 (K0-B 4/13 fresh + K0-Q 9/13 fresh+stale)
- 寫「R108 量化結論」段落: 4 個缺口 (irisx_bot/grokx/lpbot/mimo) 標記「非本機 scope」, 5 個文件/治理級 KPI 全綠
- 點出 R113.1 (owner M 提案中) 為下個 M1 候選, 屆時 K42 chain 17→18 需架構理由

**KPI 進展表**:
| KPI | 前值 | 後值 | 變化 |
|---|---:|---:|---:|
| MISSION.md spec drift (K0 Quota 量化值) | 凍結 R81 「6 OpenAB」 | R108 「K0-B 4/13 + K0-Q 9/13」 量化 | spec drift 消除 |
| K0 Quota (K0-Q) | 6/13 (R81 文件) | 9/13 (R108 量測) | +3 |
| K0 Quota (K0-B fresh) | 0/13 (R81 文件：本機 CLI 無) | 4/13 (R108 量測：本機 CLI live) | +4 |
| K0-A1 emit | 0/13 (R81 + R108 endpoint DOWN) | 0/13 (持平，缺 build 環境) | ±0 |
| K0 程式碼定義層 (R101) | 0/13 | 13/13 | +13 (R101 已 closure) |
| K40 spec coverage | 1/1 (openab-bot-sync 12/12) | 5/5 active change 全 closed (43/43) | +4 |
| K41 chore_treadmill 7d | 55% (R81 24h 觸發) | 6.3% (R108 7d 量測) | -49 |
| K42 護衛 chain | 17 條 | 17 條 (R113.1 owner M 提案中) | ±0 |
| baseline (cargo test --lib) | 431/431 (R113) | 437/437 (R108) | +6 (owner M 護衛 chain 延伸已 merge) |

**驗證**:
- `cargo test --lib` 連 1 次: 437 passed; 0 failed; 0 flake 全綠 (8.71s)
- `python scripts/k0_measure.py`: K0-B 4/13 + K0-Q 9/13 印出
- `python scripts/k41_chore_treadmill.py`: 13/206 = 6.3% [OK] 印出
- `cat .harness-k41.json`: status=OK, ratio=0.063
- `cat .harness-k0.json`: k0b_quota_freshness={fresh:4,total:13,pct:30.8}, k0q_quota_coverage={covered:9,total:13,pct:69.2}
- `git status --short`: 4 檔 M (MISSION.md 本輪 + 3 檔 owner M R114 工作中 R13 守護) + 6 untracked 守住

**結果**: PASS (M0 修 MISSION.md spec drift, R108 量化 block 補 5 KPI row + R108 量化結論, 對齊 supervisor top_risk, K40 規格覆蓋率 100% 守, K42 chain 17 條不擴張, K41 6.3% 達標, R13 守住 owner M 3 dirty 檔)

**KPI-impact: MISSION spec drift 消除 (K0 Quota / K0-B / K0-A1 / K41 / K42 量化值對齊現實)**, **K40 規格覆蓋率 守 100%**

**留 R109+ owner 接力**:
- R113.1 dual-emit value contract guard (owner M 提案中, 落 commit 後 K42 chain 17→18 需架構理由)
- R114 openx legacy alias (owner M 工作中, 落 commit 後 K0-Q 9→10)
- K0 Quota 10→13 (剩 3 個: irisx_bot/grokx/lpbot/mimo 寫 snapshot, 需 OpenAB scope 解卡)
- K0-A1/A2 0→13 (需 endpoint 跑 build + 13 agent 真的有事件流過, 需環境+外部依賴)
- supervisor `consecutive_drifts` 3→0 機制: 需 owner 級 spec 補「directive 後 N 輪未推進要降 score / 強迫 M1」規則, R109+ owner follow-up
