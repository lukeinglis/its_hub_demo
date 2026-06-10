use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{AlgorithmOutput, ScalingAlgorithm};
use crate::client::LmClient;
use crate::types::{extract_content_from_lm_response, ChatMessage};

#[async_trait]
pub trait OutcomeRewardModel: Send + Sync {
    async fn score_batch(
        &self,
        prompt_messages: &[ChatMessage],
        responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error>;
}

pub struct HttpOrmClient {
    http: reqwest::Client,
    endpoint: String,
}

#[derive(Serialize)]
struct ScoreRequest<'a> {
    prompt: &'a [ChatMessage],
    responses: &'a [String],
}

#[derive(Deserialize)]
struct ScoreResponse {
    scores: Vec<f64>,
}

impl HttpOrmClient {
    pub fn new(endpoint: &str) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to create HTTP client for ORM");

        Self {
            http,
            endpoint: endpoint.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl OutcomeRewardModel for HttpOrmClient {
    async fn score_batch(
        &self,
        prompt_messages: &[ChatMessage],
        responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error> {
        let url = format!("{}/score", self.endpoint);
        let body = ScoreRequest {
            prompt: prompt_messages,
            responses,
        };

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("ORM request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("ORM returned status {}: {}", status, body_text);
        }

        let score_resp: ScoreResponse = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("failed to parse ORM response: {}", e))?;

        Ok(score_resp.scores)
    }
}

pub fn dedupe_with_inverse(items: &[String]) -> (Vec<String>, Vec<usize>) {
    let mut uniques: Vec<String> = Vec::new();
    let mut index_of: HashMap<String, usize> = HashMap::new();
    let mut inverse: Vec<usize> = Vec::new();

    for item in items {
        let j = index_of.get(item).copied().unwrap_or_else(|| {
            let j = uniques.len();
            index_of.insert(item.clone(), j);
            uniques.push(item.clone());
            j
        });
        inverse.push(j);
    }

    (uniques, inverse)
}

pub struct BestOfN {
    orm: Box<dyn OutcomeRewardModel>,
    replace_error_with_message: Option<String>,
}

impl BestOfN {
    pub fn new(orm: Box<dyn OutcomeRewardModel>) -> Self {
        Self {
            orm,
            replace_error_with_message: None,
        }
    }

    pub fn with_error_replacement(
        orm: Box<dyn OutcomeRewardModel>,
        replace_error_with_message: Option<String>,
    ) -> Self {
        Self {
            orm,
            replace_error_with_message,
        }
    }

    fn process_responses(
        &self,
        responses: Vec<Value>,
        scores: Vec<f64>,
        selected_index: usize,
        return_response_only: bool,
    ) -> AlgorithmOutput {
        if return_response_only {
            AlgorithmOutput::ResponseOnly(responses[selected_index].clone())
        } else {
            let metadata = serde_json::json!({
                "algorithm": "best-of-n",
                "responses": responses,
                "scores": scores,
                "selected_index": selected_index,
            });
            AlgorithmOutput::Full {
                selected: responses[selected_index].clone(),
                metadata,
            }
        }
    }
}

#[async_trait]
impl ScalingAlgorithm for BestOfN {
    async fn infer(
        &self,
        client: &LmClient,
        messages: &[ChatMessage],
        budget: u32,
        return_response_only: bool,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error> {
        let results = client
            .fan_out(messages, budget, temperature, max_tokens, tools, tool_choice)
            .await;

        let fallback_msg = self
            .replace_error_with_message
            .as_deref()
            .unwrap_or("Error during generation");

        let mut responses = Vec::new();
        let mut all_failed = true;
        for result in results {
            match result {
                Ok(msg) => {
                    all_failed = false;
                    responses.push(msg);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "fan-out request failed, substituting error response");
                    responses.push(serde_json::json!({
                        "role": "assistant",
                        "content": format!("{}: {}", fallback_msg, e)
                    }));
                }
            }
        }

        if all_failed {
            anyhow::bail!("all fan-out requests failed");
        }

        if responses.is_empty() {
            anyhow::bail!("No responses to process");
        }

        let response_contents: Vec<String> = responses
            .iter()
            .map(extract_content_from_lm_response)
            .collect();

        let (unique_responses, inverse_idx) = dedupe_with_inverse(&response_contents);

        if unique_responses.len() == 1 {
            let scores = vec![1.0; responses.len()];
            return Ok(self.process_responses(responses, scores, 0, return_response_only));
        }

        let unique_scores = self.orm.score_batch(messages, &unique_responses).await?;

        let scores: Vec<f64> = inverse_idx
            .iter()
            .map(|&idx| unique_scores[idx])
            .collect();

        let max_score = scores
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let selected_index = scores
            .iter()
            .position(|&s| s == max_score)
            .unwrap_or(0);

        Ok(self.process_responses(responses, scores, selected_index, return_response_only))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dedupe_basic() {
        let items: Vec<String> = vec!["a", "b", "a", "c", "b"]
            .into_iter()
            .map(String::from)
            .collect();
        let (uniques, inverse) = dedupe_with_inverse(&items);
        assert_eq!(uniques, vec!["a", "b", "c"]);
        assert_eq!(inverse, vec![0, 1, 0, 2, 1]);
    }

    #[test]
    fn dedupe_single_unique() {
        let items: Vec<String> = vec!["a", "a", "a"]
            .into_iter()
            .map(String::from)
            .collect();
        let (uniques, inverse) = dedupe_with_inverse(&items);
        assert_eq!(uniques, vec!["a"]);
        assert_eq!(inverse, vec![0, 0, 0]);
    }

    #[test]
    fn dedupe_all_unique() {
        let items: Vec<String> = vec!["a", "b", "c"]
            .into_iter()
            .map(String::from)
            .collect();
        let (uniques, inverse) = dedupe_with_inverse(&items);
        assert_eq!(uniques, vec!["a", "b", "c"]);
        assert_eq!(inverse, vec![0, 1, 2]);
    }

    #[test]
    fn dedupe_empty() {
        let items: Vec<String> = vec![];
        let (uniques, inverse) = dedupe_with_inverse(&items);
        assert!(uniques.is_empty());
        assert!(inverse.is_empty());
    }

    #[test]
    fn dedupe_preserves_first_occurrence_order() {
        let items: Vec<String> = vec!["c", "b", "a", "b", "c"]
            .into_iter()
            .map(String::from)
            .collect();
        let (uniques, inverse) = dedupe_with_inverse(&items);
        assert_eq!(uniques, vec!["c", "b", "a"]);
        assert_eq!(inverse, vec![0, 1, 2, 1, 0]);
    }

    struct MockOrm {
        scores: Vec<f64>,
    }

    #[async_trait]
    impl OutcomeRewardModel for MockOrm {
        async fn score_batch(
            &self,
            _prompt_messages: &[ChatMessage],
            _responses: &[String],
        ) -> Result<Vec<f64>, anyhow::Error> {
            Ok(self.scores.clone())
        }
    }

    fn content_response(content: &str) -> Value {
        json!({"role": "assistant", "content": content})
    }

    #[test]
    fn score_mapping_through_inverse_index() {
        let contents: Vec<String> = vec!["a", "b", "a", "c", "b"]
            .into_iter()
            .map(String::from)
            .collect();
        let (_, inverse_idx) = dedupe_with_inverse(&contents);
        let unique_scores = vec![0.3, 0.9, 0.5];
        let scores: Vec<f64> = inverse_idx
            .iter()
            .map(|&idx| unique_scores[idx])
            .collect();
        assert_eq!(scores, vec![0.3, 0.9, 0.3, 0.5, 0.9]);
    }

    #[test]
    fn select_max_first_occurrence() {
        let scores = vec![0.3, 0.9, 0.5, 0.9];
        let max_score = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let selected = scores.iter().position(|&s| s == max_score).unwrap();
        assert_eq!(selected, 1);
    }

    #[test]
    fn select_max_tie_takes_first() {
        let scores = vec![0.5, 0.8, 0.8, 0.3];
        let max_score = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let selected = scores.iter().position(|&s| s == max_score).unwrap();
        assert_eq!(selected, 1);
    }

    #[tokio::test]
    async fn best_of_n_selects_highest_scored() {
        let orm = MockOrm {
            scores: vec![0.3, 0.9, 0.5],
        };
        let bon = BestOfN::new(Box::new(orm));

        let responses = vec![
            content_response("answer_a"),
            content_response("answer_b"),
            content_response("answer_c"),
        ];

        let scores_mapped: Vec<f64> = vec![0.3, 0.9, 0.5];
        let max_score = scores_mapped
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let selected_index = scores_mapped
            .iter()
            .position(|&s| s == max_score)
            .unwrap();

        let result = bon.process_responses(responses, scores_mapped, selected_index, true);
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert_eq!(selected["content"], "answer_b");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn best_of_n_all_same_skips_scoring() {
        struct ShouldNotBeCalled;

        #[async_trait]
        impl OutcomeRewardModel for ShouldNotBeCalled {
            async fn score_batch(
                &self,
                _prompt: &[ChatMessage],
                _responses: &[String],
            ) -> Result<Vec<f64>, anyhow::Error> {
                panic!("score_batch should not be called when all responses are identical");
            }
        }

        let contents: Vec<String> = vec!["same", "same", "same"]
            .into_iter()
            .map(String::from)
            .collect();
        let (uniques, _) = dedupe_with_inverse(&contents);
        assert_eq!(uniques.len(), 1);

        let responses = vec![
            content_response("same"),
            content_response("same"),
            content_response("same"),
        ];
        let scores = vec![1.0; responses.len()];
        let bon = BestOfN::new(Box::new(ShouldNotBeCalled));
        let result = bon.process_responses(responses, scores.clone(), 0, true);
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert_eq!(selected["content"], "same");
            }
            _ => panic!("expected ResponseOnly"),
        }
        assert_eq!(scores, vec![1.0, 1.0, 1.0]);
    }

    #[tokio::test]
    async fn best_of_n_single_response() {
        let orm = MockOrm {
            scores: vec![0.7],
        };
        let bon = BestOfN::new(Box::new(orm));

        let responses = vec![content_response("only one")];
        let contents: Vec<String> = responses
            .iter()
            .map(extract_content_from_lm_response)
            .collect();
        let (uniques, _) = dedupe_with_inverse(&contents);
        assert_eq!(uniques.len(), 1);

        let scores = vec![1.0];
        let result = bon.process_responses(responses, scores, 0, true);
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert_eq!(selected["content"], "only one");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn best_of_n_full_output_includes_metadata() {
        let orm = MockOrm {
            scores: vec![0.4, 0.8],
        };
        let bon = BestOfN::new(Box::new(orm));

        let responses = vec![
            content_response("worse"),
            content_response("better"),
        ];
        let scores = vec![0.4, 0.8];
        let result = bon.process_responses(responses, scores, 1, false);
        match result {
            AlgorithmOutput::Full { selected, metadata } => {
                assert_eq!(selected["content"], "better");
                assert_eq!(metadata["algorithm"], "best-of-n");
                assert_eq!(metadata["selected_index"], 1);
                assert_eq!(metadata["scores"].as_array().unwrap().len(), 2);
                assert_eq!(metadata["responses"].as_array().unwrap().len(), 2);
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn best_of_n_with_duplicates_maps_scores() {
        let orm = MockOrm {
            scores: vec![0.3, 0.9],
        };
        let bon = BestOfN::new(Box::new(orm));

        let responses = vec![
            content_response("a"),
            content_response("b"),
            content_response("a"),
            content_response("b"),
            content_response("a"),
        ];
        let contents: Vec<String> = responses
            .iter()
            .map(extract_content_from_lm_response)
            .collect();
        let (uniques, inverse_idx) = dedupe_with_inverse(&contents);

        assert_eq!(uniques, vec!["a", "b"]);
        assert_eq!(inverse_idx, vec![0, 1, 0, 1, 0]);

        let unique_scores = vec![0.3, 0.9];
        let scores: Vec<f64> = inverse_idx
            .iter()
            .map(|&idx| unique_scores[idx])
            .collect();
        assert_eq!(scores, vec![0.3, 0.9, 0.3, 0.9, 0.3]);

        let max_score = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let selected_index = scores.iter().position(|&s| s == max_score).unwrap();
        assert_eq!(selected_index, 1);

        let result = bon.process_responses(responses, scores, selected_index, true);
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert_eq!(selected["content"], "b");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn best_of_n_with_tool_call_responses() {
        let orm = MockOrm {
            scores: vec![0.5, 0.8],
        };
        let bon = BestOfN::new(Box::new(orm));

        let r1 = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{\"city\":\"London\"}"
                }
            }]
        });
        let r2 = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_2",
                "type": "function",
                "function": {
                    "name": "get_time",
                    "arguments": "{\"tz\":\"UTC\"}"
                }
            }]
        });

        let responses = vec![r1, r2];
        let contents: Vec<String> = responses
            .iter()
            .map(extract_content_from_lm_response)
            .collect();
        let (uniques, inverse_idx) = dedupe_with_inverse(&contents);
        assert_eq!(uniques.len(), 2);

        let unique_scores = vec![0.5, 0.8];
        let scores: Vec<f64> = inverse_idx
            .iter()
            .map(|&idx| unique_scores[idx])
            .collect();

        let max_score = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let selected_index = scores.iter().position(|&s| s == max_score).unwrap();
        assert_eq!(selected_index, 1);

        let result = bon.process_responses(responses, scores, selected_index, true);
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                let name = selected["tool_calls"][0]["function"]["name"]
                    .as_str()
                    .unwrap();
                assert_eq!(name, "get_time");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn http_orm_client_wiremock() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/score"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"scores": [0.3, 0.9, 0.5]})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = HttpOrmClient::new(&server.uri());

        let prompt = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("What is 2+2?".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];
        let responses = vec![
            "answer_a".to_string(),
            "answer_b".to_string(),
            "answer_c".to_string(),
        ];

        let scores = client.score_batch(&prompt, &responses).await.unwrap();
        assert_eq!(scores, vec![0.3, 0.9, 0.5]);
    }

    #[tokio::test]
    async fn http_orm_client_error_handling() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/score"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .expect(1)
            .mount(&server)
            .await;

        let client = HttpOrmClient::new(&server.uri());

        let prompt = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("test".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];
        let responses = vec!["a".to_string()];

        let result = client.score_batch(&prompt, &responses).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("500"));
    }
}
