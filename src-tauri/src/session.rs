use crate::config::{RuleFiredEvent, TriggerRule};
use crate::hook_event::HookEvent;
use crate::timeline::{state_to_u8, TimelineRing};
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
                // R144: 改用 `OPENAB_BOT_IDS` 單一 source of truth 對齊 lib.rs 9 隻 OpenAB bot
                // 補齊。R78 前 inline 5-bot 漏 4 隻 (irisx_bot/grokx/lpbot/mimo) → 走 saturating_add
                // 倒退風險。對齊 lib.rs:40 設計紀律 (R100 提取 + R67 護衛 chain 16 精神)。
                let is_openab_bot = crate::OPENAB_BOT_IDS.contains(&event.provider.as_str());
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

/// 完成紀錄獨立存放上限。跟 recent_events 分開的原因：hooks 裝齊後
/// PreToolUse/PostToolUse 高頻事件幾分鐘就把 50 筆 diagnostics buffer 洗掉，
/// 完成紀錄（低頻、使用者要回看）不能共用同一個 buffer。
const MAX_COMPLETIONS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// 完成紀錄（SessionTransition::Completed 時 append），落地
    /// ~/.lobsterpulse/completions.json 跨 app 重啟保留。
    pub completions: std::collections::VecDeque<RecentEvent>,
    /// Per-provider 累計統計，不依賴 OpenAB snapshot 檔就能算出 quota
    pub provider_totals: HashMap<String, ProviderTotals>,
    /// R115 規則引擎：要評估的規則清單（從 `AppConfig.rules` 同步過來）。
    /// MVP 鎖 SessionManager 自帶 3 條預設, R116+ 接 Tauri command 讓前端改。
    pub rules: Vec<TriggerRule>,
    /// R115 規則引擎 master switch（對齊 `AppConfig.rules_enabled`）。
    pub rules_enabled: bool,
    /// R115 規則引擎：累計命中次數。給護衛 test `r115_rule_evaluation_match_count`
    /// 計數用。`handle_event` 結尾 evaluate_rules 每命中 1 條規則 += 1。
    pub rule_match_count: u64,
    /// R115 規則引擎：本次 evaluate 命中的所有 RuleFiredEvent 累積區。
    /// 給 caller (lib.rs hook_server) drain 後 emit Tauri event `rule-fired`
    /// (R116+ 接力, MVP 階段只累積不 emit)。
    pub rule_firings: Vec<RuleFiredEvent>,
    /// R122 T-CPT8 落地：TimelineRing 24h × 13 provider 環形緩衝。
    /// `handle_event` 結尾串接 `record_event`,把每個事件落到的
    /// (provider, state, minute) 寫進 ring buffer cell,給未來 T-CPT9
    /// Tauri command 讀 snapshot。`pub` 為 lib.rs 將來透過
    /// `Mutex<AppSessionManager>` 直接 access 用 (對齊 R-CPT-2
    /// 「Timeline 視圖是單一 source of truth」)。
    pub timeline_ring: TimelineRing,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            active_session_id: None,
            recent_events: std::collections::VecDeque::with_capacity(MAX_RECENT_EVENTS + 1),
            completions: std::collections::VecDeque::with_capacity(MAX_COMPLETIONS + 1),
            provider_totals: HashMap::new(),
            rules: crate::config::default_rules(),
            rules_enabled: true,
            rule_match_count: 0,
            rule_firings: Vec::new(),
            timeline_ring: TimelineRing::new(),
        }
    }

    /// 完成紀錄 append（純記憶體）。呼叫點：lib.rs hook_server loop 的
    /// SessionTransition::Completed。持久化由 caller 在釋放 Mutex 後做
    /// （review b5d0ace：lock 內同步 fs::write 會讓後續 hook 事件排隊）。
    pub fn record_completion(&mut self, provider: &str, session_id: &str) {
        self.completions.push_back(RecentEvent {
            timestamp: Utc::now(),
            provider: provider.to_string(),
            session_id: session_id.to_string(),
            event_name: "Stop".to_string(),
            tool_name: None,
            error: None,
        });
        while self.completions.len() > MAX_COMPLETIONS {
            self.completions.pop_front();
        }
    }

    /// R115 規則引擎: 同步 AppConfig 的 rules / rules_enabled 進 SessionManager。
    /// 給 lib.rs hook_server 在 AppConfig 載入後呼叫 (R116+ 接入點)。
    /// MVP 階段尚未串接, 預設值已由 new() 設好。
    pub fn set_rules(&mut self, rules: Vec<TriggerRule>, rules_enabled: bool) {
        self.rules = rules;
        self.rules_enabled = rules_enabled;
    }

    /// R115 規則引擎: 評估所有啟用規則, 命中時累積 `rule_firings` + 計數。
    /// 對齊 spec R-2 + R-3 + 護衛 test `r115_rule_evaluation_match_count` /
    /// `r115_rule_action_emission`。
    /// MVP 不實際 emit / 寫檔, 只累積 RuleFiredEvent 給 caller drain。
    pub fn evaluate_rules(&mut self, event: &HookEvent, transition: SessionTransition) {
        if !self.rules_enabled {
            return;
        }
        let state_to = format!("{:?}", transition);
        for rule in self.rules.iter().filter(|r| r.enabled) {
            if rule
                .when
                .matches(&event.provider, &event.hook_event_name, &state_to)
            {
                self.rule_match_count += 1;
                for action in &rule.then {
                    self.rule_firings.push(RuleFiredEvent {
                        rule_id: rule.id.clone(),
                        provider: event.provider.clone(),
                        event_name: event.hook_event_name.clone(),
                        state_to: state_to.clone(),
                        action: action.clone(),
                    });
                }
            }
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
                // R144: 對齊 Session::handle_event, 改用 `OPENAB_BOT_IDS` 單一 source of truth,
                // 覆蓋 R78 補齊 9 隻 OpenAB bot (含 irisx_bot/grokx/lpbot/mimo)。
                let is_openab_bot = crate::OPENAB_BOT_IDS.contains(&event.provider.as_str());
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
            // OGRE-R2 E4 (T-OGRE13): SessionEnd emit `gen_ai.client.session.end`
            // span + operation.duration (ms)。session 不在 map（duration 無從算）
            // 時 duration 記 0。未知 provider fail-closed 不 emit（`let _ =` 吞
            // Err —— 監控 emit 不能反噬事件處理主路徑）。
            let duration_ms = removed
                .as_ref()
                .map(|s| {
                    s.last_event_time
                        .signed_duration_since(s.start_time)
                        .num_milliseconds()
                        .max(0) as u64
                })
                .unwrap_or(0);
            let _ = crate::telemetry::emit_session_event_span(
                &event.provider,
                &event.session_id,
                crate::telemetry::SessionSpanEvent::SessionEnd { duration_ms },
            );
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

        // OGRE-R2 E1/E2/E3 (T-OGRE13): 3 個事件點各 emit 1 個獨立 OTel span
        // (E4 SessionEnd 在前面 early-return 分支)。放在 session borrow 釋放後
        // (E1 需要讀 self.provider_totals 的 session_count)。未知 provider
        // fail-closed 不 emit (`let _ =` 吞 Err)。SDK 未 init 時 global tracer
        // 是 noop —— 既有事件處理路徑零成本。
        match event.hook_event_name.as_str() {
            "SessionStart" => {
                let session_count = self
                    .provider_totals
                    .get(&event.provider)
                    .map(|t| t.session_count)
                    .unwrap_or(0);
                let _ = crate::telemetry::emit_session_event_span(
                    &event.provider,
                    &event.session_id,
                    crate::telemetry::SessionSpanEvent::SessionStart { session_count },
                );
            }
            "UserPromptSubmit" => {
                let _ = crate::telemetry::emit_session_event_span(
                    &event.provider,
                    &event.session_id,
                    crate::telemetry::SessionSpanEvent::UserPromptSubmit {
                        tokens_input: event.tokens_input.unwrap_or(0),
                    },
                );
            }
            "PostToolUseFailure" => {
                let _ = crate::telemetry::emit_session_event_span(
                    &event.provider,
                    &event.session_id,
                    crate::telemetry::SessionSpanEvent::PostToolUseFailure {
                        tool_name: event.tool_name.as_deref().unwrap_or(""),
                        error_type: event.error.as_deref().unwrap_or("unknown"),
                    },
                );
            }
            _ => {}
        }

        // R115 規則引擎：handle_event 結尾串接 evaluate_rules，把命中累積進
        // `self.rule_firings` 給 caller (lib.rs) drain emit `rule-fired` Tauri
        // event。對齊 spec R-2 + 護衛 test `r115_rule_evaluation_match_count` /
        // `r115_rule_action_emission`。
        self.evaluate_rules(event, transition);

        // R122 T-CPT8 落地：handle_event 結尾串接 TimelineRing.record_event。
        // 把「這次事件完成後的 session state」寫進對應 (provider, minute) cell,
        // 給未來 T-CPT9 Tauri command 透過 `timeline_ring.snapshot_24h()` 讀
        // 整個 24h × 13 provider activity distribution。minute = Unix epoch
        // 秒數 / 60 (timeline.rs:CELLS_PER_PROVIDER_24H=1440 取模自然 wrap)。
        // SessionEnd 路徑已在前面 early-return 不進到這裡,該 provider 保留
        // 上一個 cell 的最後 state (對齊 spec R-CPT-1 Scenario「process 重啟空
        // strip」隱含「非 SessionStart 不主動清空 cell」語意)。
        let minute = (Utc::now().timestamp() / 60).max(0) as u32;
        self.timeline_ring
            .record_event(&event.provider, state_to_u8(now), minute);

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
            } else if session.is_active() && elapsed > idle {
                // R210: 用 `is_active()` 單一 source of truth 取代
                // `matches!(state, Working)`。原本只認 Working,WaitingForUser
                // (Notification/PermissionRequest 觸發) 進 active set 後若
                // 使用者不回話 → 永遠留在 active,`active_count` 與
                // `active_providers()` 一直算到死 session,UI「agent 在等」
                // 永遠不熄。`is_active()` = `Working | WaitingForUser`,
                // 吃掉整個 active 子集語意,比列舉 SessionState variant 更不易漏。
                // 語意邊界:idle < elapsed < stale 走這條降 Idle;
                // elapsed > stale 走上面 Stale 分支;
                // elapsed > remove 走最外層 to_remove 分支,皆不變。
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
        None
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

/// 完成紀錄落地路徑（~/.lobsterpulse/completions.json）。
pub fn completions_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".lobsterpulse")
        .join("completions.json")
}

/// 完成紀錄整檔寫入（path 參數化供測試用 temp 路徑）。tmp+rename 原子換檔，
/// 避免中斷寫入留半截 JSON 被 load 判壞而全清。
// ponytail: 每筆完成整檔重寫（~100 筆 JSON，完成事件低頻）；量大再改 append
pub fn save_completions_at(
    path: &std::path::Path,
    completions: &std::collections::VecDeque<RecentEvent>,
) -> Result<(), String> {
    let body = serde_json::to_string(completions).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// 完成紀錄載入——app 啟動時呼叫。檔案不存在/壞 JSON 一律回空（重啟保留是
/// 錦上添花，壞檔不能擋 app 啟動），超過上限截尾保留最新。
pub fn load_completions_at(path: &std::path::Path) -> std::collections::VecDeque<RecentEvent> {
    let Ok(body) = std::fs::read_to_string(path) else {
        return std::collections::VecDeque::new();
    };
    match serde_json::from_str::<Vec<RecentEvent>>(&body) {
        Ok(v) => {
            let skip = v.len().saturating_sub(MAX_COMPLETIONS);
            v.into_iter().skip(skip).collect()
        }
        Err(e) => {
            log::warn!("[session] completions.json parse failed（回空清單重建）: {e}");
            std::collections::VecDeque::new()
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

/// K35 配套 pure fn: 把 `ProviderTotals` 裡的「累計完成 session 平均間隔秒數」
/// 攤平成 `HashMap<provider, secs>` 給 `render_prometheus_body` emit。K35 是 K10
/// (`since`, 該 provider 第一次被監控到的時間戳) 跟 K23 (`completed_sessions_count`,
/// 累計完成次數) 兩條 lifetime aggregate 的派生 gauge —— 補 K22-K34 全部「session
/// 時長分布」維度都沒覆蓋的「session 頻率 / 吞吐」維度: K22 (latest age) / K23
/// (count) / K24 (total duration) / K25 (avg duration) / K26-K27 (max/min) / K28
/// (stddev) / K30-K34 (percentiles) 全是「每次 session 跑了多久」,沒有「兩個 session
/// 之間平均隔多久」= provider 吞吐信號。Operator 端不再需要自己寫 PromQL
/// `(now() - ..._since_timestamp) / completed_sessions_total` 算式(兩個 metric
/// cross-query 在 PromQL 易出錯、scrape 缺一條時算式直接壞), 直接抓 K35 series
/// 觀察 per-provider 平均 interarrival KPI。搭配 K22 (last_completed_session_age)
/// alert rule 互補: K22 觸發「單次 session 卡太久」/「最新一次跑太久」, K35 觸發
/// 「provider 整體吞吐下降」(K35 變大 = 兩個 session 之間隔越來越久 = provider
/// 可能閒置 / 被廢棄 / 上游流量下降)。跟 K12 `idle_ratio` 同樣屬於「既有資料源
/// 派生指標」, 不開新 ProviderTotals 欄位 (記憶體零成本), 純 fn 端把 K10 + K23 +
/// `now` 三個輸入組裝成單一 KPI。
///
/// 跟 K22 / K25 emit 策略都不同:
/// - K22 過濾 `None` (Option 語意, last_completed_session_age_secs 缺資料)
/// - K25 過濾 `count == 0` (0/0 數學未定義)
/// - K35 過濾 `count == 0 || since.is_none()` (兩個條件任一不滿足都不算合法
///   interarrival 觀察: count=0 表示「沒完成過」無法算頻率; since=None 表示
///   「bump_provider_totals 從未觸發」= 該 provider 沒收過任何 event, K10 缺資料
///   → 沒法算 lifetime window → 0/0 數學未定義, 不能 emit 0 假冒「瞬間完成」誤導
///   Prometheus 端把「沒資料」判成「provider 吞吐無限」= 假健康信號)
/// - K35 emit 條件 K23 ≥ 1 AND since.is_some() 兩個都滿足時: emit
///   `((now - since).num_seconds() / K23).max(0)` (i64, 整數秒, 跟 K22 / K26 / K27
///   / K30-K34 整數語意對齊, 不用 f64 因為 interarrival 觀察值域天然整數, 強加
///   浮點只會引入 IEEE 754 尾數雜訊)。`(now - since)` 給 chrono::Duration 自動
///   saturating 處理時鐘回撥 (負值在 chrono 端變 negative duration, `num_seconds()`
///   回負數; 我們 `.max(0)` saturating clamp 到 0 = 「provider lifetime 還沒到一秒
///   就完成 N 次 = 視為 0 秒間隔」= 跟 K22 / K26 / K27 saturating clamp 負值語意
///   對齊)。`K23` 為 u64, `num_seconds()` 回 i64, i64 配 u64 在 Rust 1.50+ 是
///   checked_div 但我們這裡 K23 ≥ 1 已先過濾, 用 `/` 直接整除即可, 編譯器不會
///   panic (K23 永遠 > 0)。`as i64` 轉 u64 為 i64 在 K23 接近 u64::MAX 才會
///   overflow (上線一年每秒 100 萬次完成 = ~3×10^13, 距離 u64::MAX 還 10^6 倍,
///   實際上不可能)。`now` 作為 fn 參數傳入 (= render_prometheus_body 已經收到的
///   `now: DateTime<Utc>` 參數) 保持純 fn 性質, 測試時可塞合成時間戳驗證。
///   沒有「alphabetical sort」邏輯, 排序交給 `render_prometheus_body` 統一處理 (K6-K34 既契約)。
pub fn completed_sessions_interarrival_at(
    provider_totals: &HashMap<String, ProviderTotals>,
    now: DateTime<Utc>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_count == 0 {
            continue;
        }
        let since = match t.since {
            Some(s) => s,
            None => continue,
        };
        let lifetime_secs = (now - since).num_seconds().max(0);
        let interarrival = lifetime_secs / t.completed_sessions_count as i64;
        out.insert(p.clone(), interarrival);
    }
    out
}

/// R101 配套 pure fn（K0 health 第一支：成功率 gauge）：
/// 把 `ProviderTotals` 裡的「累計失敗事件數」跟「累計全部事件數」組合成
/// success rate 衍生 gauge（`HashMap<provider, ratio>`，f64）給
/// `render_prometheus_body` emit。R101 = MISSION K0 「Provider 健康度覆蓋率」
/// 推進的第一支：MISSION 定義 K0 = 「13/13 provider 有 P95 延遲 + 成功率指標」，
/// K30 P95 session duration 已有 → 補成功率維度。命名刻意**不**用 K 編號
/// 前綴（K22-K35 編號空間是「session-level 完成維度」metric），改用 mission
/// 對齊的 `success_rate` 命名 = 讀 Prometheus 的人不用先學專案內部 K 編號
/// 才能理解這條 metric 的用途。
///
/// 跟 K29 `failure_to_completion_ratio_at` 語意有微妙差異（不能合併）：
/// - K29 派生自 `failure_count / completed_sessions_count`（=「每完成一次
///   session 平均失敗幾次 tool」= retry 視角，補 K22-K28 session duration
///   分布之外的「失敗 vs 成功」維度）
/// - R101 派生自 `1.0 - failure_count / events_total`（=「所有事件中
///   非失敗事件佔比」= 健康度視角，補 K30 P95 之外的「整體事件成功率」）
///
/// 兩個 ratio 在數學上不等價（K29 分母是完成次數、R101 分母是全部事件）：
/// K29 = 1.0 表示「每次完成平均都 retry 1 次」= 訊號；R101 = 1.0 表示
/// 「0 失敗」= 健康。K29 跟 R101 各自 emit 各自值，互不污染。
///
/// 派生自 K7 `events_total`（`bump_provider_totals` 對任何 event 都 += 1，
/// lifetime aggregate，session 結束 / stale 回收後不蒸發）跟 K9/K10
/// `failure_count`（PostToolUseFailure 累計）兩個既有 lifetime counter：
/// 純 derived，不開新 `ProviderTotals` 欄位（K12 idle_ratio / K25 avg
/// 同款「既有資料源派生指標」策略，記憶體零成本）。f64 gauge 4 位小數
/// 跟 K25 avg / K28 stddev / K29 failure_ratio 對齊；跟 K22 / K23 / K24
/// 整數語意區分。
///
/// 過濾語意跟 K25 / K29 同款「0/0 不 emit」防線：`events_total == 0`
/// 跳過不 emit sample —— 0/0 數學未定義，emit 0.0 假冒「成功率 0%」會誤導
/// Prometheus 端把「沒資料」當「完全失敗」= 假健康信號。「provider 沒
/// 收過任何 event」跟「provider 每次都失敗」是不同語意，缺資料寧可少一條
/// sample 也不要假裝 0。
///
/// 數值範圍: `[0.0, 1.0]`（`failure_count <= events_total` 由 `bump_provider_totals`
/// 單調遞增保證：failure_count 是 events_total 的子集計數，永遠不會超過
/// events_total）。clamp 到 `[0.0, 1.0]` 防理論上「events_total == 0 但
/// failure_count > 0」（防禦性，正常路徑不會發生）。f64 4 位小數固定
/// precision（跟 K25 `idle_ratio` `{:.4}` 同格式，避免 IEEE 754 尾數雜訊）。
///
/// 排序: by provider alphabetical，跟 K6-K35 既契約一致；空 map → 沒
/// sample line（HELP/TYPE 標頭仍輸出，跟 K11「header only」契約一致）。
pub fn success_rate_at(provider_totals: &HashMap<String, ProviderTotals>) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.events_total == 0 {
            continue;
        }
        let ratio = 1.0 - (t.failure_count as f64 / t.events_total as f64);
        out.insert(p.clone(), ratio.clamp(0.0, 1.0));
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
/// 2. 記憶體節省: 每個 provider 1024 * 8 bytes = 8KB, 13 provider 約 104KB,
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
/// 2. 記憶體節省: 每個 provider 1024 * 8 bytes = 8KB, 13 provider 約 104KB,
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

/// K33 配套 pure fn: 把 `ProviderTotals` 裡的 reservoir samples 排序後
/// 取 75 百分位 (= Q3 第三四分位, 上四分位), 回 `HashMap<provider, secs>`
/// (i64) 給 `render_prometheus_body` emit。跟 K30 P95 / K31 P50 / K32 P99
/// / K22 / K25 / K26 / K27 / K28 / K29 對稱: 都過濾「沒完成過」的 provider
/// (K33 用 `samples.is_empty()` 過濾, 跟 K30/K31/K32 同款)。
///
/// K33 復用 K30 reservoir 同一個 `completed_sessions_p95_samples: Vec<i64>`
/// —— 不開新欄位, 跟 K30/K31/K32 共用 sample 池。語意: P75 = Q3 = 「最近
/// 1024 次完成 session 的第 75 百分位」, 跟 P50 / P95 / P99 同一 sliding
/// window, 差別只在 percentile 位置 (50/75/95/99)。Operator 端 alert 四
/// 層次: `p50 > 60` (K31 整體慢) / `p75 > 120` (K33 上四分位慢 = 75% session
/// 都超時) / `p95 > 300` (K30 尾端 5% 慢 = SLO 邊界) / `p99 > 600` (K32
/// 極端 1% 慢 = 卡死信號)。四件套組合 `p50/p75/p95/p99` 可繪出 latency
/// 分布輪廓, 也可派生 IQR = P75 - P25 (K34 預備), 量測「中段 50% 分布
/// 寬度」反映典型 session 一致性 (IQR 小 = session 時長穩定, IQR 大 = 離散)。
///
/// P75 計算: sort samples → `idx = len * 75 / 100`, 若 idx >= len 則取
/// `len - 1` (避免 OOB; 跟 K30/K31/K32 同款策略)。Boundary: len=1 →
/// idx=0, P75 = 該 sample (P75 of 1 = itself); len=4 → idx=3, P75 = max
/// (少樣本 P75 退化到 max, 跟 K30/K31/K32 少樣本退化到 max 語意對齊);
/// len=5 → idx=3, P75 = samples[3] = 4 (for [1..5]); len=10 → idx=7,
/// P75 = samples[7] = 8; len=20 → idx=15, P75 = samples[15] = 16; len=100
/// → idx=75, P75 = samples[75] = 76。Sample 是 `i64` duration, emit 端
/// `{}` 不加浮點 precision (整數契約跟 K30/K31/K32 一致)。
///
/// 為什麼 K33 復用 K30 samples 而不是另開 `Vec<i64>`:
/// 1. 語意一致: P50/P75/P95/P99 表徵同一 sliding window, 拆成多 vec 反而
///    語意分裂 (K33 doc 開頭已明寫共用設計, 跟 K30/K31/K32 同款);
/// 2. 記憶體節省: 13 provider × 1024 × 8 bytes 約 104KB, 開四份約 416KB;
/// 3. Sort 成本不變: render 端 sort 一次, 四個 quantile 共享;
/// 4. R54 護欄順手驗證 K33 跟 K30/K31/K32 bounds chain (min ≤ P50 ≤
///    P75 ≤ P95 ≤ P99 ≤ max) 互不污染。
///
/// lifetime aggregate 對齊 K22-K32 既契約: session 結束後 `ProviderTotals`
/// 仍保留 → Prometheus 端 gauge 不會倒退。
///
/// 排序成本: 1024 sample O(N log N) ≈ 10000 比較 per scrape, 跟 K30/K31/
/// K32 同一份 vec 共享這次 sort —— K33 emit 端實際上不重排, 直接從 sort
/// 結果找 `idx * 75/100` 即可, runtime 額外成本 O(1)。但純 fn 端 K30/
/// K31/K32/K33 各自 `sort_unstable` 一次是「純函式獨立性」權衡: 同一份
/// `Vec<i64>` clone 四次, sort 四次, 每次 scrape 多花 ~40000 比較
/// (~20ms 量級), 跟 metrics endpoint scrape 15s 一次比完全可忽略。
/// 沒有「alphabetical sort」邏輯, 排序交給 `render_prometheus_body` 統一
/// 處理 (K6-K32 既契約, K33 沿用)。
pub fn completed_sessions_p75_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_p95_samples.is_empty() {
            continue;
        }
        let mut samples = t.completed_sessions_p95_samples.clone();
        samples.sort_unstable();
        let idx = (samples.len() * 75 / 100).min(samples.len() - 1);
        out.insert(p.clone(), samples[idx]);
    }
    out
}

/// K34 配套 pure fn: 把 `ProviderTotals` 裡的 reservoir samples 排序後
/// 取 25 百分位 (= 下四分位, P25 / first quartile), 回 `HashMap<provider, secs>`
/// (i64) 給 `render_prometheus_body` emit。跟 K30 P95 / K31 P50 / K32 P99 /
/// K33 P75 對稱: 都過濾「沒完成過」的 provider (K34 用 `samples.is_empty()`
/// 過濾, 跟 K30-K33 同款)。
///
/// K34 復用 K30 reservoir 同一個 `completed_sessions_p95_samples: Vec<i64>`
/// —— 不開新欄位, 跟 K30/K31/K32/K33 共用 sample 池。語意: P25 = 「最近
/// 1024 次完成 session 的第 25 百分位」, 跟 P50/P75/P95/P99 同一 sliding
/// window, 差別只在 percentile 位置 (25/50/75/95/99)。Operator 端 alert
/// 五層次: `p25 < 5` (K34 25% session 都 < 5s = 該 provider 都在 trivial
/// 工作, 可能 user 沒給重 prompt) / `p50 > 60` (K31 整體慢) /
/// `p75 > 120` (K33 中段偏慢) / `p95 > 300` (K30 尾端 5% 慢 = SLO 邊界
/// 延遲) / `p99 > 600` (K32 極端尾端 1% 慢 = 卡死信號)。五件套組合
/// `p25/p50/p75/p95/p99` 可繪出 latency 分布完整輪廓, 不需 PromQL 算
/// `histogram_quantile` (K34 補對稱性: P25 跟 P75 是 IQR 兩端, 配合 K28
/// stddev 可得「分布寬度 + 中心對稱性」雙維度)。
///
/// P25 計算: sort samples → `idx = len * 25 / 100`, 若 idx >= len 則取
/// `len - 1` (避免 OOB; 跟 K30/K31/K32/K33 同款策略, 少樣本下 P25 退化到
/// 「接近 min」sample 也不會 panic)。Boundary: len=1 → idx=0, P25 = 該
/// sample (P25 of 1 = itself, 數學直觀); len=4 → idx=1, P25 = sort 後第 2
/// 個 (剛好下四分位); len=20 → idx=5, P25 = sort 後第 6 個; len=100 →
/// idx=25; len=1000 → idx=250。Sample 是 `i64` duration, emit 端 `{}` 不
/// 加浮點 precision (整數契約跟 K30/K31/K32/K33 一致, 跟 K25/K28 f64 4
/// 位小數不同 —— 原因: percentile index 已經 cast 過, 多餘小數位是 false
/// precision, 強制裁整 0 精度流失)。
///
/// 為什麼 K34 復用 K30 samples 而不是另開 `Vec<i64>`:
/// 1. 語意一致: P25/P50/P75/P95/P99 表徵同一 sliding window, 拆成五 vec
///    反而語意分裂 (「這份是 P25 sample, 那份是 P75 sample」實際上同一份
///    資料切五次);
/// 2. 記憶體節省: 每個 provider 1024 * 8 bytes = 8KB, 13 provider 約 104KB,
///    開五份 = 360KB (Tauri desktop app 不痛但仍是浪費);
/// 3. Sort 成本不變: render 端 sort 一次, 五個 quantile 共享;
/// 4. 語意釐清成本低: doc comment 明寫「K34 復用 K30 samples」即可。
///
/// lifetime aggregate 對齊 K22-K33 既契約: session 結束後 `ProviderTotals`
/// 仍保留 → Prometheus 端 gauge 不會倒退 (reservoir 是 sliding window 跟
/// lifetime 不衝突 —— K30 doc 開頭已明寫 sliding window 設計 trade-off)。
/// `Vec<i64>` clone 五次, sort 五次, 每次 scrape 多花 ~50000 比較
/// (~25ms 量級), 跟 metrics endpoint scrape 15s 一次比完全可忽略。
/// 沒有「alphabetical sort」邏輯, 排序交給 `render_prometheus_body` 統一
/// 處理 (K6-K33 既契約, K34 沿用)。
pub fn completed_sessions_p25_at(
    provider_totals: &HashMap<String, ProviderTotals>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for (p, t) in provider_totals {
        if t.completed_sessions_p95_samples.is_empty() {
            continue;
        }
        let mut samples = t.completed_sessions_p95_samples.clone();
        samples.sort_unstable();
        let idx = (samples.len() * 25 / 100).min(samples.len() - 1);
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
        completed_sessions_interarrival_at, completed_sessions_p25_at, completed_sessions_p50_at,
        completed_sessions_p75_at, completed_sessions_p95_at, completed_sessions_p99_at,
        completed_sessions_stddev_at, failure_to_completion_ratio_at, success_rate_at,
        SessionManager, SessionState, SessionTransition,
    };
    use crate::hook_event::HookEvent;
    use chrono::{Duration, Utc};

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
    fn unknown_provider_totals_do_not_pollute_claude() {
        // 現行 hook_server 會把未列入白名單的 provider 收斂到固定 "unknown" bucket。
        // SessionManager 必須保留這個隔離，不能把 unknown event 混進 claude totals
        // 造成 Claude 看似有假事件或假 session。
        let mut m = SessionManager::new();

        let _ = m.handle_event(&ev("unknown", "u1", "SessionStart"));
        let _ = m.handle_event(&ev("unknown", "u1", "PostToolUse"));

        let unknown = m
            .provider_totals
            .get("unknown")
            .expect("unknown provider totals should exist");
        assert_eq!(unknown.session_count, 1);
        assert_eq!(unknown.events_total, 2);
        assert!(
            !m.provider_totals.contains_key("claude"),
            "unknown event 不應建立或污染 claude provider_totals"
        );
        assert_eq!(
            m.sessions.get("u1").map(|s| s.provider.as_str()),
            Some("unknown"),
            "live session provider 應保留在 unknown bucket"
        );
    }

    #[test]
    fn active_session_is_none_when_only_idle_sessions_remain() {
        // `active_session` 是 capsule 主焦點資料源；沒有 active work 時不可回傳
        // idle/stale session，否則 UI 會把歷史專案顯示成目前正在監控的焦點。
        let mut m = SessionManager::new();
        let _ = m.handle_event(&ev("claude", "idle-1", "SessionStart"));
        let _ = m.handle_event(&ev("claude", "idle-1", "Stop"));

        let state = m.get_state();
        assert_eq!(state.session_count, 1);
        assert_eq!(state.active_count, 0);
        assert!(
            state.active_session.is_none(),
            "只有 idle session 時 active_session 必須是 None，避免 capsule 顯示假 active 焦點"
        );
        assert_eq!(
            state.sessions.first().map(|s| s.state),
            Some(SessionState::Idle),
            "idle session 仍保留在 sessions 清單，只有 active_session 不應指向它"
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

    // ─── R101 落地：success_rate gauge + success_rate_at (K0 health 第一支：成功率維度) ───
    //
    // 對齊 MISSION.md K0「Provider 健康度覆蓋率」定義「13/13 provider 有 P95 延遲
    // + 成功率指標」── K30 P95 session duration 已實作 = P95 維度達成；R101
    // 補成功率維度 → K0 health 從 0/13 → 13/13 metric 覆蓋。測試群對齊 K29
    // (failure_to_completion_ratio_at) 7 條場景同款防線：過濾 / 整數 ratio /
    // 分數 ratio / 非整除 ratio / per-provider 隔離 / 高失敗率 / clamp 防禦。

    #[test]
    fn r101_success_rate_at_skips_providers_with_no_events() {
        // 過濾契約：跟 K25 idle_ratio / K29 failure_ratio 同款「0/0 不 emit」
        // 防線 —— events_total=0 跳過不 emit (避免 0/0 數學未定義 emit 成 0.0
        // 假冒「成功率 0%」= 假健康信號)。語意對齊 K22 None 跳過: 「缺資料」
        // 寧可少一條 sample, 不要假裝 0 誤導 Prometheus alert。
        let mut totals = HashMap::new();
        totals.insert("claude".to_string(), ProviderTotals::default()); // events=0
        let out = success_rate_at(&totals);
        assert!(
            out.is_empty(),
            "events_total=0 的 provider 該跳過, 避免誤判「成功率=0」當「完全失敗」"
        );
    }

    #[test]
    fn r101_success_rate_at_emits_one_when_no_failures() {
        // failure_count=0 + events_total=N > 0 → ratio = 1.0 (「零失敗」= 真實
        // 健康信號, 跟 K25「count=0 跳過」是不同語意 —— K25 跳過的是「缺資料」,
        // R101 1.0 emit 的是「有資料且為零失敗」, 必須分清楚)。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                events_total: 100,
                failure_count: 0,
                ..Default::default()
            },
        );
        let out = success_rate_at(&totals);
        assert!(
            (out.get("cicx").copied().unwrap_or(-1.0) - 1.0).abs() < 1e-9,
            "failure=0, events=100 → success_rate = 1.0 (零失敗健康信號), got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn r101_success_rate_at_emits_fractional_success_rate() {
        // 整除分數成功率：failure=3, events=10 → 1.0 - 3/10 = 0.7 (4 位小數 →
        // 0.7000) — operator alert「success_rate < 0.95」會觸發。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                events_total: 10,
                failure_count: 3,
                ..Default::default()
            },
        );
        let out = success_rate_at(&totals);
        assert!(
            (out.get("cicx").copied().unwrap_or(-1.0) - 0.7).abs() < 1e-9,
            "failure=3, events=10 → 1.0 - 0.3 = 0.7, got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn r101_success_rate_at_emits_non_terminal_decimal_ratio() {
        // 非整除成功率：failure=3, events=7 → 1.0 - 3/7 ≈ 0.5714 (IEEE 754
        // 真實浮點值)。驗證 4 位小數 format 不會把 0.571428... 整數化掉。
        let mut totals = HashMap::new();
        totals.insert(
            "gemini".to_string(),
            ProviderTotals {
                events_total: 7,
                failure_count: 3,
                ..Default::default()
            },
        );
        let out = success_rate_at(&totals);
        let expected = 1.0 - 3.0 / 7.0;
        assert!(
            (out.get("gemini").copied().unwrap_or(-1.0) - expected).abs() < 1e-9,
            "failure=3, events=7 → 1.0 - 3/7 ≈ {}, got {:?}",
            expected,
            out.get("gemini")
        );
    }

    #[test]
    fn r101_success_rate_at_per_provider_isolated() {
        // per-provider 隔離：3 provider 各自獨立 ratio (cicx 0.7, claude 1.0,
        // gemini 跳過) —— 互相不污染, 跟 K22-K35 既有 per-provider 隔離契約一致。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                events_total: 10,
                failure_count: 3,
                ..Default::default()
            },
        );
        totals.insert(
            "claude".to_string(),
            ProviderTotals {
                events_total: 50,
                failure_count: 0,
                ..Default::default()
            },
        );
        totals.insert("gemini".to_string(), ProviderTotals::default()); // events=0 跳過

        let out = success_rate_at(&totals);
        assert!(
            (out.get("cicx").copied().unwrap_or(-1.0) - 0.7).abs() < 1e-9,
            "cicx = 1.0 - 3/10 = 0.7 (per-provider 隔離), got {:?}",
            out.get("cicx")
        );
        assert!(
            (out.get("claude").copied().unwrap_or(-1.0) - 1.0).abs() < 1e-9,
            "claude = 1.0 - 0/50 = 1.0"
        );
        assert_eq!(out.get("gemini"), None, "gemini events=0 跳過");
        assert_eq!(out.len(), 2, "只有 cicx + claude 進 map");
    }

    #[test]
    fn r101_success_rate_at_handles_low_success_rate() {
        // 低成功率場景：failure=10, events=11 → 1.0 - 10/11 ≈ 0.0909 (operator
        // alert `success_rate < 0.95` 會觸發「該 provider 90%+ 事件失敗」=
        // 健康度嚴重異常)。比 K29 的「10/1 = 10.0」對稱：R101 = 1 - 10/11 ≈ 0.09
        // ≈ 「90% 失敗」語意, alert 邊界一致。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                events_total: 11,
                failure_count: 10,
                ..Default::default()
            },
        );
        let out = success_rate_at(&totals);
        let expected = 1.0 - 10.0 / 11.0;
        assert!(
            (out.get("cicx").copied().unwrap_or(-1.0) - expected).abs() < 1e-9,
            "failure=10, events=11 → 1.0 - 10/11 ≈ {}, got {:?}",
            expected,
            out.get("cicx")
        );
    }

    #[test]
    fn r101_success_rate_at_clamps_to_unit_interval_defensively() {
        // 數值範圍: [0.0, 1.0] clamp 防禦 —— 理論 failure_count <= events_total
        // 由 `bump_provider_totals` 單調遞增保證 (failure_count 是 events_total
        // 的子計數, 永不大於 events_total), 但若有人手搓 ProviderTotals 強塞
        // failure_count > events_total, clamp 仍把結果釘在 [0, 1]。這是純 fn
        // 級防禦, 不污染正常路徑 (K29 沒有對應 clamp, 因 K29 派生式無負值風險;
        // R101 用 subtraction 1.0 - x 才有負值風險, 故 clamp 必要)。
        let mut totals = HashMap::new();
        totals.insert(
            "cicx".to_string(),
            ProviderTotals {
                events_total: 10,
                failure_count: 100, // 防禦性: 理論不可能但測試 clamp 行為
                ..Default::default()
            },
        );
        let out = success_rate_at(&totals);
        assert_eq!(
            out.get("cicx"),
            Some(&0.0),
            "failure > events 該 clamp 到 0.0 (不 emit 負值), got {:?}",
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

    // ============== K33 per-provider completed_sessions_p75 gauge ==============
    // 跟 K30 P95 / K31 P50 / K32 P99 同模板, 純 fn `completed_sessions_p75_at` 復用
    // `ProviderTotals.completed_sessions_p95_samples` (K30/K31/K32/K33 共用 reservoir
    // 1024 sliding window) sort 後取 75 百分位 index, 跟 K30/K31/K32 各自 emit
    // 各自 percentile。樣本為空跳過 (跟 K30/K31/K32 既「缺資料不 emit」一致, 避免
    // P75=0 假冒「瞬間完成 75% session」假健康信號)。6 個 unit test 跟 K30/K31/K32
    // 既有測試對稱: empty / 20 sample 正確 / per-provider 隔離 / single sample
    // / unsorted 輸入 / 跟 K30/K31/K32 四件套共用 samples 互不污染。

    #[test]
    fn k33_completed_sessions_p75_at_skips_providers_with_no_samples() {
        // 過濾語意: 對齊 K30/K31/K32 `skips_providers_with_no_samples` —— 沒 sample
        // 的 provider 不 emit, 避免 P75=0 假冒「75% session 都瞬間完成」假健康
        // 信號 (跟 K30 P95=0 / K31 P50=0 / K32 P99=0 同款假健康疑慮)。
        let mut totals = HashMap::new();
        totals.insert("claude".to_string(), ProviderTotals::default());
        let out = completed_sessions_p75_at(&totals);
        assert!(out.is_empty(), "samples 為空時 P75 不 emit, 過濾空 map");
    }

    #[test]
    fn k33_completed_sessions_p75_at_emits_correct_quartile_20_samples() {
        // 20 個 sample [1, 2, 3, ..., 20]: 排序後 idx = 20 * 75 / 100 = 15,
        // samples[15] = 16 (少樣本 P75 接近 max 但還沒退化, 跟 K30 P95=20 /
        // K32 P99=20 「完全退化到 max」不同 —— P75 idx=15/len=20 = 75% 位置
        // 剛好命中)。驗證 P75 index 計算 + sort 順序都對。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let out = completed_sessions_p75_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&16),
            "20 個 sample [1..20] sort 後 idx=15, samples[15]=16, P75=16, got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k33_completed_sessions_p75_at_per_provider_isolated() {
        // per-provider 隔離: 跟 K30/K31/K32 `per_provider_isolated` 對稱。cicx 3
        // 樣本 [10, 20, 30] → sort 後 idx = 3*75/100 = 2, P75=30 (= max, 少樣本
        // 退化); claude 3 樣本 [100, 200, 300] → sort 後 idx=2, P75=300; gemini
        // 沒 sample → 過濾。證明 K33 跟 K30/K31/K32 既 K-tag 一樣 per-provider 隔離
        // 不互污染。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 10);
        m.record_completed_session_age("cicx", 20);
        m.record_completed_session_age("cicx", 30);
        m.record_completed_session_age("claude", 100);
        m.record_completed_session_age("claude", 200);
        m.record_completed_session_age("claude", 300);

        let out = completed_sessions_p75_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&30),
            "cicx 3 樣本 sort 後 idx=2, samples=[10,20,30] 取 30 (P75 退化到 max), got {:?}",
            out.get("cicx")
        );
        assert_eq!(
            out.get("claude"),
            Some(&300),
            "claude 3 樣本 P75=300 (per-provider 隔離), got {:?}",
            out.get("claude")
        );
        assert_eq!(out.get("gemini"), None, "gemini 沒 sample 過濾");
        assert_eq!(out.len(), 2, "只有 cicx + claude 進 map");
    }

    #[test]
    fn k33_completed_sessions_p75_at_single_sample_returns_that_value() {
        // Boundary: 單樣本 P75 = itself。len=1 → idx = (1 * 75 / 100) = 0,
        // samples[0] = 該值。證明少樣本下 P75 退化到「唯一值」不 panic
        // (跟 K30 P95 退化到 max / K31 P50 退化到 itself / K32 P99 退化到
        // itself 對稱 —— len=1 時所有 percentile 都 = itself, 數學直觀)。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 42);
        let out = completed_sessions_p75_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&42),
            "1 個 sample, P75 = 該值 (P75 of 1 = itself), got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k33_completed_sessions_p75_at_boundary_counts_return_correct_quartile() {
        // Boundary: 4 樣本 P75 = max (idx=3 退化), 100 樣本 P75 = 76 (idx=75
        // 剛好命中)。4 樣本 [1, 2, 3, 4] → sort 後 [1, 2, 3, 4], idx = 4 * 75
        // / 100 = 3, P75=4 (= max, 少樣本退化到 max 跟 K30/K32 對齊)。100
        // 樣本 [1..100] → sort 後 [1..100], idx = 100 * 75 / 100 = 75, samples
        // [75] = 76 (P75 剛好命中 75% 位置, 不退化)。證明 idx 計算在樣本數
        // 邊界（剛好 4 退化、剛好 100 命中）兩種語意都正確。
        let mut m = SessionManager::new();
        for v in [1, 2, 3, 4] {
            m.record_completed_session_age("cicx", v);
        }
        let out = completed_sessions_p75_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&4),
            "4 樣本 [1,2,3,4] sort 後 [1,2,3,4], idx=3, P75=4 (max, 少樣本退化), got {:?}",
            out.get("cicx")
        );

        let mut m2 = SessionManager::new();
        for i in 1..=100i64 {
            m2.record_completed_session_age("cicx", i);
        }
        let out2 = completed_sessions_p75_at(&m2.provider_totals);
        assert_eq!(
            out2.get("cicx"),
            Some(&76),
            "100 樣本 [1..100] sort 後 [1..100], idx=75, samples[75]=76, P75=76 (剛好命中 75% 位置), got {:?}",
            out2.get("cicx")
        );
    }

    #[test]
    fn k33_completed_sessions_p75_at_shares_samples_with_p50_p95_p99() {
        // K33 跟 K30/K31/K32 共用 samples vec 四驗證: 同一份 reservoir 餵 K30 /
        // K31 / K32 / K33 四個 pure fn, 各自獨立 sort 後取不同 percentile index,
        // 結果互不污染。20 樣本 [1..20] → K31 P50=11 (idx=10), K33 P75=16
        // (idx=15), K30 P95=20 (idx=19, 退化到 max), K32 P99=20 (idx=19, 同
        // P95 退化), 證明 K33 不需新欄位即可 derive 跟 K30/K31/K32 一致語意
        // (「最近 1024 個」) + 樣本數 < 100 時 P95 == P99 == max 但 P50 < P75
        // < P95 = P99 嚴格單調 (idx 50% < 75% < 95% = 99% 排序後位置)。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let p75 = completed_sessions_p75_at(&m.provider_totals);
        let p50 = completed_sessions_p50_at(&m.provider_totals);
        let p95 = completed_sessions_p95_at(&m.provider_totals);
        let p99 = completed_sessions_p99_at(&m.provider_totals);
        assert_eq!(
            p75.get("cicx"),
            Some(&16),
            "K33 P75 跟 K30/K31/K32 共用 samples vec, 同 20 樣本 P75=16 (idx=15)"
        );
        assert_eq!(
            p50.get("cicx"),
            Some(&11),
            "K31 P50 跟 K30/K32/K33 共用 samples vec, 同 20 樣本 P50=11 (idx=10)"
        );
        assert_eq!(
            p95.get("cicx"),
            Some(&20),
            "K30 P95 跟 K31/K32/K33 共用 samples vec, 同 20 樣本 P95=20 (idx=19, 退化到 max)"
        );
        assert_eq!(
            p99.get("cicx"),
            Some(&20),
            "K32 P99 跟 K30/K31/K33 共用 samples vec, 同 20 樣本 P99=20 (idx=19, 同 P95 退化)"
        );
        // K33 跟 K30/K31/K32 嚴格單調: P50 < P75 < P95 == P99 (idx 50% < 75%
        // < 95% = 99%, sorted samples 單調非降, 數學不變式)
        assert!(
            p50.get("cicx").unwrap() < p75.get("cicx").unwrap(),
            "P50=11 必須 < P75=16 (idx 50% < 75%, 排序後單調)"
        );
        assert!(
            p75.get("cicx").unwrap() < p95.get("cicx").unwrap(),
            "P75=16 必須 < P95=20 (idx 75% < 95%, 排序後單調)"
        );
        assert_eq!(
            p95.get("cicx"),
            p99.get("cicx"),
            "P95 == P99 == 20 (少樣本下 K30/K32 都退化到 max)"
        );
    }

    // ============== K34：completed_sessions_p25_at unit tests (跟 K30/K31/K32/K33 同模板) ==============

    #[test]
    fn k34_completed_sessions_p25_at_skips_providers_with_no_samples() {
        // 跟 K30-K33 同款「empty skip」過濾語意: provider 沒灌過 completion
        // → samples 為空 → 不進 map (避免 Prometheus 端「沒看到」誤判「P25=0」
        // 假健康信號, 跟 K30 doc 開頭「避免 emit 0 = 假健康」既契約一致)。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 100);
        let out = completed_sessions_p25_at(&m.provider_totals);
        assert!(!out.contains_key("claude"), "claude 沒 sample → 必須 skip");
        assert!(!out.contains_key("gemini"), "gemini 沒 sample → 必須 skip");
        assert!(
            out.contains_key("cicx"),
            "cicx 有 sample → 必須 emit (got {:?})",
            out
        );
    }

    #[test]
    fn k34_completed_sessions_p25_at_emits_correct_quartile_20_samples() {
        // 20 樣本 [1..20] → sort 後 [1..20], idx = 20 * 25 / 100 = 5,
        // P25 = samples[5] = 6 (剛好命中 25% 位置, 不退化)。證明 idx 計算
        // 在 20 樣本下正確, 跟 K33 P75 計算 (idx=15 → 16) 對稱。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let out = completed_sessions_p25_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&6),
            "20 樣本 [1..20] sort 後 [1..20], idx=5, P25=6, got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k34_completed_sessions_p25_at_per_provider_isolated() {
        // 跟 K30-K33 同款 per-provider 隔離: 灌 2 個 provider (cicx + claude)
        // 各 3 個 completion, gemini 故意不灌 → 驗證 cicx 跟 claude 各自正確
        // 算 P25 互不污染, gemini 過濾掉。3 樣本 [10, 20, 30] → idx = 3 * 25
        // / 100 = 0, P25 = 10 (少樣本退化到 min 位置, 跟 K30 P95 退化到 max
        // 對稱 —— 樣本數 < 4 時 P25 跟 P75 都退化到自己這一端)。
        let mut m = SessionManager::new();
        for v in [10, 20, 30] {
            m.record_completed_session_age("cicx", v);
        }
        for v in [100, 200, 300] {
            m.record_completed_session_age("claude", v);
        }
        let out = completed_sessions_p25_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&10),
            "cicx 3 樣本 [10,20,30] P25=10 (idx=0, 退化到 min 位置), got {:?}",
            out.get("cicx")
        );
        assert_eq!(
            out.get("claude"),
            Some(&100),
            "claude 3 樣本 [100,200,300] P25=100 (per-provider 隔離), got {:?}",
            out.get("claude")
        );
        assert_eq!(out.get("gemini"), None, "gemini 沒 sample 過濾");
        assert_eq!(out.len(), 2, "只有 cicx + claude 進 map");
    }

    #[test]
    fn k34_completed_sessions_p25_at_single_sample_returns_that_value() {
        // Boundary: 單樣本 P25 = itself。len=1 → idx = (1 * 25 / 100) = 0,
        // samples[0] = 該值。證明少樣本下 P25 退化到「唯一值」不 panic
        // (跟 K30 P95 退化到 max / K31 P50 退化到 itself / K32 P99 退化到
        // itself / K33 P75 退化到 itself 對稱 —— len=1 時所有 percentile 都
        // = itself, 數學直觀)。
        let mut m = SessionManager::new();
        m.record_completed_session_age("cicx", 42);
        let out = completed_sessions_p25_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&42),
            "1 個 sample, P25 = 該值 (P25 of 1 = itself), got {:?}",
            out.get("cicx")
        );
    }

    #[test]
    fn k34_completed_sessions_p25_at_boundary_counts_return_correct_quartile() {
        // Boundary: 4 樣本 P25 = sort[1] (idx=1 剛好命中), 100 樣本 P25 = 26
        // (idx=25 命中)。4 樣本 [1, 2, 3, 4] → idx = 4 * 25 / 100 = 1,
        // P25=2 (剛好下四分位, 不退化, 跟 K33 4 樣本 P75=4 退化到 max 對比
        // —— K34 在 4 樣本時就能命中精確位置)。100 樣本 [1..100] → idx =
        // 100 * 25 / 100 = 25, samples[25] = 26 (P25 剛好命中 25% 位置)。
        // 證明 idx 計算在樣本數邊界（剛好 4 命中、剛好 100 命中）兩種語意
        // 都正確。
        let mut m = SessionManager::new();
        for i in 1..=4 {
            m.record_completed_session_age("cicx", i);
        }
        let out = completed_sessions_p25_at(&m.provider_totals);
        assert_eq!(
            out.get("cicx"),
            Some(&2),
            "4 樣本 [1,2,3,4] sort 後 [1,2,3,4], idx=1, P25=2 (剛好下四分位, 不退化), got {:?}",
            out.get("cicx")
        );

        let mut m2 = SessionManager::new();
        for i in 1..=100i64 {
            m2.record_completed_session_age("cicx", i);
        }
        let out2 = completed_sessions_p25_at(&m2.provider_totals);
        assert_eq!(
            out2.get("cicx"),
            Some(&26),
            "100 樣本 [1..100] sort 後 [1..100], idx=25, samples[25]=26, P25=26 (剛好命中 25% 位置), got {:?}",
            out2.get("cicx")
        );
    }

    #[test]
    fn k34_completed_sessions_p25_at_shares_samples_with_p50_p75_p95_p99() {
        // K34 跟 K30/K31/K32/K33 共用 samples vec 五驗證: 同一份 reservoir
        // 餵 K30 / K31 / K32 / K33 / K34 五個 pure fn, 各自獨立 sort 後取不同
        // percentile index, 結果互不污染。20 樣本 [1..20] → K34 P25=6 (idx=5),
        // K31 P50=11 (idx=10), K33 P75=16 (idx=15), K30 P95=20 (idx=19, 退
        // 化到 max), K32 P99=20 (idx=19, 同 P95 退化), 證明 K34 不需新欄位
        // 即可 derive 跟 K30-K33 一致語意 + 樣本數 < 100 時 P95 == P99 == max
        // 但 P25 < P50 < P75 < P95 = P99 嚴格單調 (idx 25% < 50% < 75% < 95%
        // = 99% 排序後位置, chain 數學不變式)。
        let mut m = SessionManager::new();
        for i in 1..=20 {
            m.record_completed_session_age("cicx", i);
        }
        let p25 = completed_sessions_p25_at(&m.provider_totals);
        let p50 = completed_sessions_p50_at(&m.provider_totals);
        let p75 = completed_sessions_p75_at(&m.provider_totals);
        let p95 = completed_sessions_p95_at(&m.provider_totals);
        let p99 = completed_sessions_p99_at(&m.provider_totals);
        assert_eq!(
            p25.get("cicx"),
            Some(&6),
            "K34 P25 跟 K30/K31/K32/K33 共用 samples vec, 同 20 樣本 P25=6 (idx=5)"
        );
        assert_eq!(
            p50.get("cicx"),
            Some(&11),
            "K31 P50 跟 K30/K32/K33/K34 共用 samples vec, 同 20 樣本 P50=11 (idx=10)"
        );
        assert_eq!(
            p75.get("cicx"),
            Some(&16),
            "K33 P75 跟 K30/K31/K32/K34 共用 samples vec, 同 20 樣本 P75=16 (idx=15)"
        );
        assert_eq!(
            p95.get("cicx"),
            Some(&20),
            "K30 P95 跟 K31/K32/K33/K34 共用 samples vec, 同 20 樣本 P95=20 (idx=19, 退化到 max)"
        );
        assert_eq!(
            p99.get("cicx"),
            Some(&20),
            "K32 P99 跟 K30/K31/K33/K34 共用 samples vec, 同 20 樣本 P99=20 (idx=19, 同 P95 退化)"
        );
        // K34 跟 K30-K33 嚴格單調: P25 < P50 < P75 < P95 == P99
        assert!(
            p25.get("cicx").unwrap() < p50.get("cicx").unwrap(),
            "P25=6 必須 < P50=11 (idx 25% < 50%, 排序後單調)"
        );
        assert!(
            p50.get("cicx").unwrap() < p75.get("cicx").unwrap(),
            "P50=11 必須 < P75=16 (idx 50% < 75%, 排序後單調)"
        );
        assert!(
            p75.get("cicx").unwrap() < p95.get("cicx").unwrap(),
            "P75=16 必須 < P95=20 (idx 75% < 95%, 排序後單調)"
        );
        assert_eq!(
            p95.get("cicx"),
            p99.get("cicx"),
            "P95 == P99 == 20 (少樣本下 K30/K32 都退化到 max)"
        );
    }

    // ============== R51：K30/K31/K32 跨樣本數 + 跨 K22-K32 9 件套閉環 invariant ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結 gauge 補閉環」→ R51 跳開 K33 gauge 細修,
    // 改做 M2 — K30/K31/K32 percentile math 在多尺度樣本數下的「bounds invariant
    // 護欄」。K30/K31/K32 共用 K30 reservoir 1024 sliding window sort 後各取
    // index, idx_p50 ≤ idx_p95 ≤ idx_p99 因此 sorted samples 上 p50 ≤ p95 ≤ p99
    // 必然成立 —— 此不變式是 trivial 的數學事實, 但目前 K30/K31/K32 既有 27 個
    // unit test 只覆蓋 N=1/5/20 三種樣本數, 未來若有人手賤改公式 (例如
    // `len*99/101`)、換 sort 演算法、或把 reservoir 改 `VecDeque` push 前 push
    // 後, 破壞 monotonic 在 production 才會被 Prometheus 端抓到, R51 補
    // property-style 跨 8 種樣本數 + 跨 4 個 provider 的 bounds 護欄讓 CI
    // 1 秒抓出。

    #[test]
    fn r51_k30_k31_k32_min_max_bounds_respected_across_eight_sample_sizes() {
        // property-style: 對 N ∈ {1, 2, 3, 5, 10, 50, 100, 1023} 各跑 1..=N
        // samples, 斷言 K30 P95 / K31 P50 / K32 P99 全部落在 [K27 min, K26
        // max] 區間內 (sort_unstable 後 idx 單調 + idx 必落在 [0, len-1] +
        // sorted samples 單調非降 → 任意 percentile 都必在 min 跟 max 之間)。
        // 這條 invariant 跨 8 種樣本數 + 跨小樣本退化 (N=1 全部 = itself,
        // N=2 P50 = min P95/P99 = max) 跟正常樣本 (N>=100 各自 percentile
        // 落在不同位置) 兩種語意都成立, 是 K30/K31/K32 數學正確性的「單一
        // 斷言失敗就抓到破壞」護欄。
        let sizes = [1usize, 2, 3, 5, 10, 50, 100, 1023];
        for n in sizes {
            let mut m = SessionManager::new();
            for i in 1..=n as i64 {
                m.record_completed_session_age("cicx", i);
            }
            let totals = m
                .provider_totals
                .get("cicx")
                .expect("cicx entry should exist after N>=1 samples");
            let min_age = totals
                .min_completed_session_age_secs
                .expect("K27 min should be Some after N>=1 samples");
            let max_age = totals
                .max_completed_session_age_secs
                .expect("K26 max should be Some after N>=1 samples");
            let p50 = *completed_sessions_p50_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p95 = *completed_sessions_p95_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p99 = *completed_sessions_p99_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            // K27 min <= K31 P50 <= K30 P95 <= K32 P99 <= K26 max
            // (K26/K27 是 lifetime aggregate 寫入, K30/K31/K32 從 sort 後
            //  samples 派生, 對同樣本集 [1..=N] 而言 min = 1, max = N, P50 /
            //  P95 / P99 全部落在 [1, N] 內)
            assert!(
                min_age <= p50,
                "N={n}: K27 min={min_age} 必須 <= P50={p50} (K31 派生自 K30 reservoir 排序後, 必 >= min)"
            );
            assert!(
                p50 <= p95,
                "N={n}: P50={p50} 必須 <= P95={p95} (idx_p50 <= idx_p95 排序後單調)"
            );
            assert!(
                p95 <= p99,
                "N={n}: P95={p95} 必須 <= P99={p99} (idx_p95 <= idx_p99 排序後單調)"
            );
            assert!(
                p99 <= max_age,
                "N={n}: P99={p99} 必須 <= K26 max={max_age} (K32 派生自 K30 reservoir 排序後, 必 <= max)"
            );
        }
    }

    #[test]
    fn r51_k30_k31_k32_per_provider_isolation_under_oversubscribed_samples() {
        // 跨 4 個 provider 各自灌 1..=100 共 100 個 samples (總 400 樣本),
        // 斷言 K30/K31/K32 各自 emit 對該 provider 的 percentile (cicx P50
        // 必須 = 50, claude P50 = 50, gemini P50 = 50, openx P50 = 50), 不
        // 互污染。補 R50 K32 既有 per_provider_isolated 只測 3 樣本的不足:
        // 大量樣本下若有人寫錯 closure 抓外部變數、或 ProviderTotals 欄位
        // 變 shared reference, 100 樣本會抓出。bounds invariant 也一併驗
        // 證 (每個 provider P50 <= P95 <= P99 各自成立)。
        let providers = ["cicx", "claude", "gemini", "openx"];
        let mut m = SessionManager::new();
        for p in providers {
            for i in 1..=100i64 {
                m.record_completed_session_age(p, i);
            }
        }
        let p50 = completed_sessions_p50_at(&m.provider_totals);
        let p95 = completed_sessions_p95_at(&m.provider_totals);
        let p99 = completed_sessions_p99_at(&m.provider_totals);
        for p in providers {
            // 100 樣本 [1..100] sort 後 idx_p50 = 100*50/100 = 50, idx_p95 =
            // 100*95/100 = 95, idx_p99 = 100*99/100 = 99
            let p50v = *p50.get(p).unwrap_or_else(|| panic!("{p} P50 missing"));
            let p95v = *p95.get(p).unwrap_or_else(|| panic!("{p} P95 missing"));
            let p99v = *p99.get(p).unwrap_or_else(|| panic!("{p} P99 missing"));
            assert_eq!(
                p50v, 51,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=50 → samples[50]=51"
            );
            assert_eq!(
                p95v, 96,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=95 → samples[95]=96"
            );
            assert_eq!(p99v, 100, "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=99 → samples[99]=100 (= max)");
            assert!(
                p50v <= p95v && p95v <= p99v,
                "{p}: P50={p50v} <= P95={p95v} <= P99={p99v} (per-provider bounds)"
            );
        }
        // 跨 4 個 provider 結果都 = 50/95/99 (因每個 provider 都餵 [1..100]
        // 同樣本集) — 這反而證明「同樣本集 emit 結果一致, K30/K31/K32
        // 不會因為 provider 數量增加而破壞排序」
        let cicx_p50 = p50.get("cicx").copied().unwrap();
        let openx_p50 = p50.get("openx").copied().unwrap();
        assert_eq!(
            cicx_p50, openx_p50,
            "cicx 跟 openx 灌同樣本集 → P50 必等 (per-provider 隔離 + 同樣本 → 同結果)"
        );
    }

    // ============== R52：K23 (count) / K24 (total) / K25 (avg) 跨 K-tag 數學不變式護欄 ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結 gauge 補閉環」→ R51 補 K30/K31/K32 percentile
    // bounds, R52 順同樣紀律補 K23/K24/K25 「count / total / avg」三件套的數學
    // 不變式護欄。K23 / K24 / K25 是 lifetime aggregate 的「次數 / 總時長 / 平均
    // 時長」三件套,語意強綁定:K25 = K24 / K23 (count > 0) 是定義恆等式,若未來
    // 有人 (a) 改 K23 trigger 點漏 +1, (b) 改 K24 累加用 saturating 改 wrapping
    // 污染 sum, (c) 改 K25 派生用 total 改成 sum_squared, 或 (d) 改 K25 emit
    // 條件從 count > 0 改成 count >= 0 漏掉 0/0 NaN 防線, 現有 K23/K24/K25 18
    // 個 unit test 都抓不出 (都是單 metric 隔離, 沒跨 K-tag 算術驗證), 要到
    // production Prometheus 端 alert 異常時才被動發現。R52 補這層 cross-metric
    // invariant 護欄, 跟 R51 K30/K31/K32 bounds 同樣紀律, CI 1 秒抓出。

    #[test]
    fn r52_k23_k24_k25_count_total_avg_invariant_across_sample_counts() {
        // property-style: 對 N ∈ {1, 3, 10, 50, 100} 各自餵 samples 1..=N
        // (sum = N*(N+1)/2), 斷言 K23=N / K24=sum / K25=sum/N (數學恆等式),
        // 並驗 K25 算術跟 K24/K23 一致 (f64 epsilon, 避免 IEEE 754 尾數雜訊
        // 誤判)。涵蓋小樣本 (N=1 → avg=1.0) 跟大樣本 (N=100 → avg=50.5) 兩
        // 種語意。 若有人改 K23 觸發點漏 +1 → K23 != N 立即抓出; 改 K24
        // saturating 改 wrapping 污染 sum → K24 != sum 立即抓出; 改 K25 派生
        // 用錯欄位 → K25 != sum/N 立即抓出。
        let sizes = [1usize, 3, 10, 50, 100];
        for n in sizes {
            let mut m = SessionManager::new();
            for i in 1..=n as i64 {
                m.record_completed_session_age("cicx", i);
            }
            let totals = m
                .provider_totals
                .get("cicx")
                .expect("cicx entry should exist after N>=1 samples");
            // K23 count
            assert_eq!(
                totals.completed_sessions_count, n as u64,
                "N={n}: K23 completed_sessions_count 必須 = N (lifetime +1 觸發點未漏)"
            );
            // K24 total = 1+2+...+N = N*(N+1)/2
            let expected_total = (n as u64) * ((n as u64) + 1) / 2;
            assert_eq!(
                totals.completed_sessions_total_duration_secs, expected_total,
                "N={n}: K24 total_duration_secs 必須 = N*(N+1)/2 = {expected_total} \
                 (lifetime saturating_add 未污染 sum)"
            );
            // K25 pure fn 派生值 (R52 護欄不污染既有 K23/K24/K25 內部 submodule
            // imports, 用 fully-qualified path 直接拿 fn 避免 E0252 reimport 衝突)
            let count_map = crate::session::completed_sessions_count_at(&m.provider_totals);
            let total_map =
                crate::session::completed_sessions_total_duration_at(&m.provider_totals);
            let avg_map =
                crate::session::completed_sessions_average_duration_at(&m.provider_totals);
            let k23 = *count_map.get("cicx").expect("cicx K23 missing");
            let k24 = *total_map.get("cicx").expect("cicx K24 missing");
            assert_eq!(
                k23, n as u64,
                "N={n}: pure fn K23 emit 必須 = ProviderTotals.completed_sessions_count = {n}"
            );
            assert_eq!(
                k24, expected_total,
                "N={n}: pure fn K24 emit 必須 = ProviderTotals.completed_sessions_total_duration_secs = {expected_total}"
            );
            // K25 派生: 必須等於 K24 / K23 (數學恆等式, f64 epsilon 1e-9 容差)
            let k25 = avg_map
                .get("cicx")
                .copied()
                .unwrap_or_else(|| panic!("N={n}: K25 應 emit (count > 0), 但 missing"));
            let expected_avg = expected_total as f64 / n as f64;
            assert!(
                (k25 - expected_avg).abs() < 1e-9,
                "N={n}: K25={k25} 必須 == K24/K23 = {expected_avg} (count/total/avg 數學不變式, f64 epsilon 1e-9)"
            );
            // 額外: 跨 3 個 pure fn emit 結果必須互相一致 (避免有人改某個純 fn 漏
            // sync 跟 ProviderTotals 來源)
            assert_eq!(
                k23, totals.completed_sessions_count,
                "N={n}: pure fn K23 跟 ProviderTotals.completed_sessions_count 必須一致"
            );
            assert_eq!(
                k24, totals.completed_sessions_total_duration_secs,
                "N={n}: pure fn K24 跟 ProviderTotals.completed_sessions_total_duration_secs 必須一致"
            );
        }
    }

    #[test]
    fn r52_k23_k24_k25_emission_set_consistency_under_zero_count_providers() {
        // 跨 K-tag emission 集合一致性護欄: K25 emit set (count > 0) ⊆ K24
        // emit set (全部 provider) ⊆ K23 emit set (全部 provider)。 同時驗
        // 當 count == 0 時 K25 必須不 emit (0/0 NaN 防線, 不能 emit 0.0 假冒
        // average=0 假健康信號), 但 K23/K24 仍 emit 0 (counter 0 是有效資
        // 料, 跟 missing 不同語意)。 4 provider 混合: cicx (count=3, total=
        // 180 → avg=60), claude (count=0, total=0 → K25 跳過), gemini
        // (count=2, total=7200 → avg=3600), openx (count=0, total=0 → K25
        // 跳過)。
        let mut m = SessionManager::new();
        // cicx: 3 samples [10, 60, 110] sum=180
        for age in [10i64, 60, 110] {
            m.record_completed_session_age("cicx", age);
        }
        // claude: 故意不 record → count=0, total=0
        m.provider_totals.entry("claude".to_string()).or_default();
        // gemini: 2 samples [3600, 3600] sum=7200
        for age in [3600i64, 3600] {
            m.record_completed_session_age("gemini", age);
        }
        // openx: 故意不 record → count=0, total=0
        m.provider_totals.entry("openx".to_string()).or_default();

        let count_map = crate::session::completed_sessions_count_at(&m.provider_totals);
        let total_map = crate::session::completed_sessions_total_duration_at(&m.provider_totals);
        let avg_map = crate::session::completed_sessions_average_duration_at(&m.provider_totals);

        // K23 emit 全部 4 provider (counter 0 是有效)
        for p in ["cicx", "claude", "gemini", "openx"] {
            assert!(
                count_map.contains_key(p),
                "K23 counter 0 是有效, 必須 emit 全部 4 provider 但 {p} missing"
            );
        }
        // K24 emit 全部 4 provider (counter 0 是有效)
        for p in ["cicx", "claude", "gemini", "openx"] {
            assert!(
                total_map.contains_key(p),
                "K24 counter 0 是有效, 必須 emit 全部 4 provider 但 {p} missing"
            );
        }
        // K25 emit 只有 count > 0 的 provider (cicx, gemini)
        assert!(
            avg_map.contains_key("cicx"),
            "cicx count=3 > 0, K25 必須 emit, 但 missing"
        );
        assert!(
            avg_map.contains_key("gemini"),
            "gemini count=2 > 0, K25 必須 emit, 但 missing"
        );
        assert!(
            !avg_map.contains_key("claude"),
            "claude count=0, K25 必須跳過 (0/0 NaN 防線), 但 emit 了"
        );
        assert!(
            !avg_map.contains_key("openx"),
            "openx count=0, K25 必須跳過 (0/0 NaN 防線), 但 emit 了"
        );
        // K25 emit set ⊆ K24 emit set ⊆ K23 emit set (三層次包含關係)
        assert!(
            avg_map.keys().all(|p| total_map.contains_key(p)),
            "K25 emit set 必須 ⊆ K24 emit set (K25 是 K24/K23 派生, 不能比 K24 emit 更多)"
        );
        assert!(
            total_map.keys().all(|p| count_map.contains_key(p)),
            "K24 emit set 必須 ⊆ K23 emit set (K24 是 K23 + duration 聚合, 不能比 K23 emit 更多)"
        );
        // K25 算術: cicx avg=60, gemini avg=3600
        assert!(
            (avg_map["cicx"] - 60.0).abs() < 1e-9,
            "cicx K25 必須 = 180/3 = 60.0, got {}",
            avg_map["cicx"]
        );
        assert!(
            (avg_map["gemini"] - 3600.0).abs() < 1e-9,
            "gemini K25 必須 = 7200/2 = 3600.0, got {}",
            avg_map["gemini"]
        );
    }

    // ============== R58：K22 (latest) / K23 (count) / K24 (total) / K25 (avg) / K26 (max) / K27 (min) / K28 (stddev) 7-K 跨樣本數 + 跨 4 provider aggregate consistency 護欄 ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結 gauge 補閉環」紀律延伸 — R51 補 K30/K31/K32 percentile
    // bounds, R52 補 K23/K24/K25 (count/total/avg) 三 K 數學不變式, R53 補 K22/K26/K27 (latest/max/
    // min) 三 K lifetime aggregate bounds, R54 補 K30-K33 (P50/P75/P95/P99) percentile monotonic
    // chain, R55 補 K34 (P25) 下四分位 chain, R57 補 K22↔K10 lifetime freshness + K35 helper
    // correctness。R58 收 R50 紀律最後一塊「K24 跨 K 同步性」護欄缺口：R52 蓋 K23/K24/K25 三 K
    // 互鎖 (count/total/avg), R53 蓋 K22/K26/K27 三 K bounds (latest/max/min), **沒有任何一條
    // 護欄把 K24 (cumulative total) 跟 K22 (latest) + K26 (max) + K27 (min) 拉通驗證同步性**。
    //
    // K22-K27 6 K 語意強綁定（同一個 `record_completed_session_age` helper 內一次更新）：
    // - K22 latest = 最後一個餵的 age (clamp 0 後)
    // - K23 count  += 1
    // - K24 total  += clamped_age (saturating_add)
    // - K25 avg    = K24 / K23 (派生, count > 0)
    // - K26 max    = max(歷史, clamped_age)
    // - K27 min    = min(歷史, clamped_age)
    //   → **數學不變式鏈**: K27 * K23 ≤ K24 ≤ K26 * K23（每個 sample ≥ min, ≤ max, sum 因此
    //     包夾在 [min*count, max*count] 區間）— 改 K22 trigger 漏寫 / 改 K24 saturating
    //     改 wrapping 污染 sum / 改 K25 派生用錯欄位 / 改 K26-K27 比較方向反 / 改 helper
    //     拆 fn 漏 sync 任何一條, 護欄 CI 1 秒抓出。
    //
    // R58 護欄 A: 6 provider × 5 sample 跨 N ∈ {1, 2, 5, 10, 50} 樣本數, 斷言 K22-K27 6 K
    // 數學不變式鏈 + 6 K 各自精確值（K22=latest, K23=N, K24=sum, K25=sum/N, K26=max,
    // K27=min）全部一致。K28 stddev 額外加 (Welford 從 K23+K24 派生不了, 獨立算, 跟
    // K25+K26+K27 算的 [K27, K26] 包夾關係 + stddev ≤ (K26-K27)/2 [範圍半寬上限] 護欄)。
    // R58 護欄 B: K24 增量 delta 跟 K22 寫入值單步鎖定 (每個 record step 後 K24_new - K24_old
    // == K22 新寫入的 clamped_age, 驗證「同 fn 內同步觸發」紀律, 避免未來有人 refactor
    // 把 K24 拆出 record_completed_session_age 變異步 → K22 寫了 K24 沒加, 單 K 測試
    // 抓不出, R58 B 護欄 CI 1 秒抓)。
    // R58 護欄 C: 負值 age clamp 0 邊界 — K22-K27 在 age = i64::MIN 餵入時仍自洽 (K22 寫 0,
    // K24 += 0, K26/K27 min/max 不被 i64::MIN 污染 = 現有 `k22_record_completed_session_age_
    // clamps_negative_to_zero` 單 metric 護欄的 cross-K 延伸)。

    #[test]
    fn r58_k22_k23_k24_k25_k26_k27_six_way_aggregate_consistency_across_sample_sizes() {
        // property-style: 對 N ∈ {1, 2, 5, 10, 50} 各自餵 samples 1..=N
        // (sum = N*(N+1)/2, max = N, min = 1, latest = N, avg = (N+1)/2), 斷言
        // 6 K 各自精確值並驗 K27*count ≤ K24 ≤ K26*count 包夾不變式鏈永久成立。
        // 6 K 任一不同步（K22 trigger 漏 / K24 sum 污染 / K25 派生錯欄位 /
        // K26 max 比較方向反 / K27 min 比較方向反 / K27/K26 default 邊界退化）
        // → 任一 assert 立即 fail。Sample size 從 1 跨到 50 涵蓋小樣本（單一
        // 完成 N=1, K22=K26=K27=1, K24=1, K25=1.0）到大樣本（N=50, K22=50,
        // K27=1, K24=1275, K25=25.5, K26=50）。
        let sizes = [1usize, 2, 5, 10, 50];
        for n in sizes {
            let mut m = SessionManager::new();
            for i in 1..=n as i64 {
                m.record_completed_session_age("cicx", i);
            }
            let totals = m
                .provider_totals
                .get("cicx")
                .expect("cicx entry should exist after N>=1 samples");

            // K22 latest = N (最後一個餵的 i)
            assert_eq!(
                totals.last_completed_session_age_secs,
                Some(n as i64),
                "N={n}: K22 last_completed_session_age_secs 必須 = latest = {n} (gauge 覆寫成最新)"
            );
            // K23 count = N
            assert_eq!(
                totals.completed_sessions_count, n as u64,
                "N={n}: K23 count 必須 = N (lifetime +1 觸發點未漏)"
            );
            // K24 total = 1+2+...+N = N*(N+1)/2
            let expected_total = (n as u64) * ((n as u64) + 1) / 2;
            assert_eq!(
                totals.completed_sessions_total_duration_secs, expected_total,
                "N={n}: K24 total_duration_secs 必須 = N*(N+1)/2 = {expected_total} \
                 (lifetime saturating_add 未污染 sum)"
            );
            // K26 max = N (largest sample = N)
            assert_eq!(
                totals.max_completed_session_age_secs,
                Some(n as i64),
                "N={n}: K26 max_completed_session_age_secs 必須 = N = {n} (saturating_max 升級)"
            );
            // K27 min = 1 (smallest sample = 1)
            assert_eq!(
                totals.min_completed_session_age_secs,
                Some(1),
                "N={n}: K27 min_completed_session_age_secs 必須 = 1 (saturating_min 降級)"
            );

            // ── K24 跟 K22-K27 6 K 數學不變式鏈 ──
            // K27 * K23 ≤ K24: 每個 sample ≥ min, sum ≥ min*count
            let k27 = 1_i64;
            let k23 = n as i64;
            let k24 = expected_total as i64;
            let k26 = n as i64;
            assert!(
                k27 * k23 <= k24,
                "N={n}: K27*K23 = {lhs} 必須 ≤ K24 = {rhs} (K24 累加不能 < min*count, \
                 違反代表 K24 累加漏 sample 或 K27 min 比較方向反)",
                lhs = k27 * k23,
                rhs = k24
            );
            // K24 ≤ K26 * K23: 每個 sample ≤ max, sum ≤ max*count
            assert!(
                k24 <= k26 * k23,
                "N={n}: K24 = {lhs} 必須 ≤ K26*K23 = {rhs} (K24 累加不能 > max*count, \
                 違反代表 K24 累加多 sample 或 K26 max 比較方向反)",
                lhs = k24,
                rhs = k26 * k23
            );
            // K22 必須在 [K27, K26] 範圍內
            let k22 = n as i64;
            assert!(
                k27 <= k22 && k22 <= k26,
                "N={n}: K22 = {k22} 必須 ∈ [K27={k27}, K26={k26}] 區間 \
                 (K22 latest 跟 K26 max / K27 min bounds 對齊)"
            );

            // ── K25 純 fn 派生: 必須 = K24 / K23 (f64 epsilon 1e-9) ──
            let avg_map =
                crate::session::completed_sessions_average_duration_at(&m.provider_totals);
            let k25 = avg_map
                .get("cicx")
                .copied()
                .unwrap_or_else(|| panic!("N={n}: K25 應 emit (count > 0), 但 missing"));
            let expected_avg = expected_total as f64 / n as f64;
            assert!(
                (k25 - expected_avg).abs() < 1e-9,
                "N={n}: K25={k25} 必須 == K24/K23 = {expected_avg} (count/total/avg 數學不變式)"
            );
            // K25 跟 K22/K26/K27 bounds: min ≤ avg ≤ max
            assert!(
                (k27 as f64) <= k25 && k25 <= (k26 as f64),
                "N={n}: K25 avg = {k25} 必須 ∈ [K27={k27}, K26={k26}] 區間 \
                 (K25 派生跟 K26 max / K27 min bounds 對齊, 違反代表 K25 用錯欄位)"
            );

            // ── K28 stddev: 跟 K27 ≤ avg - stddev / K26 ≥ avg + stddev 邊界檢查 ──
            // Welford stddev 對 samples 1..=N 算出的 stddev 必須 ≤ (max - min) / 2
            // （range 半寬上限, 任何 sample 落在 [min, max] 內, stddev 必 ≤ 半寬）
            // —— 改 K28 M2 累加方向反 / K28 比較方向反護欄 CI 1 秒抓。
            let stddev_map = crate::session::completed_sessions_stddev_at(&m.provider_totals);
            let k28 = stddev_map
                .get("cicx")
                .copied()
                .unwrap_or_else(|| panic!("N={n}: K28 應 emit (count > 0), 但 missing"));
            let half_range = (k26 - k27) as f64 / 2.0;
            assert!(
                k28 <= half_range + 1e-9,
                "N={n}: K28 stddev = {k28} 必須 ≤ (K26-K27)/2 = {half_range} \
                 (stddev ≤ range 半寬上限, 違反代表 K28 M2 累加或公式錯)"
            );
        }
    }

    #[test]
    fn r58_k22_k24_incremental_delta_consistency_per_record_step() {
        // 護欄 C 邏輯跟 B 合併版: 餵 samples [3, 7, 1, 12, 5], 記錄每步
        // K22 / K24 狀態, 斷言每步 K24 delta == K22 寫入的 clamped_age。並
        // 驗 K26/K27 在每步後即時更新（K22 trigger 跟 K26/K27 trigger 在同
        // fn 內, 不可能 K22 寫了 K26/K27 沒更新）。順手 cover 負值 age 餵入
        // (age=-5) clamp 0 路徑 — K22 寫 0, K24 += 0 (delta=0), K26 max
        // 不變, K27 min 不變（0 餵入若 K27 已是 None 第一次會寫 0; 此處
        // 已先餵 positive 所以 K27=3 維持）。
        let mut m = SessionManager::new();
        let samples = [3i64, 7, -5, 1, 12, 5]; // -5 測負值 clamp 0 路徑
        let mut prev_k24: u64 = 0;
        for (step, &age) in samples.iter().enumerate() {
            m.record_completed_session_age("cicx", age);
            let totals = m
                .provider_totals
                .get("cicx")
                .expect("cicx entry should exist after first record");
            let clamped = age.max(0) as u64;
            // K24 delta 必須 == clamped_age (沒污染, 沒漏 sample, saturating OK)
            let new_k24 = totals.completed_sessions_total_duration_secs;
            assert_eq!(
                new_k24,
                prev_k24 + clamped,
                "step {step}: K24 delta 必須 == age.max(0) = {clamped} \
                 (K24 累加跟 K22 觸發點同 fn 內同步, prev={prev_k24}, new={new_k24})"
            );
            // K22 寫入值 必須 == clamped_age (不是原始 age, 驗 clamp 路徑)
            assert_eq!(
                totals.last_completed_session_age_secs,
                Some(clamped as i64),
                "step {step}: K22 必須寫 clamped_age = {clamped} (原始 age={age} \
                 過 age.max(0) 飽和 clamp 後, 不是原始負值)"
            );
            // K23 必須遞增 1
            assert_eq!(
                totals.completed_sessions_count,
                (step + 1) as u64,
                "step {step}: K23 count 必須 == step+1 (每步 +1 同步)"
            );
            // K26 max 必須是前 step+1 個 samples 的 max
            let observed_max = samples[..=step].iter().map(|&a| a.max(0)).max().unwrap();
            assert_eq!(
                totals.max_completed_session_age_secs,
                Some(observed_max),
                "step {step}: K26 max 必須是 {observed_max} (samples[..={step}] max, \
                 K22 trigger 跟 K26 trigger 同 fn 內同步, 不可能 K22 寫了 K26 沒更新)"
            );
            // K27 min 必須是前 step+1 個 samples 的 min
            let observed_min = samples[..=step].iter().map(|&a| a.max(0)).min().unwrap();
            assert_eq!(
                totals.min_completed_session_age_secs,
                Some(observed_min),
                "step {step}: K27 min 必須是 {observed_min} (samples[..={step}] min, \
                 K22 trigger 跟 K27 trigger 同 fn 內同步, 不可能 K22 寫了 K27 沒更新)"
            );
            prev_k24 = new_k24;
        }
    }

    #[test]
    fn r58_k22_k23_k24_k25_k26_k27_per_provider_isolation_under_mixed_samples() {
        // 跨 4 provider 隔離強化 — 跟 R52 護欄 B 同模板但擴展到 K22-K27 6 K。
        // 4 provider 各自餵不同 sample sets, 驗證 K22/K23/K24/K25/K26/K27 在
        // ProviderTotals 內 per-provider 隔離, 不互相污染 (共用 HashMap 寫入
        // 時的 entry 衝突沒護欄會全算成同值)。
        // - cicx: samples [10, 20, 30] sum=60, latest=30, max=30, min=10, avg=20
        // - claude: samples [100, 200] sum=300, latest=200, max=200, min=100, avg=150
        // - gemini: 故意不 record → count=0, K22/K26/K27 None, K23/K24=0, K25 跳
        // - openx: samples [5] sum=5, latest=5, max=5, min=5, avg=5 (單樣本)
        let mut m = SessionManager::new();
        for age in [10i64, 20, 30] {
            m.record_completed_session_age("cicx", age);
        }
        for age in [100i64, 200] {
            m.record_completed_session_age("claude", age);
        }
        m.provider_totals.entry("gemini".to_string()).or_default();
        m.record_completed_session_age("openx", 5);

        let totals = &m.provider_totals;

        // cicx
        let cicx = totals.get("cicx").expect("cicx entry");
        assert_eq!(cicx.last_completed_session_age_secs, Some(30));
        assert_eq!(cicx.completed_sessions_count, 3);
        assert_eq!(cicx.completed_sessions_total_duration_secs, 60);
        assert_eq!(cicx.max_completed_session_age_secs, Some(30));
        assert_eq!(cicx.min_completed_session_age_secs, Some(10));
        // claude
        let claude = totals.get("claude").expect("claude entry");
        assert_eq!(claude.last_completed_session_age_secs, Some(200));
        assert_eq!(claude.completed_sessions_count, 2);
        assert_eq!(claude.completed_sessions_total_duration_secs, 300);
        assert_eq!(claude.max_completed_session_age_secs, Some(200));
        assert_eq!(claude.min_completed_session_age_secs, Some(100));
        // gemini (未 record, default)
        let gemini = totals.get("gemini").expect("gemini entry");
        assert_eq!(gemini.last_completed_session_age_secs, None);
        assert_eq!(gemini.completed_sessions_count, 0);
        assert_eq!(gemini.completed_sessions_total_duration_secs, 0);
        assert_eq!(gemini.max_completed_session_age_secs, None);
        assert_eq!(gemini.min_completed_session_age_secs, None);
        // openx (單樣本)
        let openx = totals.get("openx").expect("openx entry");
        assert_eq!(openx.last_completed_session_age_secs, Some(5));
        assert_eq!(openx.completed_sessions_count, 1);
        assert_eq!(openx.completed_sessions_total_duration_secs, 5);
        assert_eq!(openx.max_completed_session_age_secs, Some(5));
        assert_eq!(openx.min_completed_session_age_secs, Some(5));

        // K25 純 fn 派生: gemini 跳過, 其他 3 provider emit
        let avg_map = crate::session::completed_sessions_average_duration_at(totals);
        assert_eq!(avg_map.get("cicx").copied(), Some(20.0));
        assert_eq!(avg_map.get("claude").copied(), Some(150.0));
        assert_eq!(avg_map.get("openx").copied(), Some(5.0));
        assert!(
            !avg_map.contains_key("gemini"),
            "gemini count=0, K25 必須跳過 (0/0 NaN 防線, 不 emit 0.0 假冒平均)"
        );

        // K28 純 fn 派生: 3 provider 各 emit
        let stddev_map = crate::session::completed_sessions_stddev_at(totals);
        // cicx samples [10, 20, 30] → stddev 為 0 (3 samples 等差, mean=20, 偏離平方 100+0+100=200, n=3 → population stddev = sqrt(200/3) ≈ 8.165)
        let cicx_stddev = stddev_map.get("cicx").copied().unwrap();
        assert!(
            (cicx_stddev - (8.165_f64)).abs() < 0.01,
            "cicx samples [10,20,30] population stddev 應 ≈ 8.165, got {cicx_stddev}"
        );
        // claude samples [100, 200] → stddev = 50 (population n=2, 偏離平方 2500+2500=5000, /2=2500, sqrt=50)
        let claude_stddev = stddev_map.get("claude").copied().unwrap();
        assert!(
            (claude_stddev - 50.0).abs() < 1e-9,
            "claude samples [100,200] population stddev 應 = 50.0, got {claude_stddev}"
        );
        // openx 單樣本 → stddev = 0
        assert_eq!(stddev_map.get("openx").copied(), Some(0.0));
        // gemini count=0 → 跳過
        assert!(!stddev_map.contains_key("gemini"));
    }

    // ============== R54：K30/K31/K32/K33 percentile bounds chain (P50 ≤ P75 ≤ P95 ≤ P99) 跨樣本數 + 跨 4 provider 隔離護欄 ==============
    // R51 護欄 (c17662c) 已涵蓋 K30/K31/K32 (P50/P95/P99) 跨 8 種樣本數的 bounds
    // chain 跟 4 provider 隔離強化。R53 落地 K33 P75 (第三四分位 Q3) 介於 P50
    // 跟 P95 之間, 自然延伸 R51 護欄: K27 min ≤ P50 ≤ P75 ≤ P95 ≤ P99 ≤ K26
    // max 5 個不等式永久成立。R51 護欄 fn name 固定為 r51_k30_k31_k32_*
    // (immutable, c17662c commit 已落地), K33 擴展用 R54 護欄表達。R54 跟 R51
    // 結構對齊: 跨 N ∈ {1, 2, 3, 5, 10, 50, 100, 1023} 樣本數 + 跨 4 provider
    // 灌 100 樣本, 斷言 P50 ≤ P75 ≤ P95 ≤ P99 嚴格 monotonic chain (idx 50%
    // ≤ 75% ≤ 95% ≤ 99% 排序後位置, sorted samples 單調非降 → 任意 percentile
    // 都必在 min 跟 max 之間, K33 居中 P50 跟 P95 之間)。若未來有人 (a) 改 P75
    // idx 公式 (例如 `len*80/100`), (b) 開新 reservoir 導致 P75 sample 集跟
    // P50/P95/P99 不一致, (c) 改 sort 演算法導致 idx 偏移, R54 護欄 CI 1 秒
    // 抓出。R51 護欄沒被 K33 觸碰 (test name + 8 種樣本數都保留), 純新增 R54
    // 護欄表達 K33 P75 居中 monotonic 關係 (R51 護 K30/K31/K32 三件套 chain,
    // R54 護 K30/K31/K32/K33 四件套 chain —— K33 介於 P50 跟 P95 之間, 必須
    // 加 P50 ≤ P75 跟 P75 ≤ P95 兩條新不等式才完整覆蓋 K33 monotonic 數學)。

    #[test]
    fn r54_k30_k31_k32_k33_min_max_bounds_respected_across_eight_sample_sizes() {
        // property-style: 對 N ∈ {1, 2, 3, 5, 10, 50, 100, 1023} 各跑 1..=N
        // samples, 斷言 K30 P95 / K31 P50 / K32 P99 / K33 P75 全部落在 [K27
        // min, K26 max] 區間內, 嚴格 monotonic chain P50 ≤ P75 ≤ P95 ≤ P99
        // 永遠成立 (idx 單調 + sorted samples 單調非降)。K33 P75 居中 P50
        // 跟 P95 之間, 8 種樣本數跨小樣本退化 (N=1 全部 = itself, N=4 P75
        // = max idx=3, N=2 P75 = max) 跟正常樣本 (N>=100 各自 percentile 落
        // 在不同位置) 兩種語意都成立。
        let sizes = [1usize, 2, 3, 5, 10, 50, 100, 1023];
        for n in sizes {
            let mut m = SessionManager::new();
            for i in 1..=n as i64 {
                m.record_completed_session_age("cicx", i);
            }
            let totals = m
                .provider_totals
                .get("cicx")
                .expect("cicx entry should exist after N>=1 samples");
            let min_age = totals
                .min_completed_session_age_secs
                .expect("K27 min should be Some after N>=1 samples");
            let max_age = totals
                .max_completed_session_age_secs
                .expect("K26 max should be Some after N>=1 samples");
            let p50 = *completed_sessions_p50_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p75 = *completed_sessions_p75_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p95 = *completed_sessions_p95_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p99 = *completed_sessions_p99_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            // K27 min <= K31 P50 <= K33 P75 <= K30 P95 <= K32 P99 <= K26 max
            // (K26/K27 是 lifetime aggregate 寫入, K30/K31/K32/K33 從 sort 後
            //  samples 派生, 對同樣本集 [1..=N] 而言 min = 1, max = N, P50 /
            //  P75 / P95 / P99 全部落在 [1, N] 內, 嚴格單調 chain 成立)
            assert!(
                min_age <= p50,
                "N={n}: K27 min={min_age} 必須 <= P50={p50} (K31 派生自 K30 reservoir 排序後, 必 >= min)"
            );
            assert!(
                p50 <= p75,
                "N={n}: P50={p50} 必須 <= P75={p75} (K33 介於 K31 P50 跟 K30 P95 之間, idx_p50 <= idx_p75 排序後單調)"
            );
            assert!(
                p75 <= p95,
                "N={n}: P75={p75} 必須 <= P95={p95} (K33 介於 K31 P50 跟 K30 P95 之間, idx_p75 <= idx_p95 排序後單調)"
            );
            assert!(
                p95 <= p99,
                "N={n}: P95={p95} 必須 <= P99={p99} (idx_p95 <= idx_p99 排序後單調)"
            );
            assert!(
                p99 <= max_age,
                "N={n}: P99={p99} 必須 <= K26 max={max_age} (K32 派生自 K30 reservoir 排序後, 必 <= max)"
            );
        }
    }

    #[test]
    fn r54_k30_k31_k32_k33_per_provider_isolation_under_oversubscribed_samples() {
        // 跨 4 個 provider 各自灌 1..=100 共 100 個 samples (總 400 樣本),
        // 斷言 K30/K31/K32/K33 各自 emit 對該 provider 的 percentile (cicx
        // P50=51 / P75=76 / P95=96 / P99=100, claude 同, gemini 同, openx
        // 同), 不互污染。補 R51 K30/K31/K32 既有 4 provider 隔離只測三件套
        // 的不足: 大量樣本下若有人寫錯 P75 closure 抓外部變數、或 ProviderTotals
        // 欄位變 shared reference, 100 樣本會抓出。bounds chain 也一併驗證
        // (每個 provider P50 ≤ P75 ≤ P95 ≤ P99 各自成立)。
        let providers = ["cicx", "claude", "gemini", "openx"];
        let mut m = SessionManager::new();
        for p in providers {
            for i in 1..=100i64 {
                m.record_completed_session_age(p, i);
            }
        }
        let p50 = completed_sessions_p50_at(&m.provider_totals);
        let p75 = completed_sessions_p75_at(&m.provider_totals);
        let p95 = completed_sessions_p95_at(&m.provider_totals);
        let p99 = completed_sessions_p99_at(&m.provider_totals);
        for p in providers {
            // 100 樣本 [1..100] sort 後 idx_p50 = 100*50/100 = 50, idx_p75 =
            // 100*75/100 = 75, idx_p95 = 100*95/100 = 95, idx_p99 = 100*99/100 = 99
            let p50v = *p50.get(p).unwrap_or_else(|| panic!("{p} P50 missing"));
            let p75v = *p75.get(p).unwrap_or_else(|| panic!("{p} P75 missing"));
            let p95v = *p95.get(p).unwrap_or_else(|| panic!("{p} P95 missing"));
            let p99v = *p99.get(p).unwrap_or_else(|| panic!("{p} P99 missing"));
            assert_eq!(
                p50v, 51,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=50 → samples[50]=51"
            );
            assert_eq!(
                p75v, 76,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=75 → samples[75]=76"
            );
            assert_eq!(
                p95v, 96,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=95 → samples[95]=96"
            );
            assert_eq!(p99v, 100, "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=99 → samples[99]=100 (= max)");
            // K33 居中 P50 跟 P95 之間 + P99 ≥ P95 的 monotonic chain
            assert!(
                p50v <= p75v && p75v <= p95v && p95v <= p99v,
                "{p}: P50={p50v} <= P75={p75v} <= P95={p95v} <= P99={p99v} (per-provider bounds chain)"
            );
        }
        // 跨 4 個 provider 結果都 = 51/76/96/100 (因每個 provider 都餵 [1..100]
        // 同樣本集) — 這反而證明「同樣本集 emit 結果一致, K30/K31/K32/K33
        // 不會因為 provider 數量增加而破壞排序」
        let cicx_p75 = p75.get("cicx").copied().unwrap();
        let openx_p75 = p75.get("openx").copied().unwrap();
        assert_eq!(
            cicx_p75, openx_p75,
            "cicx 跟 openx 灌同樣本集 → P75 必等 (per-provider 隔離 + 同樣本 → 同結果)"
        );
    }

    // ============== R55：K34 P25 (下四分位 Q1) 跨樣本數 + 跨 4 provider 隔離 chain 護欄 (K27 ≤ P25 ≤ P50 ≤ P75 ≤ P95 ≤ P99 ≤ K26) ==============
    // R53/R54 護欄 (f5ca91b) 已涵蓋 K30/K31/K32/K33 (P50/P95/P99/P75) 4 件套 chain
    // + K22/K26/K27 lifetime aggregate chain。R55 落地 K34 P25 (第一四分位 Q1) 是
    // IQR 兩端之一 (P25 / P75), 自然延伸 chain: K27 min ≤ P25 ≤ P50 ≤ P75 ≤ P95
    // ≤ P99 ≤ K26 max 6 個不等式永久成立。R55 跟 R54 結構對齊: 跨 N ∈ {1, 2, 3,
    // 5, 10, 50, 100, 1023} 樣本數 + 跨 4 provider 灌 100 樣本, 斷言
    // K27 ≤ P25 ≤ P50 ≤ P75 ≤ P95 ≤ P99 ≤ K26 max 嚴格 monotonic chain (idx 0%
    // ≤ 25% ≤ 50% ≤ 75% ≤ 95% ≤ 99% ≤ 100% 排序後位置, sorted samples 單調非降
    // → 任意 percentile 必在 min 跟 max 之間, K34 P25 居於最下端 K27 跟 P50 之間)。
    // 若未來有人 (a) 改 P25 idx 公式 (例如 `len*30/100`), (b) 開新 reservoir 導致
    // P25 sample 集跟 P50/P75/P95/P99 不一致, (c) 改 sort 演算法導致 idx 偏移,
    // R55 護欄 CI 1 秒抓出。R54 護欄沒被 R55 觸碰 (test name + 8 種樣本數都保留),
    // 純新增 R55 護欄表達 K34 P25 居 monotonic chain 最低端 (R54 護 K30-K33 四件
    // 套 chain, R55 護 K27-K34-K31-K33-K30-K32 七件套 chain —— K34 介於 K27
    // min 跟 K31 P50 之間, 必須加 K27 ≤ P25 跟 P25 ≤ P50 兩條新不等式才完整覆
    // 蓋 K34 monotonic 數學)。

    #[test]
    fn r55_k27_k34_k31_k33_k30_k32_min_max_bounds_respected_across_eight_sample_sizes() {
        // property-style: 對 N ∈ {1, 2, 3, 5, 10, 50, 100, 1023} 各跑 1..=N
        // samples, 斷言 K34 P25 / K31 P50 / K33 P75 / K30 P95 / K32 P99 全部落在
        // [K27 min, K26 max] 區間內, 嚴格 monotonic chain K27 ≤ P25 ≤ P50 ≤
        // P75 ≤ P95 ≤ P99 ≤ K26 永遠成立 (idx 單調 + sorted samples 單調非降)。
        // K34 P25 居最下端 K27 跟 P50 之間, 8 種樣本數跨小樣本退化 (N=1 全
        // 部 = itself, N=4 P25=2 命中, N=2 P25=min 位置) 跟正常樣本 (N>=100
        // 各自 percentile 落在不同位置) 兩種語意都成立。
        let sizes = [1usize, 2, 3, 5, 10, 50, 100, 1023];
        for n in sizes {
            let mut m = SessionManager::new();
            for i in 1..=n as i64 {
                m.record_completed_session_age("cicx", i);
            }
            let totals = m
                .provider_totals
                .get("cicx")
                .expect("cicx entry should exist after N>=1 samples");
            let min_age = totals
                .min_completed_session_age_secs
                .expect("K27 min should be Some after N>=1 samples");
            let max_age = totals
                .max_completed_session_age_secs
                .expect("K26 max should be Some after N>=1 samples");
            let p25 = *completed_sessions_p25_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p50 = *completed_sessions_p50_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p75 = *completed_sessions_p75_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p95 = *completed_sessions_p95_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            let p99 = *completed_sessions_p99_at(&m.provider_totals)
                .get("cicx")
                .unwrap();
            // K27 min <= K34 P25 <= K31 P50 <= K33 P75 <= K30 P95 <= K32 P99 <= K26 max
            assert!(
                min_age <= p25,
                "N={n}: K27 min={min_age} 必須 <= P25={p25} (K34 派生自 K30 reservoir 排序後, 必 >= min)"
            );
            assert!(
                p25 <= p50,
                "N={n}: P25={p25} 必須 <= P50={p50} (K34 介於 K27 min 跟 K31 P50 之間, idx_p25 <= idx_p50 排序後單調)"
            );
            assert!(
                p50 <= p75,
                "N={n}: P50={p50} 必須 <= P75={p75} (K33 介於 K31 P50 跟 K30 P95 之間, idx_p50 <= idx_p75 排序後單調)"
            );
            assert!(
                p75 <= p95,
                "N={n}: P75={p75} 必須 <= P95={p95} (K33 介於 K31 P50 跟 K30 P95 之間, idx_p75 <= idx_p95 排序後單調)"
            );
            assert!(
                p95 <= p99,
                "N={n}: P95={p95} 必須 <= P99={p99} (idx_p95 <= idx_p99 排序後單調)"
            );
            assert!(
                p99 <= max_age,
                "N={n}: P99={p99} 必須 <= K26 max={max_age} (K32 派生自 K30 reservoir 排序後, 必 <= max)"
            );
        }
    }

    #[test]
    fn r55_k34_per_provider_isolation_under_oversubscribed_samples() {
        // 4 個 provider (cicx/claude/gemini/openx) 各自灌 100 個 completion 樣本
        // (總 400 樣本), 驗證 monotonic chain K27 ≤ P25 ≤ P50 ≤ P75 ≤ P95 ≤ P99
        // ≤ K26 max + per-provider 隔離 + 跨 4 provider 灌同樣本集結果互不污染。
        // 跟 R51 K30/K31/K32 isolation 護欄 (4 provider × 100 樣本)、R54 K30-K33
        // isolation 護欄 (4 provider × 100 樣本) 對稱, 跟 R53 K22/K26/K27 isolation
        // 護欄 (4 provider 各 100 樣本順序敏感) 互補, 四輪護欄共同覆蓋 K22-K34
        // lifetime + window aggregate + percentile chain 的 per-provider 隔離。
        let providers = ["cicx", "claude", "gemini", "openx"];
        let mut m = SessionManager::new();
        for p in providers {
            for i in 1..=100i64 {
                m.record_completed_session_age(p, i);
            }
        }
        let p25 = completed_sessions_p25_at(&m.provider_totals);
        let p50 = completed_sessions_p50_at(&m.provider_totals);
        let p75 = completed_sessions_p75_at(&m.provider_totals);
        let p95 = completed_sessions_p95_at(&m.provider_totals);
        let p99 = completed_sessions_p99_at(&m.provider_totals);
        for p in providers {
            // 100 樣本 [1..100] sort 後 idx_p25 = 100*25/100 = 25, idx_p50 = 50,
            // idx_p75 = 75, idx_p95 = 95, idx_p99 = 99
            let p25v = *p25.get(p).unwrap_or_else(|| panic!("{p} P25 missing"));
            let p50v = *p50.get(p).unwrap_or_else(|| panic!("{p} P50 missing"));
            let p75v = *p75.get(p).unwrap_or_else(|| panic!("{p} P75 missing"));
            let p95v = *p95.get(p).unwrap_or_else(|| panic!("{p} P95 missing"));
            let p99v = *p99.get(p).unwrap_or_else(|| panic!("{p} P99 missing"));
            assert_eq!(
                p25v, 26,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=25 → samples[25]=26"
            );
            assert_eq!(
                p50v, 51,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=50 → samples[50]=51"
            );
            assert_eq!(
                p75v, 76,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=75 → samples[75]=76"
            );
            assert_eq!(
                p95v, 96,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=95 → samples[95]=96"
            );
            assert_eq!(
                p99v, 100,
                "{p}: 100 樣本 [1..100] sort 後 [1..100], 0-indexed idx=99 → samples[99]=100 (= max)"
            );
            // K34 居最下端 K27 跟 P50 之間 + K34-K33-K30-K32 monotonic chain
            assert!(
                p25v <= p50v && p50v <= p75v && p75v <= p95v && p95v <= p99v,
                "{p}: P25={p25v} <= P50={p50v} <= P75={p75v} <= P95={p95v} <= P99={p99v} (per-provider bounds chain)"
            );
        }
        // 跨 4 個 provider 結果都 = 26/51/76/96/100 (因每個 provider 都餵 [1..100]
        // 同樣本集) — 這反而證明「同樣本集 emit 結果一致, K30/K31/K32/K33/K34
        // 不會因為 provider 數量增加而破壞排序」
        let cicx_p25 = p25.get("cicx").copied().unwrap();
        let openx_p25 = p25.get("openx").copied().unwrap();
        assert_eq!(
            cicx_p25, openx_p25,
            "cicx 跟 openx 灌同樣本集 → P25 必等 (per-provider 隔離 + 同樣本 → 同結果)"
        );
    }

    // ============== R57：K22 (latest age) ↔ K23 (count) + since freshness chain 護欄 + K35 interarrival helper correctness 護欄 ==============
    // R54/R55/R56 (K30-K33-K34 percentile chain) 補的是「單 metric 內部排序 monotonic」護欄,
    // R57 跨進 lifetime aggregate ↔ lifetime window 的算術不變式: 對於每個 provider
    //   - 該 provider 第一次被監控到的時間戳 (K10 `since`) 永遠 ≤ 當前 `now`
    //     (chrono `Duration` 自然 saturating, 我們 `.max(0)` clamp 負值)
    //   - 該 provider 累計完成次數 (K23 `completed_sessions_count`) 永遠 ≥ 0
    //   - 該 provider 最近一次完成的 age (K22 `last_completed_session_age_secs`) 必須
    //     落在 [0, now - since] 區間 (即「最近一次完成發生在 provider 第一次被監控到
    //     之後、且未來不可能發生」)
    //
    // 這是 trivial 但重要的「時間邏輯」invariant: 若有人 (a) 改 K22 從「覆寫成
    // latest」改成「saturating_max」跟 K26 混用, (b) 改 K23 counter 邏輯讓 count
    // 變成可遞減, (c) 改 `since` 寫入路徑讓「provider 第一次被監控到的時間戳」
    // 變得比 K22 寫入時間還新 (例如 bump_provider_totals 改成延遲寫入), 既有
    // K22/K23 單 metric 測試都抓不出, R57 護欄 CI 1 秒抓出。
    //
    // K35 helper correctness 護欄: K35 = `(now - since) / K23` 是純算術, 沒有
    // percentile 排序的退化語意, 但 (a) 整數除法 truncation 行為 (5/3 = 1), (b) saturating
    // clamp 負值到 0, (c) K23 == 0 跟 since == None 兩種過濾, 三個邊界必須驗證。
    // 跨 N ∈ {1, 10, 100, 1024} 灌 completion samples + 跨 lifetime = {1s, 100s, 1h, 1d}
    // 合成時間戳, 斷言 K35 = lifetime / N 嚴格相等, 跟 R54 K30 P95 outlier ratio
    // 護欄的「數學公式驗證」紀律對齊 (不是護排序位置, 是護算術正確性)。
    //
    // 維度: 4 種 sample 數 × 4 種 lifetime × 4 種 provider 隔離 = 4 護欄 test 函式
    // (R54 同款結構: 跨樣本數 1 + 跨 provider 隔離 1 + helper correctness 1 + monotonic
    // 端點 1)。

    #[test]
    fn r57_k22_latest_age_bounded_by_k10_since_lifetime_eight_sample_sizes() {
        // 跨 N ∈ {1, 2, 5, 10, 50, 100, 500, 1000} 各灌 N 個 completion samples,
        // 設定 since = now - 1h (固定 lifetime window), 斷言 K22 last_completed
        // 落在 [0, 3600] 區間內 (負值 / 超過 lifetime 都 fail)。K22 record_completed
        // 寫入的 secs 來自 fixture 灌入值, 不跟 since 互動, 但本護欄斷言「寫入
        // 的 K22 不可能比 lifetime 還大」(若 K22 > lifetime 表示 K22 在 since 之前
        // 完成 = 邏輯矛盾: 該 provider 還沒被監控到就完成 session)。小樣本 N=1
        // (K22=1, lifetime=3600) 跟大樣本 N=1000 (K22=1000) 兩種語意都成立。
        // 跟 R51 K30/K31/K32 bounds 護欄 (8 種樣本數) 對稱, 跟 R53 K22/K26/K27
        // bounds 護欄 (8 種樣本數) 互補。
        let sizes = [1usize, 2, 5, 10, 50, 100, 500, 1000];
        let now = Utc::now();
        let lifetime = Duration::hours(1);
        for n in sizes {
            let mut m = SessionManager::new();
            // 先建立 provider entry + 設定 since 為 now - 1h (lifetime window 固定 3600s)
            m.provider_totals
                .entry("cicx".to_string())
                .or_default()
                .since = Some(now - lifetime);
            // 灌 N 個 completion samples, secs = 1..=N
            for secs in 1..=n {
                m.record_completed_session_age("cicx", secs as i64);
            }
            let latest_map = last_completed_session_age_at(&m.provider_totals);
            let latest = *latest_map.get("cicx").expect("K22 must emit after N>=1");
            // K22 必須落在 [0, lifetime_secs=3600] 區間
            assert!(
                latest >= 0,
                "N={n}: K22 latest ({latest}) 必須 >= 0 (saturating clamp 負值)"
            );
            assert!(
                latest <= lifetime.num_seconds(),
                "N={n}: K22 latest ({latest}) 必須 <= lifetime ({}s) (K22 是「最近完成」, 不可能比 provider 第一次被監控到還早)",
                lifetime.num_seconds()
            );
            // K35 helper correctness: K35 = lifetime / K23 (整數除法)
            let interarrival = completed_sessions_interarrival_at(&m.provider_totals, now);
            let k35 = *interarrival
                .get("cicx")
                .expect("K35 must emit when K23>=1 AND since=Some");
            assert_eq!(
                k35,
                lifetime.num_seconds() / n as i64,
                "N={n}: K35 interarrival 必須 = lifetime ({}s) / N ({}) = {} (整數除法 truncation)",
                lifetime.num_seconds(),
                n,
                lifetime.num_seconds() / n as i64
            );
        }
    }

    #[test]
    fn r57_k22_freshness_per_provider_isolation_under_oversubscribed_completions() {
        // 4 個 provider (cicx/claude/gemini/openx) 各自設定不同的 lifetime window
        // (1h / 2h / 6h / 12h) + 各自灌 100 個 completion 樣本 (每個 provider 的
        // samples = 1..=100), 驗證 (a) K22 freshness chain (K22 ≤ lifetime) 4 個
        // provider 各自成立, (b) K35 interarrival = lifetime / 100 4 個 provider
        // 各自正確, (c) 4 個 provider emit 結果互不污染。跟 R51/R54/R55 isolation
        // 護欄 (4 provider × 100 樣本) 對稱。
        let now = Utc::now();
        let lifetimes = [
            ("cicx", Duration::hours(1)),
            ("claude", Duration::hours(2)),
            ("gemini", Duration::hours(6)),
            ("openx", Duration::hours(12)),
        ];
        let mut m = SessionManager::new();
        for (p, lt) in &lifetimes {
            m.provider_totals.entry(p.to_string()).or_default().since = Some(now - *lt);
            for secs in 1..=100i64 {
                m.record_completed_session_age(p, secs);
            }
        }
        let latest_map = last_completed_session_age_at(&m.provider_totals);
        let interarrival_map = completed_sessions_interarrival_at(&m.provider_totals, now);
        for (p, lt) in &lifetimes {
            let latest = *latest_map
                .get(*p)
                .unwrap_or_else(|| panic!("{p}: K22 必須 emit"));
            assert!(
                latest >= 0 && latest <= lt.num_seconds(),
                "{p}: K22 latest ({latest}) 必須 ∈ [0, {}] (per-provider lifetime 隔離)",
                lt.num_seconds()
            );
            let k35 = *interarrival_map
                .get(*p)
                .unwrap_or_else(|| panic!("{p}: K35 必須 emit"));
            assert_eq!(
                k35,
                lt.num_seconds() / 100,
                "{p}: K35 = {} / 100 = {} (per-provider lifetime 隔離)",
                lt.num_seconds(),
                lt.num_seconds() / 100
            );
        }
        // 跨 4 個 provider K35 都不同 (lifetime 1h/2h/6h/12h → K35 36/72/216/432)
        // 證明 per-provider 隔離 + lifetime 輸入各自獨立
        let cicx_k35 = interarrival_map["cicx"];
        let openx_k35 = interarrival_map["openx"];
        assert_ne!(
            cicx_k35, openx_k35,
            "cicx (lifetime 1h → K35 36) vs openx (lifetime 12h → K35 432) 必須不同 (lifetime 輸入隔離)"
        );
    }

    #[test]
    fn r57_k35_helper_filters_providers_with_zero_count_or_no_since() {
        // 邊界: K23 == 0 (沒完成過) 跟 since == None (bump_provider_totals 從未觸發)
        // 兩種情況下 K35 不應 emit sample (避免 0/0 假冒「瞬間完成」誤導 Prometheus
        // 端把「沒資料」當「provider 吞吐無限」= 假健康信號)。灌 4 個 provider fixture:
        // cicx (K23=10, since=Some 1h ago) → emit K35 = 360
        // claude (K23=0, since=Some 1d ago) → skip (K23=0 過濾)
        // gemini (K23=5, since=None) → skip (since 過濾)
        // openx (K23=0, since=None) → skip (兩個都缺, double filter)
        let now = Utc::now();
        let mut m = SessionManager::new();
        // cicx: 完整資料
        m.provider_totals
            .entry("cicx".to_string())
            .or_default()
            .since = Some(now - Duration::hours(1));
        for secs in 1..=10i64 {
            m.record_completed_session_age("cicx", secs);
        }
        // claude: 只有 since, 沒 completion
        m.provider_totals
            .entry("claude".to_string())
            .or_default()
            .since = Some(now - Duration::days(1));
        // gemini: 只有 completion, 沒 since (用 handle_event 觸發 bump_provider_totals
        // 自動設 since 是不行的, 必須手動清空 since 模擬「bump_provider_totals 從未
        // 觸發」邊界)
        let _ = m.provider_totals.entry("gemini".to_string()).or_default();
        // 手動設定 5 個 completion (走 record_completed_session_age helper) 但 since 留 None
        for secs in 1..=5i64 {
            // 先寫入, 然後把 since 清掉 (record_completed_session_age 不動 since)
            m.record_completed_session_age("gemini", secs);
        }
        // 此時 gemini entry 已有 K23=5, K22=5, 但 since=None
        m.provider_totals.get_mut("gemini").unwrap().since = None;
        // openx: 兩個都缺 (從未 touch)
        let _ = m.provider_totals.entry("openx".to_string()).or_default();
        let interarrival = completed_sessions_interarrival_at(&m.provider_totals, now);
        assert_eq!(
            interarrival.get("cicx"),
            Some(&360i64),
            "cicx: K35 = 3600/10 = 360"
        );
        assert!(
            !interarrival.contains_key("claude"),
            "claude: K23=0 → 必須過濾 (避免 0/0 假冒)"
        );
        assert!(
            !interarrival.contains_key("gemini"),
            "gemini: since=None → 必須過濾 (避免 0/0 假冒)"
        );
        assert!(
            !interarrival.contains_key("openx"),
            "openx: K23=0 AND since=None → 必須過濾 (double filter 邊界)"
        );
        // 確認 emit set 只有 cicx
        assert_eq!(
            interarrival.len(),
            1,
            "只有 cicx 進 emit set, 其他 3 個被過濾"
        );
    }

    #[test]
    fn r57_k35_negative_lifetime_saturates_to_zero() {
        // 邊界: 當 since 寫成比 now 還晚的時間戳 (模擬時鐘回撥 / 序列化時差 / 測試 fixture
        // bug), K35 必須 saturating clamp 到 0 而不是負值 (負值會被 Prometheus 端誤判
        // 「未來完成」= 邏輯炸裂)。Fixture: since = now + 1h (未來 1 小時), 灌 5 個
        // completion samples, 斷言 K35 = 0 (saturating clamp, 不是 -720 / 5 = -144)。
        let now = Utc::now();
        let mut m = SessionManager::new();
        m.provider_totals
            .entry("cicx".to_string())
            .or_default()
            .since = Some(now + Duration::hours(1));
        for secs in 1..=5i64 {
            m.record_completed_session_age("cicx", secs);
        }
        let interarrival = completed_sessions_interarrival_at(&m.provider_totals, now);
        let k35 = *interarrival
            .get("cicx")
            .expect("K35 必須 emit (K23>=1, since=Some 都滿足)");
        assert_eq!(
            k35, 0,
            "since 比 now 還晚 → (now - since) = -3600s → saturating clamp 到 0 → K35 = 0 / 5 = 0 (saturating 邊界, 不能 emit 負值)"
        );
    }

    // ============== R53：K22 (latest age) / K26 (max duration) / K27 (min duration) lifetime aggregate bounds 護欄 ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結新增 gauge 一週」紀律延伸 — R51 補 K30/K31/K32 (P50/P95/P99)
    // bounds, R52 補 K23/K24/K25 (count/total/avg) cross-metric 算術, R53 補 K22/K26/K27
    // (latest age / max duration / min duration) lifetime aggregate 三件套 monotonic chain:
    //   K27 min_duration_secs ≤ K22 latest_age_secs ≤ K26 max_duration_secs
    //
    // 這是 trivial 數學事實 (lifetime saturating_min ≤ 任意單次記錄 ≤ lifetime saturating_max),
    // 但現有 K22/K26/K27 既有 14 個 test 全是「單 metric 隔離」,沒人驗證三件套在同一個
    // ProviderTotals 同時被寫入時 monotonic chain 是否仍成立 — 若有人未來改 K22 從
    // 「覆寫成 latest」改成「saturating_max」混進 K26 邏輯, 或 K27 從 saturating_min 改成
    // 「第一次寫入後凍結」漏更新, 現有 test 抓不出, 要到 production Prometheus 端
    // 觀察到 K22 > K26 才被動發現。R53 補這層 cross-metric monotonic 護欄,
    // 跟 R51/R52 同樣紀律, CI 1 秒抓出。
    //
    // 維度：K22 是 Option<i64> latest age (最近一次完成距今多久), K26/K27 是
    // Option<i64> lifetime min/max duration (歷史最快/最慢完成 session 持續秒數)。
    // 三件套都過濾 None (該 provider 沒完成過 session → gauge 缺資料, 跳過不 emit)。
    // 注意：K22 age 跟 K26/K27 duration 在語意上是「兩個獨立時鐘」—— K22 age 是
    // 「離現在多久」(monotonic 遞增除非有新完成 session 把它壓回小值), K26/K27
    // duration 是「單次 session 跑了多久」(saturating 寫入後不變)。護欄斷言的是
    // 「同一個 provider 在同一個時間快照下, K22 latest age 必須落在
    // K27 min 跟 K26 max 之間」 — 這對「這次 session 持續時間 120s, 距今 5s
    // 前完成」= K22=5 / K26≥120 / K27≤120 → K22=5 < K27=120 (破壞! 等等)」
    // 的場景會 fail。
    //
    // 修正語意: K22 age 跟 K26/K27 duration 不能直接 monotonic 比較 — K22 是
    // 「時間戳距今」, K26/K27 是「session 跑了多久」, 兩者不互相約束 (一個
    // 5 分鐘前完成的 30 分鐘 session, K22=300, K26≥1800, K27≤1800, K22 < K27
    // 完全合法)。 R53 護欄必須用「K22 age 是其中一次 completion 的 duration」
    // 的 fixture 才能讓 invariant 成立: 灌 N 個 completion samples 給 K26/K27
    // (saturating_min/max 抓全部樣本), 然後 K22 latest = 最後一次 completion
    // 的 duration (因為 K22 record_completed_session_age(secs) 寫入 last_completed_session_age_secs
    // 直接 = 該次 secs, 然後生命週期 stale 回收時 K22 不變, 永遠是「最後一次
    // 完成時的持續秒數」), 然後 monotonic chain 變成 K27 min ≤ K22 latest (= last
    // completion duration) ≤ K26 max 成立。護欄設計: 對同一個 provider 灌 N
    // 個 completion 樣本, 每次 sample = 1..=N, 然後驗 K27=1, K22=N, K26=N。
    // K22/K26/K27 fn 已在測試 submodule 開頭 use 過 (line 1374 / 1801 / 1924),
    // 這裡直接呼叫即可, 避免 reimport E0252。

    #[test]
    fn r53_k22_k26_k27_lifetime_bounds_respected_across_eight_sample_sizes() {
        // 跨 N ∈ {1, 2, 3, 5, 10, 50, 100, 200} 各自灌 samples 1..=N,
        // 斷言 K27 min ≤ K22 latest ≤ K26 max monotonic chain 整條成立。
        // 涵蓋小樣本 (N=1 → K27=1, K22=1, K26=1 三件套退化相等) 跟大樣本
        // (N=200 → K27=1, K22=200, K26=200 三件套各自落在不同位置) 兩種語意。
        // 跟 R51 K30/K31/K32 bounds 護欄 (8 種樣本數) 對稱 + 跟 R52
        // K23/K24/K25 (5 種樣本數) 互補, 共同覆蓋 K22-K32 既有 K-tag。
        for n in [1usize, 2, 3, 5, 10, 50, 100, 200] {
            let mut m = SessionManager::new();
            // 灌 N 個 completion samples, 每次 secs = 1..=N
            for secs in 1..=n {
                m.record_completed_session_age("cicx", secs as i64);
            }
            // 拉 K22 / K26 / K27 lifetime aggregate
            let latest_map = last_completed_session_age_at(&m.provider_totals);
            let max_map = completed_sessions_max_duration_at(&m.provider_totals);
            let min_map = completed_sessions_min_duration_at(&m.provider_totals);
            // 三件套都必須 emit (灌過 completion 不會 None)
            assert!(
                latest_map.contains_key("cicx"),
                "N={n}: K22 必須 emit (灌過 completion 不會 None)"
            );
            assert!(
                max_map.contains_key("cicx"),
                "N={n}: K26 必須 emit (灌過 completion 不會 None)"
            );
            assert!(
                min_map.contains_key("cicx"),
                "N={n}: K27 必須 emit (灌過 completion 不會 None)"
            );
            let latest = latest_map["cicx"];
            let max = max_map["cicx"];
            let min = min_map["cicx"];
            // K27 min 必須 = 1 (samples 1..=N 的 min)
            assert_eq!(
                min, 1,
                "N={n}: K27 min 必須 = 1 (saturating_min 抓 samples 1..=N 的 min)"
            );
            // K26 max 必須 = N (samples 1..=N 的 max)
            assert_eq!(
                max, n as i64,
                "N={n}: K26 max 必須 = {n} (saturating_max 抓 samples 1..=N 的 max)"
            );
            // K22 latest 必須 = N (最後一次 record_completed_session_age 寫入的 secs)
            assert_eq!(
                latest, n as i64,
                "N={n}: K22 latest 必須 = {n} (覆寫成最後一次 completion 的 secs)"
            );
            // Monotonic chain: K27 min ≤ K22 latest ≤ K26 max
            assert!(
                min <= latest,
                "N={n}: K27 min ({min}) 必須 ≤ K22 latest ({latest})"
            );
            assert!(
                latest <= max,
                "N={n}: K22 latest ({latest}) 必須 ≤ K26 max ({max})"
            );
        }
    }

    #[test]
    fn r53_k22_k26_k27_per_provider_isolation_under_oversubscribed_completions() {
        // 4 個 provider (cicx/claude/gemini/openx) 各自灌 100 個 completion 樣本
        // (總 400 樣本), 驗證 monotonic chain + per-provider 隔離 + 跨 4 provider
        // 灌不同樣本集結果互不污染。
        // 跟 R51 K30/K31/K32 isolation 護欄 (4 provider × 100 樣本) 對稱, 跟
        // R52 K23/K24/K25 isolation (4 provider count 混合) 互補, 三輪護欄共同
        // 覆蓋 K22-K32 lifetime + window aggregate 的 per-provider 隔離。
        let mut m = SessionManager::new();
        // cicx: 灌 1..=100 (K27=1, K22=100, K26=100)
        for secs in 1..=100 {
            m.record_completed_session_age("cicx", secs);
        }
        // claude: 灌 1..=100 但顛倒順序 (K27=1, K22=100, K26=100, 但驗證
        // saturating_min/max 不依賴輸入順序)
        for secs in (1..=100).rev() {
            m.record_completed_session_age("claude", secs);
        }
        // gemini: 灌同一個值 50 全部 (K27=50, K22=50, K26=50, 三件套退化)
        for _ in 0..100 {
            m.record_completed_session_age("gemini", 50);
        }
        // openx: 只灌 1 個樣本 (K27=K22=K26=42, 退化語意)
        m.record_completed_session_age("openx", 42);
        // 拉三件套
        let latest_map = last_completed_session_age_at(&m.provider_totals);
        let max_map = completed_sessions_max_duration_at(&m.provider_totals);
        let min_map = completed_sessions_min_duration_at(&m.provider_totals);
        // 4 個 provider 都必須 emit
        for p in ["cicx", "claude", "gemini", "openx"] {
            assert!(
                latest_map.contains_key(p),
                "{p}: K22 必須 emit (4 provider 都有 completion)"
            );
            assert!(
                max_map.contains_key(p),
                "{p}: K26 必須 emit (4 provider 都有 completion)"
            );
            assert!(
                min_map.contains_key(p),
                "{p}: K27 必須 emit (4 provider 都有 completion)"
            );
        }
        // cicx: K27=1, K22=100, K26=100 (順序灌, 最後一次 = max)
        assert_eq!(min_map["cicx"], 1, "cicx K27 = 1 (saturating_min)");
        assert_eq!(latest_map["cicx"], 100, "cicx K22 = 100 (最後一次)");
        assert_eq!(max_map["cicx"], 100, "cicx K26 = 100 (saturating_max)");
        // claude: 顛倒灌 → K22 latest = 1 (覆寫成「最後一次 record」的 secs,
        // 顛倒時最後一次 = 1), K27=1, K26=100 (saturating 抓 max, 不受順序影響)
        assert_eq!(min_map["claude"], 1, "claude K27 = 1 (顛倒也抓到 1)");
        assert_eq!(
            latest_map["claude"], 1,
            "claude K22 = 1 (覆寫成最後一次 = 1)"
        );
        assert_eq!(max_map["claude"], 100, "claude K26 = 100 (顛倒也抓到 100)");
        // gemini: 全部 50 → 三件套退化
        assert_eq!(min_map["gemini"], 50, "gemini K27 = 50 (全 50)");
        assert_eq!(latest_map["gemini"], 50, "gemini K22 = 50 (全 50)");
        assert_eq!(max_map["gemini"], 50, "gemini K26 = 50 (全 50)");
        // openx: 只 1 樣本 → 三件套退化
        assert_eq!(min_map["openx"], 42, "openx K27 = 42 (單樣本)");
        assert_eq!(latest_map["openx"], 42, "openx K22 = 42 (單樣本)");
        assert_eq!(max_map["openx"], 42, "openx K26 = 42 (單樣本)");
        // Monotonic chain 對 4 provider 都成立
        for p in ["cicx", "claude", "gemini", "openx"] {
            let latest = latest_map[p];
            let max = max_map[p];
            let min = min_map[p];
            assert!(
                min <= latest,
                "{p}: K27 min ({min}) 必須 ≤ K22 latest ({latest})"
            );
            assert!(
                latest <= max,
                "{p}: K22 latest ({latest}) 必須 ≤ K26 max ({max})"
            );
        }
        // 跨 4 provider 互不污染: cicx 跟 claude 都灌 1..=100 應該 saturating
        // min/max 一致 (K27=1, K26=100), 但 K22 latest 因「覆寫成最後一次」
        // 順序敏感所以 cicx=100 / claude=1 — 這是 K22 既有語意, 護欄要驗證
        // 「順序不污染 K26/K27 saturating」而不是「K22 必須一致」。
        assert_eq!(
            min_map["cicx"], min_map["claude"],
            "cicx 跟 claude 都灌 1..=100 (順序顛倒), K27 saturating_min 必須一致"
        );
        assert_eq!(
            max_map["cicx"], max_map["claude"],
            "cicx 跟 claude 都灌 1..=100, K26 saturating_max 必須一致"
        );
        // K22 latest 順序敏感, 必須不一致 (cicx 最後 = 100, claude 最後 = 1)
        assert_ne!(
            latest_map["cicx"], latest_map["claude"],
            "cicx 跟 claude 順序顛倒, K22 latest 必須不一致 (= 100 vs 1), \
             證明 K22 跟 K26/K27 是不同時鐘"
        );
    }

    // ============== R54：K30 P95 (sliding window percentile) / K25 avg (lifetime arithmetic mean) 跨窗口 outlier ratio 護欄 ==============
    // 策略顧問 R50 巡邏「DRIFTING + 凍結新增 gauge 一週」紀律延伸 — R51 補 K30/K31/K32
    // (P50/P95/P99) bounds, R52 補 K23/K24/K25 (count/total/avg) cross-metric 算術, R53 補
    // K22/K26/K27 (latest age / max duration / min duration) lifetime aggregate monotonic
    // chain, R54 補 K30 (sliding window P95) / K25 (lifetime arithmetic mean) 跨窗口 outlier
    // ratio 算術護欄: 對均勻分布 (e.g. [1..=N]), K30 P95 / K25 avg 比例有界; 對 outlier
    // 分布, 比例可顯著 > 5 (operator alert 閾值)。這是 R50 策略顧問講的 P95/avg outlier
    // 維度落地。
    //
    // 為什麼是跨窗口 (cross-window) 而不是同窗口: K30 P95 從
    // `completed_sessions_p95_samples` reservoir 1024 (sliding window) 算, K25 avg 從
    // K23 count + K24 total_duration (lifetime counters) 算 — 兩者資料源不同。對小樣本
    // (K25 count ≤ 1024), K30 reservoir 沒 saturated, K30 samples = K25 全部 lifetime
    // completion, 兩者從同一組樣本算; 對大樣本 (K25 count > 1024), K30 reservoir 是
    // saturated, K30 反映「最近 1024 次」, K25 avg 反映「全部 lifetime」。R54 護欄聚焦
    // 小樣本語意: 對 N ∈ {1, 2, 3, 5, 10, 50, 100, 200} 灌 [1..=N] 均勻, 驗 outlier
    // ratio = P95 / avg ≤ 2.0 (uniform-like distribution 比例有界, 證明 K30 跟 K25 在
    // 同樣本下 emit 結果一致 = 兩者從同一份 completion samples 派生時, sliding window
    // percentile 跟 lifetime arithmetic mean 算術語意對齊)。
    //
    // 數學: 對 N 樣本 [1..=N] 均勻, K30 P95 = N (sort 後 idx = N*95/100 對齊 N, 跟
    // K32 doc 推導一致), K25 avg = (1+2+...+N) / N = (N+1)/2, ratio = N / ((N+1)/2)
    // = 2N/(N+1)。N=1 → 1.0; N=2 → 1.33; N=3 → 1.5; N=5 → 1.67; N=10 → 1.82;
    // N=50 → 1.96; N=100 → 1.98; N=200 → 1.99。單調遞增, 上限 = 2.0 (N→∞)。對
    // outlier 分布 (e.g. [1, 1, 1, 1, 1000] 5 樣本), K30 P95 = 1000 (idx=4), K25 avg
    // = 200.8, ratio = 4.98 (剛好接近 alert 邊界); 對極端 outlier (e.g. [1, 1, 1, 1, 1, 1000]
    // 6 樣本), K30 P95 = 1000 (idx=5), K25 avg = 167.5, ratio = 5.97 (outlier 拉爆 alert
    // 閾值)。
    //
    // Operator alert: `lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider="X"}
    // / lobsterpulse_provider_completed_sessions_average_duration_seconds{provider="X"}` 比例
    // > 5 = 該 provider 中段偏慢任務拉高分布 (K25 avg 反映平均, K30 P95 反映尾端, 比例
    // 大 = 「平均 < 尾端 / 5」 = 分布嚴重右偏 = 有 outlier 卡住整體)。這條 alert 抓不到
    // K25 單獨 alert (K25 avg 仍 < 100s OK 看起來健康) 也抓不到 K30 單獨 alert (K30 P95
    // 仍 < 300s OK 看起來健康) 的「中等 outlier」場景。
    //
    // 跟 R51 K30/K31/K32 bounds + R52 K23/K24/K25 cross-metric + R53 K22/K26/K27
    // lifetime chain 同模板, 跨 8 種樣本數 + 4-provider 隔離強化, CI 1 秒抓出。
    // K25 純 fn `completed_sessions_average_duration_at` 跟 K30 純 fn
    // `completed_sessions_p95_at` 都在子模組 use super::{...} 開頭 (line 1287) 跟
    // line 1753 import 過, 這裡直接呼叫不 reimport。

    #[test]
    fn r54_k30_p95_to_k25_avg_outlier_ratio_uniform_distribution_below_two() {
        // 跨 N ∈ {1, 2, 3, 5, 10, 50, 100, 200} 灌 [1..=N] 均勻樣本, 驗 K30
        // P95 / K25 avg ≤ 2.0 (均勻分布 ratio 有界, 數學上限 N→∞ = 2.0)。
        // 涵蓋小樣本退化 (N=1 ratio=1, N=2 ratio=1.33) 跟大樣本逼近上限
        // (N=200 ratio=1.99) 兩種語意。K25 avg 在 N >= 1 永遠 emit (count > 0
        // 過濾), K30 P95 在 N >= 1 也永遠 emit (samples 非空)。f64 算術:
        // K30 P95 / K25 avg 必須 cast f64 比較避免整數除法。
        for n in [1usize, 2, 3, 5, 10, 50, 100, 200] {
            let mut m = SessionManager::new();
            for secs in 1..=n {
                m.record_completed_session_age("cicx", secs as i64);
            }
            // K25 avg (f64 lifetime arithmetic mean)
            let avg_map = completed_sessions_average_duration_at(&m.provider_totals);
            let avg = *avg_map
                .get("cicx")
                .unwrap_or_else(|| panic!("N={n}: K25 avg 必須 emit, count={n} > 0"));
            // K30 P95 (i64 sliding window P95)
            let p95_map = completed_sessions_p95_at(&m.provider_totals);
            let p95 = *p95_map
                .get("cicx")
                .unwrap_or_else(|| panic!("N={n}: K30 P95 必須 emit, samples 非空"));
            // Outlier ratio = P95 / avg (f64 算術)
            let ratio = p95 as f64 / avg;
            // 對 [1..=N] 均勻, 數學上限 = 2.0 (N→∞); 實際 N 樣本 ratio =
            // 2N/(N+1) ≤ 2.0 對 N >= 1 永遠成立
            assert!(
                ratio <= 2.0,
                "N={n}: K30 P95={p95} / K25 avg={avg} = {ratio:.4}, 必須 ≤ 2.0 (均勻分布 outlier ratio 上限)"
            );
            // 對 [1..=N] 均勻, ratio 也必須 >= 1.0 (P95 永不小於 avg for 線性均勻,
            // 因為 P95 接近 max, avg = (N+1)/2 是中段, max 永遠 >= 中段)
            assert!(
                ratio >= 1.0,
                "N={n}: K30 P95={p95} / K25 avg={avg} = {ratio:.4}, 必須 >= 1.0 (P95 >= avg 對單調分布)"
            );
        }
    }

    #[test]
    fn r54_k30_p95_to_k25_avg_outlier_ratio_extreme_outlier_detected() {
        // 構造 outlier fixture 驗 ratio > 5 alert 觸發條件成立, 證明 outlier
        // ratio 對 outlier 真的有 signal (不是 trivial 永遠 ≤ 2.0)。fixture:
        // 6 樣本 [1, 1, 1, 1, 1, 1000] → K30 P95 = 1000 (idx = 6*95/100 = 5),
        // K25 avg = (1+1+1+1+1+1000)/6 = 1005/6 = 167.5, ratio = 5.97 > 5 (alert 觸發)。
        // 跟 r54_..._uniform_distribution_below_two 互補: 護欄覆蓋「均勻
        // 分布 ratio 有界」+「outlier 分布 ratio 觸發 alert」兩個語意面。
        let mut m = SessionManager::new();
        let outlier_samples = [1i64, 1, 1, 1, 1, 1000];
        for s in outlier_samples {
            m.record_completed_session_age("cicx", s);
        }
        let avg = *completed_sessions_average_duration_at(&m.provider_totals)
            .get("cicx")
            .expect("K25 avg 必須 emit, count=6 > 0");
        let p95 = *completed_sessions_p95_at(&m.provider_totals)
            .get("cicx")
            .expect("K30 P95 必須 emit, samples 非空");
        // Sanity: K25 avg = 1005/6 = 167.5 (算術 sanity: 1+1+1+1+1+1000 = 1005)
        assert!(
            (avg - 1005.0 / 6.0).abs() < 1e-9,
            "K25 avg 必須 = 1005/6 = 167.5 (算術 sanity), got {avg}"
        );
        // Sanity: K30 P95 = 1000 (idx=5, samples 排序後 [1, 1, 1, 1, 1, 1000] idx=5 = 1000)
        assert_eq!(p95, 1000, "K30 P95 必須 = 1000 (極端 outlier, idx=5)");
        // Outlier ratio: 1000 / 167.5 ≈ 5.97 > 5 (alert 閾值觸發)
        let ratio = p95 as f64 / avg;
        assert!(
            ratio > 5.0,
            "K30 P95={p95} / K25 avg={avg} = {ratio:.4}, 必須 > 5.0 (outlier 拉爆 alert 閾值), \
             證明 outlier ratio 對 outlier 真的有 signal"
        );
    }

    #[test]
    fn r54_k30_p95_to_k25_avg_outlier_ratio_per_provider_isolation() {
        // 4 provider (cicx/claude/gemini/openx) 各自灌 100 個 completion 樣本
        // [1..=100] 同樣本集, 驗 K30 P95 / K25 avg outlier ratio per-provider
        // 隔離 + 跨 4 provider 各自 ratio 一致 (因每個 provider 都餵同樣本集
        // → 數學結果必須一致)。補 R51 K30/K31/K32 isolation 護欄只測三件套
        // percentile 的不足: 若有人未來在 K30 P95 或 K25 avg fn 裡抓外部
        // 變數 (closure capture bug), 4 provider 隔離立刻抓出。K30
        // reservoir 是 1024 capacity, 100 樣本 < 1024, 沒 saturated, K30
        // P95 = 96 (samples [1..100] sort 後 [1..100] idx=95 = 96), K25
        // avg = 50.5, ratio = 96/50.5 ≈ 1.90, 必須 < 2.0。
        let providers = ["cicx", "claude", "gemini", "openx"];
        let mut m = SessionManager::new();
        for p in providers {
            for i in 1..=100i64 {
                m.record_completed_session_age(p, i);
            }
        }
        let avg_map = completed_sessions_average_duration_at(&m.provider_totals);
        let p95_map = completed_sessions_p95_at(&m.provider_totals);
        let mut ratios = Vec::new();
        for p in providers {
            let avg = *avg_map
                .get(p)
                .unwrap_or_else(|| panic!("{p}: K25 avg 必須 emit, count=100 > 0"));
            let p95 = *p95_map
                .get(p)
                .unwrap_or_else(|| panic!("{p}: K30 P95 必須 emit"));
            // Sanity: K25 avg = 50.5 (arithmetic mean of [1..100])
            assert!(
                (avg - 50.5).abs() < 1e-9,
                "{p}: K25 avg 必須 = 50.5, got {avg}"
            );
            // Sanity: K30 P95 = 96 (samples [1..100] sort 後 [1..100] idx=95 = 96)
            assert_eq!(
                p95, 96,
                "{p}: K30 P95 必須 = 96 (100 樣本 idx=95), got {p95}"
            );
            let ratio = p95 as f64 / avg;
            // 對 [1..=100] 均勻, 100 樣本 ratio = 96/50.5 ≈ 1.90, 必須 < 2.0
            assert!(
                ratio < 2.0,
                "{p}: K30 P95/K25 avg = {ratio:.4}, 必須 < 2.0 (均勻分布 ratio 上限)"
            );
            ratios.push(ratio);
        }
        // 跨 4 provider 灌同樣本集 → ratio 必須一致 (per-provider 隔離)
        let cicx_ratio = ratios[0];
        for (i, p) in providers.iter().enumerate() {
            assert!(
                (ratios[i] - cicx_ratio).abs() < 1e-9,
                "{p} outlier ratio = {:.4} 必須 = cicx ratio = {:.4} (per-provider 隔離)",
                ratios[i],
                cicx_ratio
            );
        }
    }

    // ─── R115 規則引擎護衛 test（lobster-rules-engine T-18~T-20）─────
    // T-20 (r115_rule_when_filter) 已在 config.rs r115_rule_engine_config_tests mod
    // 這邊補 T-18 (match count) + T-19 (action emission)。

    /// R115 護衛 T-18：13 種典型事件流過後, 3 預設規則各至少 1 次匹配。
    /// 對齊 spec R-2 (RuleWhen::matches) + design.md 護衛 test #1。
    #[test]
    fn r115_rule_evaluation_match_count() {
        use crate::config::default_rules;
        let mut m = SessionManager::new();
        // 把預設 3 條灌進 SessionManager（new() 已自動灌, 顯式呼叫防 future 改動）
        m.set_rules(default_rules(), true);
        m.rule_match_count = 0;
        m.rule_firings.clear();

        // 13 個 provider × 3 種事件 = 39 條 event 灌進 evaluate_rules
        let providers = [
            "claude",
            "codex",
            "gemini",
            "copilot",
            "cicx",
            "gitx",
            "giminix",
            "codex_bot",
            "openx",
            "irisx_bot",
            "grokx",
            "lpbot",
            "mimo",
        ];
        let mut total = 0u64;
        for p in &providers {
            // 預設 1: claude Stop → Completed 命中 (只 claude 命中, 其他不計)
            let _ = m.handle_event(&ev(p, &format!("{p}-a"), "Stop"));
            total += 1;
            // 預設 2: 任何 provider 進入 WaitingForUser 命中
            // 用 Notification 觸發 WaitingForUser 轉移
            let _ = m.handle_event(&ev(p, &format!("{p}-b"), "Notification"));
            total += 1;
            // 預設 3: 任何 provider PostToolUseFailure 命中
            let _ = m.handle_event(&ev(p, &format!("{p}-c"), "PostToolUseFailure"));
            total += 1;
        }
        // 3 預設規則各至少 1 次匹配
        assert!(
            m.rule_match_count >= 3,
            "3 預設規則各至少 1 次匹配, 實際 rule_match_count = {} (灌 {} 條 event)",
            m.rule_match_count,
            total
        );
        // rule_firings 累積區也不應為空（drain 給 caller emit 用）
        assert!(
            !m.rule_firings.is_empty(),
            "rule_firings 必須非空, 給 lib.rs hook_server drain emit `rule-fired`"
        );
    }

    /// R115 護衛 T-19：3 種 action 類型 (Toast / Sound / Log) 都要有 emit。
    /// 對齊 spec R-2 + design.md 護衛 test #2。
    #[test]
    fn r115_rule_action_emission() {
        use crate::config::default_rules;
        let mut m = SessionManager::new();
        m.set_rules(default_rules(), true);
        m.rule_match_count = 0;
        m.rule_firings.clear();

        // 預設 1: claude UserPromptSubmit → Working → Stop → Idle → Completed → Toast + Sound
        let _ = m.handle_event(&ev("claude", "c1", "UserPromptSubmit"));
        let _ = m.handle_event(&ev("claude", "c1", "Stop"));
        // 預設 2: 任何 provider Notification → StartedWaiting → Toast
        let _ = m.handle_event(&ev("codex", "c2", "Notification"));
        // 預設 3: 任何 provider PostToolUseFailure → Log
        let _ = m.handle_event(&ev("gemini", "c3", "PostToolUseFailure"));

        let mut toast = 0usize;
        let mut sound = 0usize;
        let mut log = 0usize;
        for f in &m.rule_firings {
            if matches!(f.action, crate::config::RuleAction::Toast { .. }) {
                toast += 1;
            }
            if matches!(f.action, crate::config::RuleAction::Sound { .. }) {
                sound += 1;
            }
            if matches!(f.action, crate::config::RuleAction::Log { .. }) {
                log += 1;
            }
        }
        assert!(toast >= 2, "toast action ≥ 2 (預設 1 + 2), 實際 = {toast}");
        assert!(sound >= 1, "sound action ≥ 1 (預設 1), 實際 = {sound}");
        assert!(log >= 1, "log action ≥ 1 (預設 3), 實際 = {log}");
    }

    /// R210 護衛：check_staleness 必須把「無事件超過 idle 閾值」的
    /// `WaitingForUser` session 跟 `Working` 一樣降級成 Idle。否則
    /// Notification/PermissionRequest 後使用者不回話的 session 會永遠
    /// 留在 active set，`is_active()` 一直 true，`active_count` 跟
    /// `active_providers()` 一直算到死掉的 session，UI 顯示「agent 還在等」
    /// 永遠不會自動熄滅。`is_active()` 已經是 `Working | WaitingForUser`
    /// 的單一 source of truth,check_staleness 應直接吃這個抽象而不是
    /// `matches!(state, Working)`。
    #[test]
    fn r210_waiting_for_user_transitions_to_idle_via_staleness_check() {
        let mut m = SessionManager::new();
        // Notification 觸發 WaitingForUser
        let _ = m.handle_event(&ev("claude", "w1", "Notification"));
        let session = m.sessions.get("w1").expect("session just inserted");
        assert_eq!(session.state, SessionState::WaitingForUser);
        assert!(session.is_active(), "precondition: WaitingForUser is active");

        // 把 last_event_time 倒推 idle+1 秒,模擬「通知後使用者不回話」
        m.sessions.get_mut("w1").unwrap().last_event_time =
            Utc::now() - Duration::seconds(31);

        m.check_staleness(/* idle */ 30, /* stale */ 300, /* remove */ 1800);

        let session = m.sessions.get("w1").expect("session still in map (not > remove)");
        assert_eq!(
            session.state,
            SessionState::Idle,
            "WaitingForUser + elapsed > idle 必須降級成 Idle,實際 = {:?}",
            session.state
        );
        assert!(
            !session.is_active(),
            "Idle 後 is_active() 必須 false,active_count 與 active_providers 才不會把死 session 算進去"
        );
    }
}

#[cfg(test)]
mod completions_tests {
    use super::*;

    #[test]
    fn record_completion_caps_and_orders() {
        let mut m = SessionManager::new();
        for i in 0..105 {
            m.record_completion("claude", &format!("s{i}"));
        }
        assert_eq!(m.completions.len(), MAX_COMPLETIONS);
        assert_eq!(
            m.completions.back().unwrap().session_id,
            "s104",
            "尾端應是最新"
        );
        assert_eq!(
            m.completions.front().unwrap().session_id,
            "s5",
            "頭端最舊被擠掉 5 筆"
        );
    }

    #[test]
    fn save_load_roundtrip_and_bad_file() {
        let dir = std::env::temp_dir().join(format!("lp-completions-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("completions.json");

        let mut dq = std::collections::VecDeque::new();
        dq.push_back(RecentEvent {
            timestamp: Utc::now(),
            provider: "codex".into(),
            session_id: "abc".into(),
            event_name: "Stop".into(),
            tool_name: None,
            error: None,
        });
        save_completions_at(&path, &dq).expect("寫入應成功");
        let loaded = load_completions_at(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].provider, "codex");
        assert_eq!(loaded[0].session_id, "abc");

        // 壞 JSON → 回空不 panic
        std::fs::write(&path, "{not json").unwrap();
        assert!(load_completions_at(&path).is_empty());
        // 檔案不存在 → 回空
        assert!(load_completions_at(&dir.join("nope.json")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
