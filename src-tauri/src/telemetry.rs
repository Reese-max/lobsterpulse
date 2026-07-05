//! OTel GenAI runtime emit — `otel-genai-runtime-emit-2026-q3` Phase 2/3 落地
//! (T-OGRE11 SDK init + T-OGRE12 Tauri command + T-OGRE13 emit 入口 +
//! T-OGRE14 provider mapping + T-OGRE15 護衛 mod)。
//!
//! Spec 對齊 (`openspec/changes/otel-genai-runtime-emit-2026-q3/specs/.../spec.md`):
//! - **OGRE-R1** SDK init contract: 讀 `OTEL_EXPORTER_OTLP_ENDPOINT` env var,
//!   預設 `http://localhost:4317` gRPC; init 失敗 fail-closed 回 `Err(OtelInitError)`,
//!   不 silent fallback 到 noop exporter。
//! - **OGRE-R2** 4 事件點 emit contract: `SessionManager::handle_event` 對
//!   SessionStart / UserPromptSubmit / PostToolUseFailure / SessionEnd 各 emit
//!   1 個獨立 span (span name 走 `gen_ai.client.*` 命名空間)。
//! - **OGRE-R3** provider mapping contract: 13 個 LobsterPulse provider id
//!   (4 本機 CLI + 9 OpenAB bot) → OTel `gen_ai.provider.name`; 未知 id
//!   fail-closed 回 `Err(ProviderMappingError::UnknownProvider)` 不 fallback。
//!
//! ## 設計決策
//!
//! - **Emit 走 `opentelemetry::global` tracer**: SDK 未 init 時 global 是 noop
//!   provider, emit 零成本 — 不影響既有 452 條護衛 test 的 handle_event 路徑。
//!   注意這不違反 OGRE-R1 fail-closed: fail-closed 約束的是「init 失敗不得假裝
//!   成功」, emit 端在未 init 時本來就該無害通過。
//! - **Runtime 依 CLAUDE.md「Metrics server 獨立 runtime」前例**: tonic gRPC
//!   channel 要求在 tokio runtime context 內建立 (否則 "no reactor running"
//!   panic), Tauri setup 不在 tokio context → 自建 `tokio::runtime::Runtime`
//!   + `rt.block_on` 建 exporter/provider, 之後 `std::mem::forget(rt)` 讓
//!   worker thread 常駐 (跟 hook server / metrics server 同款 pattern,
//!   **不能** `tokio::spawn`)。
//! - **codex_bot 映射衝突裁決**: design.md mapping 表寫 `codex_bot → openai
//!   (bot alias)`, 但 spec.md OGRE-R3 明文「9 OpenAB bot → `custom.<bot_id>`」
//!   且 OGRE-R3-S3 MUST NOT 用本機 CLI 標準名 (anthropic/openai/google/github)
//!   對 OpenAB bot id。spec.md 自我宣告 source of truth → 本檔採
//!   `codex_bot → custom.codex_bot`, design.md 表為過時草案。

use opentelemetry::global;
use opentelemetry::trace::{Span, Tracer};
use opentelemetry::KeyValue;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use std::sync::atomic::{AtomicBool, Ordering};

/// OGRE-R1: OTLP endpoint env var 名 (OTel 官方標準命名)。
pub const OTLP_ENDPOINT_ENV: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
/// OGRE-R1-S3: env var 未設時的預設 gRPC endpoint。
pub const DEFAULT_OTLP_ENDPOINT: &str = "http://localhost:4317";

/// global tracer 名 (instrumentation scope)。
const TRACER_NAME: &str = "lobsterpulse";
/// OTel Resource `service.name`。
const SERVICE_NAME: &str = "lobster-pulse";

// OGRE-R2 span name 常數 (4 事件點, 對齊 design.md E1-E4)。
pub const SPAN_SESSION_CREATE: &str = "gen_ai.client.session.create";
pub const SPAN_USER_MESSAGE: &str = "gen_ai.client.user.message";
pub const SPAN_TOOL_ERROR: &str = "gen_ai.client.tool.error";
pub const SPAN_SESSION_END: &str = "gen_ai.client.session.end";

// Span attribute key 常數。`gen_ai.provider.name` 在
// opentelemetry-semantic-conventions 0.31 尚無官方常數 (semconv genai repo
// PR #2046 改名 gen_ai.system → gen_ai.provider.name, Rust crate 未同步),
// 依 spec 直接定義字串常數; `error.type` 用官方 stable 常數。
pub const ATTR_PROVIDER_NAME: &str = "gen_ai.provider.name";
pub const ATTR_SESSION_COUNT: &str = "gen_ai.client.session.count";
pub const ATTR_TOKEN_USAGE: &str = "gen_ai.client.token.usage";
pub const ATTR_TOOL_NAME: &str = "gen_ai.client.tool.name";
pub const ATTR_OPERATION_DURATION: &str = "gen_ai.client.operation.duration";
pub const ATTR_ERROR_TYPE: &str = opentelemetry_semantic_conventions::attribute::ERROR_TYPE;
/// 私有 namespace attribute: session id (trace 關聯 + 護衛 test 平行過濾用,
/// spec 各 scenario 是「at least」語意, 允許附加 attribute)。
pub const ATTR_SESSION_ID: &str = "lobsterpulse.session.id";

/// OGRE-R1-S2: SDK init fail-closed 錯誤 (不 panic、不 silent fallback)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtelInitError {
    /// endpoint 非 `http(s)://host[:port][/path]` 形狀 → init 前 fail-fast。
    /// (tonic `Endpoint::from_shared` 對 "not-a-url" 這種無 scheme 字串
    /// 會 parse 成相對 URI 直到 connect 才炸, 所以必須自己前置驗證。)
    InvalidEndpoint(String),
    /// tokio runtime 建立失敗。
    RuntimeBuild(String),
    /// OTLP exporter build 失敗。
    ExporterBuild(String),
    /// 重複 init (global provider 只能 set 一次, 第二次呼叫不得假裝成功)。
    AlreadyInitialized,
}

impl std::fmt::Display for OtelInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEndpoint(ep) => write!(f, "invalid OTLP endpoint `{ep}`"),
            Self::RuntimeBuild(e) => write!(f, "tokio runtime build failed: {e}"),
            Self::ExporterBuild(e) => write!(f, "OTLP exporter build failed: {e}"),
            Self::AlreadyInitialized => write!(f, "OTel SDK already initialized"),
        }
    }
}

/// OGRE-R3-S2: 未知 provider id fail-closed 錯誤 (不 panic、不 fallback default)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderMappingError {
    UnknownProvider(String),
}

impl std::fmt::Display for ProviderMappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProvider(id) => write!(f, "unknown provider id `{id}`"),
        }
    }
}

/// OGRE-R3: 13 條 provider id → OTel `gen_ai.provider.name` 靜態 lookup table。
///
/// 大小必須 == `hook_server::KNOWN_PROVIDERS.len()` (13), 增刪 provider 必須
/// 同 commit 更新兩處 + 護衛 test (OGRE-R3-S1)。
/// 9 隻 OpenAB bot 一律 `custom.<bot_id>` (OGRE-R3-S3, 含 codex_bot —
/// 見檔頭「codex_bot 映射衝突裁決」)。
pub const PROVIDER_OTEL_NAMES: &[(&str, &str)] = &[
    // 4 本機 CLI → OTel 標準名
    ("claude", "anthropic"),
    ("codex", "openai"),
    ("copilot", "github"),
    ("gemini", "google"),
    // 9 OpenAB bot → custom.<bot_id>
    ("cicx", "custom.cicx"),
    ("gitx", "custom.gitx"),
    ("giminix", "custom.giminix"),
    ("codex_bot", "custom.codex_bot"),
    ("openx", "custom.openx"),
    ("irisx_bot", "custom.irisx_bot"),
    ("grokx", "custom.grokx"),
    ("lpbot", "custom.lpbot"),
    ("mimo", "custom.mimo"),
];

/// OGRE-R3: provider id → OTel `gen_ai.provider.name`。
/// 未知 id 回 `Err(UnknownProvider)`, 不 panic 不 fallback (OGRE-R3-S2)。
pub fn provider_mapping(provider_id: &str) -> Result<&'static str, ProviderMappingError> {
    PROVIDER_OTEL_NAMES
        .iter()
        .find(|(id, _)| *id == provider_id)
        .map(|(_, otel_name)| *otel_name)
        .ok_or_else(|| ProviderMappingError::UnknownProvider(provider_id.to_string()))
}

/// OGRE-R1-S1/S3: endpoint 解析純函式 (env 讀取跟解析拆開, 護衛 test 不碰
/// process env 避免平行 test 互踩)。空白/空字串視為未設。
pub fn resolve_otlp_endpoint_from(raw_env: Option<&str>) -> String {
    raw_env
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_OTLP_ENDPOINT.to_string())
}

/// 讀 `OTEL_EXPORTER_OTLP_ENDPOINT` env var, 未設回預設值。
pub fn resolve_otlp_endpoint() -> String {
    resolve_otlp_endpoint_from(std::env::var(OTLP_ENDPOINT_ENV).ok().as_deref())
}

/// OGRE-R1-S2: endpoint fail-fast 驗證。要求 `http://` 或 `https://` scheme
/// + 非空 host + (若有) port 可 parse 成 u16。不引入 url crate — 這裡只做
/// fail-closed 形狀檢查, 細節交給 tonic connect。
pub fn validate_endpoint(endpoint: &str) -> Result<(), OtelInitError> {
    let invalid = || OtelInitError::InvalidEndpoint(endpoint.to_string());
    let rest = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .ok_or_else(invalid)?;
    let authority = rest.split('/').next().unwrap_or("");
    let mut parts = authority.splitn(2, ':');
    let host = parts.next().unwrap_or("");
    if host.is_empty() {
        return Err(invalid());
    }
    if let Some(port) = parts.next() {
        port.parse::<u16>().map_err(|_| invalid())?;
    }
    Ok(())
}

/// init 只允許一次 (global provider set 兩次會把第一個 provider 換掉,
/// 已 emit 的 batch 佇列會蒸發 — fail-closed 拒絕)。
static SDK_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// OGRE-R1: OTel SDK init。讀 env var → fail-fast 驗證 endpoint → 自建
/// tokio runtime 內 build tonic exporter + `SdkTracerProvider` →
/// `global::set_tracer_provider`。成功回 endpoint 字串 (給 caller log)。
///
/// 失敗 fail-closed: 回 `Err`, global provider 保持 noop, **不**假裝成功
/// (OGRE-R1-S2)。runtime 以 `std::mem::forget` 常駐 (見檔頭設計決策)。
pub fn init_otel_sdk() -> Result<String, OtelInitError> {
    let endpoint = resolve_otlp_endpoint();
    validate_endpoint(&endpoint)?;

    if SDK_INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(OtelInitError::AlreadyInitialized);
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        SDK_INITIALIZED.store(false, Ordering::SeqCst);
        OtelInitError::RuntimeBuild(e.to_string())
    })?;

    // tonic exporter 必須在 tokio runtime context 內 build (捕捉 runtime
    // handle 供 BatchSpanProcessor 背景 thread export 用)。
    let provider = rt.block_on(async {
        let exporter = SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&endpoint)
            .build()
            .map_err(|e| OtelInitError::ExporterBuild(e.to_string()))?;
        Ok::<_, OtelInitError>(
            SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .with_resource(Resource::builder().with_service_name(SERVICE_NAME).build())
                .build(),
        )
    });
    let provider = match provider {
        Ok(p) => p,
        Err(e) => {
            SDK_INITIALIZED.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };

    global::set_tracer_provider(provider);
    // runtime 常駐: worker thread 保活給 tonic channel 當 reactor
    // (CLAUDE.md「Metrics server 獨立 runtime」同款, 不能 drop)。
    std::mem::forget(rt);
    Ok(endpoint)
}

/// T-OGRE12: Tauri command。前端 (或 owner M 手動) 觸發 OTLP exporter 啟動。
/// `lib.rs::setup()` 啟動時也走同一個 `init_otel_sdk()` 入口。
#[tauri::command]
pub fn start_otlp_exporter() -> Result<String, String> {
    init_otel_sdk().map_err(|e| e.to_string())
}

/// OGRE-R2: 4 事件點的 span payload (caller = `SessionManager::handle_event`)。
#[derive(Debug, Clone, Copy)]
pub enum SessionSpanEvent<'a> {
    /// E1 → `gen_ai.client.session.create` + session.count
    SessionStart { session_count: u64 },
    /// E2 → `gen_ai.client.user.message` + token.usage (input)
    UserPromptSubmit { tokens_input: u64 },
    /// E3 → `gen_ai.client.tool.error` + tool.name + error.type
    PostToolUseFailure {
        tool_name: &'a str,
        error_type: &'a str,
    },
    /// E4 → `gen_ai.client.session.end` + operation.duration (ms)
    SessionEnd { duration_ms: u64 },
}

/// u64 → i64 飽和轉換 (OTel `KeyValue` 整數只吃 i64; token/duration 現實
/// 量級不會超過 i64::MAX, clamp 是防禦性下界)。
fn to_i64(v: u64) -> i64 {
    v.min(i64::MAX as u64) as i64
}

/// OGRE-R2: emit 1 個獨立 span。每事件 1 span、不合併 batch span
/// (OGRE-R2-S3); root span 天然拿到獨立 trace_id。
///
/// 未知 provider fail-closed: 回 `Err`, 不 emit 任何 span (OGRE-R3-S2 延伸;
/// caller 端 `let _ =` 吞掉 — 監控 emit 不能反噬事件處理主路徑)。
/// SDK 未 init 時 global tracer 是 noop, 本函式無害通過。
pub fn emit_session_event_span(
    provider: &str,
    session_id: &str,
    event: SessionSpanEvent<'_>,
) -> Result<(), ProviderMappingError> {
    let otel_name = provider_mapping(provider)?;
    let tracer = global::tracer(TRACER_NAME);
    let span_name = match event {
        SessionSpanEvent::SessionStart { .. } => SPAN_SESSION_CREATE,
        SessionSpanEvent::UserPromptSubmit { .. } => SPAN_USER_MESSAGE,
        SessionSpanEvent::PostToolUseFailure { .. } => SPAN_TOOL_ERROR,
        SessionSpanEvent::SessionEnd { .. } => SPAN_SESSION_END,
    };
    let mut span = tracer.start(span_name);
    span.set_attribute(KeyValue::new(ATTR_PROVIDER_NAME, otel_name));
    span.set_attribute(KeyValue::new(ATTR_SESSION_ID, session_id.to_string()));
    match event {
        SessionSpanEvent::SessionStart { session_count } => {
            span.set_attribute(KeyValue::new(ATTR_SESSION_COUNT, to_i64(session_count)));
        }
        SessionSpanEvent::UserPromptSubmit { tokens_input } => {
            span.set_attribute(KeyValue::new(ATTR_TOKEN_USAGE, to_i64(tokens_input)));
        }
        SessionSpanEvent::PostToolUseFailure {
            tool_name,
            error_type,
        } => {
            span.set_attribute(KeyValue::new(ATTR_TOOL_NAME, tool_name.to_string()));
            span.set_attribute(KeyValue::new(ATTR_ERROR_TYPE, error_type.to_string()));
        }
        SessionSpanEvent::SessionEnd { duration_ms } => {
            span.set_attribute(KeyValue::new(ATTR_OPERATION_DURATION, to_i64(duration_ms)));
        }
    }
    span.end();
    Ok(())
}

// ---------------------------------------------------------------------
// T-OGRE15: telemetry::tests 護衛 mod (K42 chain 20 → 21)。
//
// R97 飽和契約後開新護衛 mod 的架構理由 (對齊 spec.md「對齊 R97 紅線」段
// 預告的 +4 例外): 本 mod 守的 invariant 跨 `session.rs::handle_event`
// (emit 呼叫點) ↔ `hook_server.rs::KNOWN_PROVIDERS` (13 provider SSoT) ↔
// `telemetry.rs` (mapping 表 + span 契約) 3 個 mod 邊界, 跟 R122
// `timeline::tests` 例外同性質 — 任何單一既有 mod 的護衛都蓋不住
// 「KNOWN_PROVIDERS 加 provider 但 mapping 表沒加 → runtime 靜默丟 span」
// 這條跨界失效路徑。
//
// 平行 test 策略: global tracer provider 是 process 級單例, 本 mod 需要
// span 捕捉的 test 共用一個 OnceLock InMemorySpanExporter (只 install 一次),
// 各 test 用唯一 session_id 過濾自己的 span — 其他 mod 的 handle_event
// test 平行跑進來的 span 不會污染斷言。
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_event::HookEvent;
    use crate::hook_server::KNOWN_PROVIDERS;
    use crate::session::SessionManager;
    use opentelemetry_sdk::trace::{InMemorySpanExporter, SimpleSpanProcessor};
    use std::sync::OnceLock;

    /// 共用 global test provider (見 mod 頭「平行 test 策略」)。
    fn install_test_exporter() -> InMemorySpanExporter {
        static EXPORTER: OnceLock<InMemorySpanExporter> = OnceLock::new();
        EXPORTER
            .get_or_init(|| {
                let exporter = InMemorySpanExporter::default();
                let provider = SdkTracerProvider::builder()
                    .with_span_processor(SimpleSpanProcessor::new(exporter.clone()))
                    .build();
                global::set_tracer_provider(provider);
                exporter
            })
            .clone()
    }

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

    fn spans_for_session(
        exporter: &InMemorySpanExporter,
        sid: &str,
    ) -> Vec<opentelemetry_sdk::trace::SpanData> {
        exporter
            .get_finished_spans()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| {
                s.attributes.iter().any(|kv| {
                    kv.key.as_str() == ATTR_SESSION_ID && kv.value.as_str() == sid
                })
            })
            .collect()
    }

    fn attr<'a>(
        span: &'a opentelemetry_sdk::trace::SpanData,
        key: &str,
    ) -> Option<&'a opentelemetry::Value> {
        span.attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| &kv.value)
    }

    /// OGRE-R3-S1: mapping 表大小 = 13 且跟 KNOWN_PROVIDERS SSoT 完全對齊,
    /// 每個 id 映射到非空 OTel 名。增刪 provider 沒同步兩表 → 這裡炸。
    #[test]
    fn provider_mapping_size_is_13_matching_known_providers() {
        assert_eq!(
            PROVIDER_OTEL_NAMES.len(),
            13,
            "mapping 表必須恰好 13 條 (OGRE-R3-S1)"
        );
        assert_eq!(
            PROVIDER_OTEL_NAMES.len(),
            KNOWN_PROVIDERS.len(),
            "mapping 表大小必須對齊 hook_server::KNOWN_PROVIDERS SSoT"
        );
        for id in KNOWN_PROVIDERS {
            let otel_name = provider_mapping(id)
                .unwrap_or_else(|e| panic!("KNOWN_PROVIDERS `{id}` 缺 mapping: {e}"));
            assert!(!otel_name.is_empty(), "`{id}` 的 OTel 名不得為空");
        }
    }

    /// OGRE-R3-S3: 4 本機 CLI → OTel 標準名; 9 OpenAB bot → `custom.<bot_id>`
    /// 且 MUST NOT 用本機 CLI 標準名。
    #[test]
    fn provider_mapping_local_cli_standard_names_and_bots_custom_namespace() {
        assert_eq!(provider_mapping("claude"), Ok("anthropic"));
        assert_eq!(provider_mapping("codex"), Ok("openai"));
        assert_eq!(provider_mapping("copilot"), Ok("github"));
        assert_eq!(provider_mapping("gemini"), Ok("google"));
        let standard = ["anthropic", "openai", "github", "google"];
        for bot in crate::OPENAB_BOT_IDS {
            let otel_name = provider_mapping(bot).expect("OpenAB bot 必須有 mapping");
            assert_eq!(
                otel_name,
                format!("custom.{bot}"),
                "OpenAB bot `{bot}` 必須映射到 custom.<bot_id> (OGRE-R3-S3)"
            );
            assert!(
                !standard.contains(&otel_name),
                "OpenAB bot `{bot}` 不得用本機 CLI 標準名 (OGRE-R3-S3)"
            );
        }
    }

    /// OGRE-R3-S2: 未知 provider id → Err(UnknownProvider), 不 panic 不 fallback。
    #[test]
    fn provider_mapping_unknown_id_returns_error_without_fallback() {
        assert_eq!(
            provider_mapping("no_such_provider"),
            Err(ProviderMappingError::UnknownProvider(
                "no_such_provider".to_string()
            ))
        );
        assert_eq!(
            provider_mapping(""),
            Err(ProviderMappingError::UnknownProvider(String::new()))
        );
    }

    /// OGRE-R1-S1 + S3: env var 有值用 env var, 未設/空白 fallback 預設
    /// `http://localhost:4317` (純函式驗, 不碰 process env 避免平行 test 互踩)。
    #[test]
    fn resolve_endpoint_prefers_env_var_and_defaults_to_localhost_4317() {
        assert_eq!(
            resolve_otlp_endpoint_from(Some("http://otel-collector:4317")),
            "http://otel-collector:4317"
        );
        assert_eq!(resolve_otlp_endpoint_from(None), DEFAULT_OTLP_ENDPOINT);
        assert_eq!(resolve_otlp_endpoint_from(Some("")), DEFAULT_OTLP_ENDPOINT);
        assert_eq!(
            resolve_otlp_endpoint_from(Some("   ")),
            DEFAULT_OTLP_ENDPOINT
        );
        assert_eq!(DEFAULT_OTLP_ENDPOINT, "http://localhost:4317");
    }

    /// OGRE-R1-S2: 無效 endpoint fail-closed 回 InvalidEndpoint, 不 panic。
    /// "not-a-url" 這種無 scheme 字串是 spec 明文的必擋 case。
    #[test]
    fn validate_endpoint_fails_closed_on_invalid_and_accepts_valid() {
        for bad in ["not-a-url", "", "ftp://x:1", "http://", "http://host:notaport"] {
            assert_eq!(
                validate_endpoint(bad),
                Err(OtelInitError::InvalidEndpoint(bad.to_string())),
                "`{bad}` 必須 fail-closed"
            );
        }
        for good in [
            "http://localhost:4317",
            "https://otel-collector:4317",
            "http://127.0.0.1:4317/v1/traces",
            "http://collector.internal",
        ] {
            assert_eq!(validate_endpoint(good), Ok(()), "`{good}` 必須通過");
        }
    }

    /// OGRE-R2-S1: SessionStart → 恰好 1 個 `gen_ai.client.session.create`
    /// span, 至少帶 `gen_ai.provider.name` attribute (映射後值)。
    #[test]
    fn session_start_emits_span_with_provider_name_attribute() {
        let exporter = install_test_exporter();
        let sid = "ogre-test-session-start-e1";
        let mut mgr = SessionManager::new();
        mgr.handle_event(&ev("claude", sid, "SessionStart"));

        let spans = spans_for_session(&exporter, sid);
        assert_eq!(spans.len(), 1, "SessionStart 必須恰好 emit 1 個 span");
        assert_eq!(spans[0].name, SPAN_SESSION_CREATE);
        assert_eq!(
            attr(&spans[0], ATTR_PROVIDER_NAME).map(|v| v.as_str().into_owned()),
            Some("anthropic".to_string()),
            "span 必須帶映射後的 gen_ai.provider.name"
        );
        assert!(
            attr(&spans[0], ATTR_SESSION_COUNT).is_some(),
            "SessionStart span 必須帶 session.count attribute"
        );
    }

    /// OGRE-R2-S2: PostToolUseFailure → 恰好 1 個 `gen_ai.client.tool.error`
    /// span, 至少帶 provider.name + tool.name + error.type 3 個 attribute。
    #[test]
    fn post_tool_use_failure_emits_tool_error_span_with_3_attributes() {
        let exporter = install_test_exporter();
        let sid = "ogre-test-tool-error-e3";
        let mut mgr = SessionManager::new();
        let mut failure = ev("gemini", sid, "PostToolUseFailure");
        failure.tool_name = Some("shell".to_string());
        failure.error = Some("permission denied".to_string());
        mgr.handle_event(&failure);

        let spans = spans_for_session(&exporter, sid);
        assert_eq!(spans.len(), 1, "PostToolUseFailure 必須恰好 emit 1 個 span");
        assert_eq!(spans[0].name, SPAN_TOOL_ERROR);
        assert_eq!(
            attr(&spans[0], ATTR_PROVIDER_NAME).map(|v| v.as_str().into_owned()),
            Some("google".to_string())
        );
        assert_eq!(
            attr(&spans[0], ATTR_TOOL_NAME).map(|v| v.as_str().into_owned()),
            Some("shell".to_string())
        );
        assert_eq!(
            attr(&spans[0], ATTR_ERROR_TYPE).map(|v| v.as_str().into_owned()),
            Some("permission denied".to_string())
        );
    }

    /// OGRE-R2-S3: 同 session 依序 SessionStart / UserPromptSubmit /
    /// PostToolUseFailure / SessionEnd → 恰好 4 個獨立 span (依序、各自
    /// root span 獨立 trace_id, 不合併 batch span)。
    #[test]
    fn four_event_points_emit_four_independent_spans_in_order() {
        let exporter = install_test_exporter();
        let sid = "ogre-test-four-events-e1e2e3e4";
        let mut mgr = SessionManager::new();
        mgr.handle_event(&ev("codex_bot", sid, "SessionStart"));
        let mut prompt = ev("codex_bot", sid, "UserPromptSubmit");
        prompt.tokens_input = Some(123);
        mgr.handle_event(&prompt);
        let mut failure = ev("codex_bot", sid, "PostToolUseFailure");
        failure.tool_name = Some("exec".to_string());
        failure.error = Some("boom".to_string());
        mgr.handle_event(&failure);
        mgr.handle_event(&ev("codex_bot", sid, "SessionEnd"));

        let spans = spans_for_session(&exporter, sid);
        assert_eq!(spans.len(), 4, "4 事件點必須各 emit 1 個獨立 span");
        let names: Vec<&str> = spans.iter().map(|s| s.name.as_ref()).collect();
        assert_eq!(
            names,
            vec![
                SPAN_SESSION_CREATE,
                SPAN_USER_MESSAGE,
                SPAN_TOOL_ERROR,
                SPAN_SESSION_END
            ],
            "4 個 span 必須依事件順序 emit"
        );
        // 獨立 root span → 4 個 trace_id 互不相同 (不是合併 batch span)
        let trace_ids: std::collections::HashSet<[u8; 16]> = spans
            .iter()
            .map(|s| s.span_context.trace_id().to_bytes())
            .collect();
        assert_eq!(trace_ids.len(), 4, "4 個 span 必須有獨立 trace_id");
        // codex_bot 走 custom namespace (OGRE-R3-S3 runtime 驗證)
        assert_eq!(
            attr(&spans[0], ATTR_PROVIDER_NAME).map(|v| v.as_str().into_owned()),
            Some("custom.codex_bot".to_string())
        );
        // E2 token.usage / E4 operation.duration attribute 在位
        assert_eq!(
            attr(&spans[1], ATTR_TOKEN_USAGE),
            Some(&opentelemetry::Value::I64(123))
        );
        assert!(attr(&spans[3], ATTR_OPERATION_DURATION).is_some());
    }

    /// OGRE-R3-S2 延伸: 未知 provider 的事件不得 emit 任何 span
    /// (fail-closed 靜默跳過, 不污染 gen_ai.provider.name namespace)。
    #[test]
    fn unknown_provider_event_emits_no_span() {
        let exporter = install_test_exporter();
        let sid = "ogre-test-unknown-provider-no-span";
        let mut mgr = SessionManager::new();
        mgr.handle_event(&ev("totally_unknown_provider", sid, "SessionStart"));
        assert!(
            spans_for_session(&exporter, sid).is_empty(),
            "未知 provider 不得 emit span (fail-closed)"
        );
        assert_eq!(
            emit_session_event_span(
                "totally_unknown_provider",
                sid,
                SessionSpanEvent::SessionStart { session_count: 1 }
            ),
            Err(ProviderMappingError::UnknownProvider(
                "totally_unknown_provider".to_string()
            ))
        );
    }
}
