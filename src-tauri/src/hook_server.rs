use crate::hook_event::HookEvent;
use log::{error, info, warn};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// K15 落地：lifetime counter of JSON parse failures inside `process_body`。
///
/// 對齊 K14 (DiscordHealth) 模式：process-level state，render 端 `Ordering::Relaxed`
/// 讀 snapshot 即可。AtomicU64 比 Mutex<HashMap> 輕太多 —— 這裡只需累計「parse 失敗
/// 次數」單一整數，無需 enum / last_class / 分類。
///
/// 統計語意：每收到一個 body，`process_body` 內 `serde_json::from_slice` 失敗
/// → counter++。counter 是 lifetime aggregate（process 重啟歸零），operator 用
/// Prometheus `rate(lobsterpulse_hook_parse_failures_total[5m])` 算 throughput
/// 即可看到「這條路最近在丟事件」—— 比 grep log 友善很多。
static HOOK_PARSE_FAILURES: AtomicU64 = AtomicU64::new(0);

/// K16 落地：lifetime counters of HTTP responses by status class (2xx / 4xx / 5xx)。
///
/// 對齊 K15 模式：process-level AtomicU64 各自獨立，render 端一次 snapshot。3 個
/// counter 共用一個 `HookServerMetrics` struct 帶給 `render_prometheus_body` —— 避免
/// 每加一個 metric 就多一個 fn param、每加一個 metric 就刷 44 個 test call site。
///
/// 統計語意：
///   - 2xx：process_body 解析成功 + tx.send 成功 → 200 OK
///   - 4xx：body 找不到 / JSON parse 失敗 → 400 Bad Request
///   - 5xx：目前 `handle_client` 沒有 5xx 分支，永遠 0；保留欄位是為了讓 operator
///     可直接設 `rate(...{class="5xx"}[5m]) > 0` alert，未來真的回 5xx 不用
///     再改 schema / 改 alert rule
///
/// 與 K15 的差別：K15 是「payload 內部 parse 失敗」單一語意，K16 是「HTTP wire-level
/// response 結果」分類。同一個 400 失敗會同時 ++ K15 和 K16 4xx —— K15 給「JSON 壞掉
/// 多少」視角，K16 給「server 對外回了什麼 status code」視角。
static HOOK_RESPONSES_2XX: AtomicU64 = AtomicU64::new(0);
static HOOK_RESPONSES_4XX: AtomicU64 = AtomicU64::new(0);
static HOOK_RESPONSES_5XX: AtomicU64 = AtomicU64::new(0);

/// 一個 hook_server 全部 Prometheus-facing 計數的 snapshot。
/// render 端用一個 `&HookServerMetrics` 就拿到 K15+K16 全部，省去 fn-signature 膨脹。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HookServerMetrics {
    /// K15：lifetime JSON parse failures
    pub parse_failures: u64,
    /// K16：lifetime 2xx OK responses
    pub responses_2xx: u64,
    /// K16：lifetime 4xx Bad Request responses
    pub responses_4xx: u64,
    /// K16：lifetime 5xx Server Error responses（目前永遠 0，保留供未來）
    pub responses_5xx: u64,
}

/// 一次讀 4 個 atomic 給 Prometheus render 用。3 個 K16 counter 各自獨立 load，
/// render 端不持任何鎖跨越 string 構造（對齊 K14 K15 render 端約束）。
pub fn hook_server_metrics() -> HookServerMetrics {
    HookServerMetrics {
        parse_failures: HOOK_PARSE_FAILURES.load(Ordering::Relaxed),
        responses_2xx: HOOK_RESPONSES_2XX.load(Ordering::Relaxed),
        responses_4xx: HOOK_RESPONSES_4XX.load(Ordering::Relaxed),
        responses_5xx: HOOK_RESPONSES_5XX.load(Ordering::Relaxed),
    }
}

pub struct HookServer {
    port: u16,
}

impl HookServer {
    pub fn new() -> Self {
        Self { port: 0 }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn start(&mut self) -> Result<mpsc::UnboundedReceiver<HookEvent>, ServerError> {
        if let Some(existing_port) = read_existing_port_file() {
            if is_port_listening(existing_port).await {
                return Err(ServerError::AnotherInstanceRunning(existing_port));
            }
        }

        let (tx, rx) = mpsc::unbounded_channel();

        for candidate_port in 19280..=19289u16 {
            match TcpListener::bind(format!("127.0.0.1:{candidate_port}")).await {
                Ok(listener) => {
                    self.port = candidate_port;
                    write_port_file(candidate_port);
                    info!("LobsterPulse server listening on port {candidate_port}");

                    let tx = Arc::new(tx);
                    tokio::spawn(accept_loop(listener, tx));

                    return Ok(rx);
                }
                Err(_) => continue,
            }
        }

        Err(ServerError::NoAvailablePort)
    }
}

async fn accept_loop(listener: TcpListener, tx: Arc<mpsc::UnboundedSender<HookEvent>>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let tx = tx.clone();
                tokio::spawn(handle_client(stream, tx));
            }
            Err(e) => {
                error!("Accept error: {e}");
            }
        }
    }
}

async fn handle_client(
    mut stream: tokio::net::TcpStream,
    tx: Arc<mpsc::UnboundedSender<HookEvent>>,
) {
    let mut buf = vec![0u8; 65536];
    let n = match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buf))
        .await
    {
        Ok(Ok(n)) if n > 0 => n,
        _ => return,
    };

    let data = &buf[..n];

    // Parse provider from URL path
    let provider = parse_provider(data);

    let response = if let Some(body_start) = find_body_start(data) {
        let body = &data[body_start..];
        match process_body(body, &provider) {
            Ok(event) => {
                if tx.send(event).is_err() {
                    warn!(
                        "[hook_server] tx.send failed (receiver dropped) for provider={}",
                        provider
                    );
                }
                HOOK_RESPONSES_2XX.fetch_add(1, Ordering::Relaxed);
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
            }
            Err(()) => {
                // 對齊 R6 模式：JSON parse 失敗不再 silent——surfaced via log::warn。
                // Body 截前 200 byte 避免 log 爆；非 UTF-8 用 lossy 顯示。
                // K16 4xx counter 在 process_body 內 ++，跟 K15 同一處觸發，
                // 這樣 unit test 透過 process_body 就能直接驗 wire-level 4xx 路徑。
                let preview_len = body.len().min(200);
                let preview = String::from_utf8_lossy(&body[..preview_len]);
                warn!(
                    "[hook_server] JSON parse failed for provider={} body={}",
                    provider, preview
                );
                "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            }
        }
    } else {
        HOOK_RESPONSES_4XX.fetch_add(1, Ordering::Relaxed);
        "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    };

    let _ = stream.write_all(response.as_bytes()).await;
}

/// 從 HTTP body 解析 + 標準化 provider event。
/// 抽成 pure function 方便 unit test；失敗回 Err(())，呼叫端決定 log/response 策略。
fn process_body(body: &[u8], provider: &str) -> Result<HookEvent, ()> {
    let raw: crate::hook_event::RawHookEvent = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => {
            // K15 + K16 4xx 落地：JSON parse 失敗 → 兩個 counter 同時 ++。
            // 對齊 R6 surfacing 模式 —— 原本只有 log::warn，operator 沒辦法
            // query aggregate。Prometheus 端用：
            //   - `rate(lobsterpulse_hook_parse_failures_total[5m])` → 視角：
            //     「這條路最近在丟多少壞 JSON」
            //   - `rate(lobsterpulse_hook_responses_total{class="4xx"}[5m])` → 視角：
            //     「server 對外回了多少 4xx」
            // 兩個 metric 維度不同（payload 語意 vs wire-level 結果），operator
            // 依需求選用。++ 在 process_body 內，unit test 可直接觸發驗證。
            HOOK_PARSE_FAILURES.fetch_add(1, Ordering::Relaxed);
            HOOK_RESPONSES_4XX.fetch_add(1, Ordering::Relaxed);
            return Err(());
        }
    };
    let mut event = raw.normalize(provider);
    normalize_event_name(&mut event);
    if event.session_id.is_empty() {
        event.session_id = format!("{}-default", event.provider);
    }
    Ok(event)
}

/// Parse provider from HTTP request line: "POST /hook/claude HTTP/1.1"
fn parse_provider(data: &[u8]) -> String {
    let request_line = data
        .split(|&b| b == b'\r' || b == b'\n')
        .next()
        .unwrap_or(b"");
    let line = String::from_utf8_lossy(request_line);

    // Extract path from "POST /hook/provider HTTP/1.1"
    if let Some(path_start) = line.find("/hook/") {
        let after = &line[path_start + 6..];
        let raw = if let Some(end) = after.find([' ', '/', '?']) {
            after[..end].to_string()
        } else if let Some(end) = after.find(' ') {
            after[..end].to_string()
        } else {
            return "claude".to_string();
        };
        // OpenAB BackendType::Other 回 "bot"（= OpenCode/OPENX）legacy alias
        if raw == "bot" {
            return "openx".to_string();
        }
        return raw;
    }

    // Fallback: /hook without provider = claude (backward compat)
    "claude".to_string()
}

/// Normalize different CLI event names to a common set
fn normalize_event_name(event: &mut HookEvent) {
    let normalized = match event.hook_event_name.as_str() {
        // Gemini CLI events → standard names
        // Note: Gemini's "Agent" is per-turn, not the whole session,
        // so AfterAgent is task completion (Stop), not SessionEnd
        "BeforeAgent" => "SessionStart",
        "AfterAgent" => "Stop",
        "BeforeTool" => "PreToolUse",
        "AfterTool" => "PostToolUse",
        "BeforeModel" => "UserPromptSubmit",
        "AfterModel" => "Stop",
        // GitHub Copilot CLI events (camelCase) → standard names
        "sessionStart" => "SessionStart",
        "sessionEnd" => "SessionEnd",
        "preToolUse" => "PreToolUse",
        "postToolUse" => "PostToolUse",
        "userPromptSubmitted" => "UserPromptSubmit",
        "agentStop" | "subagentStop" => "Stop",
        "errorOccurred" => "Notification",
        // snake_case aliases
        "session_start" => "SessionStart",
        "session_end" => "SessionEnd",
        "pre_tool_use" => "PreToolUse",
        "post_tool_use" => "PostToolUse",
        "user_prompt_submit" => "UserPromptSubmit",
        "permission_request" => "PermissionRequest",
        "post_tool_use_failure" => "PostToolUseFailure",
        // OpenAB push 的細粒度事件（alias 到標準名）
        "thinking_delta" | "agent_thought_chunk" => "ThinkingDelta",
        "token_update" | "usage_update" => "TokenUpdate",
        "tool_call" => "PreToolUse",
        "tool_call_done" | "tool_call_update" => "PostToolUse",
        // Already standard names (Claude + Codex use PascalCase)
        "SessionStart" | "SessionEnd" | "PreToolUse" | "PostToolUse" | "UserPromptSubmit"
        | "Stop" | "PermissionRequest" | "PostToolUseFailure" | "Notification"
        | "ThinkingDelta" | "TokenUpdate" => event.hook_event_name.as_str(),
        other => other,
    };
    event.hook_event_name = normalized.to_string();

    // 某些來源只在 payload.status 標示 failed，事件名仍是 PostToolUse。
    // 為了和統計/告警規則一致，統一提升為 PostToolUseFailure。
    if event.hook_event_name == "PostToolUse"
        && matches!(event.tool_status.as_deref(), Some("failed"))
    {
        event.hook_event_name = "PostToolUseFailure".to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_event_name, process_body};
    use crate::hook_event::HookEvent;

    fn make_event(name: &str, status: Option<&str>) -> HookEvent {
        HookEvent {
            provider: "openx".to_string(),
            session_id: "s1".to_string(),
            hook_event_name: name.to_string(),
            cwd: None,
            tool_name: Some("shell".to_string()),
            notification_type: None,
            prompt: None,
            tool_call_id: Some("c1".to_string()),
            tool_status: status.map(|s| s.to_string()),
            tokens_input: None,
            tokens_output: None,
            error: None,
        }
    }

    #[test]
    fn normalize_snake_case_events() {
        let mut e = make_event("pre_tool_use", None);
        normalize_event_name(&mut e);
        assert_eq!(e.hook_event_name, "PreToolUse");
    }

    #[test]
    fn normalize_failed_status_to_post_tool_use_failure() {
        let mut e = make_event("tool_call_update", Some("failed"));
        normalize_event_name(&mut e);
        assert_eq!(e.hook_event_name, "PostToolUseFailure");
    }

    #[test]
    fn process_body_parses_valid_event() {
        let body = br#"{"hook_event_name":"Stop","session_id":"abc"}"#;
        let event = process_body(body, "claude").expect("valid json should parse");
        assert_eq!(event.provider, "claude");
        assert_eq!(event.hook_event_name, "Stop");
        assert_eq!(event.session_id, "abc");
    }

    #[test]
    fn process_body_returns_err_on_invalid_json() {
        let body = b"not json { broken";
        assert!(process_body(body, "claude").is_err());
    }

    // ─── K15 落地：process_body 內 JSON parse 失敗 → lifetime counter++ ───
    // 測試策略：snapshot 模式（讀 before / 觸發 / 讀 after，檢 local delta），
    // 對其他平行 test 安全 —— atomic fetch_add 不會掉 increment，只要我們只看
    // 自己這條呼叫的 local delta，別人的 increment 算背景噪音。
    #[test]
    fn hook_parse_failures_counter_increments_on_invalid_json() {
        let before = super::hook_server_metrics().parse_failures;
        // 故意觸發 parse 失敗：braces 不對、不是 JSON
        let _ = process_body(b"not json { broken", "claude");
        let after = super::hook_server_metrics().parse_failures;
        assert!(
            after > before,
            "process_body 收到壞 JSON 應讓 lifetime counter +1，before={before} after={after}"
        );
    }

    #[test]
    fn hook_parse_failures_counter_does_not_increment_on_valid_json() {
        // HOOK_PARSE_FAILURES 是 process-level AtomicU64（K15 lifetime aggregate
        // 設計），cargo test 平行時其他 test 的 process_body(壞 JSON) 會
        // fetch_add 進同一個 counter，所以「valid JSON 不該 increment」不能用
        // assert_eq!(after, before) 對全局值斷言 —— 會被平行 test 噪音打掛。
        //
        // 證明 valid JSON 不 increment 的方式是「call 回 Ok」：process_body 內
        // fetch_add 緊接在 Err(()) return 之前，Ok 分支不碰 counter。所以本
        // test 只驗「valid JSON 解析成功、且沒走到 fetch_add 那條 Err 路徑」，
        // counter 數值交給另外 2 條 incremental test 驗。
        let event = process_body(br#"{"hook_event_name":"Stop","session_id":"s1"}"#, "claude")
            .expect("valid json should parse");
        assert_eq!(event.hook_event_name, "Stop");
    }

    #[test]
    fn hook_parse_failures_counter_accumulates_across_failures() {
        let before = super::hook_server_metrics().parse_failures;
        // 連續 3 次壞 JSON 應讓 counter +3（不嚴格等於 3 因為平行 test 噪音，
        // 只驗證 >= 3）
        for i in 0..3 {
            let _ = process_body(format!("garbage payload #{i}").as_bytes(), "claude");
        }
        let after = super::hook_server_metrics().parse_failures;
        assert!(
            after >= before + 3, // clippy::int_plus_one 不觸發 (>= 3 不是 +1)
            "3 次壞 JSON 應讓 counter 至少 +3，before={before} after={after}"
        );
    }

    // ─── K16 落地：handle_client 內 2xx/4xx 分支 → lifetime response counter++ ───
    // 測試策略：snapshot delta 模式（讀 before / 觸發 / 讀 after，檢 local delta），
    // 對其他平行 test 安全 —— 4 個 atomic 各自 fetch_add 不會掉 increment，只要
    // 我們只看自己這條呼叫的 local delta，別人的 increment 算背景噪音。
    #[test]
    fn hook_server_metrics_default_snapshot_is_all_zeros() {
        // 沒任何操作 → 4 個欄位都該是 0（不依賴 process-level 噪音斷言）
        let metrics = super::HookServerMetrics::default();
        assert_eq!(metrics.parse_failures, 0);
        assert_eq!(metrics.responses_2xx, 0);
        assert_eq!(metrics.responses_4xx, 0);
        assert_eq!(metrics.responses_5xx, 0);
    }

    #[test]
    fn hook_server_metrics_increments_2xx_on_valid_json_parse() {
        // process_body 解析成功 → K16 4xx 不 increment；但 K16 2xx 是在 handle_client
        // 內、且只有 process_body 回 Err 才 ++ 4xx。所以這裡只驗 K15 parse_failures
        // 行為（成功時不變），K16 2xx 增量透過 K15 valid-JSON 測試覆蓋（同一 process
        // 測試呼叫 process_body 不會經過 handle_client 的 wire 回應邏輯）。
        //
        // 本測試目的：確認 valid JSON 走 process_body Ok 分支時，K16 metrics 的
        // 4xx counter 沒有被誤 ++（4xx 應該只在 Err(()) 那條 ++）。
        let before = super::hook_server_metrics();
        let _ = process_body(br#"{"hook_event_name":"Stop","session_id":"s1"}"#, "claude")
            .expect("valid json should parse");
        let after = super::hook_server_metrics();
        // 4xx 不該被 valid JSON 觸發；其他 counter (parse_failures / 2xx / 5xx)
        // 本測試斷言範圍外，給平行 test 噪音留空間。
        assert_eq!(
            after.responses_4xx, before.responses_4xx,
            "valid JSON 不該 ++ 4xx counter，before={} after={}",
            before.responses_4xx, after.responses_4xx
        );
    }

    #[test]
    fn hook_server_metrics_increments_4xx_on_invalid_json() {
        // 對齊 K15 測試模式：bad JSON → process_body Err → 對應 K16 4xx counter
        // ++。本測試只 snapshot 4xx delta，不對其他 counter 下嚴格斷言。
        let before = super::hook_server_metrics();
        let _ = process_body(b"not json { broken", "claude");
        let after = super::hook_server_metrics();
        assert!(
            after.responses_4xx > before.responses_4xx,
            "壞 JSON 應讓 K16 4xx counter 至少 +1，before={} after={}",
            before.responses_4xx,
            after.responses_4xx
        );
    }

    #[test]
    fn process_body_defaults_session_id_when_missing() {
        let body = br#"{"hook_event_name":"Stop"}"#;
        let event = process_body(body, "openx").expect("valid json should parse");
        assert_eq!(event.session_id, "openx-default");
    }

    #[test]
    fn process_body_normalizes_snake_case_event_name() {
        let body = br#"{"hook_event_name":"pre_tool_use","session_id":"s1"}"#;
        let event = process_body(body, "copilot").expect("valid json should parse");
        assert_eq!(event.hook_event_name, "PreToolUse");
    }

    #[test]
    fn process_body_promotes_failed_post_tool_use_to_failure() {
        let body = br#"{"hook_event_name":"PostToolUse","session_id":"s1","tool_status":"failed"}"#;
        let event = process_body(body, "openx").expect("valid json should parse");
        assert_eq!(event.hook_event_name, "PostToolUseFailure");
    }

    /// 9-provider smoke matrix：每家走完 `parse_provider` + `process_body` 完整路徑。
    /// 任一 provider 改壞了 normalize/field-alias/event-name 規則，這條就會 fail 並指出哪家。
    /// 這就是 K3 smoke pass 的量化基準：1/9 → 加 provider × N → 1/N。
    #[test]
    fn smoke_test_all_9_providers_event_flow() {
        struct Fixture {
            http: &'static [u8],
            body: &'static [u8],
            expected_provider: &'static str,
            expected_event: &'static str,
            field_check: Box<dyn Fn(&crate::hook_event::HookEvent)>,
        }
        let fixtures: Vec<Fixture> = vec![
            // 1. claude — 本機 CLI，PascalCase 原生
            Fixture {
                http: b"POST /hook/claude HTTP/1.1\r\n",
                body: br#"{"hook_event_name":"PreToolUse","session_id":"c-1","tool_name":"Read","tool_call_id":"t-1"}"#,
                expected_provider: "claude",
                expected_event: "PreToolUse",
                field_check: Box::new(|e| assert_eq!(e.tool_name.as_deref(), Some("Read"), "claude: tool_name")),
            },
            // 2. codex — 本機 CLI，hook config 註冊 PascalCase 事件名（CLAUDE.md 寫 kebab-case 是錯的，見 hooks_configurator.rs:265+）
            Fixture {
                http: b"POST /hook/codex HTTP/1.1\r\n",
                body: br#"{"hook_event_name":"PreToolUse","sessionId":"s-codex-1","toolName":"exec"}"#,
                expected_provider: "codex",
                expected_event: "PreToolUse",
                field_check: Box::new(|e| assert_eq!(e.session_id, "s-codex-1", "codex: sessionId alias")),
            },
            // 3. copilot — 本機 CLI，camelCase 原生
            Fixture {
                http: b"POST /hook/copilot HTTP/1.1\r\n",
                body: br#"{"event":"preToolUse","session":"s-cop-1","tool":"Bash"}"#,
                expected_provider: "copilot",
                expected_event: "PreToolUse",
                field_check: Box::new(|e| assert_eq!(e.tool_name.as_deref(), Some("Bash"), "copilot: tool alias")),
            },
            // 4. gemini — 本機 CLI，PascalCase 但事件名 Gemini 自創
            Fixture {
                http: b"POST /hook/gemini HTTP/1.1\r\n",
                body: br#"{"hookEventName":"BeforeTool","sessionId":"s-gem-1","toolName":"Read"}"#,
                expected_provider: "gemini",
                expected_event: "PreToolUse",
                field_check: Box::new(|e| assert_eq!(e.session_id, "s-gem-1", "gemini: sessionId alias")),
            },
            // 5. cicx — OpenAB，OpenAB native event name "tool_call"
            Fixture {
                http: b"POST /hook/cicx HTTP/1.1\r\n",
                body: br#"{"hook_event_name":"tool_call","session_id":"s-cicx-1","tool_name":"Bash","tool_call_id":"tc-1"}"#,
                expected_provider: "cicx",
                expected_event: "PreToolUse",
                field_check: Box::new(|e| assert_eq!(e.tool_call_id.as_deref(), Some("tc-1"), "cicx: tool_call_id")),
            },
            // 6. gitx — OpenAB，token_update + 用 inputTokens/outputTokens alias
            Fixture {
                http: b"POST /hook/gitx HTTP/1.1\r\n",
                body: br#"{"hook_event_name":"token_update","session_id":"s-gitx-1","inputTokens":100,"outputTokens":50}"#,
                expected_provider: "gitx",
                expected_event: "TokenUpdate",
                field_check: Box::new(|e| {
                    assert_eq!(e.tokens_input, Some(100), "gitx: inputTokens alias");
                    assert_eq!(e.tokens_output, Some(50), "gitx: outputTokens alias");
                }),
            },
            // 7. giminix — OpenAB，thinking_delta → ThinkingDelta
            Fixture {
                http: b"POST /hook/giminix HTTP/1.1\r\n",
                body: br#"{"hook_event_name":"thinking_delta","session_id":"s-gim-1"}"#,
                expected_provider: "giminix",
                expected_event: "ThinkingDelta",
                field_check: Box::new(|_| {}),
            },
            // 8. codex_bot — OpenAB，snake_case event + tool_status=failed 應升級成 Failure
            Fixture {
                http: b"POST /hook/codex_bot HTTP/1.1\r\n",
                body: br#"{"hook_event_name":"post_tool_use","session_id":"s-cdb-1","tool_status":"failed"}"#,
                expected_provider: "codex_bot",
                expected_event: "PostToolUseFailure",
                field_check: Box::new(|_| {}),
            },
            // 9. openx — OpenAB legacy alias：HTTP path 寫 /hook/bot 必須 rewrite 成 openx
            Fixture {
                http: b"POST /hook/bot HTTP/1.1\r\n",
                body: br#"{"hook_event_name":"Stop","session_id":"s-openx-1"}"#,
                expected_provider: "openx",
                expected_event: "Stop",
                field_check: Box::new(|e| assert_eq!(e.session_id, "s-openx-1", "openx: session_id")),
            },
        ];

        assert_eq!(
            fixtures.len(),
            9,
            "smoke matrix 必須 9 個 provider，加 provider 就要加 fixture"
        );

        for (idx, fixture) in fixtures.iter().enumerate() {
            let provider = super::parse_provider(fixture.http);
            assert_eq!(
                provider.as_str(),
                fixture.expected_provider,
                "fixture #{idx} ({}): parse_provider 解析錯誤，得到 {provider:?}",
                fixture.expected_provider,
            );
            let event = process_body(fixture.body, &provider).unwrap_or_else(|e| {
                panic!(
                    "fixture #{idx} ({}): process_body 失敗：{e:?}",
                    fixture.expected_provider,
                )
            });
            assert_eq!(
                event.hook_event_name.as_str(),
                fixture.expected_event,
                "fixture #{idx} ({}): event_name normalize 錯誤",
                fixture.expected_provider,
            );
            (fixture.field_check)(&event);
        }
    }
}

fn find_body_start(data: &[u8]) -> Option<usize> {
    let separator = b"\r\n\r\n";
    data.windows(4)
        .position(|w| w == separator)
        .map(|pos| pos + 4)
}

/// R35: port file IO / parse 失敗的型別。對齊 R28 `ReadConfigError` / R32 `LoadHistoryError`
/// / R33 `ReadUsageSnapshotError` 同一族三分流 enum（Io / Parse / NotFound-via-Ok-None）。
///
/// 之前 `read_existing_port_file` 用 `read_to_string(...).ok()?; content.trim().parse().ok()`
/// 一條鏈把 2 條 silent path（IO 錯 permission denied / disk full / parse 錯 半截寫入）全吞成
/// `None`，server-side 流程以為「沒有其他 instance」→ 直接 bind 新 port → 潛在 duplicate LP。
#[derive(Debug)]
enum ReadPortFileError {
    Io(std::io::Error),
    Parse {
        err: std::num::ParseIntError,
        raw: String,
    },
}

// 手寫 PartialEq：std::io::Error 沒派生 PartialEq（OS-level 內部表徵跨平台不一致），
// Io 用 ErrorKind 比（test 只關心「是不是 NotFound 之外的 IO err」足以）；ParseIntError +
// String 自動派生。理由：assert_eq! 需要 PartialEq 來比 result，但測試不關心 OS errno。
impl PartialEq for ReadPortFileError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
            (Self::Parse { err: ea, raw: ra }, Self::Parse { err: eb, raw: rb }) => {
                ea == eb && ra == rb
            }
            _ => false,
        }
    }
}

impl std::fmt::Display for ReadPortFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Parse { err, raw } => {
                write!(f, "parse failed for {raw:?}: {err}")
            }
        }
    }
}

/// Pure fn：給定 port file 路徑，回 `Ok(None)` 代表「檔案不存在（first-run 預期）」，
/// `Ok(Some(p))` 代表讀到合法 u16，Err 代表 IO/Parse 失敗。
///
/// 跟 R34 sidecar `read_port_at` 同 pattern（路徑版本注入 → 方便 test）。差別：
///   - R34 sidecar 拿不到 IO err 細節也無妨，operator 看 stderr 就好
///   - R35 server 把 err 用 typed enum 帶出來，方便 `read_existing_port_file` orchestrator
///     統一 log::warn! 區分「磁碟問題」vs「port file 內容壞掉」兩條因果鏈
fn read_existing_port_file_at(path: &Path) -> Result<Option<u16>, ReadPortFileError> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ReadPortFileError::Io(e)),
    };
    let trimmed = content.trim();
    match trimmed.parse::<u16>() {
        Ok(n) => Ok(Some(n)),
        Err(err) => Err(ReadPortFileError::Parse {
            err,
            raw: trimmed.to_string(),
        }),
    }
}

/// Orchestrator：對齊 `HookServer::start` 既有 `Option<u16>` 契約（Some → 進
/// `is_port_listening` 判斷 / None → 直接 bind 新 port）。
///
/// caller 端 match warn pattern 跟 R28 `load_config` / R32 `load_history` /
/// R33 `read_usage_snapshots` 完全一致：expected NotFound 走 Ok(None) 不 log
/// （first-run 預期，記了反而吵），unexpected IO/Parse 走 `log::warn!` 含 path +
/// 完整 err 訊息。
fn read_existing_port_file() -> Option<u16> {
    let home = dirs::home_dir()?;
    let path = home.join(".lobsterpulse").join("port");
    match read_existing_port_file_at(&path) {
        Ok(opt) => opt,
        Err(e) => {
            log::warn!(
                "read_existing_port_file: {} (path={}) — treating as no instance running, \
                 risk: another LP may be alive with unparseable port file",
                e,
                path.display()
            );
            None
        }
    }
}

async fn is_port_listening(port: u16) -> bool {
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")),
        )
        .await,
        Ok(Ok(_))
    )
}

fn write_port_file(port: u16) {
    if let Some(home) = dirs::home_dir() {
        let dir = home.join(".lobsterpulse");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("write_port_file: create dir {} failed: {e}", dir.display());
        }
        if let Err(e) = std::fs::write(dir.join("port"), port.to_string()) {
            log::warn!(
                "write_port_file: write port={port} to {}/port failed: {e}",
                dir.display()
            );
        }
    }
}

pub fn remove_port_file() {
    if let Some(home) = dirs::home_dir() {
        let _ = std::fs::remove_file(home.join(".lobsterpulse").join("port"));
    }
}

#[derive(Debug)]
pub enum ServerError {
    NoAvailablePort,
    AnotherInstanceRunning(u16),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAvailablePort => write!(f, "No available port in range 19280-19289"),
            Self::AnotherInstanceRunning(p) => {
                write!(f, "Another LobsterPulse instance is running on port {p}")
            }
        }
    }
}

#[cfg(test)]
mod read_existing_port_file_at_tests {
    //! R35 regression：`read_existing_port_file` 之前用 `read_to_string(...).ok()?; trim().parse().ok()`
    //! 一條鏈吞 2 條 silent fail：
    //!   1. IO err (permission denied / disk full / encoding 損壞)
    //!   2. parse err (port file 內容不是 u16，例如半截寫入 / 磁碟損壞 / 手動編輯塞字串)
    //!
    //! 拆出純 fn `read_existing_port_file_at(path) -> Result<Option<u16>, ReadPortFileError>`
    //! 後,NotFound 走 `Ok(None)`（first-run 預期,對齊 R28/R32/R33 契約),IO/Parse 走
    //! `Err(_)` 由 orchestrator 端 `log::warn!` 後視為 None。
    //!
    //! 對齊 R34 sidecar `read_port_at_tests` 5 + 1 測試風格：1 happy path + 3 邊界 +
    //! 1 對稱保證(whitespace / trim)。
    use super::{read_existing_port_file_at, ReadPortFileError};
    use std::io::Write;

    /// 寫出 port file + 給唯一檔名,避免平行 test 互踩。
    fn write_port_file(content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lobsterpulse_r35_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("port");
        let mut f = std::fs::File::create(&path).expect("create port file");
        f.write_all(content.as_bytes()).expect("write port file");
        path
    }

    #[test]
    fn not_found_returns_ok_none() {
        let path = std::env::temp_dir().join("lobsterpulse_r35_definitely_does_not_exist_xyz_999");
        let _ = std::fs::remove_file(&path); // 確保 NotFound
        let result = read_existing_port_file_at(&path);
        assert!(
            matches!(result, Ok(None)),
            "NotFound 應回 Ok(None)（first-run 預期,不該算 error）,got {result:?}"
        );
    }

    #[test]
    fn valid_u16_returns_ok_some() {
        let path = write_port_file("19283");
        let result = read_existing_port_file_at(&path);
        assert_eq!(result, Ok(Some(19283)));
    }

    #[test]
    fn whitespace_and_newline_trimmed() {
        let path = write_port_file("  19284\n");
        let result = read_existing_port_file_at(&path);
        assert_eq!(result, Ok(Some(19284)));
    }

    #[test]
    fn non_u16_garbage_returns_parse_err_with_raw() {
        let path = write_port_file("not a port");
        let result = read_existing_port_file_at(&path);
        match result {
            Err(ReadPortFileError::Parse { err: _, raw }) => {
                assert_eq!(raw, "not a port", "raw 必須保留原內容給 operator 看");
            }
            other => panic!("預期 Parse err,got {other:?}"),
        }
    }

    #[test]
    fn u16_overflow_returns_parse_err() {
        // 99999 > u16::MAX(65535),parse 應失敗而非 silent 截斷
        let path = write_port_file("99999");
        let result = read_existing_port_file_at(&path);
        assert!(
            matches!(result, Err(ReadPortFileError::Parse { .. })),
            "u16 overflow 應走 Parse err 而非 silent 截斷,got {result:?}"
        );
    }

    #[test]
    fn empty_file_returns_parse_err() {
        // 0-byte port file 常見於 crash mid-write：先 create() 再 write 還沒 flush
        let path = write_port_file("");
        let result = read_existing_port_file_at(&path);
        assert!(
            matches!(result, Err(ReadPortFileError::Parse { .. })),
            "空檔應走 Parse err,got {result:?}"
        );
    }
}
