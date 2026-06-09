use serde::{Deserialize, Serialize};

// --- Chat message types ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolFunction {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

impl ChatMessage {
    pub fn extract_text_content(&self) -> String {
        match &self.content {
            None => String::new(),
            Some(Content::Text(s)) => s.clone(),
            Some(Content::Parts(parts)) => parts
                .iter()
                .filter(|p| p.part_type == "text")
                .filter_map(|p| p.text.as_deref())
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

// --- Chat completion request/response ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_budget")]
    pub budget: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default = "default_true")]
    pub return_response_only: bool,
}

fn default_budget() -> u32 {
    8
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: ChatCompletionUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionChoice {
    pub index: u32,
    pub message: serde_json::Value,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// --- Configuration types ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigRequest {
    #[serde(default = "default_provider")]
    pub provider: String,
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub model: String,
    pub alg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex_patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_vote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_tool_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rm_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rm_endpoint: Option<String>,
}

fn default_provider() -> String {
    "openai".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigResponse {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
    pub models: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelsResponse {
    pub data: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub owned_by: String,
}

// --- Error types ---

#[derive(Debug, thiserror::Error)]
pub enum LmClientError {
    #[error("rate limit exceeded: {message}")]
    RateLimit { message: String, status: u16 },

    #[error("authentication failed: {message}")]
    Authentication { message: String, status: u16 },

    #[error("context length exceeded: {message}")]
    ContextLength { message: String },

    #[error("bad request: {message}")]
    BadRequest { message: String },

    #[error("server error: {message}")]
    ServerError { message: String, status: u16 },

    #[error("connection error: {0}")]
    Connection(String),
}

impl LmClientError {
    pub fn from_response(status: u16, body: &str) -> Self {
        let message = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("error")?.get("message")?.as_str().map(String::from))
            .unwrap_or_else(|| body.to_string());

        let lower = message.to_lowercase();

        if status == 429 || lower.contains("rate limit") {
            return LmClientError::RateLimit { message, status };
        }

        let context_keywords = [
            "context length",
            "maximum context",
            "too long",
            "token limit",
            "context_length_exceeded",
            "max_tokens",
        ];
        if status == 400 && context_keywords.iter().any(|kw| lower.contains(kw)) {
            return LmClientError::ContextLength { message };
        }

        if status == 401
            || status == 403
            || lower.contains("authentication")
            || lower.contains("unauthorized")
        {
            return LmClientError::Authentication { message, status };
        }

        if status == 400 {
            return LmClientError::BadRequest { message };
        }

        if status >= 500 {
            return LmClientError::ServerError { message, status };
        }

        LmClientError::Connection(message)
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LmClientError::RateLimit { .. }
                | LmClientError::ServerError { .. }
                | LmClientError::Connection(_)
        )
    }
}

// --- Utility functions ---

pub fn extract_content_from_lm_response(message: &serde_json::Value) -> String {
    let mut result = String::new();

    match message.get("content") {
        Some(serde_json::Value::String(s)) => {
            result.push_str(s);
        }
        Some(serde_json::Value::Array(parts)) => {
            for part in parts {
                if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        result.push_str(text);
                    }
                }
            }
        }
        _ => {}
    }

    if let Some(serde_json::Value::Array(tool_calls)) = message.get("tool_calls") {
        for tc in tool_calls {
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .map(|a| {
                    if let serde_json::Value::String(s) = a {
                        s.clone()
                    } else {
                        serde_json::to_string(a).unwrap_or_default()
                    }
                })
                .unwrap_or_default();
            result.push_str(&format!("[Tool call: {} Tool args: {}]", name, args));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_completion_request_roundtrip() {
        let req = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Text("Hello".to_string())),
                tool_calls: None,
                tool_call_id: None,
            }],
            budget: 4,
            temperature: Some(0.7),
            max_tokens: Some(1024),
            stream: None,
            tools: None,
            tool_choice: None,
            return_response_only: true,
        };

        let json_str = serde_json::to_string(&req).unwrap();
        let deserialized: ChatCompletionRequest = serde_json::from_str(&json_str).unwrap();
        assert_eq!(req, deserialized);
    }

    #[test]
    fn chat_completion_request_defaults() {
        let json_str = r#"{"model":"m","messages":[]}"#;
        let req: ChatCompletionRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.budget, 8);
        assert!(req.return_response_only);
    }

    #[test]
    fn chat_completion_response_from_canned_json() {
        let canned = json!({
            "id": "chatcmpl-abc123",
            "object": "chat.completion",
            "created": 1700000000u64,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hi!"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });

        let resp: ChatCompletionResponse = serde_json::from_value(canned).unwrap();
        assert_eq!(resp.id, "chatcmpl-abc123");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].finish_reason, "stop");
        assert_eq!(resp.usage.total_tokens, 15);
    }

    #[test]
    fn content_deserializes_from_string() {
        let v: Content = serde_json::from_str(r#""hello""#).unwrap();
        assert_eq!(v, Content::Text("hello".to_string()));
    }

    #[test]
    fn content_deserializes_from_parts() {
        let v: Content =
            serde_json::from_str(r#"[{"type":"text","text":"hello"}]"#).unwrap();
        match v {
            Content::Parts(parts) => {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0].text.as_deref(), Some("hello"));
            }
            _ => panic!("expected Parts variant"),
        }
    }

    #[test]
    fn chat_message_skips_none_fields() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text("hi".to_string())),
            tool_calls: None,
            tool_call_id: None,
        };
        let v: serde_json::Value = serde_json::to_value(&msg).unwrap();
        assert!(v.get("tool_calls").is_none());
        assert!(v.get("tool_call_id").is_none());
        assert!(v.get("role").is_some());
        assert!(v.get("content").is_some());
    }

    #[test]
    fn extract_text_content_none() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: None,
            tool_calls: None,
            tool_call_id: None,
        };
        assert_eq!(msg.extract_text_content(), "");
    }

    #[test]
    fn extract_text_content_string() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text("hello world".to_string())),
            tool_calls: None,
            tool_call_id: None,
        };
        assert_eq!(msg.extract_text_content(), "hello world");
    }

    #[test]
    fn extract_text_content_parts() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Parts(vec![
                ContentPart {
                    part_type: "text".to_string(),
                    text: Some("hello ".to_string()),
                },
                ContentPart {
                    part_type: "image_url".to_string(),
                    text: None,
                },
                ContentPart {
                    part_type: "text".to_string(),
                    text: Some("world".to_string()),
                },
            ])),
            tool_calls: None,
            tool_call_id: None,
        };
        assert_eq!(msg.extract_text_content(), "hello world");
    }

    #[test]
    fn extract_content_string() {
        let msg = json!({"role": "assistant", "content": "The answer is 42"});
        assert_eq!(
            extract_content_from_lm_response(&msg),
            "The answer is 42"
        );
    }

    #[test]
    fn extract_content_array() {
        let msg = json!({
            "role": "assistant",
            "content": [
                {"type": "text", "text": "part1"},
                {"type": "image_url", "url": "http://x"},
                {"type": "text", "text": "part2"}
            ]
        });
        assert_eq!(extract_content_from_lm_response(&msg), "part1part2");
    }

    #[test]
    fn extract_content_null() {
        let msg = json!({"role": "assistant", "content": null});
        assert_eq!(extract_content_from_lm_response(&msg), "");
    }

    #[test]
    fn extract_content_with_tool_calls() {
        let msg = json!({
            "role": "assistant",
            "content": "Looking up weather",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{\"city\":\"London\"}"
                }
            }]
        });
        let result = extract_content_from_lm_response(&msg);
        assert!(result.starts_with("Looking up weather"));
        assert!(result.contains("[Tool call: get_weather"));
        assert!(result.contains("Tool args: {\"city\":\"London\"}]"));
    }

    #[test]
    fn extract_content_tool_calls_only() {
        let msg = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "calc",
                    "arguments": {"x": 1}
                }
            }]
        });
        let result = extract_content_from_lm_response(&msg);
        assert!(result.contains("[Tool call: calc"));
    }

    #[test]
    fn config_request_defaults() {
        let json_str = r#"{"endpoint":"http://localhost:8100/v1","model":"m","alg":"self-consistency"}"#;
        let cfg: ConfigRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(cfg.provider, "openai");
        assert!(cfg.api_key.is_none());
        assert!(cfg.regex_patterns.is_none());
    }

    #[test]
    fn config_request_full() {
        let cfg = ConfigRequest {
            provider: "openai".to_string(),
            endpoint: "http://localhost:8100/v1".to_string(),
            api_key: Some("key123".to_string()),
            model: "test".to_string(),
            alg: "best-of-n".to_string(),
            regex_patterns: Some(vec![r"\boxed\{([^}]+)\}".to_string()]),
            tool_vote: None,
            exclude_tool_args: None,
            rm_name: Some("rm-model".to_string()),
            rm_endpoint: Some("http://localhost:8200".to_string()),
        };
        let json_str = serde_json::to_string(&cfg).unwrap();
        let deserialized: ConfigRequest = serde_json::from_str(&json_str).unwrap();
        assert_eq!(cfg, deserialized);
    }

    #[test]
    fn lm_client_error_rate_limit() {
        let err = LmClientError::from_response(429, "too many requests");
        assert!(matches!(err, LmClientError::RateLimit { .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn lm_client_error_context_length() {
        let body = r#"{"error":{"message":"This model's maximum context length is 4096 tokens"}}"#;
        let err = LmClientError::from_response(400, body);
        assert!(matches!(err, LmClientError::ContextLength { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn lm_client_error_auth() {
        let err = LmClientError::from_response(401, "invalid api key");
        assert!(matches!(err, LmClientError::Authentication { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn lm_client_error_bad_request() {
        let err = LmClientError::from_response(400, "invalid model");
        assert!(matches!(err, LmClientError::BadRequest { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn lm_client_error_server() {
        let err = LmClientError::from_response(500, "internal error");
        assert!(matches!(err, LmClientError::ServerError { .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn lm_client_error_json_body() {
        let body = r#"{"error":{"message":"rate limit exceeded","type":"rate_limit"}}"#;
        let err = LmClientError::from_response(429, body);
        match err {
            LmClientError::RateLimit { message, .. } => {
                assert_eq!(message, "rate limit exceeded");
            }
            _ => panic!("expected RateLimit"),
        }
    }

    #[test]
    fn health_response_roundtrip() {
        let hr = HealthResponse {
            status: "ok".to_string(),
            algorithm: None,
            models: 0,
        };
        let v: serde_json::Value = serde_json::to_value(&hr).unwrap();
        assert!(v.get("algorithm").is_none());
        assert_eq!(v["models"], 0);
    }

    #[test]
    fn models_response_roundtrip() {
        let mr = ModelsResponse {
            data: vec![ModelInfo {
                id: "gpt-4".to_string(),
                object: "model".to_string(),
                owned_by: "openai".to_string(),
            }],
        };
        let json_str = serde_json::to_string(&mr).unwrap();
        let deserialized: ModelsResponse = serde_json::from_str(&json_str).unwrap();
        assert_eq!(mr, deserialized);
    }
}
