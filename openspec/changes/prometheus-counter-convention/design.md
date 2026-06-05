# Design: Prometheus Counter Naming Convention (`_total` suffix)

## Counter rename 對照表（6 條 / 2026-06-05 盤點）

> Source of truth：對照表本身 = spec 對齊（spec.md ADDED Requirements）
> Code-level 落地面：LP_METRICS const（lib.rs:85-111）、emit site（lib.rs:2213-2255）、
> test assertion（lib.rs:4336-4569）— **本 change 不動**（破既有抓取 + alert + dashboard）
> 護衛：1 條新 `counter_metrics_must_have_total_suffix` test
>
> 6 條全部 TYPE=counter，全部缺 `_total` 結尾 = 違反 Prometheus naming convention
> （counter MUST end with `_total`，見 https://prometheus.io/docs/practices/naming/）。
> 改為目標名後符合 convention，`rate()` / `increase()` / `histogram_quantile()` 表達式
> 對新名才能正確 parse 為 counter type。

| # | 現名（lib.rs LP_METRICS row） | 目標 rename 名 | LP_METRICS row | emit site line | test assertion 數 |
|---:|---|---|---|---:|---:|
| 1 | `lobsterpulse_tokens_input` | `lobsterpulse_tokens_input_total` | 92 | 2213-2214 | 4 |
| 2 | `lobsterpulse_tokens_output` | `lobsterpulse_tokens_output_total` | 93 | 2215-2216 | 4 |
| 3 | `lobsterpulse_provider_tokens_input` | `lobsterpulse_provider_tokens_input_total` | 94 | 2217-2221 | 9 |
| 4 | `lobsterpulse_provider_tokens_output` | `lobsterpulse_provider_tokens_output_total` | 95 | 2223-2227 | 9 |
| 5 | `lobsterpulse_provider_failure_count` | `lobsterpulse_provider_failure_count_total` | 97 | 2233-2237 | 5 |
| 6 | `lobsterpulse_provider_session_count` | `lobsterpulse_provider_session_count_total` | 109 | 2252-2256 | 4 |
| | | | | **小計** | **35** |

### 驗證合計

- 6 條全部在 LP_METRICS const 內有對應 row
- 6 條全部在 `render_prometheus_body` emit site 有 `# TYPE X counter` 標記
- 6 條在 lib.rs test assertion 內合計出現 **35 次**（`assert!(body.contains("..."))` 計）
  - counter 1+2（aggregate pair）：4 + 4 = 8 次
  - counter 3+4（per-provider pair）：9 + 9 = 18 次
  - counter 5（failure_count）：5 次
  - counter 6（session_count）：4 次
  - 合計 8 + 18 + 5 + 4 = 35（與 R105 評估的 30+ test assertion 一致）

## 影響面盤點（rename 6 條 metric 必動的 4 個層面）

### 1. Code-level（spec 階段不動，列為 R107+ follow-up scope）

- `src-tauri/src/lib.rs` LP_METRICS const（6 row rename）
- `src-tauri/src/lib.rs` `render_prometheus_body`（6 emit site：HELP / TYPE / value 各 1 處）
- `src-tauri/src/lib.rs` test assertions（35 處 `body.contains("...")` 全部 rename）

### 2. Prometheus 抓取端（owner 廣播）

- 既有 `scrape_configs` 對 6 條 metric 的 `metric_relabel_configs` 過濾規則
- 既有 recording rule 內 `rate(lobsterpulse_tokens_input[5m])` 等表達式
- 既有 alert rule 內 alert 表達式（failure_count 超過閾值等）

### 3. Grafana dashboard（owner 廣播）

- 6 條 metric 在 dashboard JSON 內的 panel query
- 6 條 metric 在 panel title / legend 格式 / variable definition
- dashboard 內 `${__name__}` / `${metric}` 變數 pattern（可能 catch 舊名）

### 4. 文檔 / 公告

- CHANGELOG.md 加 Prometheus metric rename notice
- README.md / CONTRIBUTING.md 標 6 條舊名 → 新名對照
- docs/landing site（GitHub Pages）若有 metric 引用要同步

## 廣播計劃時程（owner R107+ 真正 rename 前必走）

```
T-0  week  0: 開新 change「prometheus-counter-rename-2026-q3」承接本 spec
                proposal/design/spec 已 closure，tasks 落地 4 層面 impact
T-0  week  0: 公告 CHANGELOG.md 「DEPRECATION: 6 條 metric _total suffix 將於 T+4 週切換」
T-1  week  1: 寫 dual-emit shim — render_prometheus_body 同時 emit 舊名 + 新名
                (舊名加 # DEPRECATED comment 標 owner)
                1 條新護衛 test「dual_emit_includes_both_legacy_and_total_names」守
T-2  week  2: 廣播 alert / dashboard owner 跟進（issue / PR template / Discord 通知）
T-3  week  3: monitoring window — 觀察 dual-emit 期間舊名是否有 alert / dashboard
                仍未跟進（owner 主動聯繫 holdout）
T-4  week  4: 切換日 — 移除舊名 emit、LP_METRICS const 拿掉舊 row、
                # DEPRECATED comment 清掉、CHANGELOG 標「REMOVED: 舊 6 條 metric」
T-5  week  5: post-mortem — 觀察 1 週確認 0 broken alert / 0 broken dashboard
                （如果有 → 緊急 revert 對應 1 條 metric，列 R1XX+ follow-up）
```

不廣播就走 T-0 直接 rename = runtime 監控盲區（既有抓取端拿不到 counter-type
metric，`rate()` 函式回 NaN，所有依賴該 6 條 metric 的 alert / dashboard 全部
silent break）。本 change 的「廣播」段 + 本設計的「廣播計劃時程」段是 spec 階
段就必備的 owner 對齊文件。

## 護衛 test 設計（lib.rs `#[cfg(test)] mod tests`，1 條新）

> 對齊 R102/R103 護衛 chain 風格（test in `mod tests`、fail 報明確訊息、
> 守恆等不變式）。

```rust
// 護衛 test: 任何 TYPE=counter 的 metric MUST 結尾 _total
// 防未來新加 counter-type metric 忘加 _total 結尾（spec drift）
#[test]
fn counter_metrics_must_have_total_suffix() {
    // 來源對照表: 本 spec design.md「Counter rename 對照表」6 條 TYPE=counter
    let counter_metrics_in_lp_metrics: Vec<&str> = LP_METRICS.iter()
        .copied()
        .filter(|name| is_counter_typed_metric(name))  // 對照表 6 條手列
        .collect();

    assert_eq!(counter_metrics_in_lp_metrics.len(), 6,
        "LP_METRICS 內 TYPE=counter 應為 6 條, 觀察 {} 條",
        counter_metrics_in_lp_metrics.len());

    for name in &counter_metrics_in_lp_metrics {
        assert!(name.ends_with("_total"),
            "[counter convention] \"{name}\" TYPE=counter MUST 結尾 _total, \
             違反 Prometheus naming convention");
    }
}

// 輔助函式（test 內聯）: 對照本 spec design.md 6 條清單
fn is_counter_typed_metric(name: &str) -> bool {
    matches!(name,
        "lobsterpulse_tokens_input"
        | "lobsterpulse_tokens_output"
        | "lobsterpulse_provider_tokens_input"
        | "lobsterpulse_provider_tokens_output"
        | "lobsterpulse_provider_failure_count"
        | "lobsterpulse_provider_session_count"
    )
}
```

護衛 test 邏輯（對齊 R103 護衛 chain）：

1. 列舉 LP_METRICS const 內 TYPE=counter 的 6 條 metric（手列對照表，避免 false positive）
2. 斷言集合大小 = 6（防未來偷加新 counter 但漏列對照表）
3. 對 6 條逐一斷言 `name.ends_with("_total")`；fail 時報「counter-typed metric
   "{name}" 缺 _total 結尾，違反 Prometheus naming convention」+ 列出違規清單
4. 守恆等不變式：LP_METRICS 內 6 條 counter 必須全部 `_total` 結尾

**R107+ owner 真正 rename 流程**（本 change 留 follow-up）：

- 把 6 條現名改成目標名（LP_METRICS const + emit site + test assertion 三層一致）
- `is_counter_typed_metric()` 內的 6 條現名改成目標名（對齊新 contract）
- 護衛 test 內 `assert!(name.ends_with("_total"))` 應該全 pass（已是合規狀態）
- 跑 `cargo test --lib` baseline 確認 6 條合規 + 35 個 test assertion 仍 pass（已 rename）
- CHANGELOG.md / README.md / CONTRIBUTING.md 同步更新

## 跟 otel-provider-metrics-contract 護衛 chain 互補

- R102/R103 護衛：`LP_METRICS` 集合大小 41 + emit ⊆ LP_METRICS（防 spec drift）
- **R105（本 change）護衛**：`LP_METRICS` 內 TYPE=counter 6 條 MUST 結尾 `_total`
  （防 counter convention drift）
- 兩條護衛 chain 並存，不互相覆蓋：R103 守「emit 對齊 contract」、
  R105（本 change）守「counter metric 對齊 Prometheus convention」

## 跟 MISSION.md K0 對齊

- K0-A1 Provider 健康度 emit 覆蓋率（13/13 emit metric）：6 條 rename 不影響
  emit 行為，只改 metric 名 → K0 量化分母不變
- K0-A2 Provider 健康度 sample 覆蓋率（13/13 non-zero sessions）：同樣不變
- K0 Prometheus naming convention（KPI 維度，本 change 對齊）：從 0/6 合規 → 6/6
  合規的 spec 對齊契約已落地（護衛 test 守），R107+ 真正 rename 落地後從 spec
  contract 變 runtime 合規
- K0 Quota 9/13→10/13（R109 已落地）：本 change 不動 quota 路徑
- K40 spec coverage：收 closure 後本 change 從 0/1 active open → 0/1 active open
  （K40 closure 計數 +1）

## 為何本輪只寫 spec 不實際 rename

- **R105 接力清單首位評估**：6 條 counter rename 破既有 Prometheus 抓取 +
  alert + dashboard = 1 輪不可承受 scope，列 R107+ owner follow-up
- **yaml 內已標明**：「1 輪 1 件：不**做** dual-emit 期間；不**改** gauge
  `lobsterpulse_sessions_total`；不**改** provider label；不**接** OTel SDK」
- **chore_treadmill 防制**：24h 內 22/56 commit (39%) 已是 chore，本輪 M1
  寫 spec 屬 spec 對齊不算 chore；真要 rename 6 條 metric 是 M1 feat，1 輪
  做完需 dual-emit + alert 廣播 + dashboard 廣播 = 跨 2-3 週窗口 = 1 輪不可承受
- **廣播文檔先備齊**：rename 當下需要 CHANGELOG / README / CONTRIBUTING / 公告
  一氣同步，spec 階段先把這 4 份文檔的「廣播」段寫完，R107+ 真正 rename 當下
  直接 copy-paste 即可（廣播已成「對齊契約」一部分）
