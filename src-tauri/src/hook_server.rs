use crate::hook_event::HookEvent;
use log::{error, info, warn};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

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
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
            }
            Err(()) => {
                // 對齊 R6 模式：JSON parse 失敗不再 silent——surfaced via log::warn。
                // Body 截前 200 byte 避免 log 爆；非 UTF-8 用 lossy 顯示。
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
        "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    };

    let _ = stream.write_all(response.as_bytes()).await;
}

/// 從 HTTP body 解析 + 標準化 provider event。
/// 抽成 pure function 方便 unit test；失敗回 Err(())，呼叫端決定 log/response 策略。
fn process_body(body: &[u8], provider: &str) -> Result<HookEvent, ()> {
    let raw: crate::hook_event::RawHookEvent = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => return Err(()),
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

fn read_existing_port_file() -> Option<u16> {
    let home = dirs::home_dir()?;
    let path = home.join(".lobsterpulse").join("port");
    let content = std::fs::read_to_string(path).ok()?;
    content.trim().parse().ok()
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
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("port"), port.to_string());
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
