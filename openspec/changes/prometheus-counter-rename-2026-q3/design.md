# Design: Prometheus Counter Rename `2026-Q3` (T-1 dual-emit shim)

## Source of Truth

R106 (2026-06-05) spec closure 的 `prometheus-counter-convention/design.md`
「Counter rename 對照表」段是本 change 的 source of truth：

| # | 現名 | 目標 rename 名 |
|---:|---|---|
| 1 | `lobsterpulse_tokens_input` | `lobsterpulse_tokens_input_total` |
| 2 | `lobsterpulse_tokens_output` | `lobsterpulse_tokens_output_total` |
| 3 | `lobsterpulse_provider_tokens_input` | `lobsterpulse_provider_tokens_input_total` |
| 4 | `lobsterpulse_provider_tokens_output` | `lobsterpulse_provider_tokens_output_total` |
| 5 | `lobsterpulse_provider_failure_count` | `lobsterpulse_provider_failure_count_total` |
| 6 | `lobsterpulse_provider_session_count` | `lobsterpulse_provider_session_count_total` |

T-1 週 dual-emit 階段：6 條**同時 emit 舊名 + 新名**，LP_METRICS const 同時
列 6 條舊 row + 6 條新 row（共 12 row，總 41 + 6 = 47）。

## Code-level 變更面（T-1 週實作範圍）

### 1. `src-tauri/src/lib.rs` LP_METRICS const（41 → 47）

```rust
// R103 補齊 41 條 → R113 T-1 dual-emit 補齊 47 條（41 + 6 條 _total 新名）
const LP_METRICS: &[&str] = &[
    // 1. Session 數量 (4) — 不變
    "lobsterpulse_sessions_total",
    "lobsterpulse_sessions_active",
    "lobsterpulse_provider_sessions",
    "lobsterpulse_provider_active",
    // 2. Token accounting (4 → 8) — 4 條新 _total 加入
    "lobsterpulse_tokens_input",           // 舊名, T-4 切換日移除
    "lobsterpulse_tokens_input_total",     // R113 T-1 新名
    "lobsterpulse_tokens_output",          // 舊名, T-4 切換日移除
    "lobsterpulse_tokens_output_total",    // R113 T-1 新名
    "lobsterpulse_provider_tokens_input",           // 舊名, T-4 切換日移除
    "lobsterpulse_provider_tokens_input_total",     // R113 T-1 新名
    "lobsterpulse_provider_tokens_output",          // 舊名, T-4 切換日移除
    "lobsterpulse_provider_tokens_output_total",    // R113 T-1 新名
    // 3. Failure & health (3 → 4) — 1 條新 _total 加入
    "lobsterpulse_provider_failure_count",          // 舊名, T-4 切換日移除
    "lobsterpulse_provider_failure_count_total",    // R113 T-1 新名
    "lobsterpulse_provider_failure_to_completion_ratio",  // gauge, 不動
    "lobsterpulse_provider_success_rate",           // gauge, 不動
    // 4. Idle / freshness (7) — 不變
    "lobsterpulse_provider_idle_seconds",
    "lobsterpulse_provider_since_timestamp",
    "lobsterpulse_provider_quota_snapshot_age_seconds",
    "lobsterpulse_quota_history_csv_age_seconds",
    "lobsterpulse_provider_last_completed_session_age_seconds",
    "lobsterpulse_provider_idle_ratio",
    "lobsterpulse_provider_max_session_age_seconds",
    // 5. Session count / duration aggregates (13 → 14) — 1 條新 _total 加入
    "lobsterpulse_provider_session_count",          // 舊名, T-4 切換日移除
    "lobsterpulse_provider_session_count_total",    // R113 T-1 新名
    "lobsterpulse_provider_completed_sessions_total",
    // ... 既有 12 條不變
    // 6. Quota signal (1) — 不變
    "lobsterpulse_provider_quota_remaining_pct",
    // 7. Event / process accounting (9) — 不變
    "lobsterpulse_provider_events_total",
    // ... 既有 8 條不變
];
```

### 2. `src-tauri/src/lib.rs` `render_prometheus_body` 6 條 counter dual-emit

每條現有 counter emit block 之後加 1 個新 emit block（HELP/TYPE/sample 三件套），
舊名加 `# DEPRECATED` comment 標 owner 切換日。

範例（counter 1+2 聚合, 對應 `tokens_input`）：

```rust
// T-1 dual-emit 階段：emit 舊名 + 新名
// 舊名加 # DEPRECATED comment 標 owner 切換日（T-4, 預計 week 4 切換）
out.push_str(
    "# HELP lobsterpulse_tokens_input Lifetime input tokens across all providers \
     (DEPRECATED: use lobsterpulse_tokens_input_total, scheduled removal week 4)\n\
     # TYPE lobsterpulse_tokens_input counter\n"
);
out.push_str(&format!("lobsterpulse_tokens_input {tot_in}\n"));
// 新名加入：未來 R116+ T-4 切換日這條會留下、舊條會移除
out.push_str(
    "# HELP lobsterpulse_tokens_input_total Lifetime input tokens across all providers\n\
     # TYPE lobsterpulse_tokens_input_total counter\n"
);
out.push_str(&format!("lobsterpulse_tokens_input_total {tot_in}\n"));
```

per-provider 6 條（counter 3+4+5+6）也是同樣 dual-emit pattern：
- 舊名 `# DEPRECATED` 標頭 + 樣本行
- 新名 `# HELP` + `# TYPE` + 樣本行（值同舊名，從同 source 讀）

### 3. R103 護衛 chain 延伸（既有 2 條 test 增 assertion，0 新 test）

**Test A**: `lp_metrics_contract_size_is_41_matching_emit_paths` →
`lp_metrics_contract_size_is_47_matching_emit_paths`

```rust
assert_eq!(
    LP_METRICS.len(),
    47,
    "LP_METRICS 應為 47 條（R103 41 條 + R113 T-1 dual-emit 6 條新 _total 名）, \
     目前 {} 條",
    LP_METRICS.len()
);
```

**Test B**: `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract`
增 6 條 dual-emit assertion：

```rust
// R113 T-1 dual-emit 護衛：6 條 counter 必須同時 emit 舊名 + 新名
// 對齊 R106 design.md 對照表 6 條 + 5 週時程 T-1 階段
let dual_emit_pairs = [
    ("lobsterpulse_tokens_input", "lobsterpulse_tokens_input_total"),
    ("lobsterpulse_tokens_output", "lobsterpulse_tokens_output_total"),
    ("lobsterpulse_provider_tokens_input", "lobsterpulse_provider_tokens_input_total"),
    ("lobsterpulse_provider_tokens_output", "lobsterpulse_provider_tokens_output_total"),
    ("lobsterpulse_provider_failure_count", "lobsterpulse_provider_failure_count_total"),
    ("lobsterpulse_provider_session_count", "lobsterpulse_provider_session_count_total"),
];
for (legacy, total) in &dual_emit_pairs {
    assert!(
        body.contains(legacy) && body.contains(total),
        "T-1 dual-emit 必須 6 條 counter 同時 emit 舊名 + 新名, 缺一: 舊={legacy} 新={total}"
    );
}
```

護衛 chain 17 → 17（既有 R103 chain 延伸，0 新 chain）。

## 影響面盤點（T-1 週內不動的部分）

### 不動（保留給 T-2 ~ T-5 接力）

- ❌ 35 個既有 `body.contains("lobsterpulse_tokens_input ...")` 等 test
  assertion：本輪 T-1 不動（T-4 切換日才改：新名接管，舊名 assertion 改為
  新名）
- ❌ 抓取端 scrape config / metric_relabel_configs：T-2 廣播
- ❌ alert rule 內 `rate(lobsterpulse_tokens_input[5m])` 表達式：T-2 廣播
- ❌ Grafana dashboard panel query：T-2 廣播
- ❌ CHANGELOG.md / README.md / CONTRIBUTING.md deprecation 公告
  marker：T-0 (R106 已 closure)，T-4 切換日再補 REMOVED 段

### 1 輪 1 件紀律

- 本 change T-1 週只動 LP_METRICS const + render_prometheus_body + R103
  護衛 chain 延伸
- T-2 ~ T-5 後續 owner follow-up 開新 change 接力

## 廣播計劃時程（沿用 R106 design.md T-0..T-5）

```
T-0  week  0: 開新 change「prometheus-counter-rename-2026-q3」承接 R106 spec
                proposal/design/spec 已 closure, tasks 落地 4 層面 impact  (DONE R106)
T-0  week  0: 公告 CHANGELOG.md 「DEPRECATION: 6 條 metric _total suffix 將於 T+4 週切換」  (DONE R106)
T-1  week  1: 寫 dual-emit shim — render_prometheus_body 同時 emit 舊名 + 新名  (本輪 R113)
                舊名加 # DEPRECATED comment 標 owner
                R103 護衛 chain 延伸（size 41 → 47 + dual-emit assertion 6 條）
T-2  week  2: 廣播 alert / dashboard owner 跟進（issue / PR template / Discord 通知）
T-3  week  3: monitoring window — 觀察 dual-emit 期間舊名是否有 alert / dashboard
                仍未跟進（owner 主動聯繫 holdout）
T-4  week  4: 切換日 — 移除舊名 emit、LP_METRICS const 拿掉舊 row、
                # DEPRECATED comment 清掉、CHANGELOG 標「REMOVED: 舊 6 條 metric」
T-5  week  5: post-mortem — 觀察 1 週確認 0 broken alert / 0 broken dashboard
                （如果有 → 緊急 revert 對應 1 條 metric，列 R1XX+ follow-up）
```

## 跟 otel-provider-metrics-contract / R103 護衛 chain 互補

- R103 護衛：`LP_METRICS` 集合大小 N + emit ⊆ LP_METRICS
  - T-1 週：N = 47 (41 + 6 新名)
  - T-4 切換日：N = 41 (舊名移除，剩新名)
- R113 T-1 dual-emit 護衛：6 條 counter 必須同時 emit 舊名 + 新名
  - T-4 切換日撤銷（不再需要 dual-emit assertion）

兩個 chain 並存不互覆蓋：R103 守「emit 對齊 contract」、R113 T-1 守
「dual-emit 階段合規」。

## 為何本輪只走 T-1 不直接 rename

- R105 接力清單首位評估：6 條 counter rename 破既有 Prometheus 抓取 +
  alert + dashboard = 1 輪不可承受 scope
- 1 輪 1 件紀律：T-1 dual-emit shim 是 5 週時程第 1 週的合理切割點
  （41 → 47 contract + 6 條新 emit + 護衛延伸 = ~50 行 Rust 變更 + 4 spec 檔）
- T-2 ~ T-5 仍需 owner 接力（廣播 + 監控 + 切換 + post-mortem），不是
  1 輪能做完的全套
- R106 spec closure 早已確立 5 週時程 T-0..T-5，本 change 走 T-1 切入口
