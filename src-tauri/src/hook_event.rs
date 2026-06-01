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
            .or_else(|| get_str(f, "title"));

        let notification_type =
            get_str(f, "notification_type").or_else(|| get_str(f, "notificationType"));

        let tool_call_id = get_str(f, "tool_call_id")
            .or_else(|| get_str(f, "toolCallId"))
            .or_else(|| get_str(f, "call_id"))
            .or_else(|| get_str(f, "callId"));

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
        }
    }
}

fn get_str(map: &std::collections::HashMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
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
}
