# Proposal: Prometheus Counter Naming Convention (`_total` suffix)

## Goal

把 LobsterPulse 現有 **6 條** TYPE=counter 但**缺 `_total` 結尾**的 `lobsterpulse_*`
metric 收斂到一份**spec 對齊契約**：

1. 明確列出 6 條 counter 的**現名 → 目標 rename 名**對照
2. 列出**影響面盤點**（emit site / LP_METRICS const / 35 個 test assertion / 既有
   Prometheus 抓取 + alert 規則 + Grafana dashboard）
3. 寫**廣播計劃**（owner R107+ 真正 rename 前要廣播的事項：dual-emit 期間
   / 抓取端 rename window / alert rule rename / dashboard 改寫）
4. 加 **1 條新護衛 test**：未來新加 counter-type metric 必須以 `_total` 結尾
   （CI-visible failure，符合 R103 T-MET7 `lp_metrics_contract_size_is_41_matching_emit_paths`
    護衛 chain 風格）

對齊 R105 接力清單首位（2026-06-05 列出）：「6 條 counter 重命名 `_total` 結尾
（破 Prometheus 抓取, 需先廣播 alert/dashboard 跟進）」。
對齊 R103 design.md「Spec drift 候選」段（2026-06-05 列為 follow-up）— 本 change
把該段**升級為正式 spec 對齊契約**而非 doc 註解。

## Background

R100 策略顧問 (2026-06-04) 風險 #1 + R102 開工 R103 對齊 26→41 條後，design.md
「Spec drift 候選」段明確列出 7 條違規 metric（含 1 條 gauge `sessions_total`
誤用 `_total` 結尾 + 6 條 counter 缺 `_total` 結尾）。R103 寫 spec 對齊契約時
**只列不修**（重命名 scope 太大），列為 follow-up。

R104 (2026-06-05) 收 otel-provider-metrics-contract closure 時，本 follow-up
移交 R105+ 接力清單首位。R105 (2026-06-05) 開 `prometheus-counter-convention/`
change 但只開 .openspec.yaml + 空 specs/ 骨架，4 個 spec 檔（proposal/design/
spec/tasks）尚未落地。

對應到 MISSION.md K0：
> **K0 Provider 健康度覆蓋率**：13/13 provider 在 /metrics 端點 emit 過 ...
> 量測方式：Prometheus exporter 對應 metric 是否存在且有非零樣本。

metric 名稱遵循 Prometheus naming convention = 抓取端能正確 parse 為 counter
type（`rate()` / `increase()` 才會回有意義值）。6 條違規 metric 在 production
Grafana 跑 `rate(lobsterpulse_tokens_input[5m])` 會拿回 gauge-like 曲線（因為
沒有 `_total` 結尾 Prometheus client libraries 可能 fallback 成 unknown type
或 counter parser 對無 `_total` 名回錯）。

## Scope

### In Scope

- 開新 `openspec/changes/prometheus-counter-convention/` change 資料夾
  （R105 已開，本輪補 4 個 spec 檔）
- 寫 4 個 spec 檔：`proposal.md` / `design.md` / `specs/prometheus-counter-convention/spec.md` / `tasks.md`
- 6 條 counter 對照表（**現名 → 目標 rename 名**）：列在 `design.md`「Counter
  rename 對照表」段
- 影響面盤點：emit site 位置（lib.rs:2213-2255）、LP_METRICS const row（lib.rs:85-111）、
  35 個 test assertion site（lib.rs:4336-4569）
- 廣播計劃：列在 `proposal.md`「廣播」段 + `design.md`「廣播計劃時程」段
- 新護衛 test 設計：`counter_metrics_must_have_total_suffix` 1 條 test 斷言
  `LP_METRICS` 內所有 TYPE=counter 的 metric 必須以 `_total` 結尾，fail 報
  「counter-typed metric "{name}" 缺 _total 結尾」+ 列出違規清單

### Out of Scope（1 輪 1 件紀律）

- **不實際 rename 任何 code 內 metric 名**：LP_METRICS const、emit site、
  test assertion 都**不動**。本 change 是 spec 階段，僅寫對照契約 + 廣播計劃。
  實際 rename = 1 輪 1 件之外的 scope（破既有 Prometheus 抓取 + alert 規則 +
  Grafana dashboard 對該 6 條 metric 的查詢），列為 R107+ owner follow-up。
- **不做 dual-emit 期間**：rename 過渡期需要「舊名 + 新名同步 emit 2 個版本」
  讓抓取端有觀察期，1 輪不做（廣播計劃中描述窗口期與做法，但本輪不實作）。
- **不改 `lobsterpulse_sessions_total`** (gauge 卻用 `_total` 結尾)：此為**反向**
  違規（gauge 不該 `_total`），跟本 change 6 條 counter rename 方向相反，
  屬不同 spec drift 類型，留 R106+ follow-up。
- **不改 `provider` label 為 OTel `gen_ai.provider.name` 命名空間**：
  純 spec 對照預留 attribute 命名空間，不動現有 label，列 R103+ follow-up。
- **不接 OTel SDK**（`opentelemetry` / `opentelemetry-otlp` crate 整合）：
  純 spec 對齊不混 SDK 整合，列 R103+ follow-up。
- **不**動 main.js（owner R90 WIP 留工作區）
- **不**動 5 supervisor untracked + 其他 openspec/changes/

## Capabilities

- `prometheus-counter-convention` — LP `lobsterpulse_*` 6 條 counter-typed
  metric 補 `_total` 結尾的 spec 對齊契約（含 6 條對照表 / 影響面盤點 /
  廣播計劃 / 1 條新護衛 test 防未來新加 counter 不帶 `_total`）。

## 廣播（owner R107+ 真正 rename 前必走）

實際 rename 之前，必須對外廣播 4 個事項（不廣播 = 既有 Prometheus 抓取 + alert
規則 + Grafana dashboard 全部 break，runtime 監控盲區）：

1. **抓取端 rename 窗口期**：dual-emit 期間長度（建議 2 個 minor release =
   約 4 週），讓既有 Prometheus server 有時間切換 scrape config
2. **alert 規則 rename**：6 條 metric 對應的所有 `rate()` / `increase()` / `sum()`
   表達式必須同步改 query（含 rule file `.rules.yml` / alertmanager 設定）
3. **Grafana dashboard panel rewrite**：6 條 metric 在 dashboard 內的 panel
   query / panel title / legend 格式（變數 `${__name__}` pattern）要同步改
4. **deprecation 公告**：CHANGELOG / README / CONTRIBUTING.md 加
   「Prometheus metric rename notice」段，標 owner 與切換日

廣播完成後，owner R107+ 開新 change 走真正 rename（LP_METRICS const + 6 emit
site + 35 test assertion 一併改 + 廣播文檔同步更新）。
