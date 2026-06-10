use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::algorithms::best_of_n::OutcomeRewardModel;
use crate::algorithms::ProcessRewardModel;
use crate::chat_messages::ChatMessages;
use crate::client::litellm::LiteLLMClient;
use crate::types::ChatMessage;

/// HTTP-based process reward model that calls an external scoring endpoint.
///
/// Rust equivalent of Python's `LocalVllmProcessRewardModel`: instead of loading
/// a local vLLM process, it calls an HTTP endpoint for step scoring.
pub struct HttpProcessRewardModel {
    http: reqwest::Client,
    endpoint: String,
}

#[derive(Serialize)]
struct PrmScoreRequest<'a> {
    prompt: &'a str,
    response_prefix: &'a str,
}

#[derive(Deserialize)]
struct PrmScoreResponse {
    scores: Vec<f64>,
}

impl HttpProcessRewardModel {
    pub fn new(endpoint: &str) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to create HTTP client for PRM");

        Self {
            http,
            endpoint: endpoint.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl ProcessRewardModel for HttpProcessRewardModel {
    async fn score(
        &self,
        prompt_or_messages: &ChatMessages,
        steps: &[String],
    ) -> Result<Vec<f64>, anyhow::Error> {
        let prompt_text = prompt_or_messages.to_prompt();
        let response_prefix = steps.join("\n\n");

        let url = format!("{}/score", self.endpoint);
        let req_body = PrmScoreRequest {
            prompt: &prompt_text,
            response_prefix: &response_prefix,
        };

        let response = self
            .http
            .post(&url)
            .json(&req_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("PRM scoring failed with status {}: {}", status, body);
        }

        let score_response: PrmScoreResponse = response.json().await?;
        Ok(score_response.scores)
    }
}

/// LLM-as-a-judge reward model for scoring responses.
///
/// Port of Python's `LLMJudgeRewardModel`. Uses an LLM to evaluate responses
/// by constructing a judge prompt with the criterion and parsing numeric scores
/// from the output.
pub struct LlmJudgeRewardModel {
    client: LiteLLMClient,
    criterion: String,
    temperature: f64,
    max_tokens: u32,
}

impl LlmJudgeRewardModel {
    pub fn new(
        client: LiteLLMClient,
        criterion: String,
        temperature: f64,
        max_tokens: u32,
    ) -> Self {
        Self {
            client,
            criterion,
            temperature,
            max_tokens,
        }
    }

    fn build_judge_prompt(
        &self,
        prompt_messages: &[ChatMessage],
        response_text: &str,
    ) -> Vec<ChatMessage> {
        let mut conversation_text = String::new();
        for msg in prompt_messages {
            let content = msg.extract_text_content();
            conversation_text.push_str(&format!("{}: {}\n", msg.role, content));
        }

        let judge_prompt = format!(
            "You are an expert judge evaluating the quality of an AI assistant's response.\n\n\
             ## Evaluation Criterion\n{criterion}\n\n\
             ## Conversation\n{conversation}\n\
             ## Response to Evaluate\n{response}\n\n\
             ## Instructions\n\
             Evaluate the response based on the criterion above.\n\
             Provide your reasoning, then give a score from 0.0 to 1.0.\n\
             You MUST end your response with exactly: Score: X.X\n\
             where X.X is a decimal number between 0.0 and 1.0.",
            criterion = self.criterion,
            conversation = conversation_text,
            response = response_text,
        );

        vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text(judge_prompt)),
            tool_calls: None,
            tool_call_id: None,
        }]
    }

    fn parse_score(text: &str) -> f64 {
        let re = Regex::new(r"[Ss]core:\s*(\d+\.?\d*)").unwrap();
        if let Some(caps) = re.captures(text) {
            if let Some(m) = caps.get(1) {
                if let Ok(score) = m.as_str().parse::<f64>() {
                    return score.clamp(0.0, 1.0);
                }
            }
        }

        let num_re = Regex::new(r"\b(0\.\d+|1\.0|0|1)\b").unwrap();
        for m in num_re.find_iter(text) {
            if let Ok(val) = m.as_str().parse::<f64>() {
                if (0.0..=1.0).contains(&val) {
                    return val;
                }
            }
        }

        0.0
    }
}

#[async_trait]
impl OutcomeRewardModel for LlmJudgeRewardModel {
    async fn score_batch(
        &self,
        prompt_messages: &[ChatMessage],
        responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error> {
        let mut scores = Vec::with_capacity(responses.len());

        for response_text in responses {
            let judge_messages = self.build_judge_prompt(prompt_messages, response_text);

            let result = self
                .client
                .chat_completion(
                    &judge_messages,
                    Some(self.temperature),
                    Some(self.max_tokens),
                    None,
                    None,
                    None,
                )
                .await;

            let score = match result {
                Ok(msg) => {
                    let content = msg
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("");
                    let parsed = Self::parse_score(content);
                    info!(
                        criterion = %self.criterion,
                        score = parsed,
                        "LLM judge scored response"
                    );
                    parsed
                }
                Err(e) => {
                    tracing::warn!(error = %e, "LLM judge call failed, defaulting to 0.0");
                    0.0
                }
            };

            scores.push(score);
        }

        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_http_prm_score() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/score"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"scores": [0.9, 0.8, 0.7]})),
            )
            .mount(&server)
            .await;

        let prm = HttpProcessRewardModel::new(&server.uri());
        let cm = ChatMessages::from_string("What is 2+2?");
        let steps = vec![
            "Step 1: Add 2+2".to_string(),
            "Step 2: = 4".to_string(),
            "Step 3: Done".to_string(),
        ];

        let scores = prm.score(&cm, &steps).await.unwrap();
        assert_eq!(scores, vec![0.9, 0.8, 0.7]);
    }

    #[tokio::test]
    async fn test_http_prm_empty_steps() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/score"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"scores": []})))
            .mount(&server)
            .await;

        let prm = HttpProcessRewardModel::new(&server.uri());
        let cm = ChatMessages::from_string("prompt");
        let steps: Vec<String> = vec![];

        let scores = prm.score(&cm, &steps).await.unwrap();
        assert!(scores.is_empty());
    }

    #[tokio::test]
    async fn test_http_prm_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/score"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let prm = HttpProcessRewardModel::new(&server.uri());
        let cm = ChatMessages::from_string("prompt");
        let steps = vec!["step 1".to_string()];

        let result = prm.score(&cm, &steps).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_score_explicit() {
        assert!((LlmJudgeRewardModel::parse_score("Great work! Score: 0.85") - 0.85).abs() < f64::EPSILON);
        assert!((LlmJudgeRewardModel::parse_score("score: 0.7") - 0.7).abs() < f64::EPSILON);
        assert!((LlmJudgeRewardModel::parse_score("Score: 1.0") - 1.0).abs() < f64::EPSILON);
        assert!((LlmJudgeRewardModel::parse_score("Score: 0") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_score_clamp() {
        assert!((LlmJudgeRewardModel::parse_score("Score: 1.5") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_score_fallback() {
        assert!((LlmJudgeRewardModel::parse_score("The quality is 0.6 overall") - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_score_no_match() {
        assert!((LlmJudgeRewardModel::parse_score("No score here") - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_llm_judge_basic_scoring() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-judge",
                "object": "chat.completion",
                "created": 1700000000u64,
                "model": "judge-model",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "This response is well-structured and accurate.\n\nScore: 0.85"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150}
            })))
            .mount(&server)
            .await;

        let client = LiteLLMClient::new(
            "judge-model",
            "openai",
            Some("test-key"),
            Some(&format!("{}/v1", server.uri())),
            None,
            8,
            3,
            None,
            None,
            None,
            std::collections::HashMap::new(),
        )
        .unwrap();

        let judge = LlmJudgeRewardModel::new(
            client,
            "overall_quality".to_string(),
            0.0,
            4096,
        );

        let prompt_msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("What is 2+2?".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let responses = vec!["The answer is 4.".to_string()];
        let scores = judge.score_batch(&prompt_msgs, &responses).await.unwrap();

        assert_eq!(scores.len(), 1);
        assert!((scores[0] - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_llm_judge_criterion_formatting() {
        let client = LiteLLMClient::new(
            "model",
            "openai",
            Some("key"),
            None,
            None,
            8,
            3,
            None,
            None,
            None,
            std::collections::HashMap::new(),
        )
        .unwrap();

        let judge = LlmJudgeRewardModel::new(
            client,
            "mathematical_correctness".to_string(),
            0.0,
            4096,
        );

        let prompt_msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("Solve x+1=3".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let messages = judge.build_judge_prompt(&prompt_msgs, "x = 2");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");

        let content = messages[0].extract_text_content();
        assert!(content.contains("mathematical_correctness"));
        assert!(content.contains("Solve x+1=3"));
        assert!(content.contains("x = 2"));
        assert!(content.contains("Score: X.X"));
    }

    #[tokio::test]
    async fn test_llm_judge_multiple_responses() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "id": "chatcmpl-judge",
                    "object": "chat.completion",
                    "created": 1700000000u64,
                    "model": "judge-model",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Analysis complete.\n\nScore: 0.9"
                        },
                        "finish_reason": "stop"
                    }],
                    "usage": {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150}
                })),
            )
            .mount(&server)
            .await;

        let client = LiteLLMClient::new(
            "judge-model",
            "openai",
            Some("test-key"),
            Some(&format!("{}/v1", server.uri())),
            None,
            8,
            3,
            None,
            None,
            None,
            std::collections::HashMap::new(),
        )
        .unwrap();

        let judge = LlmJudgeRewardModel::new(
            client,
            "overall_quality".to_string(),
            0.0,
            4096,
        );

        let prompt_msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("What is 2+2?".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let responses = vec!["4".to_string(), "5".to_string()];
        let scores = judge.score_batch(&prompt_msgs, &responses).await.unwrap();
        assert_eq!(scores.len(), 2);
    }
}
