//! LMOrchestrator: manages parallel execution of LM requests with concurrency limits.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Semaphore;

#[allow(unused_imports)]
use crate::api::errors::LmClientError;
use crate::api::lm::AbstractLanguageModel;
use crate::api::types::ChatMessage;

/// Abstract orchestrator for managing parallel LM generation.
#[async_trait::async_trait]
#[allow(clippy::too_many_arguments)]
pub trait AbstractOrchestrator: Send + Sync {
    async fn agenerate(
        &self,
        backend: &dyn AbstractLanguageModel,
        messages_lst: &[Vec<ChatMessage>],
        stop: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f64>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Vec<Result<Value, LmClientError>>;
}

/// Orchestrator for managing parallel LM generation with concurrency limits.
pub struct LMOrchestrator {
    max_concurrency: usize,
    semaphore: Option<Arc<Semaphore>>,
}

impl LMOrchestrator {
    pub fn new(max_concurrency: usize) -> Self {
        let semaphore = if max_concurrency > 0 {
            Some(Arc::new(Semaphore::new(max_concurrency)))
        } else {
            None
        };
        Self {
            max_concurrency,
            semaphore,
        }
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }
}

impl Default for LMOrchestrator {
    fn default() -> Self {
        Self::new(32)
    }
}

#[async_trait::async_trait]
impl AbstractOrchestrator for LMOrchestrator {
    async fn agenerate(
        &self,
        backend: &dyn AbstractLanguageModel,
        messages_lst: &[Vec<ChatMessage>],
        stop: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f64>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Vec<Result<Value, LmClientError>> {
        if messages_lst.is_empty() {
            return vec![];
        }

        let mut results = Vec::with_capacity(messages_lst.len());
        for msgs in messages_lst {
            if let Some(ref sem) = self.semaphore {
                let _permit = sem.acquire().await.unwrap();
            }
            let result = backend
                .agenerate_single(msgs, stop, max_tokens, temperature, None, tools, tool_choice)
                .await;
            results.push(result);
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Content;

    #[test]
    fn test_orchestrator_creation() {
        let orch = LMOrchestrator::new(32);
        assert_eq!(orch.max_concurrency(), 32);
        assert!(orch.semaphore.is_some());
    }

    #[test]
    fn test_orchestrator_unlimited() {
        let orch = LMOrchestrator::new(0);
        assert_eq!(orch.max_concurrency(), 0);
        assert!(orch.semaphore.is_none());
    }

    #[test]
    fn test_orchestrator_default() {
        let orch = LMOrchestrator::default();
        assert_eq!(orch.max_concurrency(), 32);
    }

    #[tokio::test]
    async fn test_orchestrator_empty_batch() {
        let orch = LMOrchestrator::new(4);
        let client = crate::core::lms::LmClient::new(
            "http://localhost:9999/v1",
            None,
            "test",
            8,
            1,
            None,
            None,
        )
        .unwrap();
        let backend = crate::core::lms::LmBackend::OpenAI(client);

        let results = orch
            .agenerate(&backend, &[], None, None, None, None, None)
            .await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_orchestrator_basic_generation() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 1700000000u64,
                "model": "test-model",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hello!"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
            })))
            .mount(&server)
            .await;

        let client = crate::core::lms::LmClient::new(
            &format!("{}/v1", server.uri()),
            Some("test-key"),
            "test-model",
            8,
            1,
            None,
            None,
        )
        .unwrap();
        let backend = crate::core::lms::LmBackend::OpenAI(client);

        let orch = LMOrchestrator::new(4);

        let messages_lst = vec![
            vec![ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Text("Hi".to_string())),
                tool_calls: None,
                tool_call_id: None,
            }],
            vec![ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Text("Hello".to_string())),
                tool_calls: None,
                tool_call_id: None,
            }],
        ];

        let results = orch
            .agenerate(&backend, &messages_lst, None, None, None, None, None)
            .await;

        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.is_ok());
            assert_eq!(r.as_ref().unwrap()["content"], "Hello!");
        }
    }
}
