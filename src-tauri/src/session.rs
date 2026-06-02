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
}

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
    use super::{SessionManager, SessionTransition};
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
}
