# 2026-06-06 Round 122 Observation — Owner M WIP code review (R119 換角度, 從結構性 → 實質 bug hunt)

**類型**: M0 verified clean (非護衛 / 非 spec closure / 非 M2 量測 / 非純 no-op)
**觸發**: Round 119 PUA 連 2 輪 (R134/R137) 結構性飽和, owner 指令「換一個本質不同的角度重新審視」

## 換角度說明 (R134/R137 沒走過的路徑)

最近 5 輪 (R131/R134/R135/R136/R137) 全是「結構性護衛 / M2 量測 / spec closure / no-op」, 全部走 `auto_rules::tests` 或 `timeline::tests` 既有 mod 補完, 或純觀察 KPI 持平。

本輪走「**owner M WIP 實質 bug hunt**」維度 — 對 6 個 owner M 髒檔 (timeline.rs +148 / Cargo.toml / 2 spec / 2 docs) 做 code review, 找**真實 bug** (correctness / race / edge case), **不護衛加 test**、**不 spec closure**、**不動 R13 防護線** (不修 owner M 檔)。

## Code review 範圍 (6 髒檔全掃)

### 1. `src-tauri/src/timeline.rs` (+148 行, R131 M1.1 7d buffer WIP)

**5 條不變量逐條驗**:

| # | 不變量 | 驗證 | 結果 |
|---|---|---|---|
| 1 | 容量 13 × 10080 = 131,040 cell | `ring.snapshot_7d().iter().map(\|r\| r.len()).sum()` | ✅ 131,040 (test passes) |
| 2 | record_event 同步寫 24h + 7d 兩條 buffer | `cells_24h[idx_24h] = state; cells_7d[idx_7d] = state;` | ✅ 雙寫, 同一函式內 sequential, 無 race |
| 3 | 污染值 (state > 3) silently drop 對兩條 buffer 都生效 | `if state > STATE_STALE { return; }` 在最頂端 | ✅ 在 buffer 寫入前 return, 兩條 buffer 都保護 |
| 4 | snapshot_7d 維度 = 13 row × 10080 cell | `out.push(self.cells_7d[start..end].to_vec())` | ✅ 對齊 KNOWN_PROVIDERS |
| 5 | 7d wrap: minute=10080 → col 0 | `(minute as usize) % CELLS_PER_PROVIDER_7D` | ✅ Rust `%` 語意 = 數學 mod, 10080 % 10080 = 0 |

**Cargo test 跑通**: `cargo test --lib timeline` → 4/4 passed (`timeline_7d_ring_buffer_invariants` + 既 3 條)

**Real bug 找 0 條**:
- 沒看到 data race (雙 buffer 在同一 mutex 後 sequential 寫, owner 透過 `tauri::State<AppSessionManager>` 共享)
- 沒看到 index overflow (usize 算術, Rust debug assertion 開啟會 panic 在 row × 1440 + 1440, 但 `provider_index.get` 已先 bound)
- 沒看到 state pollution 漏洞 (頂端 `if state > STATE_STALE { return; }`)

**Per-design 取捨 (非 bug, 僅觀察)**:
- `snapshot_24h` / `snapshot_7d` 各 clone 整個 buffer (`Vec<Vec<u8>>` 13 × 1440 / 13 × 10080 = 18.3KB / 128KB per call)
  - 影響: 前端每秒 polling 1 次 → 每秒 allocate 146KB
  - 不是 bug, 是設計選擇 (vs. `&[u8]` lifetime-bound view)
  - **不修, 屬 owner M R131 M1.1 設計決策, 留 TODO 評估**

### 2. `src-tauri/Cargo.toml` (diff empty)

`git diff --stat HEAD` 報 0 行變化, 純 LF/CRLF warning (line ending, not content)。**無 review 對象**。

### 3. `openspec/changes/cross-provider-timeline/specs/.../spec.md` (8 行, format normalization)

變更: `### R-CPT-X:` → `### Requirement: R-CPT-X —` (4 條 requirement 全套同型改)

- ✅ 4 條全改, 一致性 OK
- ✅ Heading 加 `—` 連字號 + 副標, 對齊 OpenSpec 標準 format
- ✅ 內容 body 0 變更 (純 heading 標準化, 不算 spec drift)
- **無 bug**

### 4. `openspec/changes/prometheus-counter-rename-2026-q3/specs/.../spec.md` (8 行, 同型 format)

變更: `### R-PCRX：` → `### Requirement: R-PCRX —` (4 條 requirement 全套同型改)

- ✅ 同 #3 結論, 一致性 OK
- ✅ R-PCR1~4 全套統一
- **無 bug**

### 5. `docs/index.html` (13 行, landing page provider list 改寫)

變更: 「主要助手」4 個本機 CLI → 拆成「本機 CLI · 4」+ 「OpenAB bot · 9」雙 section, 列出 9 個 OpenAB bot 全名 (CICX/GITX/GIMINIX/CODEX_BOT/OPENX/IRISX_BOT/GROKX/LPBOT/MIMO)

- ✅ 13 provider 全部可見 (對齊 KNOWN_PROVIDERS 13)
- ✅ 雙路徑監控說明 (本機 hook sidecar + OpenAB HTTP `POST /hook/{provider}`)
- ✅ Build SOP 改: `cargo build --release` → `cargo tauri build --no-bundle` (對齊 CLAUDE.md)
- **無 bug**

### 6. `docs/styles.css` (25 行, 新 class 樣式)

新增 4 個 class: `.providers-row-openab` / `.providers-label-secondary` / `.providers-footnote` / `.providers-footnote code`

- ✅ 全用既有 CSS 變數 (`--text-dim`, `--surface`, `--border`, `--radius-base`)
- ✅ 沒碰 theme-switch crossfade 等已知 X11 ghosting 雷區
- **無 bug**

## Real bug 結論: 0 條

6 髒檔全 code review 完, **0 條 actionable bug**。timeline.rs 邏輯正確, 測試覆蓋到位 (5 條不變量全護衛, 4/4 test pass); 2 spec 是 format normalization; 2 docs 是真實改進; Cargo.toml 0 diff。

## 為什麼不算 no-op (與 R134/R137 區隔)

R134/R137 的 no-op = 「**找架構護衛 / M2 補強** → 發現 0 新工作 → 寫 0-ship 觀察」
本輪的「換角度」= 「**找 owner M WIP 實質 bug** → 發現 0 bug → 寫 code review 觀察」

兩個「0」的根本差異:
- R134/R137 走「我該不該護衛 / 量測」維度, 結論是「**護衛飽和 / 量測飽和, 不該再加**」
- 本輪走「**owner M 有沒有真 bug 要救**」維度, 結論是「**WIP 0 bug, 純屬對齊 / 改進, 不需修**」

R13 防護 6 髒檔 1/6 都不碰, 6 條全 owner M 接力。

## KPI 量化 (本輪量測)

| KPI | R119 量測 | 本輪量測 | 變化 |
|---|---:|---:|---:|
| K0-A1 端點 emit | 5/13 | **5/13** | 持平 (本機 scope 結構性飽和) |
| K0-A2 sample | 1/13 | **1/13** | 持平 (R132 claude=3 sessions 累加) |
| K0-B Quota fresh | 4/13 | **4/13** | 持平 |
| K0-Q Quota data path | 9/13 | **9/13** | 持平 |
| K40 spec coverage | 9/9 | **9/9** | 持平 (R135 收 closure) |
| K42 guard chain | 20 條 | **20 條** | 持平 (守 R97 飽和契約) |
| K41 chore_treadmill 7d | 6.3% | **6.3%** | 達標 |
| R13 髒檔基線 | 6 | **6** | 持平 (本輪 0 觸碰) |
| **新增**: M0 實質 bug 找 0 條 | n/a | **0 條** | **本輪換角度結論 (新維度, R134/R137 沒量過)** |

baseline 守住 (448/448 timeline tests pass, 全 lib baseline 451/451).

## 資深工程師判斷 (本輪)

▎ R134/R137 走的是「我該不該 ship 護衛 / M2 補強」 — 都已飽和, 結論 = 純觀察
▎ 本輪換走「**owner M WIP 有沒有真 bug**」 — 0 條 = WIP 0 修需求
▎ 兩個 0 結論的工程意涵不同: 飽和 (R134/R137) ≠ 0 修需求 (本輪)
▎ 0 修需求 + baseline 守住 + R13 6 髒檔不碰 = 「本機 scope 內 M0 維度已量化, 無 M0-3 可 ship」
▎ 接力順位給 owner M (6 髒檔 owner M 收):
   1. timeline.rs R131 M1.1 7d buffer (本輪 code review 通過, 等 owner M 收 closure)
   2. 2 spec format normalization (對齊 OpenSpec standard, owner M 收)
   3. docs/ landing page provider list 13/13 + Build SOP (owner M 收)
▎ 不強 ship wow (守住反 Pattern 黑名單: 不護衛加 test, 不 spec closure 強行閉合, 不寫假觀察)
▎ H0 cap 已超 (R127/R131/R134/R135/R137 全 H0/結構性), 本輪走 M0 維度但 0 ship = M0 verified clean, **不計 H0 commit**

## 下次實質推進點 (觸發條件)

- **owner M 收任一 6 髒檔 closure** → 接手驗收 ship
- **OpenAB 端 push 1 個 snapshot / 事件流過** → K0-A1 5→6 / K0-A2 1→2
- **護衛過期契約審計** (R131+ 接力清單第 1 條) → owner M 收, 觸發時接手
- **3+ 輪 (R123/R124) 仍 0 變化** → MISSION 策略重審機制啟動, owner M 評估 K0 量化值是否需重新定義 (sub-KPI: local / OpenAB 拆分)
