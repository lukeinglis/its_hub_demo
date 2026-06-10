use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use its_hub_rs::SelfConsistency;
use its_hub_rs::{BestOfN, OutcomeRewardModel};
use its_hub_rs::ScalingAlgorithm;
use its_hub_rs::LmClient;
use its_hub_rs::api::types::{ChatMessage, Content};

fn user_message(text: &str) -> ChatMessage {
    ChatMessage {
        role: "user".to_string(),
        content: Some(Content::Text(text.to_string())),
        tool_calls: None,
        tool_call_id: None,
    }
}

fn make_chat_response(content: &str) -> serde_json::Value {
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

#[tokio::test]
async fn test_self_consistency_infer_flat() {
    let server = MockServer::start().await;

    let responses = vec![
        make_chat_response("42"),
        make_chat_response("42"),
        make_chat_response("42"),
        make_chat_response("7"),
        make_chat_response("99"),
    ];

    for resp in responses {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let sc = SelfConsistency::new(None, None).unwrap();
    let messages = vec![user_message("What is 6*7?")];

    let result = sc
        .infer(&client, &messages, 5, true, Some(0.7), None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::ResponseOnly(selected) => {
            assert_eq!(selected["content"].as_str().unwrap(), "42");
        }
        _ => panic!("expected ResponseOnly"),
    }
}

#[tokio::test]
async fn test_self_consistency_infer_with_regex() {
    let server = MockServer::start().await;

    let responses = vec![
        make_chat_response(r"Step 1: compute. The answer is \boxed{42}"),
        make_chat_response(r"Working it out... \boxed{42}"),
        make_chat_response(r"Result: \boxed{42}"),
        make_chat_response(r"I think \boxed{7}"),
        make_chat_response(r"Maybe \boxed{99}"),
    ];

    for resp in responses {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let sc = SelfConsistency::new(
        Some(vec![r"\\boxed\{([^}]+)\}".to_string()]),
        None,
    )
    .unwrap();
    let messages = vec![user_message("What is 6*7?")];

    let result = sc
        .infer(&client, &messages, 5, false, Some(0.7), None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::Full { metadata, .. } => {
            let counts = &metadata["response_counts"];
            assert!(
                counts.as_object().is_some(),
                "response_counts should be an object"
            );
        }
        _ => panic!("expected Full"),
    }
}

#[allow(dead_code)]
struct MockOrm {
    scores: Vec<f64>,
}

#[async_trait::async_trait]
impl OutcomeRewardModel for MockOrm {
    async fn score_batch(
        &self,
        _prompt: &[ChatMessage],
        _responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error> {
        Ok(self.scores.clone())
    }
}

#[tokio::test]
async fn test_best_of_n_infer_with_mock_orm() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("answer_a")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("answer_b")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("answer_c")))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let orm_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/score"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"scores": [0.3, 0.9, 0.5]})),
        )
        .mount(&orm_server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let orm = its_hub_rs::HttpOrmClient::new(&orm_server.uri());
    let bon = BestOfN::new(Box::new(orm));
    let messages = vec![user_message("test")];

    let result = bon
        .infer(&client, &messages, 3, true, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::ResponseOnly(selected) => {
            let content = selected["content"].as_str().unwrap();
            assert!(
                ["answer_a", "answer_b", "answer_c"].contains(&content),
                "selected response should be one of the generated answers"
            );
        }
        _ => panic!("expected ResponseOnly"),
    }
}

struct PanickingOrm;

#[async_trait::async_trait]
impl OutcomeRewardModel for PanickingOrm {
    async fn score_batch(
        &self,
        _prompt: &[ChatMessage],
        _responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error> {
        panic!("ORM should not be called when all responses are identical");
    }
}

fn make_multimodal_chat_response(parts: &[(&str, &str)]) -> serde_json::Value {
    let content_parts: Vec<serde_json::Value> = parts
        .iter()
        .map(|(ptype, text)| {
            json!({ "type": ptype, "text": text })
        })
        .collect();

    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1700000000u64,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content_parts},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    })
}

fn make_tool_call_chat_response(name: &str, args: serde_json::Value) -> serde_json::Value {
    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1700000000u64,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": args
                    }
                }]
            },
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    })
}

#[tokio::test]
async fn test_best_of_n_dedup_skips_scoring() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("same")))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let bon = BestOfN::new(Box::new(PanickingOrm));
    let messages = vec![user_message("test")];

    let result = bon
        .infer(&client, &messages, 3, true, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::ResponseOnly(selected) => {
            assert_eq!(selected["content"].as_str().unwrap(), "same");
        }
        _ => panic!("expected ResponseOnly"),
    }
}

#[tokio::test]
async fn test_fan_out_graceful_degradation() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("good")))
        .up_to_n_times(3)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        1,
        None,
        None,
    )
    .unwrap();

    let sc = SelfConsistency::with_error_replacement(
        None,
        None,
        Some("Generation failed".to_string()),
    )
    .unwrap();

    let messages = vec![user_message("test")];

    let result = sc
        .infer(&client, &messages, 5, true, None, None, None, None)
        .await;

    assert!(result.is_ok(), "algorithm should succeed with partial failures");
}

#[tokio::test]
async fn test_fan_out_all_fail() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        1,
        None,
        None,
    )
    .unwrap();

    let sc = SelfConsistency::new(None, None).unwrap();
    let messages = vec![user_message("test")];

    let result = sc
        .infer(&client, &messages, 3, true, None, None, None, None)
        .await;

    assert!(result.is_err(), "all-fail should return error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("all fan-out requests failed"),
        "error should mention all requests failed, got: {}",
        err_msg
    );
}

// --- SelfConsistency: flat projection via process_responses ---

#[test]
fn test_self_consistency_flat_projection_process_responses() {
    use its_hub_rs::SelfConsistency;
    use its_hub_rs::AlgorithmOutput;

    let sc = SelfConsistency::new(None, None).unwrap();
    let responses = vec![
        json!({"role": "assistant", "content": "answer1"}),
        json!({"role": "assistant", "content": "answer2"}),
        json!({"role": "assistant", "content": "answer1"}),
        json!({"role": "assistant", "content": "answer3"}),
    ];

    let result = sc.process_responses(responses, false).unwrap();
    match result {
        AlgorithmOutput::Full { selected, metadata } => {
            assert_eq!(selected["content"].as_str().unwrap(), "answer1");
            assert_eq!(metadata["algorithm"], "self-consistency");
            let counts = metadata["response_counts"].as_object().unwrap();
            assert_eq!(counts["answer1"].as_u64().unwrap(), 2);
            assert_eq!(counts["answer2"].as_u64().unwrap(), 1);
            assert_eq!(counts["answer3"].as_u64().unwrap(), 1);
            assert!(metadata.get("selected_index").is_some());
            assert!(metadata.get("all_responses").is_some());
        }
        _ => panic!("expected Full"),
    }
}

// --- SelfConsistency: hierarchical projection ---

#[test]
fn test_self_consistency_hierarchical_projection_process_responses() {
    use its_hub_rs::SelfConsistency;
    use its_hub_rs::AlgorithmOutput;

    let sc = SelfConsistency::new(
        Some(vec![r"Approach:\s*(\w+)".into(), r"\\boxed\{([^}]+)\}".into()]),
        None,
    )
    .unwrap();

    let responses = vec![
        json!({"role": "assistant", "content": r"Approach: algebra\nSolution: \boxed{42}"}),
        json!({"role": "assistant", "content": r"Approach: algebra\nSolution: \boxed{24}"}),
        json!({"role": "assistant", "content": r"Approach: geometry\nSolution: \boxed{42}"}),
        json!({"role": "assistant", "content": r"Approach: calculus\nSolution: \boxed{30}"}),
    ];

    let result = sc.process_responses(responses, true).unwrap();
    match result {
        AlgorithmOutput::ResponseOnly(selected) => {
            let content = selected["content"].as_str().unwrap();
            assert!(
                content.contains("algebra"),
                "should select from algebra approach (most common at level 0)"
            );
        }
        _ => panic!("expected ResponseOnly"),
    }
}

// --- SelfConsistency: return_response_only false includes metadata ---

#[test]
fn test_self_consistency_return_response_only_false() {
    use its_hub_rs::SelfConsistency;
    use its_hub_rs::AlgorithmOutput;

    let sc = SelfConsistency::new(None, None).unwrap();
    let responses = vec![
        json!({"role": "assistant", "content": "a1"}),
        json!({"role": "assistant", "content": "a2"}),
        json!({"role": "assistant", "content": "b1"}),
        json!({"role": "assistant", "content": "c1"}),
    ];

    let result = sc.process_responses(responses, false).unwrap();
    match result {
        AlgorithmOutput::Full { metadata, .. } => {
            assert_eq!(metadata["algorithm"], "self-consistency");
            let all_responses = metadata["all_responses"].as_array().unwrap();
            assert_eq!(all_responses.len(), 4);
            assert!(metadata["response_counts"].is_object());
            assert!(metadata["selected_index"].is_u64());
        }
        _ => panic!("expected Full"),
    }
}

// --- SelfConsistency: multimodal content ---

#[tokio::test]
async fn test_self_consistency_with_multimodal_content() {
    let server = MockServer::start().await;

    let responses = vec![
        make_multimodal_chat_response(&[("text", "Answer: 42")]),
        make_multimodal_chat_response(&[("text", "Answer: 24")]),
        make_multimodal_chat_response(&[("text", "Answer: 42")]),
    ];

    for resp in responses {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let sc = SelfConsistency::new(None, None).unwrap();
    let messages = vec![user_message("What is the answer?")];

    let result = sc
        .infer(&client, &messages, 3, true, Some(0.7), None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::ResponseOnly(selected) => {
            assert!(selected.get("content").is_some());
        }
        _ => panic!("expected ResponseOnly"),
    }
}

// --- BestOfN: result structure ---

#[test]
fn test_best_of_n_result_structure() {
    use its_hub_rs::BestOfN;
    use its_hub_rs::AlgorithmOutput;

    let orm = MockOrm {
        scores: vec![0.5, 0.8, 0.3],
    };
    let bon = BestOfN::new(Box::new(orm));

    let responses = vec![
        json!({"role": "assistant", "content": "response1"}),
        json!({"role": "assistant", "content": "response2"}),
        json!({"role": "assistant", "content": "response3"}),
    ];
    let scores = vec![0.5, 0.8, 0.3];

    let result = bon.process_responses(responses, scores, 1, false);
    match result {
        AlgorithmOutput::Full { selected, metadata } => {
            assert_eq!(selected["content"], "response2");
            assert_eq!(metadata["algorithm"], "best-of-n");
            assert_eq!(metadata["selected_index"], 1);
            let resp_arr = metadata["responses"].as_array().unwrap();
            assert_eq!(resp_arr.len(), 3);
            let scores_arr = metadata["scores"].as_array().unwrap();
            assert_eq!(scores_arr.len(), 3);
            assert_eq!(scores_arr[1].as_f64().unwrap(), 0.8);
        }
        _ => panic!("expected Full"),
    }
}

// --- BestOfN: multimodal content ---

#[tokio::test]
async fn test_best_of_n_with_multimodal_content() {
    let server = MockServer::start().await;

    let responses = vec![
        make_multimodal_chat_response(&[("text", "Answer is 42")]),
        make_multimodal_chat_response(&[("text", "Answer is 24")]),
    ];

    for resp in responses {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }

    let orm_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/score"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"scores": [0.3, 0.7]})),
        )
        .mount(&orm_server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let orm = its_hub_rs::HttpOrmClient::new(&orm_server.uri());
    let bon = BestOfN::new(Box::new(orm));
    let messages = vec![user_message("test")];

    let result = bon
        .infer(&client, &messages, 2, true, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::ResponseOnly(selected) => {
            assert!(selected.get("content").is_some());
        }
        _ => panic!("expected ResponseOnly"),
    }
}

// --- BeamSearch: PRM scoring influence ---

#[tokio::test]
async fn test_beam_search_with_prm_scoring() {
    use its_hub_rs::BeamSearch;
    use its_hub_rs::ProcessRewardModel;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SequentialPRM {
        scores: Vec<f64>,
        call_count: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ProcessRewardModel for SequentialPRM {
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

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("step1")))
        .mount(&server)
        .await;

    let sg = its_hub_rs::core::lms::step_generation::StepGeneration::with_step_token(
        its_hub_rs::core::lms::step_generation::StepToken::Single("\n".to_string()),
        1,
        None,
        0.8,
        false,
        None,
    );
    let prm = Arc::new(SequentialPRM {
        scores: vec![0.9, 0.1],
        call_count: AtomicUsize::new(0),
    });
    let bs = BeamSearch::new(sg, prm, 2);

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let messages = vec![user_message("Solve this problem:")];

    let result = bs
        .infer(&client, &messages, 4, false, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::Full { metadata, .. } => {
            let scores = metadata["scores"].as_array().unwrap();
            assert!(
                !scores.is_empty(),
                "beam search should produce scores from PRM"
            );
            let sel_idx = metadata["selected_index"].as_u64().unwrap() as usize;
            let max_score = scores
                .iter()
                .map(|s| s.as_f64().unwrap())
                .fold(f64::NEG_INFINITY, f64::max);
            let expected_idx = scores
                .iter()
                .position(|s| (s.as_f64().unwrap() - max_score).abs() < f64::EPSILON)
                .unwrap();
            assert_eq!(sel_idx, expected_idx);
        }
        _ => panic!("expected Full"),
    }
}

// --- BeamSearch: return_response_only both modes ---

#[tokio::test]
async fn test_beam_search_return_response_only_modes() {
    use its_hub_rs::BeamSearch;
    use its_hub_rs::ProcessRewardModel;
    use std::sync::Arc;

    struct FixedPRM;

    #[async_trait::async_trait]
    impl ProcessRewardModel for FixedPRM {
        async fn score(
            &self,
            _prompt_messages: &[ChatMessage],
            _steps: &[String],
        ) -> Result<Vec<f64>, anyhow::Error> {
            Ok(vec![0.5])
        }
    }

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("step1")))
        .mount(&server)
        .await;

    let make_bs = || {
        let sg = its_hub_rs::core::lms::step_generation::StepGeneration::with_step_token(
            its_hub_rs::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        );
        BeamSearch::new(sg, Arc::new(FixedPRM), 2)
    };

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let messages = vec![user_message("test")];

    let bs1 = make_bs();
    let result1 = bs1
        .infer(&client, &messages, 2, true, None, None, None, None)
        .await
        .unwrap();
    assert!(matches!(result1, its_hub_rs::AlgorithmOutput::ResponseOnly(_)));

    let bs2 = make_bs();
    let result2 = bs2
        .infer(&client, &messages, 2, false, None, None, None, None)
        .await
        .unwrap();
    match result2 {
        its_hub_rs::AlgorithmOutput::Full { metadata, .. } => {
            assert_eq!(metadata["algorithm"], "beam-search");
            assert!(metadata.get("scores").is_some());
            assert!(metadata.get("steps_used").is_some());
        }
        _ => panic!("expected Full"),
    }
}

// --- ParticleGibbs: reference trajectory handling ---

#[tokio::test]
async fn test_particle_gibbs_reference_trajectory() {
    use its_hub_rs::core::algorithms::particle_gibbs::{ParticleGibbs, SelectionMethod, ResamplingMethod, TemperatureMethod};
    use its_hub_rs::ProcessRewardModel;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockPRM {
        call_count: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ProcessRewardModel for MockPRM {
        async fn score(
            &self,
            _prompt_messages: &[ChatMessage],
            _steps: &[String],
        ) -> Result<Vec<f64>, anyhow::Error> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(vec![0.5 + (idx as f64 * 0.05)])
        }
    }

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("step1")))
        .mount(&server)
        .await;

    let sg = its_hub_rs::core::lms::step_generation::StepGeneration::with_step_token(
        its_hub_rs::core::lms::step_generation::StepToken::Single("\n".to_string()),
        1,
        None,
        0.8,
        false,
        None,
    );
    let prm = Arc::new(MockPRM {
        call_count: AtomicUsize::new(0),
    });
    let pg = ParticleGibbs::new(
        sg,
        prm,
        2,
        SelectionMethod::Argmax,
        1,
        false,
        0.5,
        0.5,
        ResamplingMethod::Multinomial,
        TemperatureMethod::Ess,
    );

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let messages = vec![user_message("Solve this:")];

    let result = pg
        .infer(&client, &messages, 4, false, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::Full { metadata, .. } => {
            let rl = metadata["responses_lst"].as_array().unwrap();
            assert_eq!(rl.len(), 2, "2 iterations expected");
            let ril = metadata["ref_indices_lst"].as_array().unwrap();
            assert_eq!(ril.len(), 2);
            for ri in ril {
                let indices = ri.as_array().unwrap();
                assert!(!indices.is_empty(), "each iteration should have reference indices");
            }
        }
        _ => panic!("expected Full"),
    }
}

// --- ParticleFiltering: flattened output shape ---

#[tokio::test]
async fn test_particle_filtering_flattened_output() {
    use its_hub_rs::core::algorithms::particle_gibbs::{ParticleFiltering, SelectionMethod, ResamplingMethod};
    use its_hub_rs::ProcessRewardModel;
    use std::sync::Arc;

    struct FixedPRM;

    #[async_trait::async_trait]
    impl ProcessRewardModel for FixedPRM {
        async fn score(
            &self,
            _prompt_messages: &[ChatMessage],
            _steps: &[String],
        ) -> Result<Vec<f64>, anyhow::Error> {
            Ok(vec![0.7])
        }
    }

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("step1")))
        .mount(&server)
        .await;

    let sg = its_hub_rs::core::lms::step_generation::StepGeneration::with_step_token(
        its_hub_rs::core::lms::step_generation::StepToken::Single("\n".to_string()),
        1,
        None,
        0.8,
        false,
        None,
    );
    let pf = ParticleFiltering::new(sg, Arc::new(FixedPRM), SelectionMethod::Argmax, ResamplingMethod::Multinomial);

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let messages = vec![user_message("Solve this:")];

    let result = pf
        .infer(&client, &messages, 3, false, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::Full { selected, metadata } => {
            assert_eq!(selected["role"], "assistant");
            assert_eq!(metadata["algorithm"], "particle-filtering");
            let responses = metadata["responses"].as_array().unwrap();
            assert_eq!(responses.len(), 3, "flattened from single iteration");
            let log_weights = metadata["log_weights"].as_array().unwrap();
            assert_eq!(log_weights.len(), 3);
            assert!(metadata.get("selected_index").is_some());
            assert!(metadata.get("steps_used").is_some());
        }
        _ => panic!("expected Full"),
    }
}

// --- PlanningWrapper: wrapping SC end-to-end ---

#[tokio::test]
async fn test_planning_wrapper_with_self_consistency() {
    use its_hub_rs::PlanningWrapper;
    use its_hub_rs::SelfConsistency;

    let server = MockServer::start().await;

    let plan_response = json!({
        "id": "chatcmpl-plan",
        "object": "chat.completion",
        "created": 1700000000u64,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "APPROACH 1: Direct algebra\nAPPROACH 2: Substitution\nAPPROACH 3: Graphical"
            },
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 50, "total_tokens": 60}
    });

    let solve_response = json!({
        "id": "chatcmpl-solve",
        "object": "chat.completion",
        "created": 1700000000u64,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": r"Step 1: Solve 2x+3=7\nStep 2: x=2\n\boxed{2}"
            },
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(plan_response))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(solve_response))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let sc = SelfConsistency::new(None, None).unwrap();
    let pw = PlanningWrapper::new(Box::new(sc));

    let messages = vec![user_message("Solve for x: 2x + 3 = 7")];

    let result = pw
        .infer(&client, &messages, 4, false, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::Full { selected, metadata } => {
            assert!(selected.get("content").is_some());
            assert_eq!(metadata["algorithm"], "planning-wrapper");
            assert!(metadata.get("plan").is_some());
            let approaches = metadata["approaches"].as_array().unwrap();
            assert!(!approaches.is_empty());
            assert!(metadata.get("best_approach").is_some());
            assert!(metadata.get("approach_results").is_some());
        }
        _ => panic!("expected Full"),
    }
}

// --- PlanningWrapper: wrapping BoN end-to-end ---

#[tokio::test]
async fn test_planning_wrapper_with_best_of_n() {
    use its_hub_rs::PlanningWrapper;

    let server = MockServer::start().await;

    let plan_response = json!({
        "id": "chatcmpl-plan",
        "object": "chat.completion",
        "created": 1700000000u64,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "APPROACH 1: Method A\nAPPROACH 2: Method B\nAPPROACH 3: Method C"
            },
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 50, "total_tokens": 60}
    });

    let solve_response = json!({
        "id": "chatcmpl-solve",
        "object": "chat.completion",
        "created": 1700000000u64,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "The answer is 42"
            },
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(plan_response))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(solve_response))
        .mount(&server)
        .await;

    let orm_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/score"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"scores": [0.9]})))
        .mount(&orm_server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let orm = its_hub_rs::HttpOrmClient::new(&orm_server.uri());
    let bon = BestOfN::new(Box::new(orm));
    let pw = PlanningWrapper::new(Box::new(bon));

    let messages = vec![user_message("Solve this problem")];

    let result = pw
        .infer(&client, &messages, 4, true, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::ResponseOnly(selected) => {
            assert!(selected.get("content").is_some());
        }
        _ => panic!("expected ResponseOnly"),
    }
}

// --- Tool calls: mixed responses with and without tool calls ---

#[tokio::test]
async fn test_mixed_responses_with_and_without_tool_calls() {
    use its_hub_rs::core::algorithms::self_consistency::{SelfConsistency, ToolVoteStrategy};
    use its_hub_rs::AlgorithmOutput;

    let sc = SelfConsistency::new(None, Some(ToolVoteStrategy::Name)).unwrap();

    let responses = vec![
        json!({
            "role": "assistant",
            "content": "Using calculator",
            "tool_calls": [{
                "id": "1",
                "type": "function",
                "function": {"name": "calculate", "arguments": {"x": 1}}
            }]
        }),
        json!({"role": "assistant", "content": "Direct answer: 42"}),
        json!({
            "role": "assistant",
            "content": "Let me compute",
            "tool_calls": [{
                "id": "2",
                "type": "function",
                "function": {"name": "calculate", "arguments": {"x": 1}}
            }]
        }),
    ];

    let result = sc.process_responses(responses, false).unwrap();
    match result {
        AlgorithmOutput::Full { metadata, .. } => {
            let counts = metadata["response_counts"].as_object().unwrap();
            assert!(
                counts.contains_key("calculate"),
                "tool name voting should identify calculate"
            );
            assert_eq!(counts["calculate"].as_u64().unwrap(), 2);
        }
        _ => panic!("expected Full"),
    }
}

// --- Tool calls: fallback to content voting when no tool calls ---

#[tokio::test]
async fn test_fallback_to_content_voting_when_no_tool_calls() {
    use its_hub_rs::core::algorithms::self_consistency::{SelfConsistency, ToolVoteStrategy};
    use its_hub_rs::AlgorithmOutput;

    let sc = SelfConsistency::new(
        Some(vec![r"Answer: (\d+)".into()]),
        Some(ToolVoteStrategy::Name),
    )
    .unwrap();

    let responses = vec![
        json!({"role": "assistant", "content": "Answer: 42"}),
        json!({"role": "assistant", "content": "Answer: 42"}),
        json!({"role": "assistant", "content": "Answer: 24"}),
    ];

    let result = sc.process_responses(responses, true).unwrap();
    match result {
        AlgorithmOutput::ResponseOnly(selected) => {
            let content = selected["content"].as_str().unwrap();
            assert_eq!(content, "Answer: 42");
        }
        _ => panic!("expected ResponseOnly"),
    }
}

// --- LmClient: batch generation via fan_out ---

#[tokio::test]
async fn test_batch_generation_fan_out() {
    let server = MockServer::start().await;

    let responses = vec![
        make_chat_response("Response to: Hello"),
        make_chat_response("Response to: World"),
        make_chat_response("Response to: Test"),
    ];

    for resp in responses {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let messages = vec![user_message("Hello")];
    let results = client.fan_out(&messages, 3, Some(0.7), None, None, None).await;

    let mut successes = 0;
    for r in &results {
        if r.is_ok() {
            successes += 1;
        }
    }
    assert_eq!(successes, 3, "all 3 fan-out requests should succeed");
}

// --- LmClient: concurrency control ---

#[tokio::test]
async fn test_concurrency_control_fan_out() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("ok")))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        2,
        3,
        None,
        None,
    )
    .unwrap();

    let messages = vec![user_message("test")];
    let results = client.fan_out(&messages, 5, None, None, None, None).await;

    let successes: usize = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(successes, 5, "all requests should complete despite concurrency limit of 2");
}

// --- LmClient: replace_error_with_message (via SC with error replacement) ---

#[tokio::test]
async fn test_replace_error_with_message() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("good")))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        1,
        None,
        None,
    )
    .unwrap();

    let sc = SelfConsistency::with_error_replacement(
        None,
        None,
        Some("[CUSTOM ERROR]".to_string()),
    )
    .unwrap();

    let messages = vec![user_message("test")];

    let result = sc
        .infer(&client, &messages, 3, false, None, None, None, None)
        .await;

    assert!(result.is_ok(), "should succeed with partial error replacement");
    match result.unwrap() {
        its_hub_rs::AlgorithmOutput::Full { metadata, .. } => {
            let all_responses = metadata["all_responses"].as_array().unwrap();
            let has_error = all_responses.iter().any(|r| {
                r["content"]
                    .as_str()
                    .map(|c| c.contains("[CUSTOM ERROR]"))
                    .unwrap_or(false)
            });
            assert!(has_error, "some responses should have error replacement");
            let has_good = all_responses.iter().any(|r| {
                r["content"].as_str() == Some("good")
            });
            assert!(has_good, "at least one response should succeed");
        }
        _ => panic!("expected Full"),
    }
}

// --- LmClient: replace_error_with_message in batch (some succeed, some fail) ---

#[tokio::test]
async fn test_replace_error_with_message_batch() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("good")))
        .up_to_n_times(2)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        1,
        None,
        None,
    )
    .unwrap();

    let sc = SelfConsistency::with_error_replacement(
        None,
        None,
        Some("[BATCH ERROR]".to_string()),
    )
    .unwrap();

    let messages = vec![user_message("test")];

    let result = sc
        .infer(&client, &messages, 4, false, None, None, None, None)
        .await;

    assert!(result.is_ok(), "batch with some errors should succeed");
    match result.unwrap() {
        its_hub_rs::AlgorithmOutput::Full { metadata, .. } => {
            let all_responses = metadata["all_responses"].as_array().unwrap();
            assert_eq!(all_responses.len(), 4);
            let error_count = all_responses
                .iter()
                .filter(|r| {
                    r["content"]
                        .as_str()
                        .map(|c| c.contains("[BATCH ERROR]"))
                        .unwrap_or(false)
                })
                .count();
            assert!(error_count > 0, "some responses should have error replacement");
        }
        _ => panic!("expected Full"),
    }
}

// --- Self-consistency with regex: generate scenarios ---

#[tokio::test]
async fn test_generate_scenarios_simple_chat() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("Hello!")))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    let messages = vec![user_message("Hello, world!")];
    let response = client
        .chat_completion(&messages, Some(0.7), None, None, None, None)
        .await
        .unwrap();

    assert_eq!(response["content"].as_str().unwrap(), "Hello!");
    assert_eq!(response["role"].as_str().unwrap(), "assistant");
}

#[tokio::test]
async fn test_generate_scenarios_with_system_prompt() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("Math response")))
        .mount(&server)
        .await;

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        Some("You are a math tutor.".to_string()),
        None,
    )
    .unwrap();

    let messages = vec![user_message("What is 2+2?")];
    let response = client
        .chat_completion(&messages, Some(0.7), None, None, None, None)
        .await
        .unwrap();

    assert_eq!(response["role"].as_str().unwrap(), "assistant");
    assert!(response.get("content").is_some());
}

// --- Self-consistency with tool_call responses via integration test ---

#[tokio::test]
async fn test_self_consistency_with_tool_call_responses() {
    let server = MockServer::start().await;

    let responses = vec![
        make_tool_call_chat_response("calculate", json!({"x": 42})),
        make_tool_call_chat_response("calculate", json!({"x": 42})),
        make_tool_call_chat_response("search", json!({"q": "test"})),
        make_tool_call_chat_response("calculate", json!({"x": 42})),
    ];

    for resp in responses {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }

    let client = LmClient::new(
        &format!("{}/v1", server.uri()),
        None,
        "test-model",
        8,
        3,
        None,
        None,
    )
    .unwrap();

    use its_hub_rs::core::algorithms::self_consistency::ToolVoteStrategy;
    let sc = SelfConsistency::new(None, Some(ToolVoteStrategy::Name)).unwrap();
    let messages = vec![user_message("Use a tool")];

    let result = sc
        .infer(&client, &messages, 4, false, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::AlgorithmOutput::Full { selected, metadata } => {
            let tool_name = selected["tool_calls"][0]["function"]["name"]
                .as_str()
                .unwrap();
            assert_eq!(tool_name, "calculate");
            let counts = metadata["response_counts"].as_object().unwrap();
            assert_eq!(counts["calculate"].as_u64().unwrap(), 3);
        }
        _ => panic!("expected Full"),
    }
}
