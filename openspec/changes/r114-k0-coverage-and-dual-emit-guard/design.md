# Design: R114 K0 Quota coverage + dual-emit value-equality guard

## 1. 三段改動一覽

| 段 | 改動 | 類型 | 行數估 | KPI 影響 |
|---|---|---|---:|---|
| A. R113.1 dual-emit value guard | `src-tauri/src/lib.rs` `render_prometheus_tests` mod 加 1 條 test | M0 fix | +133 | spec drift 預防，K0 報表語意 +1 |
| B. KNOWN_PROVIDERS pub const | `src-tauri/src/hook_server.rs` `const` → `pub const` | refactor | +5 | SSoT 預備，K42 chain 不擴張 |
| C. k0_measure openx alias + K0-Q | `scripts/k0_measure.py` `scan_quota_snapshots` + `main` | M1 feat | +27 | K0 Quota 9/13 → 10/13 |

3 段總計 +165 行（3 個檔案，跨 2 個語言），K42 chain 17 → 17 **不擴張**。

## 2. R113.1 dual-emit value-equality guard 設計（段 A）

### 既有護衛的漏洞

R113 T-1 dual-emit shim 既有護衛 chain（`render_prometheus_body_full_state_...`）
用 `body.contains("lobsterpulse_X")` 跟 `body.contains("lobsterpulse_X_total")`
斷言「兩條 sample line 都出現」。漏洞：

```rust
// 假設 emit 路徑 bug:
render_prometheus_body(... totals_input["cicx"] = 100 ...)
  → emit "lobsterpulse_tokens_input{cicx} 100"
  → emit "lobsterpulse_tokens_input_total{cicx} 200"  // ← 不同 source 算錯
// string contains 護衛: 兩個名字都在 body → 過
// 實際 value 已經分叉 100 vs 200 → silent contract drift
```

### R113.1 護衛設計

`render_prometheus_body_dual_emit_values_match_per_provider`：

1. **建 fixture**：用 `info_with_state` 跟 `ProviderTotals` 構造 3 個 provider
   （cicx/claude/openx）+ quota_ages/quota_pct/last_completed_age HashMap
2. **呼叫** `render_prometheus_body` 拿 Prometheus text body
3. **parse**：本地 helper `extract_metric_values(body, metric_name) -> HashMap<labels, value>`
   解析出指定 metric 的所有 `(labels, value)` pair
4. **6 條 dual-emit pair 對照**：
   ```
   lobsterpulse_tokens_input              vs lobsterpulse_tokens_input_total
   lobsterpulse_tokens_output             vs lobsterpulse_tokens_output_total
   lobsterpulse_provider_tokens_input{provider=X}    vs lobsterpulse_provider_tokens_input_total{provider=X}
   lobsterpulse_provider_tokens_output{provider=X}   vs lobsterpulse_provider_tokens_output_total{provider=X}
   lobsterpulse_provider_failure_count{provider=X}   vs lobsterpulse_provider_failure_count_total{provider=X}
   lobsterpulse_provider_session_count{provider=X}   vs lobsterpulse_provider_session_count_total{provider=X}
   ```
5. **斷言** `legacy_vals == total_vals`（HashMap 全等）

### 為什麼 HashMap 全等比對

- 對齊 R106 design.md 對照表 6 條 pair
- T-1 dual-emit 期間 legacy 跟 total 應該**共用同一 source**，value 必相等
- HashMap 全等（key + value）= 對每個 label 集合都對得到 + value 相等
- 若未來 T-2 抓取端 rename 拆解不同 source，本護衛可改為「`abs(legacy-total) <= epsilon`」容差
- T-4 切換日撤銷（屆時 legacy 刪除，護衛改為「legacy 不在 body」+ total 仍 emit 數值）

### 為什麼不開新 mod / 不擴張 K42 chain

R103 chain 17 條飽和是 R81 + R106 closure 的護欄契約。新護衛 test 寫進
`render_prometheus_tests` 既有 mod（不開新 mod），chain 17 → 17 不擴張。

## 3. KNOWN_PROVIDERS pub const 設計（段 B）

### 為什麼改

R100 策略顧問 #2 行動「單一 contract」+ R106 closure 護衛 chain 12 條
「drift guard prevents silent provider re-drift」。`hook_server::KNOWN_PROVIDERS`
是 13 provider 的 source of truth，但目前是 `const`（module-private），
`lib.rs` 將來要寫 `get_provider_coverage_report` 必須重複列 13 provider
（**會 drift**，跟 R67 護衛 chain #16 防的 case 是同類型）。

改 `pub const` 鋪路，**不寫新 function**（避免 1 輪做 2 件）。

### 影響面

- 5 行改動（`const` → `pub const` + 4 行 R114 註解說明 source of truth 對齊）
- R73 / R78 既有的 2 條護衛 test 仍守住 `KNOWN_PROVIDERS.contains(p)`
  跟 `KNOWN_PROVIDERS.len() == 13`，**無破壞**
- 將來 `lib.rs` 用 `use hook_server::KNOWN_PROVIDERS;` 引用即可，**chain
  不變**（既有護衛 test 已守住長度 13 + 13 個 id 集合對稱）

## 4. k0_measure K0-Q coverage + openx alias 設計（段 C）

### 4.1 為什麼 K0-B 不夠

K0-B Quota freshness = fresh < 24h 計數，目前 4/13。**grokx / lpbot / mimo**
這 3 個 OpenAB bot 完全沒有 snapshot（K0-B 0/3）。K0-A1 / K0-A2 是 endpoint
emit 維度，跟 quota 路徑無關。K0-B 沒抓到「snapshot 存在但 stale」的 case
（cicx/gitx/giminix/codex_bot/openx 5 個 stale 也算 data path 接上）。

R105 supervisor 警告 K0 Quota 10/13 實為 9/13（含 openx 漏算），對齊 MISSION
「usage-*.json 或等價 metric 是否被讀到」口徑應該放寬到「snapshot 存在」
（含 stale）。

### 4.2 K0-Q 維度

`k0q_quota_coverage` = 任何狀態（fresh / stale）都算 quota data path 已接上。

JSON 輸出：
```json
"k0q_quota_coverage": {"covered": 9, "total": 13, "pct": 69.2}
```

console 輸出：
```
K0-Q  Quota 覆蓋率 (fresh+stale 都有 data path): 9/13 (69.2%)
```

### 4.3 openx legacy alias 修

OpenAB `BackendType::Other` 寫 `usage-bot.json`（legacy），`hook_server.rs`
`parse_provider("bot") → "openx"` 自動 rewrite。本腳本原本只 glob
`usage-openx.json*` 永遠漏算 openx。

修法：openx 加第二個 base name `usage-bot`，跟主檔名併行 glob：

```python
base_names = [f"usage-{bot}"]
if bot == "openx":
    base_names.append("usage-bot")
all_files: List[Path] = []
for base in base_names:
    all_files.extend(QUOTA_DIR.glob(f"{base}.json*"))
```

對齊 `hook_server.rs:393-401` `parse_provider` 別名語意，K0 報表永遠少算 1 個
provider 的漏洞正式 closure。

### 4.4 修後 KPI 量測

| 維度 | 修前 | 修後 | 變化 |
|---|---:|---:|---:|
| K0-B Quota fresh | 4/13 | 4/13 | 0 |
| K0-Q Quota fresh+stale | (未量測) | 9/13 | +1 維度 |
| openx 漏算 | 永遠 0 | 計入 stale bucket | 修 |

grokx / lpbot / mimo = 3 個 missing snapshot 仍不在 R114 scope（需 OpenAB
端 snapshot 寫入鏈路，非本機 scope），留 R115+ 接力。

## 5. 護衛 test 設計總覽

| Test | 位置 | 守住什麼 |
|---|---|---|
| `render_prometheus_body_dual_emit_values_match_per_provider` | `lib.rs:11382` `render_prometheus_tests` mod | R113.1 dual-emit 6 條 pair value 全等 |
| 既有 R103 / R106 / R110 / R112 / R113 護衛 | 各自 mod | 不破壞，chain 17 → 17 |

R114 不開新 mod，K42 chain 17 → 17 **不擴張**（守住 R81 飽和契約）。

## 6. Rollout 順序

1. Commit 1: `fix(metrics): R114 M0 R113.1 dual-emit value-equality guard` — `lib.rs`
2. Commit 2: `feat(scripts): R114 M1 k0_measure K0-Q coverage + openx legacy alias` — `k0_measure.py`
3. Commit 3: `refactor(hook_server): R114 KNOWN_PROVIDERS pub const SSoT prep` — `hook_server.rs`
4. 寫 `engineering-log.md` R114 紀錄 + KPI 進展表
5. 收 spec closure：`tasks.md` 全勾 + `.openspec.yaml` status=closed

3 個 commit 對齊 1 輪 1 切片（3 段都屬 R114）的 SOP：M0/M1 優先，refactor 殿後。
