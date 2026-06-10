use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use rand::Rng;
use tracing::{debug, warn};

use crate::api::errors::LmClientError;
use crate::api::types::ChatMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointType {
    OpenAI,
    Vllm,
}

pub struct LmClient {
    http: reqwest::Client,
    endpoint: String,
    model_name: String,
    max_concurrency: usize,
    max_retries: u32,
    endpoint_type: EndpointType,
    system_prompt: Option<String>,
    include_stop_str_in_output: Option<bool>,
}

impl LmClient {
    pub fn new(
        endpoint: &str,
        api_key: Option<&str>,
        model_name: &str,
        max_concurrency: usize,
        max_retries: u32,
        system_prompt: Option<String>,
        include_stop_str_in_output: Option<bool>,
    ) -> Result<Self, LmClientError> {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(key) = api_key {
            let auth_value = format!("Bearer {}", key);
            default_headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&auth_value)
                    .map_err(|e| LmClientError::Connection(format!("invalid api key header: {}", e)))?,
            );
        }

        let http = reqwest::Client::builder()
            .pool_max_idle_per_host(20)
            .pool_idle_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(120))
            .default_headers(default_headers)
            .build()
            .map_err(|e| LmClientError::Connection(format!("failed to create HTTP client: {}", e)))?;

        let endpoint_type = if endpoint.contains("openai") {
            EndpointType::OpenAI
        } else {
            EndpointType::Vllm
        };

        Ok(Self {
            http,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model_name: model_name.to_string(),
            max_concurrency,
            max_retries,
            endpoint_type,
            system_prompt,
            include_stop_str_in_output,
        })
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn endpoint_type(&self) -> EndpointType {
        self.endpoint_type
    }

    fn chat_completion_url(&self) -> String {
        format!("{}/chat/completions", self.endpoint)
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

        if self.endpoint_type == EndpointType::Vllm {
            let effective_last = messages.last();
            if let Some(last) = effective_last {
                if last.role == "assistant" {
                    let extra = obj
                        .entry("extra_body")
                        .or_insert_with(|| serde_json::json!({}));
                    let extra_obj = extra.as_object_mut().unwrap();
                    extra_obj.insert("add_generation_prompt".to_string(), Value::Bool(false));
                    extra_obj.insert("continue_final_message".to_string(), Value::Bool(true));
                    obj.insert("add_generation_prompt".to_string(), Value::Bool(false));
                    obj.insert("continue_final_message".to_string(), Value::Bool(true));
                }
            }

            if let Some(include_stop) = self.include_stop_str_in_output {
                let extra = obj
                    .entry("extra_body")
                    .or_insert_with(|| serde_json::json!({}));
                let extra_obj = extra.as_object_mut().unwrap();
                extra_obj.insert(
                    "include_stop_str_in_output".to_string(),
                    Value::Bool(include_stop),
                );
                obj.insert(
                    "include_stop_str_in_output".to_string(),
                    Value::Bool(include_stop),
                );
            }
        }

        if let Some(t) = temperature {
            obj.insert("temperature".to_string(), serde_json::json!(t));
        }
        if let Some(mt) = max_tokens {
            obj.insert("max_tokens".to_string(), serde_json::json!(mt));
        }
        if let Some(s) = stop {
            obj.insert("stop".to_string(), serde_json::json!(s));
        }
        if let Some(t) = tools {
            obj.insert("tools".to_string(), t.clone());
        }
        if let Some(tc) = tool_choice {
            obj.insert("tool_choice".to_string(), tc.clone());
        }

        body
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
            .map_err(|e| LmClientError::Connection(format!("failed to parse response JSON: {}", e)))?;

        let message = response_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .cloned()
            .ok_or_else(|| {
                LmClientError::Connection("response missing choices[0].message".to_string())
            })?;

        Ok(message)
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
        let body = self.build_request_body(messages, temperature, max_tokens, stop, tools, tool_choice);
        self.request_with_retry(&body).await
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

    pub async fn generate_batch(
        &self,
        messages_batch: &[Vec<ChatMessage>],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        stop: Option<&str>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Vec<Result<Value, LmClientError>> {
        let permits = std::cmp::min(messages_batch.len(), self.max_concurrency);
        let semaphore = Arc::new(Semaphore::new(permits));

        let bodies: Vec<Value> = messages_batch
            .iter()
            .map(|msgs| self.build_request_body(msgs, temperature, max_tokens, stop, tools, tool_choice))
            .collect();

        let mut join_set = JoinSet::new();

        for (i, body) in bodies.into_iter().enumerate() {
            let sem = semaphore.clone();
            let url = self.chat_completion_url();
            let http = self.http.clone();
            let max_retries = self.max_retries;

            join_set.spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                debug!(task = i, "starting batch request");
                retry_single_request(&http, &url, &body, max_retries).await
            });
        }

        let mut results = Vec::with_capacity(messages_batch.len());
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

    /// Evaluate a prompt/response pair. Not yet implemented.
    pub async fn evaluate(
        &self,
        _messages: &[ChatMessage],
        _response: &str,
    ) -> Result<Value, LmClientError> {
        Err(LmClientError::Connection(
            "evaluate() is not implemented".to_string(),
        ))
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
        let body = self.build_request_body(messages, temperature, max_tokens, None, tools, tool_choice);
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

        let response_json: Value = response
            .json()
            .await
            .map_err(|e| LmClientError::Connection(format!("failed to parse response JSON: {}", e)))?;

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

    #[tokio::test]
    async fn chat_completion_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("Hi there!")))
            .mount(&server)
            .await;

        let client = LmClient::new(
            &format!("{}/v1", server.uri()),
            Some("test-key"),
            "test-model",
            8,
            3,
            None,
            None,
        )
        .unwrap();

        let result = client
            .chat_completion(&test_messages(), None, None, None, None, None)
            .await;

        let msg = result.unwrap();
        assert_eq!(msg["content"], "Hi there!");
        assert_eq!(msg["role"], "assistant");
    }

    #[tokio::test]
    async fn chat_completion_auth_error_no_retry() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(json!({"error": {"message": "invalid api key"}})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = LmClient::new(
            &format!("{}/v1", server.uri()),
            Some("bad-key"),
            "test-model",
            8,
            3,
            None,
            None,
        )
        .unwrap();

        let result = client
            .chat_completion(&test_messages(), None, None, None, None, None)
            .await;

        assert!(matches!(
            result,
            Err(LmClientError::Authentication { .. })
        ));
    }

    #[tokio::test]
    async fn chat_completion_retry_on_429() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_json(json!({"error": {"message": "rate limit exceeded"}})),
            )
            .up_to_n_times(2)
            .expect(2)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("Finally!")))
            .expect(1)
            .mount(&server)
            .await;

        let client = LmClient::new(
            &format!("{}/v1", server.uri()),
            Some("test-key"),
            "test-model",
            8,
            5,
            None,
            None,
        )
        .unwrap();

        let result = client
            .chat_completion(&test_messages(), None, None, None, None, None)
            .await;

        let msg = result.unwrap();
        assert_eq!(msg["content"], "Finally!");
    }

    #[tokio::test]
    async fn chat_completion_retry_on_500() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(500).set_body_string("internal server error"),
            )
            .up_to_n_times(2)
            .expect(2)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(make_chat_response("recovered")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = LmClient::new(
            &format!("{}/v1", server.uri()),
            Some("test-key"),
            "test-model",
            8,
            5,
            None,
            None,
        )
        .unwrap();

        let result = client
            .chat_completion(&test_messages(), None, None, None, None, None)
            .await;

        let msg = result.unwrap();
        assert_eq!(msg["content"], "recovered");
    }

    #[tokio::test]
    async fn fan_out_collects_all_results() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("response")))
            .mount(&server)
            .await;

        let client = LmClient::new(
            &format!("{}/v1", server.uri()),
            Some("test-key"),
            "test-model",
            8,
            3,
            None,
            None,
        )
        .unwrap();

        let results = client
            .fan_out(&test_messages(), 5, None, None, None, None)
            .await;

        assert_eq!(results.len(), 5);
        for r in &results {
            assert!(r.is_ok());
            assert_eq!(r.as_ref().unwrap()["content"], "response");
        }
    }

    #[tokio::test]
    async fn fan_out_handles_mixed_results() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("ok")))
            .mount(&server)
            .await;

        let client = LmClient::new(
            &format!("{}/v1", server.uri()),
            Some("test-key"),
            "test-model",
            4,
            3,
            None,
            None,
        )
        .unwrap();

        let results = client
            .fan_out(&test_messages(), 3, Some(0.7), Some(1024), None, None)
            .await;

        assert_eq!(results.len(), 3);
        let successes: Vec<_> = results.iter().filter(|r| r.is_ok()).collect();
        assert_eq!(successes.len(), 3);
    }

    #[tokio::test]
    async fn endpoint_type_detection() {
        let vllm = LmClient::new(
            "http://localhost:8100/v1",
            None,
            "model",
            8,
            3,
            None,
            None,
        )
        .unwrap();
        assert_eq!(vllm.endpoint_type(), EndpointType::Vllm);

        let openai = LmClient::new(
            "https://api.openai.com/v1",
            Some("key"),
            "gpt-4",
            8,
            3,
            None,
            None,
        )
        .unwrap();
        assert_eq!(openai.endpoint_type(), EndpointType::OpenAI);
    }

    #[test]
    fn build_request_body_basic() {
        let client = LmClient::new(
            "http://localhost:8100/v1",
            None,
            "test-model",
            8,
            3,
            None,
            None,
        )
        .unwrap();

        let body = client.build_request_body(&test_messages(), Some(0.7), Some(100), None, None, None);
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["max_tokens"], 100);
        assert!(body.get("stop").is_none());
    }

    #[test]
    fn build_request_body_vllm_assistant_continuation() {
        let client = LmClient::new(
            "http://localhost:8100/v1",
            None,
            "test-model",
            8,
            3,
            None,
            None,
        )
        .unwrap();

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Text("Hello".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(Content::Text("Starting...".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let body = client.build_request_body(&messages, None, None, None, None, None);
        assert_eq!(body["add_generation_prompt"], false);
        assert_eq!(body["continue_final_message"], true);
        assert_eq!(body["extra_body"]["add_generation_prompt"], false);
        assert_eq!(body["extra_body"]["continue_final_message"], true);
    }

    #[test]
    fn build_request_body_openai_no_vllm_fields() {
        let client = LmClient::new(
            "https://api.openai.com/v1",
            Some("key"),
            "gpt-4",
            8,
            3,
            None,
            None,
        )
        .unwrap();

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Text("Hello".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(Content::Text("Starting...".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let body = client.build_request_body(&messages, None, None, None, None, None);
        assert!(body.get("add_generation_prompt").is_none());
        assert!(body.get("continue_final_message").is_none());
        assert!(body.get("extra_body").is_none());
    }

    #[test]
    fn error_from_response_classification() {
        let err = LmClientError::from_response(429, "too many requests");
        assert!(matches!(err, LmClientError::RateLimit { .. }));
        assert!(err.is_retryable());

        let err = LmClientError::from_response(
            400,
            r#"{"error":{"message":"maximum context length exceeded"}}"#,
        );
        assert!(matches!(err, LmClientError::ContextLength { .. }));
        assert!(!err.is_retryable());

        let err = LmClientError::from_response(401, "unauthorized");
        assert!(matches!(err, LmClientError::Authentication { .. }));
        assert!(!err.is_retryable());

        let err = LmClientError::from_response(403, "forbidden");
        assert!(matches!(err, LmClientError::Authentication { .. }));

        let err = LmClientError::from_response(400, "invalid model");
        assert!(matches!(err, LmClientError::BadRequest { .. }));
        assert!(!err.is_retryable());

        let err = LmClientError::from_response(500, "internal server error");
        assert!(matches!(err, LmClientError::ServerError { .. }));
        assert!(err.is_retryable());

        let err = LmClientError::from_response(502, "bad gateway");
        assert!(matches!(err, LmClientError::ServerError { .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn error_json_body_parsing() {
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
    fn build_request_body_prepends_system_prompt() {
        let client = LmClient::new(
            "http://localhost:8100/v1",
            None,
            "test-model",
            8,
            3,
            Some("You are a math tutor.".to_string()),
            None,
        )
        .unwrap();

        let body = client.build_request_body(&test_messages(), None, None, None, None, None);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are a math tutor.");
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn build_request_body_no_system_prompt_when_none() {
        let client = LmClient::new(
            "http://localhost:8100/v1",
            None,
            "test-model",
            8,
            3,
            None,
            None,
        )
        .unwrap();

        let body = client.build_request_body(&test_messages(), None, None, None, None, None);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }

    #[test]
    fn build_request_body_vllm_include_stop_str() {
        let client = LmClient::new(
            "http://localhost:8100/v1",
            None,
            "test-model",
            8,
            3,
            None,
            Some(true),
        )
        .unwrap();

        let body = client.build_request_body(&test_messages(), None, None, None, None, None);
        assert_eq!(body["include_stop_str_in_output"], true);
        assert_eq!(body["extra_body"]["include_stop_str_in_output"], true);
    }

    #[test]
    fn build_request_body_openai_no_include_stop_str() {
        let client = LmClient::new(
            "https://api.openai.com/v1",
            Some("key"),
            "gpt-4",
            8,
            3,
            None,
            Some(true),
        )
        .unwrap();

        let body = client.build_request_body(&test_messages(), None, None, None, None, None);
        assert!(body.get("include_stop_str_in_output").is_none());
        assert!(body.get("extra_body").is_none());
    }

    #[test]
    fn build_request_body_vllm_assistant_with_stop_str() {
        let client = LmClient::new(
            "http://localhost:8100/v1",
            None,
            "test-model",
            8,
            3,
            None,
            Some(false),
        )
        .unwrap();

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Text("Hello".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(Content::Text("Starting...".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let body = client.build_request_body(&messages, None, None, None, None, None);
        assert_eq!(body["extra_body"]["add_generation_prompt"], false);
        assert_eq!(body["extra_body"]["continue_final_message"], true);
        assert_eq!(body["extra_body"]["include_stop_str_in_output"], false);
        assert_eq!(body["include_stop_str_in_output"], false);
    }
}
