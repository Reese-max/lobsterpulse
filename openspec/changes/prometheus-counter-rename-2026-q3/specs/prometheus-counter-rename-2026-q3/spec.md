# Spec: Prometheus Counter Rename `2026-Q3` (T-1 dual-emit shim)

> 對應 capability：`prometheus-counter-rename-2026-q3`（見 proposal.md Capabilities 段）。
> Source of truth：R106 已 closure 的 `prometheus-counter-convention/spec.md`
> Requirements + Scenarios（6 條對照表 + 護衛 test 設計）。本 spec 走 T-1 週
> dual-emit shim 實作切入口，T-2 ~ T-5 後續 owner follow-up。

## ADDED Requirements

### Requirement: R-PCR1 — T-1 dual-emit 階段 LP_METRICS const 41 → 47

> 對應 R106 spec 對齊契約 R-1「6 條 counter 必須 `_total` 結尾」。

T-1 dual-emit 階段，`LP_METRICS` const 必須包含 R106 design.md 對照表的 6 條
counter 的**舊名 + 新名**（共 12 row 增量 6 row，總 41 + 6 = 47）：

- `lobsterpulse_tokens_input` + `lobsterpulse_tokens_input_total`
- `lobsterpulse_tokens_output` + `lobsterpulse_tokens_output_total`
- `lobsterpulse_provider_tokens_input` + `lobsterpulse_provider_tokens_input_total`
- `lobsterpulse_provider_tokens_output` + `lobsterpulse_provider_tokens_output_total`
- `lobsterpulse_provider_failure_count` + `lobsterpulse_provider_failure_count_total`
- `lobsterpulse_provider_session_count` + `lobsterpulse_provider_session_count_total`

#### Scenario

- **S-PCR1.1**: T-1 週 `LP_METRICS.len() == 47`（R103 護衛 chain 既有 size
  test 41 → 47），R103 chain `lp_metrics_contract_size_is_41` 改名為
  `_is_47`
- **S-PCR1.2**: 6 條新 `_total` 名 100% 出現在 LP_METRICS const（防漏列）
- **S-PCR1.3**: T-4 切換日後，6 條舊名從 LP_METRICS const 移除（`LP_METRICS.len()`
  回到 41），R114+ owner follow-up

### Requirement: R-PCR2 — T-1 dual-emit 階段 `render_prometheus_body` 雙名 emit

> 對應 R106 design.md 廣播計劃時程 T-1 段。

T-1 週，`render_prometheus_body` 必須對 6 條 counter 同時 emit 舊名 + 新名，
讓 Prometheus 抓取端有 4 週觀察期（T-2 ~ T-3）切換 scrape config。舊名 emit
時 HELP comment 必須標 `# DEPRECATED: use {new_name}, scheduled removal week 4`，
讓抓取端 operator 一眼能識別。

#### Scenario

- **S-PCR2.1**: 6 條 counter 對應的 sample line（HELP/TYPE/sample 三件套）
  在 body 內**同時出現**舊名 + 新名，值同 source（從同 map 讀），R113
  dual-emit assertion 守
- **S-PCR2.2**: 舊名 emit 的 HELP comment 含 `# DEPRECATED:` 標記，標明
  scheduled removal = week 4
- **S-PCR2.3**: T-4 切換日後舊名 emit block 移除（render_prometheus_body
  只 emit 新名），R114+ owner follow-up

### Requirement: R-PCR3 — R103 護衛 chain 延伸，0 新護衛 chain

> 對齊 K42 chain 17 條飽和契約。

T-1 dual-emit 護衛必須**延伸既有 R103 護衛 chain**，不開新護衛 chain（K42
chain 17 條不擴張）。具體：R103 chain 既有 2 條 test 增 assertion：

- `lp_metrics_contract_size_is_41` → `_is_47`（41 → 47）
- `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract`：
  加 6 條 dual-emit assertion（每條斷言舊名 + 新名兩 sample line 同時出現）

#### Scenario

- **S-PCR3.1**: K42 chain 計數 = 17（T-1 週內不變），owner/spectra 護衛 test
  守 17 條飽和
- **S-PCR3.2**: R103 護衛 chain `lp_metrics_contract_size_is_47` 1/1 pass
- **S-PCR3.3**: R103 護衛 chain `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract`
  加 6 條 dual-emit assertion 1/1 pass

### Requirement: R-PCR4 — 1 輪 1 件紀律，T-2 ~ T-5 後續 owner 接力

> 對齊 R106 spec closure 5 週時程分階段 + CLAUDE.md「一輪一件事」紀律。

T-1 週（`prometheus-counter-rename-2026-q3` change）只動：

- LP_METRICS const 41 → 47（6 條新 row）
- render_prometheus_body 6 條 counter dual-emit block
- R103 護衛 chain 既有 2 條 test 增 assertion

**不動**：

- 35 個既有 `body.contains("lobsterpulse_tokens_input ...")` 等 test
  assertion（T-4 切換日才改）
- 抓取端 scrape config / metric_relabel_configs（T-2 廣播）
- alert rule / Grafana dashboard（T-2 廣播）
- `lobsterpulse_sessions_total` gauge 反向違規（R106+ follow-up）

#### Scenario

- **S-PCR4.1**: T-1 週 spec 落地 1 輪 1 件，0 scope 蔓延到 T-2 ~ T-5 範圍
- **S-PCR4.2**: T-2 ~ T-5 後續 owner 接力清單明確列出（owner follow-up
  不混進本 change），engineering-log 落地紀錄含 R114+ 接力首位
