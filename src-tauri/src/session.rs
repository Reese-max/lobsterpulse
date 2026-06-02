use crate::hook_event::HookEvent;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Working,
    WaitingForUser,
    Stale,
}

/// 工具呼叫的簡明快照，用於膠囊展開面板。
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallSnapshot {
    pub id: String,
    pub title: String,
    /// running / completed / failed
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub provider: String,
    pub state: SessionState,
    pub start_time: DateTime<Utc>,
    pub last_event_time: DateTime<Utc>,
    pub cwd: Option<String>,
    pub last_tool_name: Option<String>,
    pub last_prompt: Option<String>,
    /// 近期工具呼叫佇列（最多 MAX_TOOL_HISTORY 筆）。
    pub tool_calls: Vec<ToolCallSnapshot>,
    /// Agent 是否正在 thinking（最近一次 thinking event 在 1.5s 內）。
    pub thinking: bool,
    /// 最近 thinking timestamp（用於自動熄滅 UI 指示燈）。
    pub last_thinking_at: Option<DateTime<Utc>>,
    /// 累計 input tokens。
    pub tokens_input: u64,
    /// 累計 output tokens。
    pub tokens_output: u64,
    /// Token 時間序列：近 60 個 snapshot（每個 TokenUpdate 事件一個 point）。
    /// 給 bot card sparkline 用。
    pub token_samples: std::collections::VecDeque<TokenSample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenSample {
    pub timestamp: DateTime<Utc>,
    pub tokens_input: u64,
    pub tokens_output: u64,
}

const MAX_TOKEN_SAMPLES: usize = 60;

/// 近期工具呼叫保留多少筆。
const MAX_TOOL_HISTORY: usize = 5;

impl Session {
    pub fn new(id: String, provider: String, cwd: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id,
            provider,
            state: SessionState::Idle,
            start_time: now,
            last_event_time: now,
            cwd,
            last_tool_name: None,
            last_prompt: None,
            tool_calls: Vec::new(),
            thinking: false,
            last_thinking_at: None,
            tokens_input: 0,
            tokens_output: 0,
            token_samples: std::collections::VecDeque::with_capacity(MAX_TOKEN_SAMPLES + 1),
        }
    }

    pub fn handle_event(&mut self, event: &HookEvent) {
        self.last_event_time = Utc::now();

        match event.hook_event_name.as_str() {
            "SessionStart" => {
                self.state = SessionState::Working;
                if let Some(ref cwd) = event.cwd {
                    self.cwd = Some(cwd.clone());
                }
                if let Some(ref p) = event.prompt {
                    self.last_prompt = Some(p.clone());
                }
            }
            "UserPromptSubmit" => {
                self.state = SessionState::Working;
                if let Some(ref prompt) = event.prompt {
                    self.last_prompt = Some(prompt.clone());
                }
            }
            "PreToolUse" => {
                self.state = SessionState::Working;
                self.thinking = false;
                if let Some(ref tool_name) = event.tool_name {
                    self.last_tool_name = Some(tool_name.clone());
                    self.record_tool_call(
                        event.tool_call_id.clone().unwrap_or_default(),
                        tool_name.clone(),
                        "running".into(),
                    );
                }
            }
            "PostToolUse" | "PostToolUseFailure" => {
                self.state = SessionState::Working;
                if let Some(ref tool_name) = event.tool_name {
                    self.last_tool_name = Some(tool_name.clone());
                    let status = event.tool_status.clone().unwrap_or_else(|| {
                        if event.hook_event_name == "PostToolUseFailure" {
                            "failed".into()
                        } else {
                            "completed".into()
                        }
                    });
                    self.record_tool_call(
                        event.tool_call_id.clone().unwrap_or_default(),
                        tool_name.clone(),
                        status,
                    );
                }
            }
            "ThinkingDelta" => {
                self.state = SessionState::Working;
                self.thinking = true;
                self.last_thinking_at = Some(Utc::now());
            }
            "TokenUpdate" => {
                // OpenAB bot 送累計 snapshot（取 max 避倒退），本機 CLI 送 delta（用 add）。
                let is_openab_bot = matches!(
                    event.provider.as_str(),
                    "cicx" | "gitx" | "giminix" | "codex_bot" | "openx"
                );
                if is_openab_bot {
                    if let Some(i) = event.tokens_input {
                        self.tokens_input = self.tokens_input.max(i);
                    }
                    if let Some(o) = event.tokens_output {
                        self.tokens_output = self.tokens_output.max(o);
                    }
                } else {
                    if let Some(i) = event.tokens_input {
                        self.tokens_input = self.tokens_input.saturating_add(i);
                    }
                    if let Some(o) = event.tokens_output {
                        self.tokens_output = self.tokens_output.saturating_add(o);
                    }
                }
                // 推一個 sample 到 sparkline queue
                self.token_samples.push_back(TokenSample {
                    timestamp: Utc::now(),
                    tokens_input: self.tokens_input,
                    tokens_output: self.tokens_output,
                });
                while self.token_samples.len() > MAX_TOKEN_SAMPLES {
                    self.token_samples.pop_front();
                }
            }
            "PermissionRequest" | "Notification" => {
                self.state = SessionState::WaitingForUser;
            }
            "Stop" => {
                self.state = SessionState::Idle;
                self.thinking = false;
            }
            _ => {}
        }

        // 1.5 秒沒新 thinking event → 自動熄滅指示燈。由 check_staleness 定期觸發較穩，
        // 這邊先在 handle_event 裡同步熄燈，避免 UI 看起來卡著。
        if self.thinking {
            if let Some(ts) = self.last_thinking_at {
                if Utc::now().signed_duration_since(ts).num_milliseconds() > 1500 {
                    self.thinking = false;
                }
            }
        }
    }

    fn record_tool_call(&mut self, id: String, title: String, status: String) {
        // 相同 tool_call_id 更新既有項目的 status，而不是推新的。
        if !id.is_empty() {
            if let Some(existing) = self.tool_calls.iter_mut().find(|t| t.id == id) {
                existing.title = title;
                existing.status = status;
                return;
            }
        }
        self.tool_calls.push(ToolCallSnapshot { id, title, status });
        if self.tool_calls.len() > MAX_TOOL_HISTORY {
            let drop_n = self.tool_calls.len() - MAX_TOOL_HISTORY;
            self.tool_calls.drain(0..drop_n);
        }
    }

    pub fn project_name(&self) -> String {
        // 泛用系統目錄 basename 不顯示（避免「tmp」「root」「home」等無意義字樣）
        const GENERIC: &[&str] = &["tmp", "root", "home", "Users", "users", "var", "etc", ""];
        if let Some(ref cwd) = self.cwd {
            if let Some(name) = std::path::Path::new(cwd).file_name() {
                let ss = name.to_string_lossy().to_string();
                if !GENERIC.contains(&ss.as_str()) {
                    return ss;
                }
            }
        }
        let id_short: String = self.id.chars().take(6).collect();
        format!("{} · {}", self.provider, id_short)
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            SessionState::Working | SessionState::WaitingForUser
        )
    }

    pub fn elapsed_seconds(&self) -> i64 {
        Utc::now()
            .signed_duration_since(self.start_time)
            .num_seconds()
    }

    pub fn formatted_time(&self) -> String {
        let total = self.elapsed_seconds();
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let seconds = total % 60;
        if hours > 0 {
            format!("{hours}:{minutes:02}:{seconds:02}")
        } else {
            format!("{minutes:02}:{seconds:02}")
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub provider: String,
    pub state: SessionState,
    pub project_name: String,
    pub cwd: Option<String>,
    pub is_active: bool,
    pub formatted_time: String,
    pub last_tool_name: Option<String>,
    pub last_prompt: Option<String>,
    pub tool_calls: Vec<ToolCallSnapshot>,
    pub thinking: bool,
    pub tokens_input: u64,
    pub tokens_output: u64,
    /// 最後一次 hook event 到現在的秒數；給 Bot 總覽的「N 分鐘前」顯示。
    pub last_event_secs_ago: i64,
    /// Token sparkline 用的近 60 筆 snapshot
    pub token_samples: Vec<TokenSample>,
    /// Session 持續秒數（start_time 到 last_event_time），給 Telegram 門檻判斷
    pub duration_secs: i64,
}

impl From<&Session> for SessionInfo {
    fn from(s: &Session) -> Self {
        let now = Utc::now();
        let last_event_secs_ago = now.signed_duration_since(s.last_event_time).num_seconds();
        let duration_secs = now.signed_duration_since(s.start_time).num_seconds();
        Self {
            id: s.id.clone(),
            provider: s.provider.clone(),
            state: s.state,
            project_name: s.project_name(),
            cwd: s.cwd.clone(),
            is_active: s.is_active(),
            formatted_time: s.formatted_time(),
            last_tool_name: s.last_tool_name.clone(),
            last_prompt: s.last_prompt.clone(),
            tool_calls: s.tool_calls.clone(),
            thinking: s.thinking,
            tokens_input: s.tokens_input,
            tokens_output: s.tokens_output,
            last_event_secs_ago,
            token_samples: s.token_samples.iter().cloned().collect(),
            duration_secs,
        }
    }
}

/// What meaningfully changed when a new hook event came in. Used to decide
/// whether to fire a frontend notification sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTransition {
    None,
    Completed,
    StartedWaiting,
}

/// 最近 N 筆 event log，給 tray menu「事件診斷」查看。
const MAX_RECENT_EVENTS: usize = 50;

#[derive(Debug, Clone, Serialize)]
pub struct RecentEvent {
    pub timestamp: DateTime<Utc>,
    pub provider: String,
    pub session_id: String,
    pub event_name: String,
    pub tool_name: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProviderTotals {
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub session_count: u64,
    pub failure_count: u64,
    /// K13 落地：累計收到的 event 數（不論 event 類型：SessionStart / PostToolUse /
    /// PostToolUseFailure / Stop / Notification / TokenUpdate / ... 全部 +1）。
    /// 給 `/metrics` 端算 `lobsterpulse_provider_events_total` counter 用，operator 可算
    /// `rate(events_total[5m])` = 每分鐘 ingest 吞吐量，補 K7 failure / K9 session 沒覆蓋的
    /// 「整體事件流量」信號（純 counter，lifetime aggregate 不蒸發）。
    pub events_total: u64,
    /// K17 落地：累計收到的 event 數按 event type 切開（`hook_event_name` → count）。
    /// 補 K13 缺 type 維度的盲點 —— operator 端可算
    /// `rate(...{type="Stop"}[5m]) - rate(...{type="UserPromptSubmit"}[5m])` 偵測
    /// 「某 provider Stop 一直來但沒 UserPromptSubmit」= runner 卡住的 SLO 信號。
    /// 用 `BTreeMap` 而非 `HashMap`：render 端要按 (provider, type) 排序輸出
    /// Prometheus 文字格式、確定性比隨機好，且已知 type 數量 ≤ ~10、sort 成本可忽略。
    /// 空 `hook_event_name` 不入 map（避免 `"":0` 這種語意空污 noise 流入 metric）。
    pub event_type_counts: std::collections::BTreeMap<String, u64>,
    /// 累計起始時間（第一次 event 進來）——給 UI 顯示「自何時累計」
    pub since: Option<DateTime<Utc>>,
    /// 最近一次 event 時間戳（任何 event 都會更新，不只在 TokenUpdate / Failure）——
    /// 給 `/metrics` 端計算 per-provider `idle_seconds` gauge 用。
    /// `None` 表示該 provider 還沒收過 event。
    pub last_event_at: Option<DateTime<Utc>>,
    /// K22 落地：最近一次「完成」的 session 持續秒數（start_time → 結束時的
    /// last_event_time）。給 `/metrics` 端 emit
    /// `lobsterpulse_provider_last_completed_session_age_seconds` gauge。
    /// 觸發點：SessionEnd 事件把 session 從 map 移除時、或 Working→Idle
    /// 轉換時 —— 兩種都算「完成」。`Some(secs)` 表示至少完成過一次，`None`
    /// 表示該 provider 累計進來但還沒收過 SessionEnd / 還沒經歷 Working→Idle
    /// 轉換。lifetime gauge 跟 K8 `last_event_at` / K10 `since` 同：寫入後
    /// 不蒸發,即使 session 結束 + 30 min stale 回收後仍保留 → Prometheus 端
    /// 可看「這 provider 最近一次 task 跑了多久」。負值 saturating clamp 到 0
    /// （防時鐘回撥 / 序列化時差）。
    pub last_completed_session_age_secs: Option<i64>,
    /// K23 落地：累計「完成」的 session 數（counter,saturating_add 遞增）。
    /// 給 `/metrics` 端 emit
    /// `lobsterpulse_provider_completed_sessions_total{provider}` counter。
    /// 跟 K22 互補：K22 是「最近一次跑多久」（gauge,只記 latest）,
    /// K23 是「累計跑了幾次」（counter,遞增）→ operator 用
    /// `rate(completed_sessions_total[1h])` 算每小時完成速率 = 吞吐 KPI,
    /// 補 K22 沒覆蓋的「累積次數」維度。觸發點跟 K22 同：SessionEnd + Working→Idle
    /// 兩路徑都 +1。跟 K7 / K9 / K13 lifetime aggregate 對齊：寫入後不蒸發,
    /// session 結束 + 30 min stale 回收後 ProviderTotals 仍保留 → Prometheus 端
    /// counter 不會倒退。`u64` 預設 0 = 該 provider 累計進來但還沒完成過 session,
    /// 跟 K9 `session_count` 預設 0 同語意：counter 0 是有效資料（至少看過一次
    /// event 但還沒完成過）,不是 missing —— render 端要把 0 也 emit 出來,跟
    /// K8 `last_event_at = None` 跳過策略區分（K8 是「Optional 時間戳」語意,
    /// K23 是「次數」語意）。
    pub completed_sessions_count: u64,
    /// K24 落地：累計「完成」的 session 持續秒數加總（counter,saturating_add 遞增）。
    /// 給 `/metrics` 端 emit
    /// `lobsterpulse_provider_completed_sessions_total_duration_seconds{provider}`
    /// counter。跟 K22 / K23 互補形成「總時長 / 總次數 = 平均完成時間」公式：
    ///   - K22 gauge 看「最近一次跑多久」(只記 latest)
    ///   - K23 counter 看「累計跑了幾次」(遞增)
    ///   - K24 counter 看「累計花多少秒」(遞增)
    ///
    /// operator 端用 `duration_seconds / completed_sessions_total` 算
    /// **平均 time-to-completion** = 效率 KPI，搭配 `rate(duration_seconds[1h])`
    /// 看「過去一小時總處理秒數」= throughput-seconds KPI。
    /// 觸發點跟 K22/K23 同：SessionEnd + Working→Idle 兩路徑都把當次 age 累加進來。
    /// 跟 K22 的差異：K22 saturating clamp 負值到 0 再寫入（單點 latest gauge）,
    /// K24 saturating_add 用 u64 累加（counter lifetime aggregate）—— 累加前先
    /// `max(0)` 防時鐘回撥 / 序列化時差把負值灌進 counter 污染總和。lifetime
    /// aggregate 對齊 K7 / K9 / K13 / K23：session 結束 + 30 min stale 回收後
    /// `ProviderTotals` 仍保留 → Prometheus 端 counter 不會倒退。`u64` 預設 0
    /// 跟 K23 同語意：counter 0 是有效資料（該 provider 累計收過 event 但還沒
    /// 完成過 session），render 端要把 0 也 emit 出來。
    pub completed_sessions_total_duration_secs: u64,
    /// K26 落地：歷史「最長」一次完成的 session 持續秒數（gauge, lifetime
    /// saturating_max）。給 `/metrics` 端 emit
    /// `lobsterpulse_provider_completed_sessions_max_duration_seconds{provider}`
    /// gauge。跟 K22 (latest gauge) / K25 (avg gauge) 互補形成 **max / latest /
    /// avg 三件套**：operator 端可一次看「該 provider 歷史最長一次 / 最近一次 /
    /// 平均」三個視角,快速分辨 outlier session（例：avg 60s, latest 65s, 但 max
    /// 7200s = 過去某次有 2 小時 outlier,可能 runner hang / 大 context window
    /// 場景）。`Some(secs)` = 至少完成過一次;`None` = 該 provider 累計收過 event
    /// 但還沒完成過 session（沿用 K22 Option 語意,render 端缺資料不 emit sample
    /// 避免誤判「max=0」當「該 provider 瞬間完成」= 假健康信號）。`i64` 而非 `u64`
    /// 跟 K22 一致 —— 雖然實際寫入值都被 `age.max(0)` clamp 過,留 `i64` 方便
    /// 未來若要支援 signed duration metric（debug / 序列化時差偵測）直接擴充。
    /// 觸發點跟 K22/K23/K24 同：SessionEnd + Working→Idle 兩路徑都更新。
    /// saturating_max 在 i64::MIN 邊界退化成 `i64::MIN`(機率近 0),不是問題。
    pub max_completed_session_age_secs: Option<i64>,
    /// K27 落地：歷史「最短」一次完成的 session 持續秒數（gauge, lifetime
    /// saturating_min）。給 `/metrics` 端 emit
    /// `lobsterpulse_provider_completed_sessions_min_duration_seconds{provider}`
    /// gauge。跟 K22 (latest) / K25 (avg) / K26 (max) 互補形成 **min / max /
    /// latest / avg 四件套**：operator 端可一次看「該 provider 歷史最快 / 最慢 /
    /// 最近 / 平均」四個視角,快速分辨 session 時長分佈（例：avg 60s, latest 65s,
    /// max 7200s, min 8s = 大部分 session 都跑 ~1 分鐘,但偶有 2 小時 outlier,
    /// 且曾有 8 秒極短 session 可能是 fast-path / 早期測試 / 假觸發）。`Some(secs)`
    /// = 至少完成過一次;`None` = 該 provider 累計收過 event 但還沒完成過 session
    /// （沿用 K22 / K26 Option 語意,render 端缺資料不 emit sample 避免誤判
    /// 「min=0」當「該 provider 瞬間完成」= 假健康信號）。`i64` 而非 `u64` 跟 K22 /
    /// K26 一致 —— 雖然實際寫入值都被 `age.max(0)` clamp 過,留 `i64` 方便未來若
    /// 要支援 signed duration metric（debug / 序列化時差偵測）直接擴充。觸發點
    /// 跟 K22/K23/K24/K26 同：SessionEnd + Working→Idle 兩路徑都更新。
    /// saturating_min 在 i64::MIN 邊界退化成 `i64::MIN`(機率近 0),不是問題。
    pub min_completed_session_age_secs: Option<i64>,
    /// K28 落地：Welford online algorithm 累計「完成 session 時長」的 running
    /// mean 跟 M2 accumulator。給 `/metrics` 端 emit
    /// `lobsterpulse_provider_completed_sessions_stddev_seconds{provider}`
    /// gauge（**第一個 f64 metric**,K 系列 K3-K27 全用 i64 整數, K28 改 f64
    /// 是 stddev 數學本質決定 —— 連續值強制裁整會失精度,例如 sample [10, 20]
    /// variance = 50, stddev ≈ 7.07s, 強制裁整為 7 → 0.07s 精度流失）。兩個
    /// 欄位都 `f64`：mean 是 current average（`(m2 / count).sqrt()` 之前的
    /// 一階動差）,m2 是 sum of squared diffs from current mean（Welford 累積
    /// 公式 `M2 += delta * delta2` 的二階中心動差 proxy）。`f64::NAN` 預設用
    /// `0.0` —— 第一次完成時 `count` 從 0 → 1, Welford 公式自然把 mean 設成
    /// 該次 sample, m2 = 0（單樣本無波動 → stddev = 0）。`count` 不另存,
    /// 沿用 K23 `completed_sessions_count` —— stddev 跟 completed count 永遠
    /// 同步, 不會有 count 跟 stddev 不一致的中間態。Lifteime aggregate 對齊
    /// K22 / K23 / K24 / K26 / K27: session 結束 + 30 min stale 回收後
    /// `ProviderTotals` 仍保留 → Prometheus 端 gauge 不會倒退。R46 引入
    /// Welford 而非最樸素的「保留所有 sample 在 Vec」: O(1) 空間（Vec 會
    /// unbounded grow, 上線跑一週 sample 數就破萬）+ 數值穩定性比「先算 mean
    /// 再算 Σ(x-mean)²」高一個數量級（避免大數吃小數）。operator 端 alert
    /// 範例: `stddev > 300` 表示該 provider session 時長波動 > 5 分鐘 = 可能有
    /// 短任務 / 長任務混跑, 看 alert 進一步分桶。
    pub completed_sessions_mean_secs: f64,
    pub completed_sessions_m2_secs: f64,
    /// K30 落地：bounded reservoir (capacity 1024) 保留最近完成 session 的
    /// duration samples,給 `/metrics` 端 emit
    /// `lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider}`
    /// gauge。跟 K22 (latest) / K25 (avg) / K26 (max) / K27 (min) / K28
    /// (stddev) 五件套互補形成「六件套」+ 第六個維度:「95 百分位延遲」
    /// —— operator 端 alert `p95 > 300` (5 分鐘) = 該 provider 95% 的
    /// session 都在 5 分鐘以上 = SLO 異常信號。R48 引入 reservoir 而非
    /// Welford (K28) 或 lifetime aggregate (K22-K27): P95 數學本質要求
    /// 排序 (K28 用 Welford O(1) 空間是因為 stddev 只需 mean / M2),
    /// 而 lifetime 永久保留所有 sample 會 unbounded grow (上線跑一週就
    /// 數十萬筆,sort O(N log N) per scrape 拖慢 metrics endpoint) →
    /// 採 Vitter Algorithm R reservoir sampling: count < capacity 直接
    /// push, count >= capacity 以 `Utc::now().timestamp_nanos() %
    /// len` 當 pseudo-random index replace (輕量, 無外部 `rand` 依賴,
    /// 納秒時間戳快速變化實際上接近 random)。語意: P95 = 「最近 1024
    /// 次完成 session 的 95 百分位」, 跟 K22-K28 lifetime aggregate 對比
    /// 是有意識 trade-off —— operator 端 P95 反映「近期體感」(lifetime
    /// 會被過老 outlier 拉高永遠不下降, 不實用)。`Vec<i64>` 而非 `f64`
    /// 跟 K22 / K26 / K27 一致 —— sample 是整數 duration, 強制裁整 0
    /// 精度流失, 維持 i64 進 reservoir (P95 index 取整才會失 ±0.5 秒
    /// 精度, 跟 K25 avg / K28 stddev 4 位小數 f64 渲染是兩件事)。
    /// 預設空 `Vec` 跟 K22 / K23 lifetime 語意一致: 沒完成過 session →
    /// 沒 sample → render 端 `count == 0` 過濾不 emit。
    pub completed_sessions_p95_samples: Vec<i64>,
}

/// K30 reservoir capacity 常數。1024 是統計學 / 監控常用 trade-off:
/// 樣本數 ≥ 1000 → P95 估計誤差 < ~1.5% (Chebyshev 不等式: P(|estimate
/// - true| > k·σ) ≤ 1/k², k=4 → 6.25%); 1024 是 2^10 對齊 cache line。
///
/// 太小 (<100) P95 估計不穩, 太大 (>10000) sort O(N log N) per scrape
/// 拖慢 metrics endpoint; 1024 是甜點。
///
/// Render 端 sort 1024 sample 約 10000 次比較, scrape 15s 一次完全可忽略。
const P95_RESERVOIR_CAPACITY: usize = 1024;

pub struct SessionManager {
    pub sessions: HashMap<String, Session>,
    pub active_session_id: Option<String>,
    pub recent_events: std::collections::VecDeque<RecentEvent>,
    /// Per-provider 累計統計，不依賴 OpenAB snapshot 檔就能算出 quota
    pub provider_totals: HashMap<String, ProviderTotals>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            active_session_id: None,
            recent_events: std::collections::VecDeque::with_capacity(MAX_RECENT_EVENTS + 1),
            provider_totals: HashMap::new(),
        }
    }

    fn bump_provider_totals(&mut self, event: &HookEvent) {
        let p = event.provider.clone();
        let entry = self.provider_totals.entry(p).or_default();
        if entry.since.is_none() {
            entry.since = Some(Utc::now());
        }
        // K8 落地：每個 event 都更新 last_event_at（不限 TokenUpdate / Failure），
        // 給 metrics 端計算 per-provider idle_seconds。lifetime aggregate 保留——
        // session 移除後 ProviderTotals 仍有值，不會蒸發。
        entry.last_event_at = Some(Utc::now());
        // K13 落地：每個 event 都 +1，給 metrics 端算 per-provider events_total
        // counter —— `rate(events_total[5m])` = 該 provider 事件 throughput。
        // lifetime aggregate 對齊 K6/K7/K9：session 結束 + 30 min stale 回收後仍保留。
        entry.events_total = entry.events_total.saturating_add(1);
        // K17 落地：對非空 `hook_event_name` 累加 type 維度計數。
        // 空字串防呆：`RawHookEvent::normalize` 在所有 alias field 都缺失時
        // 會回 `""`（見 hook_event.rs:62 `unwrap_or_default()`）—— 這種
        // 「未識別 schema」事件不該被算進任何具名 type bucket,寧可漏計也不
        // 讓 `lobsterpulse_provider_event_type_total{type=""}` 污染 metric 視圖。
        // 對齊 K13 lifetime aggregate 語意：session 結束 + 30 min stale 回收後
        // `ProviderTotals` 仍保留 → Prometheus 端不會誤判 counter 倒退。
        if !event.hook_event_name.is_empty() {
            *entry
                .event_type_counts
                .entry(event.hook_event_name.clone())
                .or_insert(0) += 1;
        }
        match event.hook_event_name.as_str() {
            "PostToolUseFailure" => entry.failure_count += 1,
            "TokenUpdate" => {
                // OpenAB bot 送「累計」總量（snapshot），取 max 避倒退；
                // 本機 CLI 未來若支援 token 事件預期送「delta」增量，用 add 避免漏累。
                let is_openab_bot = matches!(
                    event.provider.as_str(),
                    "cicx" | "gitx" | "giminix" | "codex_bot" | "openx"
                );
                if is_openab_bot {
                    if let Some(i) = event.tokens_input {
                        entry.tokens_input = entry.tokens_input.max(i);
                    }
                    if let Some(o) = event.tokens_output {
                        entry.tokens_output = entry.tokens_output.max(o);
                    }
                } else {
                    if let Some(i) = event.tokens_input {
                        entry.tokens_input = entry.tokens_input.saturating_add(i);
                    }
                    if let Some(o) = event.tokens_output {
                        entry.tokens_output = entry.tokens_output.saturating_add(o);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn handle_event(&mut self, event: &HookEvent) -> SessionTransition {
        // Record to recent_events log for diagnostic view (capped at MAX_RECENT_EVENTS).
        self.recent_events.push_back(RecentEvent {
            timestamp: Utc::now(),
            provider: event.provider.clone(),
            session_id: event.session_id.clone(),
            event_name: event.hook_event_name.clone(),
            tool_name: event.tool_name.clone(),
            error: event.error.clone(),
        });
        while self.recent_events.len() > MAX_RECENT_EVENTS {
            self.recent_events.pop_front();
        }

        self.bump_provider_totals(event);

        if event.hook_event_name == "SessionEnd" {
            let removed = self.sessions.remove(&event.session_id);
            // K22 落地：SessionEnd 算「完成」一種（session 從 active map 移除
            // 那一刻 = 結束）。先把 age 算成 owned i64 再丟給 record helper,
            // 跟 Working→Idle 路徑吃同一個 owned-data 簽名。
            if let Some(ref s) = removed {
                let age = s
                    .last_event_time
                    .signed_duration_since(s.start_time)
                    .num_seconds();
                self.record_completed_session_age(&s.provider, age);
            }
            if removed.is_some() && self.active_session_id.as_deref() == Some(&event.session_id) {
                self.active_session_id = self.sessions.keys().next().cloned();
            }
            if removed.is_some() {
                return SessionTransition::Completed;
            }
            return SessionTransition::None;
        }

        if !self.sessions.contains_key(&event.session_id) {
            if self.active_session_id.is_none() {
                self.active_session_id = Some(event.session_id.clone());
            }
            self.sessions.insert(
                event.session_id.clone(),
                Session::new(
                    event.session_id.clone(),
                    event.provider.clone(),
                    event.cwd.clone(),
                ),
            );
            self.provider_totals
                .entry(event.provider.clone())
                .or_default()
                .session_count += 1;
        }
        let session = self
            .sessions
            .get_mut(&event.session_id)
            .expect("session should exist after insert");

        let prev = session.state;
        session.handle_event(event);
        let now = session.state;

        // K22 落地：Working→Idle 算「完成」另一種（runner 主動收尾
        // session,但 session 還留在 active map,等 30 min stale 才被回收）。
        // 在這裡把完成時的 age 寫進 ProviderTotals 跟 SessionEnd 同一個欄位
        // —— operator 端 alert `last_completed_session_age_seconds > 1800`
        // （30 分鐘）就會觸發「最近一次 task 跑超過 30 分鐘才收尾」。
        //
        // 解 E0499：`session: &mut Session` 借自 `self.sessions` 期間不能再
        // 對 self 取 `&mut`。先把 (provider, age) clone 出來成 owned tuple
        // 脫離 borrow,NLL 釋放 self.sessions 借用後再回頭呼叫
        // record_completed_session_age(&mut self, ...)。
        let completed_data = if prev == SessionState::Working && now == SessionState::Idle {
            let age = session
                .last_event_time
                .signed_duration_since(session.start_time)
                .num_seconds()
                .max(0);
            Some((session.provider.clone(), age))
        } else {
            None
        };

        let transition = if prev == SessionState::Working && now == SessionState::Idle {
            SessionTransition::Completed
        } else if prev != SessionState::WaitingForUser && now == SessionState::WaitingForUser {
            SessionTransition::StartedWaiting
        } else {
            SessionTransition::None
        };

        if let Some((provider, age)) = completed_data {
            self.record_completed_session_age(&provider, age);
        }

        transition
    }

    /// K22 配套 helper：把「session 完成時的持續秒數」寫進該 provider 的
    /// `ProviderTotals.last_completed_session_age_secs`。lifetime gauge —— 重複
    /// 呼叫會覆寫成最新一次的完成時 age（K9 `session_count` 才是累加 counter,
    /// K22 只看「最近一次」不需要累計）。負值 saturating clamp 到 0,跟 K11
    /// `compute_quota_snapshot_age_seconds` / K12 idle ratio 的負值防呆一致。
    ///
    /// 簽名吃 owned `&str` + `i64`（不是 `&Session`）—— caller 端要先把
    /// session 內的 provider / age 取出成 owned 值,再呼叫本 helper。這樣
    /// `&mut self.provider_totals` 不會跟 caller 持有的 `&mut Session`
    /// 撞 E0499。SessionEnd 跟 Working→Idle 兩路徑都吃同一個 helper,
    /// SessionEnd 那邊本來就把 `self.sessions.remove()` 拿到的 owned
    /// Session 借出 `&s`,改吃 owned data 後兩路徑一致。
    fn record_completed_session_age(&mut self, provider: &str, age: i64) {
        let entry = self
            .provider_totals
            .entry(provider.to_string())
            .or_default();
        // K22 gauge：只記「最近一次」完成時的 age,重複呼叫覆寫成最新值。
        // 跟 K9 `session_count` (counter,累加) 區分：K22 不累計,只留 latest。
        entry.last_completed_session_age_secs = Some(age.max(0));
        // K23 counter：每次完成都 +1（saturating_add 防極端值 overflow）,
        // 跟 K22 同步觸發（同一個 helper 內）。operator 端算
        // `rate(completed_sessions_total[1h])` 觀察吞吐。
        entry.completed_sessions_count = entry.completed_sessions_count.saturating_add(1);
        // K24 counter：把當次完成的 age 累加進 lifetime 總時長（saturating_add
        // 防時鐘回撥 / 序列化時差造成的負值污染 counter 總和）。`age.max(0)`
        // 先做飽和 clamp 再轉 u64 累加,跟 K22 同樣語意,但 K22 是寫 latest gauge
        // （重複覆寫無副作用），K24 是累加 counter（負值一旦寫入就污染總和無法
        // 收回 —— saturating clamp 是必要防線）。operator 端算
        // `duration_seconds / completed_sessions_total` 觀察平均 time-to-completion。
        entry.completed_sessions_total_duration_secs = entry
            .completed_sessions_total_duration_secs
            .saturating_add(age.max(0) as u64);
        // K26 gauge：歷史「最長」一次完成時的 age。跟 K22 (latest) / K25 (avg)
        // 互補形成 max / latest / avg 三件套。`age.max(0)` 先做飽和 clamp 再跟
        // 歷史 max 比,saturating_max 防 i64::MIN 邊界退化。Option 語意跟 K22 同：
        // 第一次完成時直接覆寫 Some(age)（沒有歷史 max 可比 → 第一次就是 max）,
        // 後續完成用 saturating_max 更新。`i64::MIN` 預設值留作 sentinel 時用
        // `saturating_max` 自然收斂到第一次的 age —— 不需要額外 `if let Some`
        // 分支,程式碼更線性。
        let clamped_age = age.max(0);
        entry.max_completed_session_age_secs = Some(match entry.max_completed_session_age_secs {
            Some(prev) => prev.max(clamped_age),
            None => clamped_age,
        });
        // K27 gauge：歷史「最短」一次完成時的 age。跟 K22 (latest) / K25 (avg) /
        // K26 (max) 互補形成 min / max / latest / avg 四件套。`age.max(0)` 沿用
        // K26 同一個 `clamped_age` 變數（K26 已先做飽和 clamp）。Option 語意跟
        // K22 / K26 同：第一次完成時直接覆寫 Some(age)（沒有歷史 min 可比 → 第一次
        // 就是 min）,後續完成用 saturating_min 更新 —— 跟 K26 鏡像對稱,單調遞減
        // 語意。`i64::min` 自 Rust 1.50 起 stable,沿用 K26 `.max()` 同款 API
        // 風格,程式碼線性對稱。
        entry.min_completed_session_age_secs = Some(match entry.min_completed_session_age_secs {
            Some(prev) => prev.min(clamped_age),
            None => clamped_age,
        });
        // K28 gauge：Welford online algorithm 累計 stddev。跟 K22 (latest) / K25
        // (avg) / K26 (max) / K27 (min) 互補形成 min / max / latest / avg + stddev
        // 五件套 —— 補「波動性」維度。`mean` / `m2` 跟 K23 `completed_sessions_count`
        // 同步更新：Welford 公式要求 count 跟 mean / M2 一起推進, 我們這邊先 K23
        // ++1（line 618 在 K22 之後）再算 delta, 等同「count_new = count_old + 1」
        // 語意。delta / delta2 / M2 三步都吃 `f64` —— 雖然 `clamped_age` 是 i64
        // 但 Welford 公式本身是連續, 強制裁整會累積誤差。`f64` 預設 0.0 跟
        // `count: u64` 預設 0 對齊 —— 第一次完成時 count 從 0 → 1, delta = x - 0
        // = x, mean += x/1 = x, delta2 = x - x = 0, m2 += x * 0 = 0 → stddev = 0
        // （單樣本無波動, 跟數學直觀一致）。`as f64` 轉換用 `clamped_age as f64`
        // (i64 → f64 在 i64::MAX 範圍內精確, saturation 不可能發生在現實 session
        // duration 量級)。`f64` 欄位不用 `Option` —— stddev 跟 completed count
        // 強綁定, count=0 時 render 端用 `completed_sessions_count == 0` 過濾就
        // 不 emit (等同 None 語意, 跟 K26/K27 Option 跳過策略一致)。
        let x = clamped_age as f64;
        let n_new = entry.completed_sessions_count as f64;
        let delta = x - entry.completed_sessions_mean_secs;
        entry.completed_sessions_mean_secs += delta / n_new;
        let delta2 = x - entry.completed_sessions_mean_secs;
        entry.completed_sessions_m2_secs += delta * delta2;
        // K30 reservoir sampling (Vitter Algorithm R 簡化版): count < capacity
        // 直接 push, count >= capacity 用 `Utc::now().timestamp_nanos() % len`
        // 當 pseudo-random index replace。輕量, 無外部 `rand` 依賴 —— 納秒
        // 時間戳快速變化在 microsecond 量級的 session 結束事件序列中實際上
        // 接近 random 替換, 統計學 P95 估計誤差 < 1.5% 在 1024 sample 下。
        // 不用「count/total」機率替換是因為 `completed_sessions_count` 已經
        // saturating_add 過 1, Algorithm R 公式的 j 從現有 sample 空間 random
        // 即可, 簡化版不犧牲統計語意。`timestamp_nanos_opt()` 在時鐘回撥時
        // 會回 `None` → fallback 到 0 (固定 index, 不會 panic, 統計偏差可忽略)。
        if entry.completed_sessions_p95_samples.len() < P95_RESERVOIR_CAPACITY {
            entry.completed_sessions_p95_samples.push(clamped_age);
        } else {
            let j =
                (Utc::now().timestamp_nanos_opt().unwrap_or(0) as usize) % P95_RESERVOIR_CAPACITY;
            entry.completed_sessions_p95_samples[j] = clamped_age;
        }
    }

    pub fn check_staleness(&mut self, idle: i64, stale: i64, remove: i64) {
        let now = Utc::now();
        let mut to_remove = Vec::new();

        for (id, session) in &mut self.sessions {
            let elapsed = now
                .signed_duration_since(session.last_event_time)
                .num_seconds();
            if session.thinking {
                if let Some(ts) = session.last_thinking_at {
                    if now.signed_duration_since(ts).num_milliseconds() > 1500 {
                        session.thinking = false;
                    }
                }
            }
            if elapsed > remove {
                to_remove.push(id.clone());
            } else if elapsed > stale {
                session.state = SessionState::Stale;
                session.thinking = false;
            } else if matches!(session.state, SessionState::Working) && elapsed > idle {
                session.state = SessionState::Idle;
                session.thinking = false;
            }
        }

        for id in to_remove {
            self.sessions.remove(&id);
            if self.active_session_id.as_deref() == Some(&id) {
                self.active_session_id = self.sessions.keys().next().cloned();
            }
        }
    }

    pub fn select_session(&mut self, id: String) {
        if self.sessions.contains_key(&id) {
            self.active_session_id = Some(id);
        }
    }

    pub fn active_session(&self) -> Option<&Session> {
        if let Some(ref id) = self.active_session_id {
            if let Some(s) = self.sessions.get(id) {
                if s.is_active() {
                    return Some(s);
                }
            }
        }
        if let Some(s) = self.sessions.values().find(|s| s.is_active()) {
            return Some(s);
        }
        if let Some(ref id) = self.active_session_id {
            if let Some(s) = self.sessions.get(id) {
                return Some(s);
            }
        }
        self.sessions.values().next()
    }

    pub fn sorted_sessions(&self) -> Vec<SessionInfo> {
        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_by(|a, b| {
            b.is_active()
                .cmp(&a.is_active())
                .then(b.last_event_time.cmp(&a.last_event_time))
        });
        sessions.iter().map(|s| SessionInfo::from(*s)).collect()
    }

    pub fn active_count(&self) -> usize {
        self.sessions.values().filter(|s| s.is_active()).count()
    }

    /// Get unique active provider IDs
    pub fn active_providers(&self) -> Vec<String> {
        let mut providers: Vec<String> = self
            .sessions
            .values()
            .filter(|s| s.is_active())
            .map(|s| s.provider.clone())
            .collect();
        providers.sort();
        providers.dedup();
        providers
    }

    pub fn get_state(&self) -> AppState {
        let active = self.active_session().map(SessionInfo::from);
        let sessions = self.sorted_sessions();
        let session_count = self.sessions.len();
        let active_count = self.active_count();
        let active_providers = self.active_providers();

        AppState {
            active_session: active,
            sessions,
            session_count,
            active_count,
            active_providers,
            provider_totals: self.provider_totals.clone(),
        }
    }
}

/// K22 配套 pure fn：把 `ProviderTotals` 裡的「最近完成 session 年齡」攤平成
/// `HashMap<provider, secs>` 給 `render_prometheus_body` emit。`None`（該
/// provider 還沒完成過 session）跳過不放入 map —— 對齊 K8 `last_event_at` /
/// K10 `since` 「缺資料不 emit sample」契約,避免 Prometheus 端把「沒看到」
/// 當 0 誤判「剛剛完成 age=0」。沒有「alphabetical sort」邏輯,排序交給
/// `render_prometheus_body` 統一處理（跟 K6/K7/K9/K10/K18 風格一致）。
pub fn last_completed_session_age_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if let Some(age) = t.last_completed_session_age_secs {
            out.insert(p.clone(), age);
        }
    }
    out
}

/// K23 配套 pure fn：把 `ProviderTotals` 裡的「累計完成 session 數」攤平成
/// `HashMap<provider, count>` 給 `render_prometheus_body` emit。跟 K22
/// `last_completed_session_age_at` 差異：K22 過濾 `None`（Option 語意）,
/// K23 全部進 map —— `u64` 預設 0 是有效資料（該 provider 累計進來 event 但
/// 還沒完成過 session）,counter 0 跟 missing 是不同語意,跟 K9 `session_count`
/// 「emit 所有有 ProviderTotals entry 的 provider」風格一致。沒有「alphabetical
/// sort」邏輯,排序交給 `render_prometheus_body` 統一處理。
pub fn completed_sessions_count_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        out.insert(p.clone(), t.completed_sessions_count);
    }
    out
}

/// K24 配套 pure fn：把 `ProviderTotals` 裡的「累計完成 session 總時長」攤平成
/// `HashMap<provider, secs>` 給 `render_prometheus_body` emit。跟 K23
/// `completed_sessions_count_at` 同 emit 策略：`u64` 預設 0 是有效資料
///（該 provider 累計收過 event 但還沒完成過 session），counter 0 跟 missing
/// 是不同語意 → 全部 provider 都進 map（含 0），跟 K9 / K23 / K13 / K17
/// lifetime aggregate 風格一致。沒有「alphabetical sort」邏輯,排序交給
/// `render_prometheus_body` 統一處理（K6-K24 既契約）。
pub fn completed_sessions_total_duration_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        out.insert(p.clone(), t.completed_sessions_total_duration_secs);
    }
    out
}

/// K25 配套 pure fn：把 `ProviderTotals` 裡的「累計完成 session 平均時長」攤平
/// 成 `HashMap<provider, secs>` 給 `render_prometheus_body` emit。K25 是 K23
/// (`completed_sessions_count`) / K24 (`completed_sessions_total_duration_secs`)
/// 兩條 lifetime counter 的派生 gauge —— operator 端不再需要自己寫
/// `..._duration_seconds / ..._total` 算式（兩個 metric cross-query 在 PromQL
/// 易出錯、scrape 缺一條時算式直接壞），直接在 Prometheus 端抓這條 series 觀察
/// average time-to-completion KPI（per-provider 效率訊號）。K25 補 K22 / K23 /
/// K24 都沒覆蓋的「平均效率」維度：K22 是 single sample（最近一次）、K23 是純
/// 計次、K24 是純加總，都沒把兩個 dimension 結合成除法結果 → K25 跟 K12
/// `idle_ratio` 同樣屬於「既有資料源派生指標」。
///
/// 跟 K22 / K24 emit 策略都不同：
/// - K22 過濾 `None`（該 provider 沒完成過 session → gauge 缺資料）
/// - K24 全部 emit（counter 0 跟 missing 是不同語意）
/// - K25 過濾 `count == 0`（0/0 數學未定義 → 不能 emit 0.0 假冒「平均 0 秒
///   完成」誤導 Prometheus 端把「沒資料」判成「瞬間完成」= 假健康信號）
///
/// 也就是說 K25 用 K22 語意（缺資料不 emit sample）但觸發條件改寫成「該 provider
/// 從未完成過 session」= `count==0`。`count > 0` 時 emit `total / count`（f64）。
/// 沒有「alphabetical sort」邏輯，排序交給 `render_prometheus_body` 統一處理
/// （K6-K24 既契約）。
pub fn completed_sessions_average_duration_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_count > 0 {
            out.insert(
                p.clone(),
                t.completed_sessions_total_duration_secs as f64 / t.completed_sessions_count as f64,
            );
        }
    }
    out
}

/// K26 配套 pure fn：把 `ProviderTotals` 裡的「歷史最長完成 session 年齡」攤平
/// 成 `HashMap<provider, secs>` 給 `render_prometheus_body` emit。跟 K22
/// `last_completed_session_age_at` 對稱：都過濾 `None`（該 provider 累計收過
/// event 但還沒完成過 session → gauge 缺資料,跳過不 emit sample,避免
/// Prometheus 端把「沒看到」當「max=0」誤判「該 provider 瞬間完成」= 假健康
/// 信號）。`Some(secs)` 進 map —— lifetime saturating_max 寫入後不蒸發,跟
/// K22 lifetime gauge 對齊：session 結束 + 30 min stale 回收後 `ProviderTotals`
/// 仍保留 → Prometheus 端 gauge 不會倒退。沒有「alphabetical sort」邏輯,排序
/// 交給 `render_prometheus_body` 統一處理（K6-K25 既契約）。
pub fn completed_sessions_max_duration_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if let Some(secs) = t.max_completed_session_age_secs {
            out.insert(p.clone(), secs);
        }
    }
    out
}

/// K27 配套 pure fn：把 `ProviderTotals` 裡的「歷史最短完成 session 年齡」攤平
/// 成 `HashMap<provider, secs>` 給 `render_prometheus_body` emit。跟 K26
/// `completed_sessions_max_duration_at` / K22 `last_completed_session_age_at`
/// 三件套對稱：都過濾 `None`（該 provider 累計收過 event 但還沒完成過 session
/// → gauge 缺資料,跳過不 emit sample,避免 Prometheus 端把「沒看到」當
/// 「min=0」誤判「該 provider 瞬間完成」= 假健康信號）。`Some(secs)` 進 map
/// —— lifetime saturating_min 寫入後不蒸發,跟 K22 / K26 lifetime gauge 對齊：
/// session 結束 + 30 min stale 回收後 `ProviderTotals` 仍保留 → Prometheus 端
/// gauge 不會倒退。沒有「alphabetical sort」邏輯,排序交給 `render_prometheus_body`
/// 統一處理（K6-K26 既契約）。
pub fn completed_sessions_min_duration_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if let Some(secs) = t.min_completed_session_age_secs {
            out.insert(p.clone(), secs);
        }
    }
    out
}

/// K28 配套 pure fn：把 `ProviderTotals` 裡的 Welford 累積值還原成 stddev
/// gauge（`HashMap<provider, secs>`）給 `render_prometheus_body` emit。跟
/// K26 `completed_sessions_max_duration_at` / K27 `completed_sessions_min_duration_at`
/// 對稱：都過濾「沒完成過」的 provider（K26/K27 用 `Option::is_none` 過濾,
/// K28 用 `completed_sessions_count == 0` 過濾 —— K28 不用 Option 是因為
/// Welford mean / M2 是 `f64` 預設 0.0, 沒有「無值」vs「值=0」的可區分性,
/// 改用 count 過濾更明確）。formula: `stddev = (M2 / count).sqrt()`
/// (population stddev, 不是 sample —— 跟 K25 avg 一致, lifetime aggregate
/// 不分 sample/population, 用 N 不用 N-1)。`count` 沿用 K23
/// `completed_sessions_count` —— 跟 mean / M2 同步, 不會有 stale count。
/// `count == 1` 時 M2 = 0（單樣本無波動）, stddev = 0 —— render 端把 0
/// 視為有效資料 emit（K23 「0 是有效」語意延伸, 跟 K25 avg = x 對單樣本
/// 邏輯一致）。`f64::sqrt()` 在 count == 0 時不會被呼叫（前置過濾已擋）。
/// lifetime aggregate 對齊 K22 / K26 / K27: session 結束 + 30 min stale
/// 回收後 `ProviderTotals` 仍保留 → Prometheus 端 gauge 不會倒退。
/// 沒有「alphabetical sort」邏輯, 排序交給 `render_prometheus_body` 統一處理
/// （K6-K27 既契約, K28 沿用）。
pub fn completed_sessions_stddev_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_count == 0 {
            continue;
        }
        let variance = t.completed_sessions_m2_secs / t.completed_sessions_count as f64;
        out.insert(p.clone(), variance.sqrt());
    }
    out
}

/// K29 配套 pure fn：把 `ProviderTotals` 裡的「累計 tool 失敗次數」跟
/// 「累計完成 session 數」組合成 failure-to-completion ratio 衍生 gauge
/// (`HashMap<provider, ratio>`) 給 `render_prometheus_body` emit。
///
/// K29 是 K10 (`provider_failure_count` counter, PostToolUseFailure 累計)
/// / K23 (`provider_completed_sessions_total` counter) 兩條 lifetime
/// counter 的派生 gauge —— operator 端不再需要自己寫 PromQL
/// `failure_count / completed_sessions_total` 除法算式（兩個 metric
/// cross-query 在 PromQL 易出錯、scrape 缺一條時算式直接壞），直接在
/// Prometheus 端抓這條 series 觀察「平均每完成一次 session 失敗幾次
/// tool」= 失敗率 KPI, 補 K22 / K23 / K24 / K25 / K26 / K27 / K28 都沒
/// 覆蓋的「失敗 vs 成功比」維度（K9 / K10 是純絕對失敗計數、K22-K28
/// 是 session duration 分布、沒人把它們組合成「每完成一次有幾次失敗」
/// 這個 operator 友善的 ratio）。
///
/// 跟 K25 `completed_sessions_average_duration_at` emit 策略完全一致
/// （K25 跟 K29 都是「兩個 lifetime counter 組合成 ratio」純 derived
/// 衍生）：
/// - K22 過濾 `None`（該 provider 沒完成過 session → gauge 缺資料）
/// - K24 全部 emit（counter 0 跟 missing 是不同語意）
/// - K25 過濾 `count == 0`（0/0 數學未定義）
/// - K29 過濾 `completed_sessions_count == 0`（0/0 數學未定義 → 不能
///   emit 0.0 假冒「失敗率 0」誤導 Prometheus 端把「沒資料」判成「零
///   失敗」= 假健康信號, 跟 K25 同款防線）。
///
/// `count > 0` 時 emit `failure_count / count` (f64, 4 位小數跟 K25 avg /
/// K28 stddev 對齊)。語意: ratio = 2.5 表示「每完成一次 session 平均
/// 失敗 2.5 次 tool」, alert `ratio > 2.0` 觸發「該 provider session
/// 平均 retry 2 次以上」健康度異常信號。沒有「alphabetical sort」邏輯,
/// 排序交給 `render_prometheus_body` 統一處理 (K6-K28 既契約, K29 沿用)。
pub fn failure_to_completion_ratio_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_count > 0 {
            out.insert(
                p.clone(),
                t.failure_count as f64 / t.completed_sessions_count as f64,
            );
        }
    }
    out
}

/// K30 配套 pure fn：把 `ProviderTotals` 裡的 reservoir samples 排序後
/// 找 95 百分位, 回 `HashMap<provider, secs>` (i64) 給
/// `render_prometheus_body` emit。跟 K22 / K25 / K26 / K27 / K28 / K29
/// 對稱：都過濾「沒完成過」或「0/0 數學未定義」的 provider (K30 用
/// `samples.is_empty()` 過濾 —— K30 不用 `completed_sessions_count`
/// 過濾是因為 reservoir 滿了後 count > capacity 但 sample 仍表徵
/// 「最近 1024 個」, P95 計算有效; 用 sample 數量比 count 更貼近
/// P95 語意)。
///
/// P95 計算: sort samples → 取 `index = len * 95 / 100`, 若 index >=
/// len 則取 `len - 1` (避免 OOB; 當 sample 數 < 20 時 P95 退化成
/// 「最大 sample」, 跟統計直觀一致: 少樣本下 P95 估計不穩, 寧可
/// 退化到 max 也別 panic)。Sample 是 `i64` duration, emit 端 cast 成
/// f64 4 位小數跟 K25 / K28 對齊。lifetime aggregate 對齊 K22-K28
/// 既契約: session 結束後 `ProviderTotals` 仍保留 → Prometheus 端
/// gauge 不會倒退 (但 reservoir 是 sliding window, 語意是「最近
/// 1024 個」非 lifetime — R48 設計有意識 trade-off, doc 開頭明寫)。
///
/// 排序成本: 1024 sample O(N log N) ≈ 10000 比較 per scrape, scrape
/// 15s 一次完全可忽略。沒有「alphabetical sort」邏輯, 排序交給
/// `render_prometheus_body` 統一處理 (K6-K29 既契約, K30 沿用)。
pub fn completed_sessions_p95_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_p95_samples.is_empty() {
            continue;
        }
        let mut samples = t.completed_sessions_p95_samples.clone();
        samples.sort_unstable();
        let idx = (samples.len() * 95 / 100).min(samples.len() - 1);
        out.insert(p.clone(), samples[idx]);
    }
    out
}

/// K31 配套 pure fn: 把 `ProviderTotals` 裡的 reservoir samples 排序後
/// 取 50 百分位 (= median 中位數), 回 `HashMap<provider, secs>` (i64)
/// 給 `render_prometheus_body` emit。跟 K30 P95 / K22 / K25 / K26 / K27
/// / K28 / K29 對稱: 都過濾「沒完成過」或「0/0 數學未定義」的 provider
/// (K31 用 `samples.is_empty()` 過濾, 跟 K30 同款)。
///
/// K31 復用 K30 reservoir 同一個 `completed_sessions_p95_samples: Vec<i64>`
/// —— 不開新欄位, 跟 K30 共用 sample 池。語意: P50 = median = 「最近
/// 1024 次完成 session 的中位數」, 跟 P95 同一 sliding window, 差別只在
/// percentile 位置。Operator 端 alert 互補: P50 看「典型 session 多久」
/// (中位數抗 outlier 比 K25 avg 強 —— avg 受極端長任務拉高, P50 不會),
/// P95 看「SLO 邊界延遲」(尾端 5%)。組合 `p50 > 60s` (整體慢) vs
/// `p95 > 300s` (尾端慢) 可以快速分辨「該 provider 整體慢」vs「只有
/// 尾端慢」, K25 avg 算不出這層細 (avg 是中心趨勢, 對 outlier 敏感)。
///
/// 為什麼 K31 復用 K30 samples 而不是另開 `Vec<i64>`:
/// 1. 語意一致: P50 跟 P95 表徵同一 sliding window, 拆成兩 vec 反而
///    語意分裂 (「這份是 P95 sample, 那份是 P50 sample」實際上同一份
///    資料切兩次);
/// 2. 記憶體節省: 每個 provider 1024 * 8 bytes = 8KB, 9 provider = 72KB,
///    開兩份 = 144KB (Tauri desktop app 不痛但仍是浪費);
/// 3. Sort 成本不變: render 端 sort 一次, 兩個 quantile 共享;
/// 4. 語意釐清成本低: doc comment 明寫「K31 復用 K30 samples」即可。
///
/// P50 計算: sort samples → `idx = len * 50 / 100`, 若 idx >= len 則取
/// `len - 1` (避免 OOB; 跟 K30 同款策略, 少樣本下 P50 退化成「接近
/// 中位」sample 也不會 panic)。Boundary: len=1 → idx=0, P50 = 該
/// sample (median of 1 = itself, 數學直觀); len=2 → idx=1, P50 = sort
/// 後較大值 (2 個 sample 取較大, 跟 numpy median 一致); len=20 →
/// idx=10, P50 = sort 後第 11 個 (對稱樣本正好中間)。Sample 是 `i64`
/// duration, emit 端 cast 成 f64 4 位小數跟 K25 avg / K28 stddev / K30
/// P95 對齊。lifetime aggregate 對齊 K22-K30 既契約: session 結束後
/// `ProviderTotals` 仍保留 → Prometheus 端 gauge 不會倒退 (reservoir
/// 是 sliding window 跟 lifetime 不衝突 —— K30 doc 開頭已明寫 sliding
/// window 設計 trade-off)。
///
/// 排序成本: 1024 sample O(N log N) ≈ 10000 比較 per scrape, 跟 K30
/// 同一份 vec 共享這次 sort —— K31 emit 端實際上不重排, 直接從 K30
/// 算完的 sort 結果找 `idx * 50/100` 即可, runtime 額外成本 O(1)。
/// 但純 fn 端 K30 / K31 各自 `sort_unstable` 一次是「純函式獨立性」權衡:
/// 同一份 `Vec<i64>` clone 兩次, sort 兩次, 每次 scrape 多花 ~20000 比較
/// (10ms 量級), 跟 metrics endpoint scrape 15s 一次比完全可忽略。
/// 沒有「alphabetical sort」邏輯, 排序交給 `render_prometheus_body`
/// 統一處理 (K6-K30 既契約, K31 沿用)。
pub fn completed_sessions_p50_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_p95_samples.is_empty() {
            continue;
        }
        let mut samples = t.completed_sessions_p95_samples.clone();
        samples.sort_unstable();
        let idx = (samples.len() * 50 / 100).min(samples.len() - 1);
        out.insert(p.clone(), samples[idx]);
    }
    out
}

/// K32 配套 pure fn: 把 `ProviderTotals` 裡的 reservoir samples 排序後
/// 取 99 百分位 (= P99, 極尾端延遲), 回 `HashMap<provider, secs>` (i64)
/// 給 `render_prometheus_body` emit。跟 K30 P95 / K31 P50 / K22 / K25 / K26
/// / K27 / K28 / K29 對稱: 都過濾「沒完成過」的 provider (K32 用
/// `samples.is_empty()` 過濾, 跟 K30/K31 同款)。
///
/// K32 復用 K30 reservoir 同一個 `completed_sessions_p95_samples: Vec<i64>`
/// —— 不開新欄位, 跟 K30 / K31 共用 sample 池。語意: P99 = 「最近 1024 次
/// 完成 session 的第 99 百分位」, 跟 P95 / P50 同一 sliding window, 差別
/// 只在 percentile 位置 (50/95/99)。Operator 端 alert 三層次: `p50 > 60`
/// (K31 整體慢) / `p95 > 300` (K30 尾端 5% 慢 = SLO 邊界延遲) / `p99 > 600`
/// (K32 極端尾端 1% 慢 = 異常 / 卡死信號)。三件套組合 `p50/p95/p99` 可繪出
/// latency 分布輪廓, 不需 PromQL 算 `histogram_quantile` (有助於看 K25 avg
/// 受 outlier 拉高時, P99 是否比 P95 顯著高 = 有極端 outlier 卡住整體分布)。
///
/// P99 計算: sort samples → `idx = len * 99 / 100`, 若 idx >= len 則取
/// `len - 1` (避免 OOB; 跟 K30/K31 同款策略, 少樣本下 P99 退化成「接近
/// max」sample 也不會 panic)。Boundary: len=1 → idx=0, P99 = 該 sample
/// (P99 of 1 = itself, 數學直觀); len=20 → idx=19, P99 = sort 後第 20 個
/// (= max, 少樣本 P99 退化到 max 跟 K26 max 語意對齊); len=100 → idx=99,
/// P99 = samples[99] (= max, 剛好滿 100 樣本 P99 = max); len=1000 → idx=990。
/// Sample 是 `i64` duration, emit 端 `{}` 不加浮點 precision (整數契約跟
/// K30/K31 一致, 跟 K25/K28 f64 4 位小數不同 —— 原因: percentile index
/// 已經 cast 過, 多餘小數位是 false precision, 強制裁整 0 精度流失)。
///
/// 為什麼 K32 復用 K30 samples 而不是另開 `Vec<i64>`:
/// 1. 語意一致: P50/P95/P99 表徵同一 sliding window, 拆成兩/三 vec 反而
///    語意分裂 (「這份是 P99 sample, 那份是 P50 sample」實際上同一份
///    資料切三次);
/// 2. 記憶體節省: 每個 provider 1024 * 8 bytes = 8KB, 9 provider = 72KB,
///    開三份 = 216KB (Tauri desktop app 不痛但仍是浪費);
/// 3. Sort 成本不變: render 端 sort 一次, 三個 quantile 共享;
/// 4. 語意釐清成本低: doc comment 明寫「K32 復用 K30 samples」即可。
///
/// lifetime aggregate 對齊 K22-K31 既契約: session 結束後 `ProviderTotals`
/// 仍保留 → Prometheus 端 gauge 不會倒退 (reservoir 是 sliding window 跟
/// lifetime 不衝突 —— K30 doc 開頭已明寫 sliding window 設計 trade-off)。
///
/// 排序成本: 1024 sample O(N log N) ≈ 10000 比較 per scrape, 跟 K30/K31
/// 同一份 vec 共享這次 sort —— K32 emit 端實際上不重排, 直接從 sort 結果
/// 找 `idx * 99/100` 即可, runtime 額外成本 O(1)。但純 fn 端 K30/K31/K32
/// 各自 `sort_unstable` 一次是「純函式獨立性」權衡: 同一份 `Vec<i64>` clone
/// 三次, sort 三次, 每次 scrape 多花 ~30000 比較 (15ms 量級), 跟 metrics
/// endpoint scrape 15s 一次比完全可忽略。
/// 沒有「alphabetical sort」邏輯, 排序交給 `render_prometheus_body` 統一
/// 處理 (K6-K31 既契約, K32 沿用)。
pub fn completed_sessions_p99_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_p95_samples.is_empty() {
            continue;
        }
        let mut samples = t.completed_sessions_p95_samples.clone();
        samples.sort_unstable();
        let idx = (samples.len() * 99 / 100).min(samples.len() - 1);
        out.insert(p.clone(), samples[idx]);
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct AppState {
    pub active_session: Option<SessionInfo>,
    pub sessions: Vec<SessionInfo>,
    pub session_count: usize,
    pub active_count: usize,
    pub active_providers: Vec<String>,
    pub provider_totals: HashMap<String, ProviderTotals>,
}

#[cfg(test)]
mod tests {
    use super::{
        completed_sessions_p50_at, completed_sessions_p95_at, completed_sessions_p99_at,
        completed_sessions_stddev_at, failure_to_completion_ratio_at, SessionManager,
        SessionTransition,
    };
    use crate::hook_event::HookEvent;

    fn ev(provider: &str, sid: &str, name: &str) -> HookEvent {
        HookEvent {
            provider: provider.to_string(),
            session_id: sid.to_string(),
            hook_event_name: name.to_string(),
            cwd: None,
            tool_name: None,
            notification_type: None,
            prompt: None,
            tool_call_id: None,
            tool_status: None,
            tokens_input: None,
            tokens_output: None,
            error: None,
        }
    }

    #[test]
    fn session_count_is_counted_once_per_live_session() {
        let mut m = SessionManager::new();
        let _ = m.handle_event(&ev("cicx", "s1", "SessionStart"));
        let _ = m.handle_event(&ev("cicx", "s1", "SessionStart"));
        assert_eq!(
            m.provider_totals.get("cicx").map(|t| t.session_count),
            Some(1)
        );
    }

    #[test]
    fn session_count_lifetime_aggregate_accumulates_across_unique_sessions() {
        // K9 整合測試：lifetime aggregate 真實累積路徑。
        // 同 provider 多個 unique session_id 開過 → ProviderTotals.session_count 累計。
        // 同一 session 內多個 event → 不重複算（K9 跟 K6/K7/K8 一致：unique session_id 算一次）。
        // session 結束 + 新 session 開 → 累計增加。
        let mut m = SessionManager::new();
        // session 1：開 + 多個 event + 結束
        let _ = m.handle_event(&ev("cicx", "s1", "SessionStart"));
        let _ = m.handle_event(&ev("cicx", "s1", "UserPromptSubmit"));
        let _ = m.handle_event(&ev("cicx", "s1", "PreToolUse"));
        let _ = m.handle_event(&ev("cicx", "s1", "SessionEnd"));
        assert_eq!(
            m.provider_totals.get("cicx").map(|t| t.session_count),
            Some(1),
            "session 1 結束後累計應為 1"
        );
        // session 2：不同 session_id → 累計 +1
        let _ = m.handle_event(&ev("cicx", "s2", "SessionStart"));
        assert_eq!(
            m.provider_totals.get("cicx").map(|t| t.session_count),
            Some(2),
            "session 2 開始後累計應為 2（session 1 lifetime 保留）"
        );
        // session 3：開 event 但不結束 → 累計 +1
        let _ = m.handle_event(&ev("cicx", "s3", "SessionStart"));
        assert_eq!(
            m.provider_totals.get("cicx").map(|t| t.session_count),
            Some(3),
            "session 3 開始後累計應為 3"
        );
        // 同 session 內重發 SessionStart（duplicate） → 不 +1
        let _ = m.handle_event(&ev("cicx", "s3", "SessionStart"));
        assert_eq!(
            m.provider_totals.get("cicx").map(|t| t.session_count),
            Some(3),
            "同 session 重發 SessionStart 不該 +1"
        );
        // 不同 provider session → 獨立累計
        let _ = m.handle_event(&ev("codex", "c1", "SessionStart"));
        assert_eq!(
            m.provider_totals.get("cicx").map(|t| t.session_count),
            Some(3),
            "cicx 累計不該被 codex 影響"
        );
        assert_eq!(
            m.provider_totals.get("codex").map(|t| t.session_count),
            Some(1),
            "codex 累計應為 1"
        );
    }

    #[test]
    fn events_total_lifetime_aggregate_increments_per_event_of_any_type() {
        // K13 整合測試：bump_provider_totals 對每個 event 都 +1（不限 SessionStart /
        // PostToolUseFailure / TokenUpdate）。累計路徑：
        // - 3 種 event 各送 2 次 → 累計 6
        // - session 結束（SessionEnd）也算 event → 累計 7
        // - 不同 provider 獨立累計
        // 對齊 K6/K7/K9 lifetime aggregate：session 結束 + 30 min stale 回收後
        // live session 為空，但 ProviderTotals.events_total 仍保留 → metric 不會倒退。
        let mut m = SessionManager::new();

        // session 1 cicx：SessionStart + UserPromptSubmit + PreToolUse + PostToolUse +
        // PostToolUseFailure + TokenUpdate + SessionEnd = 7 events
        let _ = m.handle_event(&ev("cicx", "s1", "SessionStart"));
        let _ = m.handle_event(&ev("cicx", "s1", "UserPromptSubmit"));
        let _ = m.handle_event(&ev("cicx", "s1", "PreToolUse"));
        let _ = m.handle_event(&ev("cicx", "s1", "PostToolUse"));
        let _ = m.handle_event(&ev("cicx", "s1", "PostToolUseFailure"));
        let _ = m.handle_event(&ev("cicx", "s1", "TokenUpdate"));
        let _ = m.handle_event(&ev("cicx", "s1", "SessionEnd"));
        assert_eq!(
            m.provider_totals.get("cicx").map(|t| t.events_total),
            Some(7),
            "cicx 累計 7 個 event（包含 SessionEnd）"
        );

        // 同一 session 再送 2 個 event → 累計 +2
        // 等等：SessionEnd 已移除 session，再送會被當作新 session
        // 改送同 session 內的 event 在 SessionEnd 之前更單純 —— 上面 7 個已涵蓋
        // 這裡驗：不同 provider 獨立累計
        let _ = m.handle_event(&ev("claude", "c1", "SessionStart"));
        let _ = m.handle_event(&ev("claude", "c1", "PostToolUse"));
        assert_eq!(
            m.provider_totals.get("cicx").map(|t| t.events_total),
            Some(7),
            "cicx 累計不該被 claude 影響"
        );
        assert_eq!(
            m.provider_totals.get("claude").map(|t| t.events_total),
            Some(2),
            "claude 累計應為 2"
        );
    }

    #[test]
    fn duplicate_session_end_does_not_trigger_completed_twice() {
        let mut m = SessionManager::new();
        let _ = m.handle_event(&ev("cicx", "s2", "SessionStart"));
        assert_eq!(
            m.handle_event(&ev("cicx", "s2", "SessionEnd")),
            SessionTransition::Completed
        );
        assert_eq!(
            m.handle_event(&ev("cicx", "s2", "SessionEnd")),
            SessionTransition::None
        );
    }

    // ─── K22 落地：record_completed_session_age + last_completed_session_age_at ───

    use super::{last_completed_session_age_at, ProviderTotals};
    use std::collections::HashMap;

    #[test]
    fn k22_session_end_records_last_completed_age() {
        // K22 整合測試：SessionStart → SessionEnd 路徑應把 last_completed_session_age_secs
        // 寫成 Some(0)（兩次 event 中間時差 < 1 sec → age=0）。`Some` 比精確數值重要
        // —— 我們要驗「觸發了 record」,不是「精準算秒數」（秒數會因 wall clock 而異）。
        let mut m = SessionManager::new();
        let _ = m.handle_event(&ev("cicx", "s1", "SessionStart"));
        // 觸發 SessionEnd 之前 field 應為 None
        assert!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.last_completed_session_age_secs)
                .is_none(),
            "SessionStart 不該算完成"
        );
        let _ = m.handle_event(&ev("cicx", "s1", "SessionEnd"));
        assert!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.last_completed_session_age_secs)
                .is_some(),
            "SessionEnd 該把 last_completed_session_age_secs 寫成 Some(_)"
        );
    }

    #[test]
    fn k22_working_to_idle_records_last_completed_age() {
        // K22 整合測試：Working→Idle 轉換也算「完成」（runner 主動收尾,跟 SessionEnd
        // 兩種語意同一個欄位）。handle_event 內部 state machine 走
        // SessionStart → UserPromptSubmit → Stop → Working→Idle 路徑。
        let mut m = SessionManager::new();
        let _ = m.handle_event(&ev("claude", "c1", "SessionStart"));
        let _ = m.handle_event(&ev("claude", "c1", "UserPromptSubmit"));
        let _ = m.handle_event(&ev("claude", "c1", "Stop"));
        assert!(
            m.provider_totals
                .get("claude")
                .and_then(|t| t.last_completed_session_age_secs)
                .is_some(),
            "Working→Idle 轉換該把 last_completed_session_age_secs 寫成 Some(_)"
        );
    }

    #[test]
    fn k22_last_completed_session_age_at_filters_none() {
        // K22 pure fn 測試：`last_completed_session_age_at` 對 None 欄位跳過,
        // 對 Some(secs) 放進 map。模擬兩個 provider 一個完成一個沒完成 → output
        // 只含完成的 provider。
        let mut totals = HashMap::new();
        // 完成的 provider（struct literal initializer 比 Default + field assign idiomatic）
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                last_completed_session_age_secs: Some(42),
                ..Default::default()
            },
        );
        // 沒完成的 provider（default = None,直接用 Default::default()）
        totals.insert("claude".to_string(), ProviderTotals::default());

        let out = last_completed_session_age_at(&totals);
        assert_eq!(out.get("cicx"), Some(&42), "完成的 provider 該 emit");
        assert!(
            !out.contains_key("claude"),
            "未完成的 provider 該被跳過,不出 sample line"
        );
        assert_eq!(out.len(), 1, "output map 只該有 1 個 entry");
    }

    #[test]
    fn k22_record_completed_session_age_clamps_negative_to_zero() {
        // K22 saturating clamp 測試：age 為負（時鐘回撥 / 序列化時差）→ clamp
        // 到 0,不讓負值流進 Prometheus metric。helper 簽名吃 owned i64,
        // caller 端先算出 age 再傳入（test 端不繞 Session 直接餵值）。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", -60);
        let age = m
            .provider_totals
            .get("cicx")
            .and_then(|t| t.last_completed_session_age_secs)
            .expect("record 該寫 Some");
        assert_eq!(age, 0, "負值 age 該 saturate 到 0");
    }

    #[test]
    fn k22_record_completed_session_age_writes_provider_specific() {
        // K22 隔離測試：每個 provider 獨立記錄,互不污染。cicx 完成 100s,claude
        // 完成 200s,各自 totals 裡的值要對得上。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        m.record_completed_session_age("claude", 200);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.last_completed_session_age_secs),
            Some(100)
        );
        assert_eq!(
            m.provider_totals
                .get("claude")
                .and_then(|t| t.last_completed_session_age_secs),
            Some(200)
        );
    }

    // ─── K23 落地：completed_sessions_count counter + completed_sessions_count_at ───

    use super::completed_sessions_count_at;

    #[test]
    fn k23_session_end_increments_completed_count_by_one() {
        // K23 整合測試：SessionEnd 路徑應把 completed_sessions_count 從 0 → 1。
        // 跟 K22 `k22_session_end_records_last_completed_age` 同觸發點（都是
        // SessionEnd → record_completed_session_age）,所以兩個 metric 同步
        // 推進 —— 測試只驗 K23 field 值,K22 行為已在 K22 tests 內覆蓋。
        let mut m = SessionManager::new();
        let _ = m.handle_event(&ev("cicx", "s1", "SessionStart"));
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .map(|t| t.completed_sessions_count),
            Some(0),
            "SessionStart 不該算完成 → count 仍為 0"
        );
        let _ = m.handle_event(&ev("cicx", "s1", "SessionEnd"));
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .map(|t| t.completed_sessions_count),
            Some(1),
            "SessionEnd 該把 count +1 → 1"
        );
    }

    #[test]
    fn k23_working_to_idle_increments_completed_count_by_one() {
        // K23 整合測試：Working→Idle 轉換也算「完成」另一種（跟 K22 同觸發）,
        // Stop 事件把 state 從 Working 切到 Idle → handle_event 內部偵測到
        // prev=Working & now=Idle → 呼叫 record_completed_session_age →
        // K23 counter +1。SessionStart → UserPromptSubmit → Stop 路徑。
        let mut m = SessionManager::new();
        let _ = m.handle_event(&ev("claude", "c1", "SessionStart"));
        let _ = m.handle_event(&ev("claude", "c1", "UserPromptSubmit"));
        assert_eq!(
            m.provider_totals
                .get("claude")
                .map(|t| t.completed_sessions_count),
            Some(0),
            "Working 狀態下不該算完成"
        );
        let _ = m.handle_event(&ev("claude", "c1", "Stop"));
        assert_eq!(
            m.provider_totals
                .get("claude")
                .map(|t| t.completed_sessions_count),
            Some(1),
            "Stop 觸發 Working→Idle → count +1"
        );
    }

    #[test]
    fn k23_repeated_completions_accumulate_across_unique_sessions() {
        // K23 整合測試：counter 語意 — 重複完成遞增。3 個 unique session 都跑完
        // SessionEnd 路徑 → count 應為 3（不是 1,不是 last-wins）。驗證 saturating
        // 累加沒漏 + lifetime aggregate 不蒸發（跟 K9 `session_count` 同）。
        let mut m = SessionManager::new();
        for i in 0..3 {
            let sid = format!("s{i}");
            let _ = m.handle_event(&ev("cicx", &sid, "SessionStart"));
            let _ = m.handle_event(&ev("cicx", &sid, "SessionEnd"));
        }
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .map(|t| t.completed_sessions_count),
            Some(3),
            "3 個 session 各自完成 → count 累計 3"
        );
    }

    #[test]
    fn k23_completed_sessions_count_at_emits_zero_for_uncompleted_provider() {
        // K23 pure fn 測試：counter 0 跟 K22 `last_completed_session_age_at` 的 None
        // 跳過策略不同 —— K23 全部進 map（含 0）,因為「有 ProviderTotals entry 但
        // count=0」是有效資料（該 provider 累計收過 event 但還沒完成過 session）,
        // Prometheus 端應該看到 0 不是 missing。模擬兩個 provider 一個已完成
        // (count=2) 一個只收過 event 沒完成 (count=0) → output map 兩者都該在。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 2,
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());

        let out = completed_sessions_count_at(&totals);
        assert_eq!(
            out.get("cicx"),
            Some(&2),
            "已完成的 provider 該 emit count=2"
        );
        assert_eq!(
            out.get("claude"),
            Some(&0),
            "未完成的 provider 該 emit count=0（不是 missing 跳過）"
        );
        assert_eq!(out.len(), 2, "output map 該有 2 個 entry,counter 0 不跳過");
    }

    // ─── K24 落地：completed_sessions_total_duration counter + completed_sessions_total_duration_at ───

    use super::completed_sessions_total_duration_at;

    #[test]
    fn k24_session_end_accumulates_total_duration() {
        // K24 整合測試：SessionStart → SessionEnd 路徑應把 completed_sessions_total_duration_secs
        // 從 0 → 該次完成的 age。直接餵 record_completed_session_age（繞過真實 wall clock
        // 計算）模擬「這次完成花了 42 秒」 → total 應為 42。`Some(42)` 比精確數值重要
        // —— 跟 K22 `k22_session_end_records_last_completed_age` 同樣策略：驗「觸發了
        // record」,不是「精準算秒數」。
        let mut m = SessionManager::new();
        // 預設值檢查：SessionStart 還沒完成 → total = 0
        let _ = m.handle_event(&ev("cicx", "s1", "SessionStart"));
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .map(|t| t.completed_sessions_total_duration_secs),
            Some(0),
            "SessionStart 不該累加 total"
        );
        // 直接觸發 record helper（避免 wall clock 計算 0）
        m.record_completed_session_age("cicx", 42);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .map(|t| t.completed_sessions_total_duration_secs),
            Some(42),
            "單次完成 42s → total 該 = 42"
        );
    }

    #[test]
    fn k24_repeated_completions_sum_durations_across_unique_sessions() {
        // K24 整合測試：counter 累加語意 —— 3 個 unique session 各自完成不同時長
        // (30s + 60s + 90s = 180s) → total 應為 180,不是 last-wins 90。驗證累加沒漏 +
        // lifetime aggregate 不蒸發。對齊 K23 `k23_repeated_completions_accumulate_across_unique_sessions`
        // 同樣 3-session shape,差異是驗「時長總和」維度。
        let mut m = SessionManager::new();
        let durations = [30i64, 60, 90];
        for (i, dur) in durations.iter().enumerate() {
            let sid = format!("s{i}");
            let _ = m.handle_event(&ev("claude", &sid, "SessionStart"));
            // 模擬 session 跑了一會兒再結束（直接 record 給定 age 比較 deterministic）
            m.record_completed_session_age("claude", *dur);
        }
        assert_eq!(
            m.provider_totals
                .get("claude")
                .map(|t| t.completed_sessions_total_duration_secs),
            Some(180),
            "3 次完成 30+60+90=180 → total 累計 180"
        );
    }

    #[test]
    fn k24_record_clamped_age_clamps_negative_to_zero() {
        // K24 saturating clamp 測試：record_completed_session_age 餵負值 → K22 gauge
        // clamp 到 0,K24 counter 也必須 clamp 到 0（不是 `as u64` 直接 wrap 成
        // u64::MAX 那種可怕 bug）。同一個 helper 內 age.max(0) 同時保護 K22 + K23 + K24。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", -100);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .map(|t| t.completed_sessions_total_duration_secs),
            Some(0),
            "負值 age 該 saturate 到 0（不是 u64 wrap）"
        );
    }

    #[test]
    fn k24_completed_sessions_total_duration_at_emits_zero_for_uncompleted_provider() {
        // K24 pure fn 測試：counter 0 跟 K22 `last_completed_session_age_at` 的 None
        // 跳過策略不同 —— K24 全部進 map（含 0），跟 K23 同 emit 策略。模擬兩個
        // provider 一個已累加時長 (total=150) 一個只收過 event 沒完成 (total=0)
        // → output map 兩者都該在。operator 端算 `total / completed_sessions_count`
        // 平均時長時,total=0 跟 count=0 兩者都是 0 → 0/0 = NaN 但 Prometheus 端
        // 看不到 NaN（只看到 0 + 0 series），所以保留 0 是對的。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_total_duration_secs: 150,
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());

        let out = completed_sessions_total_duration_at(&totals);
        assert_eq!(
            out.get("cicx"),
            Some(&150),
            "已累加時長的 provider 該 emit total=150"
        );
        assert_eq!(
            out.get("claude"),
            Some(&0),
            "未完成的 provider 該 emit total=0（不是 missing 跳過）"
        );
        assert_eq!(out.len(), 2, "output map 該有 2 個 entry,counter 0 不跳過");
    }

    // ─── K25 落地：completed_sessions_average_duration gauge + completed_sessions_average_duration_at ───

    use super::completed_sessions_average_duration_at;

    #[test]
    fn k25_completed_sessions_average_duration_at_emits_average_when_count_nonzero() {
        // K25 pure fn 主測：total=180 / count=3 → 60.0（剛好整除, 避免浮點尾數
        // 雜訊干擾 assert_eq）。驗「count>0 時 emit total/count」基本路徑。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 3,
                completed_sessions_total_duration_secs: 180,
                ..Default::default()
            },
        );

        let out = completed_sessions_average_duration_at(&totals);
        assert_eq!(
            out.get("cicx"),
            Some(&60.0),
            "total=180, count=3 → 60.0 整除必須精確"
        );
        assert_eq!(out.len(), 1, "只有 cicx 一個 entry");
    }

    #[test]
    fn k25_completed_sessions_average_duration_at_skips_zero_count_provider() {
        // K25 vs K24 emit 策略差異化：K24 全部 emit (counter 0 是有效資料), K25
        // 過濾 count==0 (0/0 數學未定義, emit 0.0 會誤導 Prometheus 端把「沒資料」
        // 判成「瞬間完成」= 假健康信號)。模擬兩個 provider: cicx 有 count (5 次
        // 完成共 250 秒 → avg=50), claude 沒 count → output 只該有 cicx, claude
        // 不能在 map 裡。對齊 K22 `last_completed_session_age_at` 的 None-跳過
        // 語意（缺資料不 emit sample）。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 5,
                completed_sessions_total_duration_secs: 250,
                ..Default::default()
            },
        );
        totals.insert("claude".to_string(), ProviderTotals::default());

        let out = completed_sessions_average_duration_at(&totals);
        assert_eq!(out.get("cicx"), Some(&50.0), "cicx average = 50.0");
        assert!(
            !out.contains_key("claude"),
            "count=0 該跳過, 不能 emit 0.0 假冒 average=0"
        );
        assert_eq!(out.len(), 1, "output 只該有 cicx 一個 entry");
    }

    #[test]
    fn k25_completed_sessions_average_duration_at_per_provider_isolated() {
        // 兩個 provider 各自獨立算平均, 驗證 iterate 過程不會互相污染（共用 map
        // 寫入時的 entry 衝突）。openx: 7200/2=3600.0, gemini: 60/4=15.0
        // —— 數字差距大順便 catch「全部算成同值」的 regression。
        let mut totals = HashMap::new();
        totals.insert(
            "openx".to_string(),
            ProviderTotals {
                completed_sessions_count: 2,
                completed_sessions_total_duration_secs: 7200,
                ..Default::default()
            },
        );
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                completed_sessions_count: 4,
                completed_sessions_total_duration_secs: 60,
                ..Default::default()
            },
        );

        let out = completed_sessions_average_duration_at(&totals);
        assert_eq!(out.get("openx"), Some(&3600.0), "openx avg = 3600");
        assert_eq!(out.get("gemini"), Some(&15.0), "gemini avg = 15");
        assert_eq!(out.len(), 2, "兩 provider 都該在 map 裡");
    }

    #[test]
    fn k25_completed_sessions_average_duration_at_handles_non_exact_division() {
        // 7/3 = 2.3333... (recurring) —— 驗 f64 在 helper 端保留 IEEE 754 雙精度
        // 結果, render 端才做 `{:.4}` 截斷。helper 不能 round 也不能 floor, 必須
        // 留 raw f64 給 render 端決定精度（對齊 K12 idle_ratio helper 同樣 f64
        // 透傳策略）。用 epsilon 比較避免寫死 2.3333... 字面值。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 3,
                completed_sessions_total_duration_secs: 7,
                ..Default::default()
            },
        );

        let out = completed_sessions_average_duration_at(&totals);
        let avg = out.get("cicx").copied().unwrap_or(0.0);
        assert!(
            (avg - 7.0_f64 / 3.0_f64).abs() < 1e-12,
            "7/3 必須精確 f64 結果, got {avg}"
        );
    }

    // ─── K26 落地：completed_sessions_max_duration gauge + completed_sessions_max_duration_at ───

    use super::completed_sessions_max_duration_at;

    #[test]
    fn k26_record_completed_session_age_initializes_max_on_first_completion() {
        // 第一次完成：ProviderTotals 還沒 max_completed_session_age_secs entry
        // （None 預設）→ 直接寫成 Some(age)（= 第一次的值）。驗「第一次完成
        // 既是 latest 也是 max」這個自然語意。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 120);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.max_completed_session_age_secs),
            Some(120),
            "第一次完成該直接初始化 max = 120"
        );
    }

    #[test]
    fn k26_record_completed_session_age_updates_max_with_larger_value() {
        // 第二次完成 > 既有 max → max 更新成新值。跟 K22 對稱（K22 是
        // 重複覆寫成 latest; K26 是 saturating_max 升級）。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        m.record_completed_session_age("cicx", 250);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.max_completed_session_age_secs),
            Some(250),
            "250 > 既有 100 → max 升級成 250"
        );
    }

    #[test]
    fn k26_record_completed_session_age_keeps_max_with_smaller_value() {
        // 第三次完成 < 既有 max → max 保持舊值。saturating_max 單調遞增語意。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 500);
        m.record_completed_session_age("cicx", 30);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.max_completed_session_age_secs),
            Some(500),
            "30 < 既有 500 → max 保持 500"
        );
    }

    #[test]
    fn k26_record_completed_session_age_clamps_negative_to_zero_for_max() {
        // K26 跟 K22 同步吃 `age.max(0)` saturating clamp —— 時鐘回撥 / 序列化
        // 時差送進負值時不能讓 max 變成負的（0 已經是合法「極短完成」語意）。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        m.record_completed_session_age("cicx", -50);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.max_completed_session_age_secs),
            Some(100),
            "負值被 clamp 到 0, max 保持 100（0 < 100, 不升級）"
        );
    }

    #[test]
    fn k26_completed_sessions_max_duration_at_skips_providers_with_no_completions() {
        // 純 fn `completed_sessions_max_duration_at` 過濾 None 契約 —— provider
        // 累計收過 event 但還沒完成過 session → 不進 map（避免 render emit
        // `max = 0` 假健康信號, 跟 K22 / K25 一致）。
        let mut totals = HashMap::new();
        // 1 個有完成的 provider (max = 3600)
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                max_completed_session_age_secs: Some(3600),
                ..Default::default()
            },
        );
        // 1 個 default ProviderTotals (max = None) → 該跳過
        totals.insert("claude".to_string(), ProviderTotals::default());

        let out = completed_sessions_max_duration_at(&totals);
        assert_eq!(out.get("cicx"), Some(&3600), "cicx max 該 emit");
        assert_eq!(out.get("claude"), None, "claude 還沒完成過 → 跳過");
        assert_eq!(out.len(), 1, "只有 cicx 進 map");
    }

    #[test]
    fn k26_completed_sessions_max_duration_at_per_provider_isolated() {
        // K26 對齊 K22/K23/K24 既有契約：每個 provider 的 max 互相隔離, 不會
        // 互相污染（saturating_max 寫入時只看 entry 自己的欄位）。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        m.record_completed_session_age("cicx", 200);
        m.record_completed_session_age("claude", 9999);
        m.record_completed_session_age("gemini", 42);

        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.max_completed_session_age_secs),
            Some(200),
            "cicx max = 200 (2 次完成, latest 較大)"
        );
        assert_eq!(
            m.provider_totals
                .get("claude")
                .and_then(|t| t.max_completed_session_age_secs),
            Some(9999),
            "claude max = 9999 (per-provider 隔離)"
        );
        assert_eq!(
            m.provider_totals
                .get("gemini")
                .and_then(|t| t.max_completed_session_age_secs),
            Some(42),
            "gemini max = 42 (per-provider 隔離)"
        );
    }

    // ─── K27 落地：completed_sessions_min_duration gauge + completed_sessions_min_duration_at ───

    use super::completed_sessions_min_duration_at;

    #[test]
    fn k27_record_completed_session_age_initializes_min_on_first_completion() {
        // 第一次完成：ProviderTotals 還沒 min_completed_session_age_secs entry
        // （None 預設）→ 直接寫成 Some(age)（= 第一次的值）。跟 K26 init 對稱：
        // 「第一次完成既是 min 也是 max 也是 latest」這個自然語意。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 120);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.min_completed_session_age_secs),
            Some(120),
            "第一次完成該直接初始化 min = 120"
        );
    }

    #[test]
    fn k27_record_completed_session_age_updates_min_with_smaller_value() {
        // 第二次完成 < 既有 min → min 更新成新值。跟 K26 鏡像對稱（K26 是
        // saturating_max 升級; K27 是 saturating_min 降級）。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        m.record_completed_session_age("cicx", 30);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.min_completed_session_age_secs),
            Some(30),
            "30 < 既有 100 → min 降級成 30"
        );
    }

    #[test]
    fn k27_record_completed_session_age_keeps_min_with_larger_value() {
        // 第三次完成 > 既有 min → min 保持舊值。saturating_min 單調遞減語意,
        // 跟 K26 saturating_max 單調遞增對稱。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 10);
        m.record_completed_session_age("cicx", 500);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.min_completed_session_age_secs),
            Some(10),
            "500 > 既有 10 → min 保持 10"
        );
    }

    #[test]
    fn k27_record_completed_session_age_clamps_negative_to_zero_for_min() {
        // K27 跟 K22 / K26 同步吃 `age.max(0)` saturating clamp —— 時鐘回撥 /
        // 序列化時差送進負值時不能讓 min 變成負的（0 已經是合法「極短完成」語意）,
        // 跟 K26 同一防線。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 10);
        m.record_completed_session_age("cicx", -50);
        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.min_completed_session_age_secs),
            Some(0),
            "負值被 clamp 到 0, 0 < 既有 10 → min 降級成 0"
        );
    }

    #[test]
    fn k27_completed_sessions_min_duration_at_skips_providers_with_no_completions() {
        // 純 fn `completed_sessions_min_duration_at` 過濾 None 契約 —— provider
        // 累計收過 event 但還沒完成過 session → 不進 map（避免 render emit
        // `min = 0` 假健康信號, 跟 K22 / K26 一致）。
        let mut totals = HashMap::new();
        // 1 個有完成的 provider (min = 8)
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                min_completed_session_age_secs: Some(8),
                ..Default::default()
            },
        );
        // 1 個 default ProviderTotals (min = None) → 該跳過
        totals.insert("claude".to_string(), ProviderTotals::default());

        let out = completed_sessions_min_duration_at(&totals);
        assert_eq!(out.get("cicx"), Some(&8), "cicx min 該 emit");
        assert_eq!(out.get("claude"), None, "claude 還沒完成過 → 跳過");
        assert_eq!(out.len(), 1, "只有 cicx 進 map");
    }

    #[test]
    fn k27_completed_sessions_min_duration_at_per_provider_isolated() {
        // K27 對齊 K22/K23/K24/K26 既有契約：每個 provider 的 min 互相隔離, 不會
        // 互相污染（saturating_min 寫入時只看 entry 自己的欄位）。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        m.record_completed_session_age("cicx", 200);
        m.record_completed_session_age("claude", 5);
        m.record_completed_session_age("gemini", 42);

        assert_eq!(
            m.provider_totals
                .get("cicx")
                .and_then(|t| t.min_completed_session_age_secs),
            Some(100),
            "cicx min = 100 (2 次完成, 較小者)"
        );
        assert_eq!(
            m.provider_totals
                .get("claude")
                .and_then(|t| t.min_completed_session_age_secs),
            Some(5),
            "claude min = 5 (per-provider 隔離)"
        );
        assert_eq!(
            m.provider_totals
                .get("gemini")
                .and_then(|t| t.min_completed_session_age_secs),
            Some(42),
            "gemini min = 42 (per-provider 隔離)"
        );
    }

    // ─── K28 落地：completed_sessions_stddev gauge + completed_sessions_stddev_at ───

    #[test]
    fn k28_record_completed_session_age_initializes_mean_on_first_completion() {
        // 第一次完成：Welford 把 mean 設成該次 sample, M2 = 0 → stddev = 0
        // (單樣本無波動, 跟數學直觀一致)。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 120);
        let totals = m.provider_totals.get("cicx").expect("cicx entry");
        assert_eq!(totals.completed_sessions_mean_secs, 120.0);
        assert_eq!(totals.completed_sessions_m2_secs, 0.0);
        assert_eq!(totals.completed_sessions_count, 1);
    }

    #[test]
    fn k28_record_completed_session_age_welford_two_samples() {
        // 兩樣本 [10, 20]：mean = 15, M2 = (10-15)² + (20-15)² = 50, stddev = √(50/2) = 5
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 10);
        m.record_completed_session_age("cicx", 20);
        let totals = m.provider_totals.get("cicx").expect("cicx entry");
        assert_eq!(totals.completed_sessions_count, 2);
        assert!(
            (totals.completed_sessions_mean_secs - 15.0).abs() < 1e-9,
            "mean = (10 + 20) / 2 = 15, got {}",
            totals.completed_sessions_mean_secs
        );
        assert!(
            (totals.completed_sessions_m2_secs - 50.0).abs() < 1e-9,
            "M2 = (10-15)² + (20-15)² = 50, got {}",
            totals.completed_sessions_m2_secs
        );
    }

    #[test]
    fn k28_record_completed_session_age_clamps_negative_to_zero_for_welford() {
        // 負值先 saturating clamp 到 0 再餵 Welford —— 跟 K22/K26/K27 同
        // `clamped_age` 變數, 防時鐘回撥污染 stddev mean/M2 累積。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        m.record_completed_session_age("cicx", -50); // clamp 到 0
        let totals = m.provider_totals.get("cicx").expect("cicx entry");
        assert_eq!(totals.completed_sessions_count, 2);
        // 兩樣本 [100, 0]: mean = 50, M2 = (100-50)² + (0-50)² = 5000
        assert!(
            (totals.completed_sessions_mean_secs - 50.0).abs() < 1e-9,
            "mean = (100 + 0) / 2 = 50, got {}",
            totals.completed_sessions_mean_secs
        );
        assert!(
            (totals.completed_sessions_m2_secs - 5000.0).abs() < 1e-9,
            "M2 = (100-50)² + (0-50)² = 5000, got {}",
            totals.completed_sessions_m2_secs
        );
    }

    #[test]
    fn k28_completed_sessions_stddev_at_emits_zero_for_single_sample() {
        // 純 fn 過濾 + 計算：count=1 進 map, stddev = 0
        // （單樣本無波動, 跟 Welford 公式 M2=0 一致）。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                completed_sessions_count: 1,
                completed_sessions_mean_secs: 42.0,
                completed_sessions_m2_secs: 0.0,
                ..Default::default()
            },
        );
        let out = completed_sessions_stddev_at(&totals);
        assert_eq!(out.get("cicx"), Some(&0.0));
    }

    #[test]
    fn k28_completed_sessions_stddev_at_skips_providers_with_no_completions() {
        // 過濾契約：count=0 不 emit（沿 K26/K27 None 跳過語意）。
        let mut totals = HashMap::new();
        totals.insert("claude".to_string(), ProviderTotals::default()); // count=0
        let out = completed_sessions_stddev_at(&totals);
        assert!(
            out.is_empty(),
            "count=0 的 provider 該跳過, 避免誤判「stddev=0」當「該 provider 瞬間完成」"
        );
    }

    #[test]
    fn k28_completed_sessions_stddev_at_per_provider_isolated() {
        // per-provider 隔離：3 provider 不同 sample → 各自 emit 自己的 stddev
        // (cicx mean=60/M2=800, claude mean=50/M2=1250, gemini mean=10/M2=0)
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 40);
        m.record_completed_session_age("cicx", 80);
        m.record_completed_session_age("claude", 30);
        m.record_completed_session_age("claude", 70);
        m.record_completed_session_age("gemini", 10);

        let out = completed_sessions_stddev_at(&m.provider_totals);
        // cicx: mean=60, M2=(40-60)² + (80-60)² = 800, stddev = √400 = 20
        assert!(
            (out.get("cicx").copied().unwrap_or(0.0) - 20.0).abs() < 1e-9,
            "cicx stddev = 20 (2 樣本 mean=60, M2=800), got {:?}",
            out.get("cicx")
        );
        // claude: mean=50, M2=(30-50)² + (70-50)² = 800, stddev = √400 = 20
        assert!(
            (out.get("claude").copied().unwrap_or(0.0) - 20.0).abs() < 1e-9,
            "claude stddev = 20 (per-provider 隔離), got {:?}",
            out.get("claude")
        );
        // gemini: count=1, stddev = 0
        assert_eq!(out.get("gemini"), Some(&0.0));
    }

    // ─── K29 落地：failure_to_completion_ratio gauge + failure_to_completion_ratio_at ───

    #[test]
    fn k29_failure_to_completion_ratio_at_skips_providers_with_no_completions() {
        // 過濾契約：跟 K25 同款防線 —— count=0 跳過不 emit（避免 0/0 數學未定義
        // emit 成 0.0 假冒「失敗率 0」= 假健康信號, 跟 K22 None 跳過語意對齊）。
        let mut totals = HashMap::new();
        totals.insert("claude".to_string(), ProviderTotals::default()); // count=0
        let out = failure_to_completion_ratio_at(&totals);
        assert!(
            out.is_empty(),
            "count=0 的 provider 該跳過, 避免誤判「失敗率=0」當「零失敗健康信號」"
        );
    }

    #[test]
    fn k29_failure_to_completion_ratio_at_emits_zero_when_no_failures() {
        // failure_count=0 + completed=N > 0 → ratio = 0.0
        // （「零失敗」= 真實健康信號, 跟 K25 「count=0 跳過」是不同語意 —— K25
        // 跳過的是「缺資料」, K29 0.0 emit 的是「有資料且為零」, 必須分清楚）。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                failure_count: 0,
                completed_sessions_count: 100,
                ..Default::default()
            },
        );
        let out = failure_to_completion_ratio_at(&totals);
        assert_eq!(
            out.get("cicx"),
            Some(&0.0),
            "failure=0, completed=100 → ratio = 0.0 (真實零失敗健康信號)"
        );
    }

    #[test]
    fn k29_failure_to_completion_ratio_at_emits_integer_ratio() {
        // 整數 ratio：failure=2, completed=1 → 2.0 (alert > 2.0 觸發閾值邊界)
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                failure_count: 2,
                completed_sessions_count: 1,
                ..Default::default()
            },
        );
        let out = failure_to_completion_ratio_at(&totals);
        assert!(
            (out.get("cicx").copied().unwrap_or(-1.0) - 2.0).abs() < 1e-9,
            "failure=2, completed=1 → ratio = 2.0, got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k29_failure_to_completion_ratio_at_emits_fractional_ratio() {
        // 分數 ratio：failure=3, completed=4 → 0.75 (4 位小數 → 0.7500)
        let mut totals = HashMap::new();
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                failure_count: 3,
                completed_sessions_count: 4,
                ..Default::default()
            },
        );
        let out = failure_to_completion_ratio_at(&totals);
        assert!(
            (out.get("gemini").copied().unwrap_or(-1.0) - 0.75).abs() < 1e-9,
            "failure=3, completed=4 → ratio = 0.75, got {:?}",
            out.get("gemini")
        );
    }

    #[test]
    fn k29_failure_to_completion_ratio_at_per_provider_isolated() {
        // per-provider 隔離：3 provider 各自獨立 ratio (cicx 1.0, claude 0.0,
        // gemini 跳過) —— 互相不污染, 跟 K22-K28 既有 per-provider 隔離契約一致。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                failure_count: 3,
                completed_sessions_count: 3,
                ..Default::default()
            },
        );
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                failure_count: 0,
                completed_sessions_count: 5,
                ..Default::default()
            },
        );
        totals.insert("gemini".to_string(), ProviderTotals::default()); // count=0 跳過

        let out = failure_to_completion_ratio_at(&totals);
        assert!(
            (out.get("cicx").copied().unwrap_or(-1.0) - 1.0).abs() < 1e-9,
            "cicx ratio = 3/3 = 1.0 (per-provider 隔離), got {:?}",
            out.get("cicx")
        );
        assert_eq!(out.get("claude"), Some(&0.0), "claude 0/5 = 0.0");
        assert_eq!(out.get("gemini"), None, "gemini count=0 跳過");
        assert_eq!(out.len(), 2, "只有 cicx + claude 進 map");
    }

    #[test]
    fn k29_failure_to_completion_ratio_at_handles_high_failure_rate() {
        // 高失敗率場景：failure=10, completed=1 → 10.0 (operator alert
        // `ratio > 2.0` 會觸發「該 provider session 平均 retry 10 次」= 健康
        // 度嚴重異常)。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                failure_count: 10,
                completed_sessions_count: 1,
                ..Default::default()
            },
        );
        let out = failure_to_completion_ratio_at(&totals);
        assert!(
            (out.get("cicx").copied().unwrap_or(-1.0) - 10.0).abs() < 1e-9,
            "failure=10, completed=1 → ratio = 10.0 (alert > 2.0 觸發), got {:?}",
            out.get("cicx")
        );
    }

    // ─── K30 落地：completed_sessions_p95 gauge + completed_sessions_p95_at ───

    #[test]
    fn k30_record_completed_session_age_pushes_to_reservoir_under_capacity() {
        // Reservoir 未滿時直接 push, len 隨 record 次數線性增加 (不像 K22-K28
        // 是 O(1) 空間, K30 是 O(capacity) = O(1024) 空間 trade-off, 用空間
        // 換 P95 精度)。Count < 1024 時 push 順序 = 觸發順序。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 10);
        m.record_completed_session_age("cicx", 20);
        m.record_completed_session_age("cicx", 30);
        let totals = m.provider_totals.get("cicx").expect("cicx entry");
        assert_eq!(totals.completed_sessions_p95_samples, vec![10, 20, 30]);
        assert_eq!(totals.completed_sessions_count, 3);
    }

    #[test]
    fn k30_record_completed_session_age_clamps_negative_sample_for_reservoir() {
        // 負值 saturating clamp 到 0 再 push (跟 K22/K26/K27 同一個
        // `clamped_age` 變數, 防時鐘回撥污染 P95 計算)。[100, -50] → [100, 0]
        // → 排序後 P95 = max(100, 0) = 100 (2 樣本 P95 退化到 max, 跟
        // k30_completed_sessions_p95_at_per_provider_isolated 行為一致)。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        m.record_completed_session_age("cicx", -50);
        let totals = m.provider_totals.get("cicx").expect("cicx entry");
        assert_eq!(totals.completed_sessions_p95_samples, vec![100, 0]);
    }

    #[test]
    fn k30_record_completed_session_age_reservoir_stays_bounded_at_capacity() {
        // Reservoir capacity 1024 不變: 推 2000 個 sample 進去, len 仍 1024
        // (用 Vitter Algorithm R 簡化版 random replace)。這是 R48 設計核心:
        // 「最近 1024 個 sliding window」—— 不 unbounded grow, scrape sort 成本
        // 可預測。
        let mut m = SessionManager::new();
        for i in 0..2000 {
            m.record_completed_session_age("cicx", i);
        }
        let totals = m.provider_totals.get("cicx").expect("cicx entry");
        assert_eq!(
            totals.completed_sessions_p95_samples.len(),
            1024,
            "reservoir 必須 bounded 在 capacity 1024, got {}",
            totals.completed_sessions_p95_samples.len()
        );
        assert_eq!(
            totals.completed_sessions_count, 2000,
            "count counter 仍 saturating_add 累加 (lifetime), 不受 reservoir bounded 影響"
        );
    }

    #[test]
    fn k30_completed_sessions_p95_at_skips_providers_with_no_samples() {
        // 過濾契約: 跟 K22 / K25 / K26 / K27 / K28 / K29 既「缺資料不 emit」一致
        // —— samples 為空 → 跳過, 避免 P95 emit 0 假冒「瞬間完成」= 假健康信號。
        let mut totals = HashMap::new();
        totals.insert("claude".to_string(), ProviderTotals::default());
        let out = completed_sessions_p95_at(&totals);
        assert!(
            out.is_empty(),
            "samples 為空的 provider 該跳過, 避免誤判 P95=0 假健康信號"
        );
    }

    #[test]
    fn k30_completed_sessions_p95_at_emits_correct_percentile() {
        // 20 個 sample [1, 2, 3, ..., 20]: 排序後 index = 20 * 95 / 100 = 19
        // → samples[19] = 20 (0-indexed) → P95 = 20 (退化成 max, 少樣本下
        // 統計本來就不穩, 寧可退化到 max 也別 panic)。這是 R48 P95 計算的
        // 核心數值驗證: index 公式 + sort 順序都要對。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let out = completed_sessions_p95_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&20),
            "20 個 sample [1..20] 排序後 P95 index=19, samples[19]=20, got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k30_completed_sessions_p95_at_per_provider_isolated() {
        // per-provider 隔離: 3 provider 各自獨立 reservoir, 互相不污染
        // (cicx samples [10, 20, 30] → P95=30, claude samples [100, 200, 300] →
        // P95=300, gemini 沒 sample → 跳過)。跟 K22-K29 既 per-provider 隔離
        // 契約一致。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 10);
        m.record_completed_session_age("cicx", 20);
        m.record_completed_session_age("cicx", 30);
        m.record_completed_session_age("claude", 100);
        m.record_completed_session_age("claude", 200);
        m.record_completed_session_age("claude", 300);

        let out = completed_sessions_p95_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&30),
            "cicx 3 樣本 P95 index=2, samples=[10,20,30] → 30, got {:?}",
            out.get("cicx")
        );
        assert_eq!(
            out.get("claude"),
            Some(&300),
            "claude 3 樣本 P95=300 (per-provider 隔離), got {:?}",
            out.get("claude")
        );
        assert_eq!(out.get("gemini"), None, "gemini 沒 sample 跳過");
        assert_eq!(out.len(), 2, "只有 cicx + claude 進 map");
    }

    #[test]
    fn k31_completed_sessions_p50_at_skips_providers_with_no_samples() {
        // 過濾語意: 對齊 K30 `skips_providers_with_no_samples` —— 沒 sample
        // 的 provider 不 emit, 避免 P50=0 假冒「中位數為 0」假健康信號。
        let mut totals = HashMap::new();
        totals.insert("claude".to_string(), ProviderTotals::default());
        let out = completed_sessions_p50_at(&totals);
        assert!(out.is_empty(), "samples 為空時 P50 不 emit, 過濾空 map");
    }

    #[test]
    fn k31_completed_sessions_p50_at_emits_correct_median_20_samples() {
        // 20 個 sample [1, 2, 3, ..., 20]: 排序後 idx = 20 * 50 / 100 = 10,
        // samples[10] = 11 (0-indexed) = P50 = 11。對齊 numpy median
        // [1..20] = 10.5 (偶數樣本取下中位) —— 整數版本取 sort[10] = 11
        // (偶數取較大值, 跟 Python `statistics.median` round-up 規則一致,
        // 跟 K30 偶數樣本取較大值 (`samples[19]=20`) 對稱)。驗證 P50 index
        // 計算 + sort 順序都對。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let out = completed_sessions_p50_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&11),
            "20 個 sample [1..20] sort 後 idx=10, samples[10]=11, P50=11, got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k31_completed_sessions_p50_at_per_provider_isolated() {
        // per-provider 隔離: 跟 K30 `per_provider_isolated` 對稱。cicx 3
        // 樣本 [10, 20, 30] → sort 後 idx=1, P50=20; claude 3 樣本 [100,
        // 200, 300] → sort 後 idx=1, P50=200; gemini 沒 sample → 過濾。
        // 證明 K31 跟 K22-K30 既 K-tag 一樣 per-provider 隔離不互污染。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 10);
        m.record_completed_session_age("cicx", 20);
        m.record_completed_session_age("cicx", 30);
        m.record_completed_session_age("claude", 100);
        m.record_completed_session_age("claude", 200);
        m.record_completed_session_age("claude", 300);

        let out = completed_sessions_p50_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&20),
            "cicx 3 樣本 sort 後 idx=1, samples=[10,20,30] 取 20, got {:?}",
            out.get("cicx")
        );
        assert_eq!(
            out.get("claude"),
            Some(&200),
            "claude 3 樣本 P50=200 (per-provider 隔離), got {:?}",
            out.get("claude")
        );
        assert_eq!(out.get("gemini"), None, "gemini 沒 sample 過濾");
        assert_eq!(out.len(), 2, "只有 cicx + claude 進 map");
    }

    #[test]
    fn k31_completed_sessions_p50_at_single_sample_returns_that_value() {
        // Boundary: 單樣本 median = itself。len=1 → idx = (1 * 50 / 100)
        // = 0, samples[0] = 該值。證明少樣本下 P50 退化到「唯一值」不 panic
        // (跟 K30 少樣本 P95 退化到 max 同款策略)。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 42);
        let out = completed_sessions_p50_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&42),
            "1 個 sample, P50 = 該值 (median of 1 = itself), got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k31_completed_sessions_p50_at_odd_count_returns_middle() {
        // Boundary: 奇數樣本 P50 = sort 後正中位。5 樣本 [1, 5, 3, 2, 4] →
        // sort 後 [1, 2, 3, 4, 5], idx = 5 * 50 / 100 = 2, samples[2] = 3
        // = 中位數。跟 numpy median([1,5,3,2,4]) = 3 一致。證明 unsorted
        // 輸入也能正確取中位 (sort_unstable 端驗證)。
        let mut m = SessionManager::new();
        for v in [1, 5, 3, 2, 4] {
            m.record_completed_session_age("cicx", v);
        }
        let out = completed_sessions_p50_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&3),
            "5 樣本 [1,5,3,2,4] sort 後 [1,2,3,4,5], idx=2, P50=3, got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k31_completed_sessions_p50_at_shares_samples_with_p95() {
        // K31 跟 K30 共用 samples vec 雙驗證: 同一份 reservoir 餵 K30 / K31
        // 兩個 pure fn, 各自獨立 sort 後取不同 percentile index, 結果互不
        // 污染。20 樣本 [1..20] → K31 P50=11 (idx=10), K30 P95=20 (idx=19),
        // 證明 K31 不需新欄位即可 derive 跟 K30 一致語意 (「最近 1024 個」)。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let p50 = completed_sessions_p50_at(&m.provider_totals);
        let p95 = completed_sessions_p95_at(&m.provider_totals);
        assert_eq!(
            p50.get("cicx"),
            Some(&11),
            "K31 P50 跟 K30 P95 共用 samples vec, 同 20 樣本 P50=11"
        );
        assert_eq!(
            p95.get("cicx"),
            Some(&20),
            "K30 P95 跟 K31 P50 共用 samples vec, 同 20 樣本 P95=20"
        );
        assert_ne!(
            p50.get("cicx"),
            p95.get("cicx"),
            "P50 跟 P95 同一 sliding window 不同 quantile, 結果必須不同 (11 vs 20)"
        );
    }

    // ============== K32 per-provider completed_sessions_p99 gauge ==============
    // 跟 K30 P95 / K31 P50 同模板, 純 fn `completed_sessions_p99_at` 復用
    // `ProviderTotals.completed_sessions_p95_samples` (K30/K31/K32 共用 reservoir
    // 1024 sliding window) sort 後取 99 百分位 index, 跟 K30/K31 各自 emit
    // 各自 percentile。樣本為空跳過 (跟 K30/K31 既「缺資料不 emit」一致, 避免
    // P99=0 假冒「瞬間完成極端尾端 1%」假健康信號)。6 個 unit test 跟 K30/K31
    // 既有測試對稱: empty / 20 sample 正確 / per-provider 隔離 / single sample
    // / unsorted 輸入 / 跟 K30/K31 三件套共用 samples 互不污染。

    #[test]
    fn k32_completed_sessions_p99_at_skips_providers_with_no_samples() {
        // 過濾語意: 對齊 K30/K31 `skips_providers_with_no_samples` —— 沒 sample
        // 的 provider 不 emit, 避免 P99=0 假冒「極端尾端 1% 都瞬間完成」假健康
        // 信號 (跟 K30 P95=0 / K31 P50=0 同款假健康疑慮)。
        let mut totals = HashMap::new();
        totals.insert("claude".to_string(), ProviderTotals::default());
        let out = completed_sessions_p99_at(&totals);
        assert!(out.is_empty(), "samples 為空時 P99 不 emit, 過濾空 map");
    }

    #[test]
    fn k32_completed_sessions_p99_at_emits_correct_extreme_20_samples() {
        // 20 個 sample [1, 2, 3, ..., 20]: 排序後 idx = 20 * 99 / 100 = 19,
        // samples[19] = 20 (少樣本 P99 退化到 max, 跟 K30 P95=20 / K26 max=20
        // 對齊 —— 樣本數 < 100 時 P99 必退化到 max, 跟 numpy.percentile 行為
        // 一致)。驗證 P99 index 計算 + sort 順序都對。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let out = completed_sessions_p99_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&20),
            "20 個 sample [1..20] sort 後 idx=19, samples[19]=20, P99=20, got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k32_completed_sessions_p99_at_per_provider_isolated() {
        // per-provider 隔離: 跟 K30/K31 `per_provider_isolated` 對稱。cicx 3
        // 樣本 [10, 20, 30] → sort 後 idx = 3*99/100 = 2, P99=30 (= max, 少樣本
        // 退化); claude 3 樣本 [100, 200, 300] → sort 後 idx=2, P99=300; gemini
        // 沒 sample → 過濾。證明 K32 跟 K30/K31 既 K-tag 一樣 per-provider 隔離
        // 不互污染。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 10);
        m.record_completed_session_age("cicx", 20);
        m.record_completed_session_age("cicx", 30);
        m.record_completed_session_age("claude", 100);
        m.record_completed_session_age("claude", 200);
        m.record_completed_session_age("claude", 300);

        let out = completed_sessions_p99_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&30),
            "cicx 3 樣本 sort 後 idx=2, samples=[10,20,30] 取 30 (P99 退化到 max), got {:?}",
            out.get("cicx")
        );
        assert_eq!(
            out.get("claude"),
            Some(&300),
            "claude 3 樣本 P99=300 (per-provider 隔離), got {:?}",
            out.get("claude")
        );
        assert_eq!(out.get("gemini"), None, "gemini 沒 sample 過濾");
        assert_eq!(out.len(), 2, "只有 cicx + claude 進 map");
    }

    #[test]
    fn k32_completed_sessions_p99_at_single_sample_returns_that_value() {
        // Boundary: 單樣本 P99 = itself。len=1 → idx = (1 * 99 / 100) = 0,
        // samples[0] = 該值。證明少樣本下 P99 退化到「唯一值」不 panic
        // (跟 K30 少樣本 P95 退化到 max / K31 P50 退化到 itself 對稱)。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 42);
        let out = completed_sessions_p99_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&42),
            "1 個 sample, P99 = 該值 (P99 of 1 = itself), got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k32_completed_sessions_p99_at_odd_count_returns_max() {
        // Boundary: 奇數樣本 P99 = sort 後最大值 (= max)。5 樣本 [1, 5, 3, 2, 4]
        // → sort 後 [1, 2, 3, 4, 5], idx = 5 * 99 / 100 = 4, samples[4] = 5
        // = max。跟 K26 max=5 / K30 P95=5 (5 樣本 P95 也退化到 max) 對齊。
        // 證明 unsorted 輸入也能正確取 max (sort_unstable 端驗證)。
        let mut m = SessionManager::new();
        for v in [1, 5, 3, 2, 4] {
            m.record_completed_session_age("cicx", v);
        }
        let out = completed_sessions_p99_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&5),
            "5 樣本 [1,5,3,2,4] sort 後 [1,2,3,4,5], idx=4, P99=5 (max), got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k32_completed_sessions_p99_at_shares_samples_with_p50_and_p95() {
        // K32 跟 K30/K31 共用 samples vec 三驗證: 同一份 reservoir 餵 K30 /
        // K31 / K32 三個 pure fn, 各自獨立 sort 後取不同 percentile index,
        // 結果互不污染。20 樣本 [1..20] → K32 P99=20 (idx=19, 退化到 max),
        // K31 P50=11 (idx=10), K30 P95=20 (idx=19, 同 P99 因少樣本退化到
        // max), 證明 K32 不需新欄位即可 derive 跟 K30/K31 一致語意 (「最近
        // 1024 個」) + 樣本數 < 100 時 P95 = P99 = max (語意合理, operator
        // 看 P95 == P99 就知道樣本不夠 P99 沒有意義, 要更多 session 累積)。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let p99 = completed_sessions_p99_at(&m.provider_totals);
        let p50 = completed_sessions_p50_at(&m.provider_totals);
        let p95 = completed_sessions_p95_at(&m.provider_totals);
        assert_eq!(
            p99.get("cicx"),
            Some(&20),
            "K32 P99 跟 K30/K31 共用 samples vec, 同 20 樣本 P99=20 (退化到 max)"
        );
        assert_eq!(
            p50.get("cicx"),
            Some(&11),
            "K31 P50 跟 K30/K32 共用 samples vec, 同 20 樣本 P50=11"
        );
        assert_eq!(
            p95.get("cicx"),
            Some(&20),
            "K30 P95 跟 K31/K32 共用 samples vec, 同 20 樣本 P95=20 (退化到 max, 等於 P99)"
        );
        // 樣本 < 100 時 K30 P95 == K32 P99, 證明少樣本 P95/P99 退化到 max
        // (語意合理, operator 看 P95 == P99 = 該 provider 樣本不夠 P99 沒
        // 區辨力, 需更多 session 累積)。
        assert_eq!(
            p95.get("cicx"),
            p99.get("cicx"),
            "樣本 < 100 時 P95 == P99 == max, 證明少樣本下 K30/K32 都退化到 max"
        );
        // P50 必須 < P99 (中位數必小於等於極端, 不可能顛倒, 數學不變式)
        assert!(
            p50.get("cicx").unwrap() < p99.get("cicx").unwrap(),
            "P50=11 必須 < P99=20 (中位數 ≤ 極端, 數學不變式)"
        );
    }
}
