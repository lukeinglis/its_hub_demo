use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rand::Rng;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{debug, warn};

use crate::api::errors::LmClientError;
use crate::api::types::ChatMessage;

/// Supported provider identifiers for request/response adaptation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    OpenAI,
    Anthropic,
    Bedrock,
    VertexAI,
    Custom(String),
}

impl Provider {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "openai" => Provider::OpenAI,
            "anthropic" => Provider::Anthropic,
            "bedrock" | "aws_bedrock" => Provider::Bedrock,
            "vertex_ai" | "vertex" => Provider::VertexAI,
            other => Provider::Custom(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Provider::OpenAI => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Bedrock => "bedrock",
            Provider::VertexAI => "vertex_ai",
            Provider::Custom(s) => s.as_str(),
        }
    }

    fn default_api_base(&self) -> Option<&str> {
        match self {
            Provider::OpenAI => Some("https://api.openai.com/v1"),
            Provider::Anthropic => Some("https://api.anthropic.com/v1"),
            _ => None,
        }
    }
}

/// Multi-provider LLM client inspired by Python's LiteLLMLanguageModel.
///
/// Instead of wrapping the litellm Python library, this uses configurable
/// provider-specific request/response adapters over HTTP.
pub struct LiteLLMClient {
    http: reqwest::Client,
    model_name: String,
    provider: Provider,
    api_base: String,
    system_prompt: Option<String>,
    max_concurrency: usize,
    max_retries: u32,
    default_temperature: Option<f64>,
    default_max_tokens: Option<u32>,
    default_stop: Option<String>,
    extra_kwargs: HashMap<String, Value>,
}

impl LiteLLMClient {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model_name: &str,
        provider: &str,
        api_key: Option<&str>,
        api_base: Option<&str>,
        system_prompt: Option<String>,
        max_concurrency: usize,
        max_retries: u32,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        stop: Option<String>,
        extra_kwargs: HashMap<String, Value>,
    ) -> Result<Self, LmClientError> {
        let provider = Provider::parse(provider);

        let effective_api_base = api_base
            .map(|s| s.to_string())
            .or_else(|| provider.default_api_base().map(|s| s.to_string()))
            .ok_or_else(|| {
                LmClientError::Connection(format!(
                    "no api_base provided and no default for provider '{}'",
                    provider.as_str()
                ))
            })?;

        let mut default_headers = HeaderMap::new();
        default_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        match &provider {
            Provider::Anthropic => {
                if let Some(key) = api_key {
                    default_headers.insert(
                        "x-api-key",
                        HeaderValue::from_str(key).map_err(|e| {
                            LmClientError::Connection(format!("invalid api key header: {}", e))
                        })?,
                    );
                }
                default_headers.insert(
                    "anthropic-version",
                    HeaderValue::from_static("2023-06-01"),
                );
            }
            _ => {
                if let Some(key) = api_key {
                    let auth_value = format!("Bearer {}", key);
                    default_headers.insert(
                        AUTHORIZATION,
                        HeaderValue::from_str(&auth_value).map_err(|e| {
                            LmClientError::Connection(format!("invalid api key header: {}", e))
                        })?,
                    );
                }
            }
        }

        let http = reqwest::Client::builder()
            .pool_max_idle_per_host(20)
            .pool_idle_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(120))
            .default_headers(default_headers)
            .build()
            .map_err(|e| {
                LmClientError::Connection(format!("failed to create HTTP client: {}", e))
            })?;

        Ok(Self {
            http,
            model_name: model_name.to_string(),
            provider,
            api_base: effective_api_base.trim_end_matches('/').to_string(),
            system_prompt,
            max_concurrency,
            max_retries,
            default_temperature: temperature,
            default_max_tokens: max_tokens,
            default_stop: stop,
            extra_kwargs,
        })
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn provider(&self) -> &Provider {
        &self.provider
    }

    fn chat_completion_url(&self) -> String {
        match &self.provider {
            Provider::Anthropic => format!("{}/messages", self.api_base),
            _ => format!("{}/chat/completions", self.api_base),
        }
    }

    fn build_request_body(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        stop: Option<&str>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Value {
        let mut messages_json: Vec<Value> = Vec::new();

        if let Some(ref sp) = self.system_prompt {
            messages_json.push(serde_json::json!({
                "role": "system",
                "content": sp,
            }));
        }

        for m in messages {
            messages_json.push(serde_json::to_value(m).unwrap_or(Value::Null));
        }

        let mut body = serde_json::json!({
            "model": self.model_name,
            "messages": messages_json,
        });
        let obj = body.as_object_mut().unwrap();

        let temp = temperature.or(self.default_temperature);
        if let Some(t) = temp {
            obj.insert("temperature".to_string(), serde_json::json!(t));
        } else {
            obj.insert("temperature".to_string(), serde_json::json!(0.7));
        }

        let mt = max_tokens.or(self.default_max_tokens);
        if let Some(m) = mt {
            obj.insert("max_tokens".to_string(), serde_json::json!(m));
        }

        let st = stop.map(|s| s.to_string()).or(self.default_stop.clone());
        if let Some(s) = st {
            obj.insert("stop".to_string(), serde_json::json!(s));
        }

        if let Some(t) = tools {
            obj.insert("tools".to_string(), t.clone());
        }
        if let Some(tc) = tool_choice {
            obj.insert("tool_choice".to_string(), tc.clone());
        }

        for (k, v) in &self.extra_kwargs {
            obj.insert(k.clone(), v.clone());
        }

        body
    }

    fn extract_message_from_response(&self, response_json: &Value) -> Result<Value, LmClientError> {
        response_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .cloned()
            .ok_or_else(|| {
                LmClientError::Connection(
                    "response missing choices[0].message".to_string(),
                )
            })
    }

    async fn single_request(&self, body: &Value) -> Result<Value, LmClientError> {
        let url = self.chat_completion_url();
        let response = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| LmClientError::Connection(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body_text = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read response body".to_string());
            return Err(LmClientError::from_response(status, &body_text));
        }

        let response_json: Value = response
            .json()
            .await
            .map_err(|e| {
                LmClientError::Connection(format!("failed to parse response JSON: {}", e))
            })?;

        self.extract_message_from_response(&response_json)
    }

    async fn request_with_retry(&self, body: &Value) -> Result<Value, LmClientError> {
        let mut attempt = 0u32;
        let mut delay = Duration::from_millis(500);
        let max_delay = Duration::from_secs(60);

        loop {
            match self.single_request(body).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    attempt += 1;
                    if !e.is_retryable() || attempt >= self.max_retries {
                        return Err(e);
                    }
                    warn!(
                        attempt,
                        max_retries = self.max_retries,
                        error = %e,
                        "retryable error, backing off"
                    );
                    let jitter = Duration::from_millis(rand::thread_rng().gen_range(0..1000));
                    tokio::time::sleep(delay + jitter).await;
                    delay = (delay * 2).min(max_delay);
                }
            }
        }
    }

    pub async fn chat_completion(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        stop: Option<&str>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<Value, LmClientError> {
        let body =
            self.build_request_body(messages, temperature, max_tokens, stop, tools, tool_choice);
        self.request_with_retry(&body).await
    }

    /// Synchronous wrapper around `chat_completion` for use in non-async contexts.
    ///
    /// Panics if called from within a tokio runtime. Use `chat_completion` directly
    /// when an async context is available.
    pub fn generate_sync(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        stop: Option<&str>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<Value, LmClientError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                LmClientError::Connection(format!("failed to create tokio runtime: {}", e))
            })?;
        rt.block_on(self.chat_completion(messages, temperature, max_tokens, stop, tools, tool_choice))
    }

    pub async fn fan_out(
        &self,
        messages: &[ChatMessage],
        budget: u32,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Vec<Result<Value, LmClientError>> {
        let body =
            self.build_request_body(messages, temperature, max_tokens, None, tools, tool_choice);
        let permits = std::cmp::min(budget as usize, self.max_concurrency);
        let semaphore = Arc::new(Semaphore::new(permits));
        let body = Arc::new(body);

        let mut join_set = JoinSet::new();

        for i in 0..budget {
            let sem = semaphore.clone();
            let body = body.clone();
            let url = self.chat_completion_url();
            let http = self.http.clone();
            let max_retries = self.max_retries;

            join_set.spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                debug!(task = i, "starting fan-out request");
                retry_single_request(&http, &url, &body, max_retries).await
            });
        }

        let mut results = Vec::with_capacity(budget as usize);
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(result) => results.push(result),
                Err(e) => results.push(Err(LmClientError::Connection(format!(
                    "task panicked: {}",
                    e
                )))),
            }
        }
        results
    }
}

async fn retry_single_request(
    http: &reqwest::Client,
    url: &str,
    body: &Value,
    max_retries: u32,
) -> Result<Value, LmClientError> {
    let mut attempt = 0u32;
    let mut delay = Duration::from_millis(500);
    let max_delay = Duration::from_secs(60);

    loop {
        let response = http
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|e| LmClientError::Connection(e.to_string()));

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                attempt += 1;
                if !e.is_retryable() || attempt >= max_retries {
                    return Err(e);
                }
                warn!(attempt, error = %e, "retryable connection error, backing off");
                let jitter = Duration::from_millis(rand::thread_rng().gen_range(0..1000));
                tokio::time::sleep(delay + jitter).await;
                delay = (delay * 2).min(max_delay);
                continue;
            }
        };

        let status = response.status().as_u16();
        if status != 200 {
            let body_text = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read response body".to_string());
            let err = LmClientError::from_response(status, &body_text);
            attempt += 1;
            if !err.is_retryable() || attempt >= max_retries {
                return Err(err);
            }
            warn!(attempt, error = %err, "retryable HTTP error, backing off");
            let jitter = Duration::from_millis(rand::thread_rng().gen_range(0..1000));
            tokio::time::sleep(delay + jitter).await;
            delay = (delay * 2).min(max_delay);
            continue;
        }

        let response_json: Value = response.json().await.map_err(|e| {
            LmClientError::Connection(format!("failed to parse response JSON: {}", e))
        })?;

        let message = response_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .cloned()
            .ok_or_else(|| {
                LmClientError::Connection("response missing choices[0].message".to_string())
            })?;

        return Ok(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Content;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_chat_response(content: &str) -> Value {
        json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "created": 1700000000u64,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        })
    }

    fn test_messages() -> Vec<ChatMessage> {
        vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text("Hello".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }]
    }

    #[test]
    fn test_provider_from_str() {
        assert_eq!(Provider::parse("openai"), Provider::OpenAI);
        assert_eq!(Provider::parse("OpenAI"), Provider::OpenAI);
        assert_eq!(Provider::parse("anthropic"), Provider::Anthropic);
        assert_eq!(Provider::parse("bedrock"), Provider::Bedrock);
        assert_eq!(Provider::parse("aws_bedrock"), Provider::Bedrock);
        assert_eq!(Provider::parse("vertex_ai"), Provider::VertexAI);
        assert_eq!(Provider::parse("vertex"), Provider::VertexAI);
        assert_eq!(
            Provider::parse("ollama"),
            Provider::Custom("ollama".to_string())
        );
    }

    #[test]
    fn test_litellm_client_provider_config() {
        let client = LiteLLMClient::new(
            "gpt-4",
            "openai",
            Some("sk-test"),
            None,
            Some("You are helpful.".to_string()),
            8,
            3,
            Some(0.5),
            Some(1024),
            None,
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(client.model_name(), "gpt-4");
        assert_eq!(client.provider(), &Provider::OpenAI);

        let body = client.build_request_body(&test_messages(), None, None, None, None, None);
        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["max_tokens"], 1024);

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are helpful.");
    }

    #[test]
    fn test_litellm_client_anthropic_url() {
        let client = LiteLLMClient::new(
            "claude-3-sonnet",
            "anthropic",
            Some("sk-ant-test"),
            None,
            None,
            8,
            3,
            None,
            None,
            None,
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(
            client.chat_completion_url(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn test_litellm_client_custom_api_base() {
        let client = LiteLLMClient::new(
            "my-model",
            "ollama",
            None,
            Some("http://localhost:11434/v1"),
            None,
            8,
            3,
            None,
            None,
            None,
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(
            client.chat_completion_url(),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn test_litellm_client_no_api_base_custom_provider_errors() {
        let result = LiteLLMClient::new(
            "model",
            "some-custom",
            None,
            None,
            None,
            8,
            3,
            None,
            None,
            None,
            HashMap::new(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_litellm_client_extra_kwargs() {
        let mut extra = HashMap::new();
        extra.insert("top_p".to_string(), json!(0.9));
        extra.insert("seed".to_string(), json!(42));

        let client = LiteLLMClient::new(
            "gpt-4",
            "openai",
            Some("key"),
            None,
            None,
            8,
            3,
            None,
            None,
            None,
            extra,
        )
        .unwrap();

        let body = client.build_request_body(&test_messages(), None, None, None, None, None);
        assert_eq!(body["top_p"], 0.9);
        assert_eq!(body["seed"], 42);
    }

    #[test]
    fn test_litellm_client_override_defaults() {
        let client = LiteLLMClient::new(
            "gpt-4",
            "openai",
            Some("key"),
            None,
            None,
            8,
            3,
            Some(0.5),
            Some(512),
            Some("\n\n".to_string()),
            HashMap::new(),
        )
        .unwrap();

        let body = client.build_request_body(
            &test_messages(),
            Some(0.9),
            Some(2048),
            Some("STOP"),
            None,
            None,
        );
        assert_eq!(body["temperature"], 0.9);
        assert_eq!(body["max_tokens"], 2048);
        assert_eq!(body["stop"], "STOP");
    }

    #[test]
    fn test_litellm_client_default_temperature_fallback() {
        let client = LiteLLMClient::new(
            "gpt-4",
            "openai",
            Some("key"),
            None,
            None,
            8,
            3,
            None,
            None,
            None,
            HashMap::new(),
        )
        .unwrap();

        let body = client.build_request_body(&test_messages(), None, None, None, None, None);
        assert_eq!(body["temperature"], 0.7);
    }

    #[tokio::test]
    async fn test_litellm_chat_completion_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(make_chat_response("Hello from LiteLLM")),
            )
            .mount(&server)
            .await;

        let client = LiteLLMClient::new(
            "test-model",
            "openai",
            Some("test-key"),
            Some(&format!("{}/v1", server.uri())),
            None,
            8,
            3,
            None,
            None,
            None,
            HashMap::new(),
        )
        .unwrap();

        let result = client
            .chat_completion(&test_messages(), None, None, None, None, None)
            .await;

        let msg = result.unwrap();
        assert_eq!(msg["content"], "Hello from LiteLLM");
        assert_eq!(msg["role"], "assistant");
    }

    #[tokio::test]
    async fn test_litellm_fan_out() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(make_chat_response("fan-out response")),
            )
            .mount(&server)
            .await;

        let client = LiteLLMClient::new(
            "test-model",
            "openai",
            Some("test-key"),
            Some(&format!("{}/v1", server.uri())),
            None,
            8,
            3,
            None,
            None,
            None,
            HashMap::new(),
        )
        .unwrap();

        let results = client
            .fan_out(&test_messages(), 3, None, None, None, None)
            .await;

        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.is_ok());
            assert_eq!(r.as_ref().unwrap()["content"], "fan-out response");
        }
    }
}
