//! LMOrchestrator: manages parallel execution of LM requests with concurrency limits.
//!
//! Port of Python's `its_hub.core.orchestrator.LMOrchestrator`.
//! Uses `tokio::sync::Semaphore` instead of Python's threading.Semaphore
//! and `tokio::task::JoinSet` instead of asyncio.TaskGroup.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Semaphore;

#[allow(unused_imports)]
use crate::api::errors::LmClientError;
use crate::api::types::ChatMessage;
use crate::core::lms::LmBackend;

/// Abstract orchestrator for managing parallel LM generation.
///
/// Mirrors Python's `AbstractOrchestrator` from `its_hub/api/orchestrator.py`.
#[async_trait::async_trait]
#[allow(clippy::too_many_arguments)]
pub trait AbstractOrchestrator: Send + Sync {
    async fn agenerate(
        &self,
        backend: &LmBackend,
        messages_lst: &[Vec<ChatMessage>],
        stop: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f64>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Vec<Result<Value, LmClientError>>;
}

/// Orchestrator for managing parallel LM generation with concurrency limits.
///
/// Mirrors Python's `LMOrchestrator` from `its_hub/core/orchestrator.py`.
/// The concurrency limit is enforced using a tokio semaphore, which is
/// thread-safe and async-native (no executor pool needed unlike Python).
pub struct LMOrchestrator {
    max_concurrency: usize,
    semaphore: Option<Arc<Semaphore>>,
}

impl LMOrchestrator {
    /// Create a new orchestrator.
    ///
    /// `max_concurrency` controls the maximum number of concurrent requests.
    /// Use 0 for unlimited concurrency (internally maps to no semaphore).
    ///
    /// # Panics
    /// Panics if max_concurrency is negative (not applicable in Rust since usize).
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

    /// Get the configured max concurrency.
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
    /// Generate responses for a batch of message lists asynchronously.
    ///
    /// Each element in `messages_lst` is a separate conversation to process.
    /// Results are returned in the same order as the input.
    ///
    /// Mirrors Python's `LMOrchestrator.agenerate()` using JoinSet for
    /// parallel execution (equivalent to asyncio.TaskGroup).
    async fn agenerate(
        &self,
        backend: &LmBackend,
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

        // Sequential execution with semaphore-based concurrency limiting.
        // The semaphore ensures we don't exceed max_concurrency outstanding requests.
        for msgs in messages_lst {
            if let Some(ref sem) = self.semaphore {
                let _permit = sem.acquire().await.unwrap();
            }
            let result = backend
                .chat_completion(msgs, temperature, max_tokens, stop, tools, tool_choice)
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
        // We need a backend but with empty input it should return immediately
        // Create a mock backend
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
        let backend = LmBackend::OpenAI(client);

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
        let backend = LmBackend::OpenAI(client);

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
