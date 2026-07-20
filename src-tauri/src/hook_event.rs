use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Raw event as received from any CLI — fields vary by provider
#[derive(Debug, Clone, Deserialize)]
pub struct RawHookEvent {
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, Value>,
}

/// Normalized event used internally
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookEvent {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    /// Stable tool call id — 同一個 tool invocation 跨 PreToolUse/PostToolUse 共用，
    /// 避免 UI 把同一次呼叫誤判成兩筆紀錄。
    #[serde(default)]
    pub tool_call_id: Option<String>,
    /// Tool call lifecycle status：running / completed / failed。
    #[serde(default)]
    pub tool_status: Option<String>,
    /// Agent token 累計輸入量（由 OpenAB ACP usage_update 帶進來）。
    #[serde(default)]
    pub tokens_input: Option<u64>,
    /// Agent token 累計輸出量。
    #[serde(default)]
    pub tokens_output: Option<u64>,
    /// 失敗事件的錯誤文字（PostToolUseFailure 用）
    #[serde(default)]
    pub error: Option<String>,
    /// 終端機候選 PID 鏈（server 端在 TCP 連線期間反查 hook 進程父鏈填入，
    /// 不來自 CLI payload——hook exe 不能重建所以 payload 動不了）。
    #[serde(default)]
    pub terminal_pids: Vec<u32>,
    /// UserPromptSubmit 當下實抓的分頁標題（前景視窗屬於本 session 終端機
    /// 鏈時才有值）。跳轉時優先用它選分頁，不靠猜專案名。
    #[serde(default)]
    pub tab_title: Option<String>,
}

impl RawHookEvent {
    /// Normalize raw fields from any CLI into a standard HookEvent
    pub fn normalize(self, provider: &str) -> HookEvent {
        let f = &self.fields;

        // session_id: try multiple field names
        let session_id = get_str(f, "session_id")
            .or_else(|| get_str(f, "sessionId"))
            .or_else(|| get_str(f, "session"))
            .unwrap_or_default();

        // hook_event_name: try multiple field names
        let hook_event_name = get_str(f, "hook_event_name")
            .or_else(|| get_str(f, "hookEventName"))
            .or_else(|| get_str(f, "event"))
            .or_else(|| get_str(f, "type"))
            .unwrap_or_default();

        // cwd: try multiple field names
        let cwd = get_str(f, "cwd")
            .or_else(|| get_str(f, "workingDirectory"))
            .or_else(|| get_str(f, "projectDir"));

        // prompt: try multiple field names
        let prompt = get_str(f, "prompt")
            .or_else(|| get_str(f, "initialPrompt"))
            .or_else(|| get_str(f, "input"))
            .or_else(|| get_str(f, "message"))
            .or_else(|| get_str(f, "userPrompt"));

        // tool_name
        let tool_name = get_str(f, "tool_name")
            .or_else(|| get_str(f, "toolName"))
            .or_else(|| get_str(f, "tool"))
            .or_else(|| get_str(f, "title"))
            // Claude tool_use blocks 用 `name` 當工具名稱（Anthropic 官方 schema）
            .or_else(|| get_str(f, "name"));

        let notification_type =
            get_str(f, "notification_type").or_else(|| get_str(f, "notificationType"));

        // tool_call_id — Claude tool_use blocks 用 `id`（Anthropic 官方 schema），
        // 部分 OpenAB bot 用 `tool_use_id` 區隔用途。少了這兩個 alias 會讓
        // PreToolUse/PostToolUse 配對 dedup 失效。
        let tool_call_id = get_str(f, "tool_call_id")
            .or_else(|| get_str(f, "toolCallId"))
            .or_else(|| get_str(f, "call_id"))
            .or_else(|| get_str(f, "callId"))
            .or_else(|| get_str(f, "id"))
            .or_else(|| get_str(f, "tool_use_id"));

        let tool_status = get_str(f, "tool_status")
            .or_else(|| get_str(f, "toolStatus"))
            .or_else(|| get_str(f, "status"))
            .map(|s| normalize_tool_status(&s));

        let tokens_input = f
            .get("tokens_input")
            .or_else(|| f.get("inputTokens"))
            .or_else(|| f.get("tokens_in"))
            .or_else(|| f.get("prompt_tokens"))
            .or_else(|| f.get("input_tokens"))
            .or_else(|| get_nested_value(f, "usage", "inputTokens"))
            .or_else(|| get_nested_value(f, "usage", "input_tokens"))
            // OpenAI/Codex style camelCase 變體
            .or_else(|| get_nested_value(f, "usage", "promptTokens"))
            .or_else(|| get_nested_value(f, "usage", "prompt_tokens"))
            .and_then(get_u64);
        let tokens_output = f
            .get("tokens_output")
            .or_else(|| f.get("outputTokens"))
            .or_else(|| f.get("tokens_out"))
            .or_else(|| f.get("completion_tokens"))
            .or_else(|| f.get("output_tokens"))
            .or_else(|| get_nested_value(f, "usage", "outputTokens"))
            .or_else(|| get_nested_value(f, "usage", "output_tokens"))
            .or_else(|| get_nested_value(f, "usage", "completion_tokens"))
            // OpenAI/Codex style camelCase 變體
            .or_else(|| get_nested_value(f, "usage", "completionTokens"))
            .and_then(get_u64);

        let failed_status = get_str(f, "status")
            .map(|s| normalize_tool_status(&s) == "failed")
            .unwrap_or(false);
        let error = get_str(f, "error")
            .or_else(|| get_str(f, "errorMessage"))
            .or_else(|| get_str(f, "error_message"))
            .or_else(|| get_str(f, "stderr"))
            .or_else(|| {
                get_str(f, "message").filter(|_| {
                    get_str(f, "hook_event_name").as_deref() == Some("PostToolUseFailure")
                        || get_str(f, "hookEventName").as_deref() == Some("PostToolUseFailure")
                })
            })
            .or_else(|| {
                if failed_status {
                    get_str(f, "message")
                        .or_else(|| get_str(f, "details"))
                        .or_else(|| get_str(f, "detail"))
                } else {
                    None
                }
            });

        HookEvent {
            provider: provider.to_string(),
            session_id,
            hook_event_name,
            cwd,
            tool_name,
            notification_type,
            prompt,
            tool_call_id,
            tool_status,
            tokens_input,
            tokens_output,
            error,
            terminal_pids: Vec::new(),
            tab_title: None,
        }
    }
}

fn get_str(map: &std::collections::HashMap<String, Value>, key: &str) -> Option<String> {
    // trim 統一去掉首尾空白：避免 `session_id: "  s1  "` 與 `"s1"` 被視為不同 session。
    // 空字串 / 純空白視為不存在（None），讓 `unwrap_or_default()` fallback 生效。
    map.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn get_nested_value<'a>(
    map: &'a std::collections::HashMap<String, Value>,
    parent: &str,
    child: &str,
) -> Option<&'a Value> {
    map.get(parent).and_then(|v| v.get(child))
}

fn get_u64(v: &Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    if let Some(s) = v.as_str() {
        return s.trim().parse::<u64>().ok();
    }
    if let Some(f) = v.as_f64() {
        if f.is_finite() && f >= 0.0 {
            return Some(f as u64);
        }
    }
    None
}

fn normalize_tool_status(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    match s.as_str() {
        "running" | "in_progress" | "in-progress" | "pending" => "running".to_string(),
        "ok" | "success" | "succeeded" | "done" | "completed" => "completed".to_string(),
        "error" | "failed" | "fail" | "aborted" | "cancelled" | "canceled" => "failed".to_string(),
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::{get_u64, normalize_tool_status, RawHookEvent};
    use serde_json::json;

    #[test]
    fn parse_u64_from_multiple_json_types() {
        assert_eq!(get_u64(&json!(123)), Some(123));
        assert_eq!(get_u64(&json!("456")), Some(456));
        assert_eq!(get_u64(&json!(78.9)), Some(78));
    }

    #[test]
    fn parse_u64_rejects_negative_and_invalid() {
        assert_eq!(get_u64(&json!(-1)), None);
        assert_eq!(get_u64(&json!("abc")), None);
        assert_eq!(get_u64(&json!(null)), None);
    }

    #[test]
    fn normalize_tool_status_aliases() {
        assert_eq!(normalize_tool_status("in_progress"), "running");
        assert_eq!(normalize_tool_status("Succeeded"), "completed");
        assert_eq!(normalize_tool_status("ERROR"), "failed");
    }

    #[test]
    fn normalize_reads_usage_and_tool_alias_fields() {
        let raw: RawHookEvent = serde_json::from_value(json!({
            "event": "tool_call_update",
            "sessionId": "s1",
            "title": "shell",
            "status": "in_progress",
            "usage": {
                "input_tokens": 123,
                "outputTokens": 45
            }
        }))
        .expect("raw event json should deserialize");
        let ev = raw.normalize("openx");
        assert_eq!(ev.session_id, "s1");
        assert_eq!(ev.tool_name.as_deref(), Some("shell"));
        assert_eq!(ev.tool_status.as_deref(), Some("running"));
        assert_eq!(ev.tokens_input, Some(123));
        assert_eq!(ev.tokens_output, Some(45));
    }

    #[test]
    fn normalize_extracts_error_from_failed_status_message() {
        let raw: RawHookEvent = serde_json::from_value(json!({
            "event": "tool_call_update",
            "sessionId": "s2",
            "status": "failed",
            "message": "permission denied"
        }))
        .expect("raw event json should deserialize");
        let ev = raw.normalize("openx");
        assert_eq!(ev.tool_status.as_deref(), Some("failed"));
        assert_eq!(ev.error.as_deref(), Some("permission denied"));
    }

    /// R107.2: tool_name 補 `name` alias（Claude tool_use.name 官方 schema）
    #[test]
    fn normalize_reads_tool_name_from_name_field() {
        let raw: RawHookEvent = serde_json::from_value(json!({
            "event": "PreToolUse",
            "session_id": "s3",
            "name": "Read"
        }))
        .expect("raw event json should deserialize");
        let ev = raw.normalize("claude");
        assert_eq!(ev.tool_name.as_deref(), Some("Read"));
    }

    /// R107.2: tool_call_id 補 `id` / `tool_use_id` alias（Claude tool_use.id）
    #[test]
    fn normalize_reads_tool_call_id_from_anthropic_fields() {
        // `id` 欄位
        let raw: RawHookEvent = serde_json::from_value(json!({
            "event": "PreToolUse",
            "session_id": "s4",
            "id": "toolu_abc123"
        }))
        .expect("raw event json should deserialize");
        let ev = raw.normalize("claude");
        assert_eq!(ev.tool_call_id.as_deref(), Some("toolu_abc123"));

        // `tool_use_id` 欄位
        let raw2: RawHookEvent = serde_json::from_value(json!({
            "event": "PreToolUse",
            "session_id": "s5",
            "tool_use_id": "toolu_xyz789"
        }))
        .expect("raw event json should deserialize");
        let ev2 = raw2.normalize("claude");
        assert_eq!(ev2.tool_call_id.as_deref(), Some("toolu_xyz789"));
    }

    /// R107.2: tokens_input nested 補 `promptTokens` camelCase
    #[test]
    fn normalize_reads_prompt_tokens_camelcase() {
        let raw: RawHookEvent = serde_json::from_value(json!({
            "event": "tool_call_update",
            "session_id": "s6",
            "usage": {
                "promptTokens": 999,
                "completionTokens": 42
            }
        }))
        .expect("raw event json should deserialize");
        let ev = raw.normalize("codex");
        assert_eq!(ev.tokens_input, Some(999));
        assert_eq!(ev.tokens_output, Some(42));
    }

    /// R107.1: get_str 內部 trim + 空字串視為 None。
    /// `session_id: "  s7  "` 應被正規化為 "s7"，
    /// `"   "` 純空白應讓 `unwrap_or_default()` fallback 生效。
    #[test]
    fn normalize_trims_whitespace_in_string_fields() {
        let raw: RawHookEvent = serde_json::from_value(json!({
            "event": "SessionStart",
            "session_id": "  s7  ",
            "prompt": "\thello\n",
            "cwd": " /tmp/proj "
        }))
        .expect("raw event json should deserialize");
        let ev = raw.normalize("claude");
        assert_eq!(ev.session_id, "s7");
        assert_eq!(ev.prompt.as_deref(), Some("hello"));
        assert_eq!(ev.cwd.as_deref(), Some("/tmp/proj"));
    }

    /// R107.1: 純空白欄位視為不存在，觸發 unwrap_or_default() fallback。
    /// session_id 全空白 → 走 default 空字串（讓 session 走 default bucket 而非污染命名空間）。
    #[test]
    fn normalize_treats_blank_string_as_missing() {
        let raw: RawHookEvent = serde_json::from_value(json!({
            "event": "SessionStart",
            "session_id": "   "
        }))
        .expect("raw event json should deserialize");
        let ev = raw.normalize("claude");
        assert_eq!(ev.session_id, "");
    }
}
