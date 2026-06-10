use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::api::{AlgorithmOutput, ProcessRewardModel, ScalingAlgorithm};
use crate::core::lms::step_generation::StepGeneration;
use crate::core::lms::LmClient;
use crate::api::types::ChatMessage;

/// Convert a slice of ChatMessage into a single prompt string for step-generation.
fn messages_to_prompt(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.extract_text_content()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone)]
pub struct Path {
    pub steps: Vec<String>,
    pub is_stopped: bool,
    pub score: f64,
}

impl Default for Path {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            is_stopped: false,
            score: 0.0,
        }
    }
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn deepcopy(&self) -> Self {
        Self {
            steps: self.steps.clone(),
            is_stopped: self.is_stopped,
            score: self.score,
        }
    }
}

pub struct BeamSearch {
    sg: StepGeneration,
    prm: Arc<dyn ProcessRewardModel>,
    beam_width: usize,
}

impl BeamSearch {
    pub fn new(sg: StepGeneration, prm: Arc<dyn ProcessRewardModel>, beam_width: usize) -> Self {
        Self {
            sg,
            prm,
            beam_width,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn search_one_level(
        &self,
        client: &LmClient,
        candidates: &mut [Path],
        prompt: &str,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<(), anyhow::Error> {
        let was_stopped: Vec<bool> = candidates.iter().map(|c| c.is_stopped).collect();

        let mut prompts: Vec<&str> = Vec::new();
        let mut steps_so_far: Vec<&[String]> = Vec::new();
        for (c, &stopped) in candidates.iter().zip(was_stopped.iter()) {
            if stopped {
                continue;
            }
            prompts.push(prompt);
            steps_so_far.push(&c.steps);
        }

        let sg_results = self
            .forward_steps(client, &prompts, &steps_so_far, temperature, max_tokens, tools, tool_choice)
            .await?;

        let max_steps = self.sg.max_steps as usize;
        let mut i = 0;
        for (c, &stopped) in candidates.iter_mut().zip(was_stopped.iter()) {
            if stopped {
                continue;
            }
            let (next_step, is_stopped) = &sg_results[i];
            c.steps.push(next_step.clone());
            c.is_stopped = *is_stopped || c.steps.len() >= max_steps;
            i += 1;
        }

        let mut score_inputs: Vec<String> = Vec::new();
        for (c, &stopped) in candidates.iter().zip(was_stopped.iter()) {
            if stopped {
                continue;
            }
            score_inputs.push(self.sg.post_process(&c.steps, true));
        }

        let prompt_messages = &[ChatMessage {
            role: "user".to_string(),
            content: Some(crate::api::types::Content::Text(prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];
        let scores = self.score_batch(prompt_messages, &score_inputs).await?;

        let mut i = 0;
        for (c, &stopped) in candidates.iter_mut().zip(was_stopped.iter()) {
            if stopped {
                continue;
            }
            c.score = scores[i];
            i += 1;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn forward_steps(
        &self,
        client: &LmClient,
        prompts: &[&str],
        steps_so_far: &[&[String]],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<Vec<(String, bool)>, anyhow::Error> {
        let mut results = Vec::with_capacity(prompts.len());

        for (prompt, steps) in prompts.iter().zip(steps_so_far.iter()) {
            let result = self
                .sg
                .forward_step(client, prompt, steps, temperature, max_tokens, tools, tool_choice)
                .await?;
            results.push(result);
        }

        Ok(results)
    }

    async fn score_batch(
        &self,
        prompt_messages: &[ChatMessage],
        responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error> {
        let mut scores = Vec::with_capacity(responses.len());
        for response in responses {
            let steps = vec![response.clone()];
            let step_scores = self.prm.score(prompt_messages, &steps).await?;
            let score = step_scores.last().copied().unwrap_or(0.0);
            scores.push(score);
        }
        Ok(scores)
    }
}

#[async_trait]
impl ScalingAlgorithm for BeamSearch {
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
        let budget = budget as usize;
        let bw = self.beam_width;

        if budget < bw {
            anyhow::bail!("budget must be greater than or equal to beam_width");
        }
        if !budget.is_multiple_of(bw) {
            anyhow::bail!("budget must be divisible by beam_width");
        }

        let num_beams = budget / bw;

        let prompt = messages_to_prompt(messages);

        let mut candidates: Vec<Path> = (0..num_beams).map(|_| Path::new()).collect();

        while !candidates.iter().all(|c| c.is_stopped) {
            self.search_one_level(
                client,
                &mut candidates,
                &prompt,
                temperature,
                max_tokens,
                tools,
                tool_choice,
            )
            .await?;

            candidates.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            candidates.truncate(bw);

            let mut new_candidates = Vec::with_capacity(num_beams * bw);
            for _ in 0..num_beams {
                for c in &candidates {
                    new_candidates.push(c.deepcopy());
                }
            }
            candidates = new_candidates;
        }

        let scores: Vec<f64> = candidates.iter().map(|c| c.score).collect();
        let steps_used: Vec<usize> = candidates.iter().map(|c| c.steps.len()).collect();
        let responses: Vec<Value> = candidates
            .iter()
            .map(|c| {
                serde_json::json!({
                    "role": "assistant",
                    "content": self.sg.post_process(&c.steps, true),
                })
            })
            .collect();

        let selected_index = scores
            .iter()
            .enumerate()
            .reduce(|(max_i, max_v), (i, v)| {
                if v > max_v { (i, v) } else { (max_i, max_v) }
            })
            .map(|(i, _)| i)
            .unwrap_or(0);

        if return_response_only {
            Ok(AlgorithmOutput::ResponseOnly(
                responses[selected_index].clone(),
            ))
        } else {
            let metadata = serde_json::json!({
                "algorithm": "beam-search",
                "responses": responses,
                "scores": scores,
                "selected_index": selected_index,
                "steps_used": steps_used,
            });
            Ok(AlgorithmOutput::Full {
                selected: responses[selected_index].clone(),
                metadata,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockPRM {
        scores: Vec<f64>,
        call_count: AtomicUsize,
    }

    impl MockPRM {
        fn new(scores: Vec<f64>) -> Self {
            Self {
                scores,
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl ProcessRewardModel for MockPRM {
        async fn score(
            &self,
            _prompt_messages: &[ChatMessage],
            _steps: &[String],
        ) -> Result<Vec<f64>, anyhow::Error> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let score = self.scores[idx % self.scores.len()];
            Ok(vec![score])
        }
    }

    fn make_client(server_uri: &str) -> LmClient {
        LmClient::new(
            &format!("{}/v1", server_uri),
            Some("test-key"),
            "test-model",
            8,
            3,
            None,
            None,
        )
        .unwrap()
    }

    fn user_message(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: Some(crate::api::types::Content::Text(text.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn system_message(text: &str) -> ChatMessage {
        ChatMessage {
            role: "system".to_string(),
            content: Some(crate::api::types::Content::Text(text.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn mock_chat_response(content: &str) -> serde_json::Value {
        serde_json::json!({
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

    #[test]
    fn test_path_creation() {
        let path = Path::new();
        assert!(path.steps.is_empty());
        assert!(!path.is_stopped);
        assert!((path.score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_path_deepcopy() {
        let original_steps = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let path = Path {
            steps: original_steps.clone(),
            is_stopped: false,
            score: 1.0,
        };
        let mut copied = path.deepcopy();
        copied.steps.push("d".to_string());

        assert_eq!(path.steps, original_steps);
        assert_eq!(copied.steps.len(), 4);
        assert!(!copied.is_stopped);
        assert!((copied.score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_path_deepcopy_independence() {
        let path = Path {
            steps: vec!["step1".to_string()],
            is_stopped: true,
            score: 0.5,
        };
        let copy = path.deepcopy();

        assert_eq!(copy.steps, vec!["step1".to_string()]);
        assert!(copy.is_stopped);
        assert!((copy.score - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_beam_search_result_structure() {
        let responses = vec![
            serde_json::json!({"role": "assistant", "content": "response1"}),
            serde_json::json!({"role": "assistant", "content": "response2"}),
            serde_json::json!({"role": "assistant", "content": "response3"}),
        ];
        let scores = vec![0.5, 0.8, 0.3];
        let steps_used = vec![2, 3, 1];

        let selected_index = scores
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);

        assert_eq!(selected_index, 1);
        assert_eq!(responses[selected_index]["content"], "response2");

        let metadata = serde_json::json!({
            "algorithm": "beam-search",
            "responses": responses,
            "scores": scores,
            "selected_index": selected_index,
            "steps_used": steps_used,
        });

        assert_eq!(metadata["selected_index"], 1);
        assert_eq!(metadata["steps_used"].as_array().unwrap().len(), 3);
        assert_eq!(metadata["scores"].as_array().unwrap().len(), 3);
        assert_eq!(metadata["responses"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_beam_search_budget_validation() {
        use wiremock::MockServer;

        let server = MockServer::start().await;
        let client = make_client(&server.uri());

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.5]));
        let bs = BeamSearch::new(sg, prm, 2);

        let messages = vec![user_message("test")];

        let result = bs
            .infer(&client, &messages, 3, true, None, None, None, None)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("budget must be divisible by beam_width"));
    }

    #[tokio::test]
    async fn test_beam_search_budget_too_small() {
        use wiremock::MockServer;

        let server = MockServer::start().await;
        let client = make_client(&server.uri());

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.5]));
        let bs = BeamSearch::new(sg, prm, 4);

        let messages = vec![user_message("test")];

        let result = bs
            .infer(&client, &messages, 2, true, None, None, None, None)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("budget must be greater than or equal to beam_width"));
    }

    #[tokio::test]
    async fn test_beam_search_basic() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.9]));
        let bs = BeamSearch::new(sg, prm, 2);

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this problem:")];

        let result = bs
            .infer(&client, &messages, 2, true, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::ResponseOnly(val) => {
                assert!(val.get("content").is_some());
                assert_eq!(val["role"], "assistant");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn test_beam_search_full_output() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("good_step")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.9, 0.1]));
        let bs = BeamSearch::new(sg, prm, 2);

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = bs
            .infer(&client, &messages, 4, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { selected, metadata } => {
                assert_eq!(selected["role"], "assistant");
                assert_eq!(metadata["algorithm"], "beam-search");
                assert!(metadata.get("responses").is_some());
                assert!(metadata.get("scores").is_some());
                assert!(metadata.get("selected_index").is_some());
                assert!(metadata.get("steps_used").is_some());

                let sel_idx = metadata["selected_index"].as_u64().unwrap() as usize;
                let scores = metadata["scores"].as_array().unwrap();
                let max_score = scores
                    .iter()
                    .map(|s| s.as_f64().unwrap())
                    .fold(f64::NEG_INFINITY, f64::max);
                let max_idx = scores
                    .iter()
                    .position(|s| (s.as_f64().unwrap() - max_score).abs() < f64::EPSILON)
                    .unwrap();
                assert_eq!(sel_idx, max_idx);
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn test_beam_search_with_chat_messages_string() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.9]));
        let bs = BeamSearch::new(sg, prm, 2);

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this problem:")];

        let result = bs
            .infer(&client, &messages, 2, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { selected, .. } => {
                assert_eq!(selected["role"], "assistant");
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn test_beam_search_with_conversation_history() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.8, 0.6]));
        let bs = BeamSearch::new(sg, prm, 2);

        let client = make_client(&server.uri());
        let messages = vec![
            system_message("You are a problem solver"),
            user_message("Solve step by step:"),
        ];

        let result = bs
            .infer(&client, &messages, 2, true, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::ResponseOnly(val) => {
                assert_eq!(val["role"], "assistant");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn test_beam_search_selects_highest_score() {
        use std::sync::Mutex;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, Respond, Request, ResponseTemplate};

        let server = MockServer::start().await;

        struct AlternatingResponder {
            responses: Vec<&'static str>,
            counter: Mutex<usize>,
        }

        impl Respond for AlternatingResponder {
            fn respond(&self, _request: &Request) -> ResponseTemplate {
                let mut counter = self.counter.lock().unwrap();
                let content = self.responses[*counter % self.responses.len()];
                *counter += 1;
                ResponseTemplate::new(200).set_body_json(mock_chat_response(content))
            }
        }

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(AlternatingResponder {
                responses: vec!["good_step", "bad_step"],
                counter: Mutex::new(0),
            })
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.9, 0.1, 0.8, 0.2]));
        let bs = BeamSearch::new(sg, prm, 2);

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = bs
            .infer(&client, &messages, 4, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { metadata, .. } => {
                let scores = metadata["scores"].as_array().unwrap();
                let selected = metadata["selected_index"].as_u64().unwrap() as usize;
                let max_score = scores
                    .iter()
                    .map(|s| s.as_f64().unwrap())
                    .fold(f64::NEG_INFINITY, f64::max);
                let expected_idx = scores
                    .iter()
                    .position(|s| (s.as_f64().unwrap() - max_score).abs() < f64::EPSILON)
                    .unwrap();
                assert_eq!(selected, expected_idx);
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn test_beam_search_early_stop_on_empty() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            10,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.5]));
        let bs = BeamSearch::new(sg, prm, 2);

        let client = make_client(&server.uri());
        let messages = vec![user_message("test")];

        let result = bs
            .infer(&client, &messages, 2, true, None, None, None, None)
            .await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_post_process_integration() {
        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            5,
            None,
            0.8,
            false,
            None,
        );

        let steps = vec!["step1".to_string(), "step2".to_string(), "step3".to_string()];
        let processed = sg.post_process(&steps, true);
        assert_eq!(processed, "step1\nstep2\nstep3");

        let processed_not_stopped = sg.post_process(&steps, false);
        assert_eq!(processed_not_stopped, "step1\nstep2\nstep3\n");
    }

    #[test]
    fn test_beam_search_construction() {
        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            5,
            None,
            0.8,
            false,
            None,
        );
        let prm = Arc::new(MockPRM::new(vec![0.5]));
        let bs = BeamSearch::new(sg, prm, 3);

        assert_eq!(bs.beam_width, 3);
    }

    #[test]
    fn test_build_stop_string_with_step_token() {
        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            5,
            None,
            0.8,
            false,
            None,
        );

        let stop = sg.build_stop_string();
        assert_eq!(stop, Some("\n".to_string()));
    }

    #[test]
    fn test_build_stop_string_with_stop_token_only() {
        let sg = StepGeneration::with_tokens_per_step(50, 5, Some("END".to_string()), 0.8, false, None)
            .unwrap();

        let stop = sg.build_stop_string();
        assert_eq!(stop, Some("END".to_string()));
    }

    #[test]
    fn test_contains_stop_token() {
        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            5,
            Some("END".to_string()),
            0.8,
            false,
            None,
        );

        assert!(sg.contains_stop_token("some text END here"));
        assert!(!sg.contains_stop_token("no stop here"));
    }
}
