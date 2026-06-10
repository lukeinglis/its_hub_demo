use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use its_hub_rs::algorithms::self_consistency::SelfConsistency;
use its_hub_rs::algorithms::best_of_n::{BestOfN, OutcomeRewardModel};
use its_hub_rs::algorithms::ScalingAlgorithm;
use its_hub_rs::client::LmClient;
use its_hub_rs::types::{ChatMessage, Content};

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
        its_hub_rs::algorithms::AlgorithmOutput::ResponseOnly(selected) => {
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
        its_hub_rs::algorithms::AlgorithmOutput::Full { metadata, .. } => {
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

    let orm = its_hub_rs::algorithms::best_of_n::HttpOrmClient::new(&orm_server.uri());
    let bon = BestOfN::new(Box::new(orm));
    let messages = vec![user_message("test")];

    let result = bon
        .infer(&client, &messages, 3, true, None, None, None, None)
        .await
        .unwrap();

    match result {
        its_hub_rs::algorithms::AlgorithmOutput::ResponseOnly(selected) => {
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
        its_hub_rs::algorithms::AlgorithmOutput::ResponseOnly(selected) => {
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
