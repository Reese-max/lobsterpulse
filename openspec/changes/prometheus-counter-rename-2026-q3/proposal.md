# Proposal: Prometheus Counter Rename `2026-Q3` (T-1 dual-emit shim)

## Goal

承接 R106 (2026-06-05) 已收 closure 的 `prometheus-counter-convention` spec 對齊契約，
走 T-1 週 dual-emit shim 實作 — 真正把 spec 對照表的 6 條現名 → 目標名
**雙名同時 emit** 到 `/metrics` 端點，讓 Prometheus 抓取端有 4 週觀察期
（T-2 ~ T-3）切換 scrape config + alert rule + Grafana dashboard。

T-1 週具體交付：
1. `LP_METRICS` const 41 → 47（6 條新 `_total` suffix 名加入 contract）
2. `render_prometheus_body` 對 6 條 counter 同時 emit 舊名 + 新名
   （舊名加 `# DEPRECATED` comment 標 owner 切換日）
3. R103 護衛 chain 延伸：`lp_metrics_contract_size_is_41` → `_is_47`
   （41 → 47）+ `_full_state_all_emits_in_lp_metrics_contract` 加 6 條
   dual-emit assertion（每條斷言舊名 + 新名兩條 sample line 都出現在 body）
4. K42 chain 17 條**不擴張**：新 assertion 寫進既有 R103 護衛 test，
   不開新護衛 test（避免 chain 17 → 18 破壞 R81 飽和契約）

對齊 R106 (2026-06-05) 接力清單首位：「prometheus-counter-rename-2026-q3:
開新 change 走實際 rename 6 條 metric (LP_METRICS const 6 row + emit + 35
test assertion)」，本 change 是 T-1 週（5 週時程第 1 週）的實作切入口，
T-2 ~ T-5 後續 owner follow-up 仍待 R114+ 接力。

## Background

R100 策略顧問 (2026-06-04) 風險 #1 + R102 開工 R103 對齊 26→41 條後，design.md
「Spec drift 候選」段明確列出 7 條違規 metric（含 1 條 gauge `sessions_total`
誤用 `_total` 結尾 + 6 條 counter 缺 `_total` 結尾）。R103 寫 spec 對齊契約時
**只列不修**（重命名 scope 太大，破既有 Prometheus 抓取 + alert + dashboard =
1 輪不可承受），列為 R104+ follow-up。

R105 (2026-06-05) 接力清單首位「6 條 counter 重命名 `_total` 結尾（破 Prometheus
抓取, 需先廣播 alert/dashboard 跟進）」，開 `prometheus-counter-convention/`
change 升級成正式 spec 對齊契約。R106 (2026-06-05) 收 closure（4 個 spec 檔 +
8/8 tasks [x] + .openspec.yaml status=closed），6 條現名 → 目標名對照表
+ 影響面盤點 + 5 週時程 + 護衛 test 設計全 closure。

R106 接力清單首位明確列出本 change 名稱 `prometheus-counter-rename-2026-q3`
與 T-1 dual-emit 範圍，本輪 R113 開工走 T-1 週實作。

對應到 MISSION.md K0：
> **K0 Provider 健康度覆蓋率**：13/13 provider 在 /metrics 端點 emit 過 ...
> 量測方式：Prometheus exporter 對應 metric 是否存在且有非零樣本。

metric 名稱遵循 Prometheus naming convention = 抓取端能正確 parse 為 counter
type（`rate()` / `increase()` 才會回有意義值）。T-1 dual-emit 期間雙名同時
emit，抓取端可在 T-2 ~ T-3 觀察期內漸進切換 scrape config，不會 silent break。

## Scope

### In Scope（T-1 dual-emit 週）

- 開新 `openspec/changes/prometheus-counter-rename-2026-q3/` change 資料夾
  （本 change，T-1 週實作切入口）
- 寫 4 個 spec 檔：`proposal.md` / `design.md` /
  `specs/prometheus-counter-rename-2026-q3/spec.md` / `tasks.md`
- `src-tauri/src/lib.rs` `LP_METRICS` const 加入 6 條新 `_total` suffix 名
  （對齊 R106 design.md 對照表，41 → 47）
- `src-tauri/src/lib.rs` `render_prometheus_body` 對 6 條 counter 同時 emit
  舊名 + 新名（舊名加 `# DEPRECATED` comment）
- 延伸 R103 護衛 chain 既有 2 條 test：
  - `lp_metrics_contract_size_is_41` → `_is_47`（41 → 47）
  - `render_prometheus_body_full_state_all_emits_in_lp_metrics_contract`：
    加 6 條 dual-emit assertion（每條斷言舊名 + 新名兩 sample line 同時出現）

### Out of Scope（1 輪 1 件紀律 + 5 週時程分階段）

- ❌ **T-2 抓取端 / alert rule / Grafana dashboard 廣播公告** — R114+
  owner follow-up，由 alert / dashboard owner 跟進，本 change 不動
- ❌ **T-3 監控窗口** — R115+ owner follow-up
- ❌ **T-4 切換日移除舊名 emit + 拿掉 LP_METRICS 舊 row** — R116+ owner follow-up
- ❌ **T-5 post-mortem** — R117+ owner follow-up
- ❌ **改 `lobsterpulse_sessions_total` (gauge 卻用 `_total` 反向違規)** —
  R106+ follow-up，不同 spec drift 類型
- ❌ **改 `provider` label 為 OTel `gen_ai.provider.name` 命名空間** —
  R103+ follow-up
- ❌ **接 OTel SDK** — R103+ follow-up
- ❌ **不**動 main.js（owner R90 WIP 留工作區）
- ❌ **不**動 6 untracked + 其他 openspec/changes/

## Capabilities

- `prometheus-counter-rename-2026-q3` — T-1 dual-emit shim 實作切入口
  （承接 R106 spec 對齊契約，走 5 週時程第 1 週）

## 與上游 spec 對齊契約

- R106 spec `prometheus-counter-convention` 已 closure（status=closed），
  6 條現名 → 目標名對照表為 source of truth
- R103 護衛 chain 已有 2 條 contract guard test，本 change 延伸既有 chain
  （K42 chain 17 條不擴張）

## 與 MISSION 對齊

- K0 Prometheus naming convention 維度：從 spec contract（0/6 合規契約）→ 6/6
  dual-emit 階段（runtime 6/6 真正合規 = T-4 切換日，本輪 T-1 走半程）
- K0-A1 Provider emit 覆蓋率 / K0-A2 sample 覆蓋率：本 change 不動
  emit 行為（雙名 emit，sample 數變 2x，K0-A1/A2 分母不變）
- K0 Quota 即時性：本 change 不動 quota 路徑
- K40 spec coverage：本 change 從 0/1 active open → 1/1 active open
  （T-1 週 spec 落地，後續 R114+ 接力 T-2 ~ T-5）
- K41 chore_treadmill：本輪 feat + 護衛 chain 延伸不算 chore 紀律
- K42 chain 17 條飽和：R103 chain 延伸不算新 chain（既有 test 增 assertion），
  chain 17 → 17 不擴張
