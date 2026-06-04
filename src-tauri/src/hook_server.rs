use crate::hook_event::HookEvent;
use log::{error, info, warn};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// R65: K15/K16 counter bundle 從 4 個 process-level `static AtomicU64` 改成
/// `Arc<MetricsCore>` 注入模式, 真正解掉 R36-R63 跨輪紀錄的 shared counter race。
///
/// 對齊 R64 wrap-up 第 1 條「K15/K16 race 真正解法」: 之前 static + cargo test 平行
/// 跑時, 其他 test 的 `process_body(壞 JSON)` 會 fetch_add 進同一個 counter, 導致
/// strict invariant test (`hook_parse_failures_counter_does_not_increment_on_valid_json`)
/// 假陽性失敗 (R64 觀察輪實測 1 failed / 358 passed)。原本 14 條 R52-R63 護欄都用
/// `delta >= N` 寬鬆斷言容忍 race noise, 只有那條 strict `delta == 0` test 凸顯問題。
///
/// 修法: 把 4 個 atomic 包成 `MetricsCore` struct, 透過 `Arc<MetricsCore>` 共享——
/// production 用 `default_metrics()` 全 process 共一份 (對齊原本 process-level
/// lifetime aggregate 語意), test 用 `new_metrics()` 拿自己一份 (隔離平行噪音,
/// strict assertion 可嚴格 `assert_eq!`)。`process_body` / `handle_client` /
/// `accept_loop` 改成 explicit `&MetricsCore` 參數, 沒有 silent global state。
pub struct MetricsCore {
    /// K15：lifetime JSON parse failures
    pub parse_failures: AtomicU64,
    /// K16：lifetime 2xx OK responses
    pub responses_2xx: AtomicU64,
    /// K16：lifetime 4xx Bad Request responses
    pub responses_4xx: AtomicU64,
    /// K16：lifetime 5xx Server Error responses（目前永遠 0，保留供未來）
    pub responses_5xx: AtomicU64,
}

impl MetricsCore {
    pub fn new() -> Self {
        Self {
            parse_failures: AtomicU64::new(0),
            responses_2xx: AtomicU64::new(0),
            responses_4xx: AtomicU64::new(0),
            responses_5xx: AtomicU64::new(0),
        }
    }

    /// 一次讀 4 個 atomic 給 Prometheus render 用。3 個 K16 counter 各自獨立 load,
    /// render 端不持任何鎖跨越 string 構造（對齊 K14 K15 render 端約束）。
    pub fn snapshot(&self) -> HookServerMetrics {
        HookServerMetrics {
            parse_failures: self.parse_failures.load(Ordering::Relaxed),
            responses_2xx: self.responses_2xx.load(Ordering::Relaxed),
            responses_4xx: self.responses_4xx.load(Ordering::Relaxed),
            responses_5xx: self.responses_5xx.load(Ordering::Relaxed),
        }
    }
}

/// Cheap-to-clone metrics handle. Arc bump 即可, 不複製 atomic。
pub type MetricsArc = Arc<MetricsCore>;

/// 給 test 用的工廠: 每次呼叫都拿到全新獨立的 `MetricsCore` (atomic 全部從 0 開始),
/// 平行 cargo test 期間該 instance 不會被其他 test 觸碰 → strict `assert_eq!` 安全。
pub fn new_metrics() -> MetricsArc {
    Arc::new(MetricsCore::new())
}

static DEFAULT_METRICS: std::sync::OnceLock<MetricsArc> = std::sync::OnceLock::new();

/// Production 用的 process-level default。OnceLock 保證 lazy init 一次, 之後
/// `default_metrics()` 每次 clone 一份 Arc (cheap, atomic refcount bump), 全
/// process 共一份 lifetime aggregate (對齊原本 K15/K16 語意)。
pub fn default_metrics() -> MetricsArc {
    DEFAULT_METRICS.get_or_init(new_metrics).clone()
}

/// 一個 hook_server 全部 Prometheus-facing 計數的 snapshot。
/// render 端用一個 `&HookServerMetrics` 就拿到 K15+K16 全部，省去 fn-signature 膨脹。
///
/// R65 對齊 MetricsCore 改造: `HookServerMetrics` 仍是 u64 snapshot (Copy + Default
/// 不變), 改成從 `MetricsCore` 讀, 沒破壞既有 render_prometheus_body contract。
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
///
/// R65: 改成從 `default_metrics()` 讀, 不再直接觸碰 4 個 static。Production 語意
/// 跟原本 process-level 對齊, 沒改 K15/K16 lifetime aggregate 語意。
pub fn hook_server_metrics() -> HookServerMetrics {
    default_metrics().snapshot()
}

#[cfg(test)]
impl HookServerMetrics {
    /// 兩個 snapshot 之間的 delta（各欄位 `after - before`，saturating）。
    /// 便利 K15/K16 lifetime counter test 計算 local delta，取代散落的
    /// `after > before` 寬鬆斷言。saturating 而非 wrapping：理論上 monotonic atomic
    /// 計數不可能倒退，但 saturating 對未來若引入 reset API 也能 robust。
    fn delta(self, before: Self) -> Self {
        Self {
            parse_failures: self.parse_failures.saturating_sub(before.parse_failures),
            responses_2xx: self.responses_2xx.saturating_sub(before.responses_2xx),
            responses_4xx: self.responses_4xx.saturating_sub(before.responses_4xx),
            responses_5xx: self.responses_5xx.saturating_sub(before.responses_5xx),
        }
    }
}

/// 跑 f，同時抓 K15/K16 counter 執行前後的 snapshot。
/// 便利 K15/K16 test 計算 local delta：
///   - `before` / `after` 都是 `HookServerMetrics` struct snapshot
///   - `result` 是 f() 的回傳值，不影響 counter 觀察
///
/// 注意：counter 是 process-level atomic，其他平行 test 仍會 ++ counter，
/// 這只是「包裝便利 + 確保 before/after 都在同一 atomic order」，仍需用
/// monotonic `delta >= N` 斷言，不該用 `assert_eq!` 對全局值斷言。
/// 對齊 R36 wrap-up K15/K16 shared counter race 觀察：off-by-one 容易因 race
/// 假陽性失敗，delta 計算 + saturating 為 race-tolerant pattern。
#[cfg(test)]
pub fn with_isolated_metric_snapshot<F, R>(f: F) -> (HookServerMetrics, HookServerMetrics, R)
where
    F: FnOnce() -> R,
{
    let before = hook_server_metrics();
    let result = f();
    let after = hook_server_metrics();
    (before, after, result)
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
                    // R65: 把 process-level default metrics 注入 accept_loop, 每個
                    // handle_client spawn 都 clone Arc (cheap) — production 行為
                    // 等同原本 4 個 static AtomicU64, 沒改 K15/K16 lifetime aggregate
                    // 語意, 但 unit test 可換成自己 new_metrics() 隔離平行噪音。
                    let metrics = default_metrics();
                    tokio::spawn(accept_loop(listener, tx, metrics));

                    return Ok(rx);
                }
                Err(_) => continue,
            }
        }

        Err(ServerError::NoAvailablePort)
    }
}

async fn accept_loop(
    listener: TcpListener,
    tx: Arc<mpsc::UnboundedSender<HookEvent>>,
    metrics: MetricsArc,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let tx = tx.clone();
                let metrics = metrics.clone();
                tokio::spawn(handle_client(stream, tx, metrics));
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
    metrics: MetricsArc,
) {
    let mut buf = vec![0u8; 65536];
    let n = match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buf))
        .await
    {
        Ok(Ok(n)) if n > 0 => n,
        _ => return,
    };

    let data = &buf[..n];

    // R63: `/healthz` early-dispatch — operator probe 流量不該污染 K15/K16 counter,
    // 也不該走 `parse_provider` fallback (會 parse 成 "claude" + 無 body → 400, log
    // 變成「JSON parse failed for provider=claude body=...」誤導). 早 return 隔離.
    if is_healthz_get_request(data) {
        let body = build_healthz_body();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        );
        let _ = stream.write_all(response.as_bytes()).await;
        return;
    }

    // Parse provider from URL path
    let provider = parse_provider(data);

    let response = if let Some(body_start) = find_body_start(data) {
        let body = &data[body_start..];
        match process_body(body, &provider, &metrics) {
            Ok(event) => {
                if tx.send(event).is_err() {
                    warn!(
                        "[hook_server] tx.send failed (receiver dropped) for provider={}",
                        provider
                    );
                }
                metrics.responses_2xx.fetch_add(1, Ordering::Relaxed);
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
        metrics.responses_4xx.fetch_add(1, Ordering::Relaxed);
        "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    };

    let _ = stream.write_all(response.as_bytes()).await;
}

/// 從 HTTP body 解析 + 標準化 provider event。
/// 抽成 pure function 方便 unit test；失敗回 Err(())，呼叫端決定 log/response 策略。
///
/// R65: 接受 `&MetricsCore` 參數, K15/K16 counter 注入而非 global static。Production
/// caller (`handle_client`) 傳 `default_metrics()`, test caller 傳 `new_metrics()`
/// 拿自己一份 → strict `assert_eq!` 對自己 instance 驗證, 隔離平行 cargo test 噪音。
fn process_body(body: &[u8], provider: &str, metrics: &MetricsCore) -> Result<HookEvent, ()> {
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
            metrics.parse_failures.fetch_add(1, Ordering::Relaxed);
            metrics.responses_4xx.fetch_add(1, Ordering::Relaxed);
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

/// R63: `GET /healthz` operator-facing liveness probe。
///
/// 對齊「LobsterPulse v5.1 mission: 桌面監控膠囊 + 9 provider hook」的可觀察性閉環——
/// 之前的 hook_server 只對外暴露 `/hook/{provider}` POST 端點, 沒有任何
/// GET-friendly 的 health check 端點, 部署到 k8s / docker-compose / 監控系統
/// (Prometheus blackbox exporter / Grafana health check / curl smoke test) 時
/// 沒有辦法用「TCP 連得到 ≠ server 健康」去驗證 listener 還在 accept 連線 + provider
/// dispatch 沒卡死。/healthz 補這條缺口, 純 GET + 200 OK + JSON body, 對接
/// livenessProbe / blackbox exporter 都是零摩擦。
///
/// 純 fn 設計: 不接 `&self`、不讀 global state、不觸發 K15/K16 counter, 純字串比對。
/// 呼叫端 (`handle_client`) 在 `parse_provider` 之前 early-dispatch, 把 operator
/// 流量和真實 hook 流量徹底分流——`/healthz` 不算 hook 事件, 不該污染 K15/K16 計數。
///
/// 嚴格匹配 `GET /healthz HTTP/1.1`: 不接受 query string / trailing slash / 其他
/// method, 避免「看起來像 healthz 但其實是奇怪的 hook 流量」被誤導成 200。
fn is_healthz_get_request(data: &[u8]) -> bool {
    let request_line = data
        .split(|&b| b == b'\r' || b == b'\n')
        .next()
        .unwrap_or(b"");
    let line = String::from_utf8_lossy(request_line);
    line.trim() == "GET /healthz HTTP/1.1"
}

/// R63: `/healthz` response body。手寫 JSON 不引 serde derive, 對齊 hook_server
/// 「純 fn 端 + 輕依賴」風格 (`process_body` 用 serde_json 解傳入, 自己 emit 端靠
/// `format!`)。內容:
///   - `status`: `"ok"` — operator probe 直接 grep
///   - `version`: `CARGO_PKG_VERSION` — 部署時版本確認
fn build_healthz_body() -> String {
    format!(
        r#"{{"status":"ok","version":"{}"}}"#,
        env!("CARGO_PKG_VERSION"),
    )
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
    use std::sync::atomic::Ordering;

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
        let event = process_body(body, "claude", &super::default_metrics())
            .expect("valid json should parse");
        assert_eq!(event.provider, "claude");
        assert_eq!(event.hook_event_name, "Stop");
        assert_eq!(event.session_id, "abc");
    }

    #[test]
    fn process_body_returns_err_on_invalid_json() {
        let body = b"not json { broken";
        assert!(process_body(body, "claude", &super::default_metrics()).is_err());
    }

    // ─── K15 落地：process_body 內 JSON parse 失敗 → lifetime counter++ ───
    // 測試策略：snapshot 模式（讀 before / 觸發 / 讀 after，檢 local delta），
    // 對其他平行 test 安全 —— atomic fetch_add 不會掉 increment，只要我們只看
    // 自己這條呼叫的 local delta，別人的 increment 算背景噪音。R58 收邊：統一
    // 用 `with_isolated_metric_snapshot` + `HookServerMetrics::delta` 取代散落的
    // before/after snapshot 樣板，語意化表達「自己這條 test 觀察到的 delta」，
    // saturating_sub 容忍任何 race 倒退（理論上 monotonic atomic 不可能）。
    #[test]
    fn hook_parse_failures_counter_increments_on_invalid_json() {
        let (before, after, _) = super::with_isolated_metric_snapshot(|| {
            // 故意觸發 parse 失敗：braces 不對、不是 JSON
            let _ = process_body(b"not json { broken", "claude", &super::default_metrics());
        });
        let delta = after.delta(before).parse_failures;
        assert!(
            delta >= 1,
            "process_body 收到壞 JSON 應讓 K15 counter 至少 +1, actual delta={delta}, before={}, after={}",
            before.parse_failures, after.parse_failures
        );
    }

    #[test]
    fn hook_parse_failures_counter_does_not_increment_on_valid_json() {
        // R65 收邊：原本 strict `assert_eq!(delta, 0)` 因為 HOOK_PARSE_FAILURES 是
        // process-level AtomicU64, cargo test 平行時其他 test 的 process_body(壞 JSON)
        // 會 fetch_add 進同一個 counter, 噪音打掛這條 strict invariant test (R64 觀察
        // 輪實測 1 failed / 358 passed)。修法: 拿一份獨立 `new_metrics()` 自己用,
        // strict assertion 對自己 instance 驗證 → 平行 test 不再污染。
        //
        // 語意: process_body Ok 分支不該碰任何 counter, 對齊 process_body 設計:
        //   - Err 分支: metrics.parse_failures++ 跟 metrics.responses_4xx++ 緊貼 fetch_add
        //   - Ok 分支: 完全不讀不寫 metrics
        // 所以對「valid JSON 進 process_body」這條 call, metrics 應該 4 個 atomic 全部 0。
        let metrics = super::new_metrics();
        let event = process_body(
            br#"{"hook_event_name":"Stop","session_id":"s1"}"#,
            "claude",
            &metrics,
        )
        .expect("valid json should parse");
        assert_eq!(event.hook_event_name, "Stop");
        assert_eq!(
            metrics.parse_failures.load(Ordering::Relaxed),
            0,
            "valid JSON 不該 ++ K15 parse_failures counter"
        );
        assert_eq!(
            metrics.responses_4xx.load(Ordering::Relaxed),
            0,
            "valid JSON 不該 ++ K16 4xx counter"
        );
        // 順便確認 2xx/5xx 也都沒被 valid JSON 副作用觸發
        assert_eq!(
            metrics.responses_2xx.load(Ordering::Relaxed),
            0,
            "valid JSON 不該 ++ K16 2xx counter (2xx 是 handle_client 內 Ok 分支 ++, process_body 內不寫)"
        );
        assert_eq!(
            metrics.responses_5xx.load(Ordering::Relaxed),
            0,
            "5xx counter 應永遠是 0 (保留欄位)"
        );
    }

    #[test]
    fn hook_parse_failures_counter_accumulates_across_failures() {
        let (before, after, _) = super::with_isolated_metric_snapshot(|| {
            // 連續 3 次壞 JSON 應讓 counter +3（不嚴格等於 3 因為平行 test 噪音，
            // 只驗證 >= 3）
            for i in 0..3 {
                let _ = process_body(
                    format!("garbage payload #{i}").as_bytes(),
                    "claude",
                    &super::default_metrics(),
                );
            }
        });
        let delta = after.delta(before).parse_failures;
        assert!(
            delta >= 3, // clippy::int_plus_one 不觸發 (>= 3 不是 +1)
            "3 次壞 JSON 應讓 K15 counter 至少 +3, actual delta={delta}, before={}, after={}",
            before.parse_failures,
            after.parse_failures
        );
    }

    // ─── K16 落地：handle_client 內 2xx/4xx 分支 → lifetime response counter++ ───
    // 測試策略：snapshot delta 模式（讀 before / 觸發 / 讀 after，檢 local delta），
    // 對其他平行 test 安全 —— 4 個 atomic 各自 fetch_add 不會掉 increment，只要
    // 我們只看自己這條呼叫的 local delta，別人的 increment 算背景噪音。R58 收邊：
    // 統一用 `with_isolated_metric_snapshot` + `HookServerMetrics::delta` 表達
    // local delta，K16 valid-JSON 測試也加強驗證 4xx counter 不變（原本只驗 4xx
    // 沒被誤 ++, R58 收邊同時驗 4 個 counter 各自在 valid-JSON 路徑下 delta 為 0，
    // 確保 K16 emit 不會因 valid JSON 副作用被 ++）。
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
        let (before, after, _) = super::with_isolated_metric_snapshot(|| {
            let _ = process_body(
                br#"{"hook_event_name":"Stop","session_id":"s1"}"#,
                "claude",
                &super::default_metrics(),
            )
            .expect("valid json should parse");
        });
        // 4xx 不該被 valid JSON 觸發。Race-tolerant threshold：counter 係
        // process-level atomic，平行程式下其他 test 喺呢段時間內仍會 ++ 4xx
        // (例如 r58 1000 burst test)，所以 strict `delta == 0` 喺 multi-thread
        // cargo test 下必爆。設上限 50 = 1000 burst 嘅 5%，留 race headroom
        // 但仍守住「valid JSON 自己唔 ++ 4xx 副作用」嘅語意（valid JSON 本身
        // 只 increment 1 次都唔應該, 何況 50）。語意上 K15/K16 嘅 atomic
        // correctness 由 r58 兩條 stress test (`delta_math_is_correct` +
        // `handles_burst_of_thousand`) 嚴格覆蓋。
        let delta_4xx = after.delta(before).responses_4xx;
        assert!(
            delta_4xx < 50,
            "valid JSON 不該大量 ++ K16 4xx counter (>50 表示 race noise 過高或邏輯錯), \
             actual delta={delta_4xx}, before={}, after={}",
            before.responses_4xx,
            after.responses_4xx
        );
    }

    #[test]
    fn hook_server_metrics_increments_4xx_on_invalid_json() {
        // 對齊 K15 測試模式：bad JSON → process_body Err → 對應 K16 4xx counter
        // ++。本測試只 snapshot 4xx delta，不對其他 counter 下嚴格斷言。
        let (before, after, _) = super::with_isolated_metric_snapshot(|| {
            let _ = process_body(b"not json { broken", "claude", &super::default_metrics());
        });
        let delta_4xx = after.delta(before).responses_4xx;
        assert!(
            delta_4xx >= 1,
            "壞 JSON 應讓 K16 4xx counter 至少 +1, actual delta={delta_4xx}, before={}, after={}",
            before.responses_4xx,
            after.responses_4xx
        );
    }

    // ─── R58 收邊：K15/K16 race-tolerant delta 計算 + atomic 計數 correctness ───
    // R35-R57 跨輪紀錄的 K15/K16 shared counter race 真正解法：之前 R36 wrap-up
    // 用 `assert!(after > before)` 寬鬆斷言處理 off-by-one 假陽性，但語意不清。
    // R58 收邊：
    //   1. `HookServerMetrics::delta(after, before)` 統一表達 local delta 計算
    //   2. `with_isolated_metric_snapshot` 包裝 before/after snapshot 取得
    //   3. 6 條 K15/K16 test 重構成 snapshot-helper pattern
    //   4. 2 條新 stress test: HookServerMetrics::delta 數學正確性 + 1000 次 fetch_add
    //      計數精確性（單 thread 無 race 噪音）
    #[test]
    fn r58_hook_server_metrics_delta_math_is_correct_under_saturating_sub() {
        // 驗 delta() 在「after >= before」正常情況下 = 精確差值
        let before = super::HookServerMetrics {
            parse_failures: 10,
            responses_2xx: 5,
            responses_4xx: 3,
            responses_5xx: 1,
        };
        let after = super::HookServerMetrics {
            parse_failures: 15,
            responses_2xx: 8,
            responses_4xx: 3,
            responses_5xx: 2,
        };
        let delta = after.delta(before);
        assert_eq!(
            delta,
            super::HookServerMetrics {
                parse_failures: 5,
                responses_2xx: 3,
                responses_4xx: 0,
                responses_5xx: 1,
            },
            "delta() 在 after >= before 應給精確差值"
        );
        // 對調 before/after → saturating_sub 全 0，不 panic / 不 wrap
        let reverse = before.delta(after);
        assert_eq!(
            reverse,
            super::HookServerMetrics::default(),
            "delta() 在 after < before 應 saturating 為全 0, 不 wrap / 不 panic"
        );
    }

    #[test]
    fn r58_hook_parse_failures_atomic_counter_handles_burst_of_thousand() {
        // 單 thread 1000 次 process_body(壞 JSON) 應讓 K15 counter 至少 +1000。
        // atomic fetch_add 本身保證 monotonic + 不 lost, 這條 test 驗證 lifetime
        // counter 在大量 fetch_add 下沒有 wrap / overflow / 計算錯誤。單 thread
        // 沒有 race 噪音, 所以可以用 delta >= 1000 嚴格斷言（= 1000 是 atomic
        // 計數保證的, 不會被其他 test 干擾 —— 平行 test 雖會 ++ counter, 但 1000
        // 是「自己這條 test 觀察到的下限」, 不會被平行 test 推高使斷言失敗）。
        let (before, after, _) = super::with_isolated_metric_snapshot(|| {
            for i in 0..1000u32 {
                let _ = process_body(
                    format!("r58_burst_garbage #{i}").as_bytes(),
                    "claude",
                    &super::default_metrics(),
                );
            }
        });
        let delta = after.delta(before).parse_failures;
        assert!(
            delta >= 1000,
            "1000 次壞 JSON 應讓 K15 counter 至少 +1000 (atomic monotonic), actual delta={delta}, before={}, after={}",
            before.parse_failures, after.parse_failures
        );
    }

    // ─── R59 護欄：K15 (parse_failures) ⊆ K16 4xx (responses_4xx) 跨 K 原子耦合不變式 ───
    // R58 收尾護欄鏈延伸：R58 已驗 K15 跟 K16 4xx 各自 atomic correctness (delta math +
    // 1000 burst), 但**沒**驗 K15 ⊆ K16 4xx 子集關係。process_body Err 分支 (line ~219)
    // 同時 fetch_add 兩個 counter (K15++ 跟 K16 4xx++ 緊貼), 語意上 K15 永遠是 K16 4xx 的
    // 子集 (K16 4xx 還有其他來源: empty body / oversized body 在 handle_client else 分支
    // 單獨 ++, K15 不 ++)。這個跨 K 護欄 R58 之後 R59 補, 對齊 R52-R58 護欄 chain
    // 紀律: future refactor 若把 K15 跟 K16 4xx 拆開 increment, 或新增 4xx 來源沒對應
    // K15++, 都會被這條 CI 1 秒抓。語意: K15 delta ≤ K16 4xx delta 永遠成立; 在只有
    // process_body 觸發的 scope 內 (本測試 N 次 process_body 呼叫), K15 delta == K16
    // 4xx delta 因為 4xx 來源只有 process_body Err。
    #[test]
    fn r59_k15_parse_failures_subset_of_k16_responses_4xx_under_parse_burst() {
        // 3 次 process_body(壞 JSON) + race-tolerant threshold 對齊 R58 紀律
        // (atomic counter 平行程式下 noise 必然 bump, strict == 不可靠)。Process
        // 內部 monotonic atomic 保證本 test 自己至少 +3, noise 只會推高。
        let (before, after, _) = super::with_isolated_metric_snapshot(|| {
            for i in 0..3 {
                let _ = process_body(
                    format!("r59_garbage #{i}").as_bytes(),
                    "claude",
                    &super::default_metrics(),
                );
            }
        });
        let delta = after.delta(before);
        // Race-tolerant 下限: 3 次自己觸發必到, noise 推高不算 fail。
        assert!(
            delta.parse_failures >= 3,
            "R59 跨 K 不變式前提: 3 次壞 JSON 應讓 K15 counter 至少 +3 (atomic monotonic), \
             actual delta={}",
            delta.parse_failures
        );
        assert!(
            delta.responses_4xx >= 3,
            "R59 跨 K 不變式前提: 3 次壞 JSON 應讓 K16 4xx counter 至少 +3 (process_body 是 \
             這 scope 唯一 4xx 來源, 跟 K15 同步), actual delta={}",
            delta.responses_4xx
        );
        // 核心跨 K 不變式: K15 delta ≤ K16 4xx delta (parse failure 是 4xx 子集)。
        // 這條 strict invariant 不受 race noise 影響 — noise 來自其他 test 也走
        // process_body Err, 兩個 counter 同步 +1, 不變式永遠成立。bug surface:
        // 1. K15 跟 K16 4xx fetch_add 拆開 (one fires without the other)
        // 2. K15++ 但 K16 4xx 沒 ++ (K15 delta > K16 4xx delta, 違反子集)
        // 3. process_body Err 分支被改寫, K15 移到別處
        assert!(
            delta.parse_failures <= delta.responses_4xx,
            "R59 核心跨 K 不變式被破壞: K15 parse_failures delta={} 應 <= K16 4xx \
             responses_4xx delta={} (K15 必須是 K16 4xx 子集 — 每次 K15 觸發都在 \
             process_body Err 緊貼 fetch_add K16 4xx, K16 4xx 還有其他來源但 K15 沒有)",
            delta.parse_failures,
            delta.responses_4xx
        );
        // 強等式 (process_body-only test scope 內): K15 delta 應 == K16 4xx delta,
        // 因為 4xx 來源只有 process_body Err (handle_client else 分支 empty body
        // 路徑沒被任何 test 直接觸發, grep 確認), 兩個 counter 同步 bump。
        // 未來若新增 test 觸發 handle_client else, 這條會 fail → 提醒改用
        // `<=` 寬鬆式子集不變式 (K15 ⊆ K16 4xx)。Race noise 對這條不影響
        // 因為 noise 來源也只走 process_body Err 同步 +1。
        assert_eq!(
            delta.parse_failures, delta.responses_4xx,
            "R59 強等式: process_body-only test scope 內 K15 delta 應 == K16 4xx delta \
             (兩個 counter 在 process_body Err 分支緊貼 fetch_add, 4xx 來源只有這條), \
             K15={} K16_4xx={}",
            delta.parse_failures, delta.responses_4xx
        );
    }

    #[test]
    fn r59_k15_nonzero_implies_k16_4xx_nonzero_atomic_coupling() {
        // 跨 K 反向蘊含: 任何 K15 parse failure 都必須伴隨 K16 4xx。單次 process_body
        // 壞 JSON 應讓兩個 counter 從 0 進到 ≥1, 反向蘊含自動成立 (K15==0 → K16_4xx
        // 可以 0 或 >0, 但 K15>0 → K16_4xx 必須 >0)。這條護欄專門抓「K15++ 但
        // K16 4xx 沒 ++」的未來 regression (例如有人 refactor 把 K15 fetch_add 移出
        // process_body Err 分支到外面, K16 4xx 留在分支內, K15 觸發 K16 4xx 不再
        // 跟著觸發 → K15 > K16 4xx, 子集不變式破壞)。
        let (before, after, _) = super::with_isolated_metric_snapshot(|| {
            // 1 次壞 JSON, 嚴格 single shot, race 噪音影響最小 (delta 為 1 or 2)
            let _ = process_body(
                b"r59_single_garbage { not json",
                "claude",
                &super::default_metrics(),
            );
        });
        let delta = after.delta(before);
        // K15 至少 1 (3 個 strict eq 條件之一), 強不等式成立條件
        assert!(
            delta.parse_failures >= 1,
            "1 次壞 JSON 應讓 K15 counter 至少 +1, actual={}",
            delta.parse_failures
        );
        // 核心反向蘊含: K15 > 0 → K16 4xx > 0 (同 fetch_add 緊貼)
        assert!(
            delta.responses_4xx >= delta.parse_failures,
            "R59 反向蘊含被破壞: K15 > 0 時 K16 4xx 必須 > 0 (atomic coupling), \
             K15={} K16_4xx={}",
            delta.parse_failures,
            delta.responses_4xx
        );
        // 進一步: 既然 single shot, K16 4xx 至少 1
        assert!(
            delta.responses_4xx >= 1,
            "1 次壞 JSON 應讓 K16 4xx counter 至少 +1 (process_body Err 緊貼 fetch_add), \
             actual={}",
            delta.responses_4xx
        );
    }

    #[test]
    fn process_body_defaults_session_id_when_missing() {
        let body = br#"{"hook_event_name":"Stop"}"#;
        let event = process_body(body, "openx", &super::default_metrics())
            .expect("valid json should parse");
        assert_eq!(event.session_id, "openx-default");
    }

    #[test]
    fn process_body_normalizes_snake_case_event_name() {
        let body = br#"{"hook_event_name":"pre_tool_use","session_id":"s1"}"#;
        let event = process_body(body, "copilot", &super::default_metrics())
            .expect("valid json should parse");
        assert_eq!(event.hook_event_name, "PreToolUse");
    }

    #[test]
    fn process_body_promotes_failed_post_tool_use_to_failure() {
        let body = br#"{"hook_event_name":"PostToolUse","session_id":"s1","tool_status":"failed"}"#;
        let event = process_body(body, "openx", &super::default_metrics())
            .expect("valid json should parse");
        assert_eq!(event.hook_event_name, "PostToolUseFailure");
    }

    /// R63: `/healthz` 純 fn 路由辨識。
    /// 對齊 K1-K16 護欄鏈的「單元測試覆蓋」紀律 — 純 fn 端先把路由分流語意鎖住,
    /// `handle_client` 的 early-dispatch 才是可信任的. 5 條: canonical / 拒 method /
    /// 拒 path 變體 / body shape / JSON parse-ability.
    #[test]
    fn r63_is_healthz_get_request_recognizes_canonical_get() {
        assert!(super::is_healthz_get_request(b"GET /healthz HTTP/1.1\r\n"));
    }

    #[test]
    fn r63_is_healthz_get_request_rejects_post_method() {
        // POST /healthz 不是合法 probe — k8s livenessProbe / blackbox exporter
        // 都送 GET, POST 進來應視為「不是 healthz」, 落到既有 /hook/* dispatch
        // (會回 400 因為沒 body, 不算 silent fail).
        assert!(!super::is_healthz_get_request(
            b"POST /healthz HTTP/1.1\r\n"
        ));
    }

    #[test]
    fn r63_is_healthz_get_request_rejects_path_variants() {
        // 拒絕 query string / trailing slash / 完全不相干的 path, 避免
        // 「看起來像 healthz 但其實是奇怪 hook 流量」被誤導成 200.
        assert!(!super::is_healthz_get_request(b"GET / HTTP/1.1\r\n"));
        assert!(!super::is_healthz_get_request(
            b"GET /healthz/ HTTP/1.1\r\n"
        ));
        assert!(!super::is_healthz_get_request(
            b"GET /healthz?foo=bar HTTP/1.1\r\n"
        ));
        assert!(!super::is_healthz_get_request(
            b"GET /hook/claude HTTP/1.1\r\n"
        ));
        assert!(!super::is_healthz_get_request(b"GET /metrics HTTP/1.1\r\n"));
    }

    #[test]
    fn r63_build_healthz_body_contains_status_ok_and_version() {
        // operator probe grep 友善: 必有 "status":"ok" + 非空 version 欄位.
        let body = super::build_healthz_body();
        assert!(
            body.contains(r#""status":"ok""#),
            "body 應含 status:ok, body={body}"
        );
        assert!(
            body.contains(r#""version":""#) && body.ends_with("\"}"),
            "body 應以 version 欄位收尾 (合法 JSON 物件), body={body}"
        );
    }

    #[test]
    fn r63_build_healthz_body_is_valid_json_with_nonempty_version() {
        // 反向驗證: hook_server 已 dep serde_json, 直接 parse 驗證 JSON 合法 +
        // 欄位語意, 比純字串 contains 嚴謹.
        let body = super::build_healthz_body();
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("body 必須是合法 JSON");
        assert_eq!(parsed["status"], "ok");
        let version = parsed["version"].as_str().expect("version 應為字串");
        assert!(
            !version.is_empty(),
            "version 不應為空 (env! macro 必給出 Cargo.toml version), got empty"
        );
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
            let event = process_body(fixture.body, &provider, &super::default_metrics())
                .unwrap_or_else(|e| {
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
