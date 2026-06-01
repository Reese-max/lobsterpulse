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
    /// 累計起始時間（第一次 event 進來）——給 UI 顯示「自何時累計」
    pub since: Option<DateTime<Utc>>,
    /// 最近一次 event 時間戳（任何 event 都會更新，不只在 TokenUpdate / Failure）——
    /// 給 `/metrics` 端計算 per-provider `idle_seconds` gauge 用。
    /// `None` 表示該 provider 還沒收過 event。
    pub last_event_at: Option<DateTime<Utc>>,
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

        if prev == SessionState::Working && now == SessionState::Idle {
            SessionTransition::Completed
        } else if prev != SessionState::WaitingForUser && now == SessionState::WaitingForUser {
            SessionTransition::StartedWaiting
        } else {
            SessionTransition::None
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
}
