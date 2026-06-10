use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::api::OutcomeRewardModel;
use crate::api::ProcessRewardModel;
use crate::api::types::ChatMessages;
use crate::core::lms::litellm::LiteLLMClient;
use crate::api::types::ChatMessage;

/// Scoring mode for the LLM judge reward model.
///
/// Controls whether the judge evaluates each response independently (pointwise)
/// or compares all responses together and produces a ranking (groupwise).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgeMode {
    /// Score each response independently (current default behavior).
    Pointwise,
    /// Present all responses to the judge in a single prompt,
    /// ask for a ranking, and derive scores from placement.
    Groupwise,
}

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
    mode: JudgeMode,
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
            mode: JudgeMode::Pointwise,
        }
    }

    /// Create a judge with a specific scoring mode.
    pub fn with_mode(
        client: LiteLLMClient,
        criterion: String,
        temperature: f64,
        max_tokens: u32,
        mode: JudgeMode,
    ) -> Self {
        Self {
            client,
            criterion,
            temperature,
            max_tokens,
            mode,
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

    fn build_groupwise_prompt(
        &self,
        prompt_messages: &[ChatMessage],
        responses: &[String],
    ) -> Vec<ChatMessage> {
        let mut conversation_text = String::new();
        for msg in prompt_messages {
            let content = msg.extract_text_content();
            conversation_text.push_str(&format!("{}: {}\n", msg.role, content));
        }

        let mut responses_text = String::new();
        for (i, resp) in responses.iter().enumerate() {
            responses_text.push_str(&format!(
                "### Response {}\n{}\n\n",
                i + 1,
                resp
            ));
        }

        let judge_prompt = format!(
            "You are an expert judge evaluating the quality of AI assistant responses.\n\n\
             ## Evaluation Criterion\n{criterion}\n\n\
             ## Conversation\n{conversation}\n\
             ## Responses to Rank\n{responses}\n\
             ## Instructions\n\
             Compare all {n} responses based on the criterion above.\n\
             Rank them from best to worst.\n\
             You MUST end your response with a line in this exact format:\n\
             Ranking: [best_number, ..., worst_number]\n\
             where each number is the response number (1-indexed).\n\
             For example, if Response 3 is best and Response 1 is worst out of 3:\n\
             Ranking: [3, 2, 1]",
            criterion = self.criterion,
            conversation = conversation_text,
            responses = responses_text,
            n = responses.len(),
        );

        vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text(judge_prompt)),
            tool_calls: None,
            tool_call_id: None,
        }]
    }

    /// Parse a ranking list from the judge output and convert to scores.
    ///
    /// The judge is expected to output something like `Ranking: [3, 1, 2]`.
    /// The first-place entry gets score 1.0, last place gets 0.0, with
    /// intermediate places linearly interpolated.
    fn parse_ranking(text: &str, n: usize) -> Vec<f64> {
        let re = Regex::new(r"[Rr]anking:\s*\[([^\]]+)\]").unwrap();
        if let Some(caps) = re.captures(text) {
            if let Some(m) = caps.get(1) {
                let indices: Vec<usize> = m
                    .as_str()
                    .split(',')
                    .filter_map(|s| s.trim().parse::<usize>().ok())
                    .collect();

                if !indices.is_empty() && indices.len() <= n {
                    let mut scores = vec![0.0_f64; n];
                    let total_ranked = indices.len();
                    for (rank, &idx) in indices.iter().enumerate() {
                        if idx >= 1 && idx <= n {
                            let score = if total_ranked == 1 {
                                1.0
                            } else {
                                1.0 - (rank as f64 / (total_ranked - 1) as f64)
                            };
                            scores[idx - 1] = score;
                        }
                    }
                    return scores;
                }
            }
        }

        // Fallback: all equal scores
        vec![0.0; n]
    }
}

#[async_trait]
impl OutcomeRewardModel for LlmJudgeRewardModel {
    async fn score_batch(
        &self,
        prompt_messages: &[ChatMessage],
        responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error> {
        match self.mode {
            JudgeMode::Pointwise => self.score_batch_pointwise(prompt_messages, responses).await,
            JudgeMode::Groupwise => self.score_batch_groupwise(prompt_messages, responses).await,
        }
    }
}

impl LlmJudgeRewardModel {
    async fn score_batch_pointwise(
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
                        "LLM judge scored response (pointwise)"
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

    async fn score_batch_groupwise(
        &self,
        prompt_messages: &[ChatMessage],
        responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error> {
        if responses.is_empty() {
            return Ok(vec![]);
        }

        if responses.len() == 1 {
            return Ok(vec![1.0]);
        }

        let judge_messages = self.build_groupwise_prompt(prompt_messages, responses);

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

        match result {
            Ok(msg) => {
                let content = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let scores = Self::parse_ranking(content, responses.len());
                info!(
                    criterion = %self.criterion,
                    ?scores,
                    "LLM judge ranked responses (groupwise)"
                );
                Ok(scores)
            }
            Err(e) => {
                tracing::warn!(error = %e, "LLM groupwise judge call failed, returning uniform zeros");
                Ok(vec![0.0; responses.len()])
            }
        }
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
    fn test_parse_ranking_basic() {
        let scores = LlmJudgeRewardModel::parse_ranking(
            "Good analysis.\n\nRanking: [2, 3, 1]",
            3,
        );
        assert_eq!(scores.len(), 3);
        // Response 2 is 1st place (score 1.0), Response 3 is 2nd (0.5), Response 1 is 3rd (0.0)
        assert!((scores[1] - 1.0).abs() < f64::EPSILON); // Response 2
        assert!((scores[2] - 0.5).abs() < f64::EPSILON); // Response 3
        assert!((scores[0] - 0.0).abs() < f64::EPSILON); // Response 1
    }

    #[test]
    fn test_parse_ranking_two_items() {
        let scores = LlmJudgeRewardModel::parse_ranking("Ranking: [1, 2]", 2);
        assert_eq!(scores.len(), 2);
        assert!((scores[0] - 1.0).abs() < f64::EPSILON);
        assert!((scores[1] - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_ranking_no_match() {
        let scores = LlmJudgeRewardModel::parse_ranking("No ranking here", 3);
        assert_eq!(scores, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_parse_ranking_single() {
        let scores = LlmJudgeRewardModel::parse_ranking("Ranking: [1]", 1);
        assert_eq!(scores.len(), 1);
        assert!((scores[0] - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_llm_judge_groupwise_scoring() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-judge-gw",
                "object": "chat.completion",
                "created": 1700000000u64,
                "model": "judge-model",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Response 2 is the best because it is most accurate. Response 1 is okay. Response 3 is wrong.\n\nRanking: [2, 1, 3]"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 200, "completion_tokens": 80, "total_tokens": 280}
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

        let judge = LlmJudgeRewardModel::with_mode(
            client,
            "overall_quality".to_string(),
            0.0,
            4096,
            JudgeMode::Groupwise,
        );

        let prompt_msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("What is 2+2?".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let responses = vec![
            "The answer is 4.".to_string(),
            "2+2 equals 4, because addition is commutative.".to_string(),
            "Maybe 5?".to_string(),
        ];
        let scores = judge.score_batch(&prompt_msgs, &responses).await.unwrap();

        assert_eq!(scores.len(), 3);
        // Ranking [2, 1, 3]: Response 2 = 1.0, Response 1 = 0.5, Response 3 = 0.0
        assert!((scores[1] - 1.0).abs() < f64::EPSILON);
        assert!((scores[0] - 0.5).abs() < f64::EPSILON);
        assert!((scores[2] - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_llm_judge_groupwise_single_response() {
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

        let judge = LlmJudgeRewardModel::with_mode(
            client,
            "quality".to_string(),
            0.0,
            4096,
            JudgeMode::Groupwise,
        );

        let prompt_msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("hi".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let responses = vec!["hello".to_string()];
        let scores = judge.score_batch(&prompt_msgs, &responses).await.unwrap();
        assert_eq!(scores, vec![1.0]);
    }

    #[tokio::test]
    async fn test_llm_judge_groupwise_empty() {
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

        let judge = LlmJudgeRewardModel::with_mode(
            client,
            "quality".to_string(),
            0.0,
            4096,
            JudgeMode::Groupwise,
        );

        let prompt_msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("hi".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let responses: Vec<String> = vec![];
        let scores = judge.score_batch(&prompt_msgs, &responses).await.unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_groupwise_prompt_contains_all_responses() {
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

        let judge = LlmJudgeRewardModel::with_mode(
            client,
            "accuracy".to_string(),
            0.0,
            4096,
            JudgeMode::Groupwise,
        );

        let prompt_msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("What is 1+1?".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let responses = vec!["2".to_string(), "3".to_string(), "two".to_string()];
        let messages = judge.build_groupwise_prompt(&prompt_msgs, &responses);
        assert_eq!(messages.len(), 1);

        let content = messages[0].extract_text_content();
        assert!(content.contains("accuracy"));
        assert!(content.contains("Response 1"));
        assert!(content.contains("Response 2"));
        assert!(content.contains("Response 3"));
        assert!(content.contains("Ranking:"));
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
