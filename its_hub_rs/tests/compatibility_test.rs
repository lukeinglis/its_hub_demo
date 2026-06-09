use std::sync::Arc;

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use its_hub_rs::server;
use its_hub_rs::server::state::AppState;

async fn start_test_server() -> String {
    let state = Arc::new(AppState::new());
    let app = server::app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    base_url
}

fn load_fixture(name: &str) -> Value {
    let fixture_path = format!(
        "{}/tests/compatibility/fixtures/{}.json",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let content = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", fixture_path, e));
    serde_json::from_str(&content).unwrap()
}

fn validate_response_schema(response: &Value, schema: &Value) {
    for field in schema["required_fields"].as_array().unwrap() {
        let field_name = field.as_str().unwrap();
        assert!(
            response.get(field_name).is_some(),
            "missing required field: {}",
            field_name
        );
    }

    let id = response["id"].as_str().unwrap();
    let prefix = schema["id_prefix"].as_str().unwrap();
    assert!(
        id.starts_with(prefix),
        "id '{}' does not start with '{}'",
        id,
        prefix
    );

    assert_eq!(
        response["object"].as_str().unwrap(),
        schema["object_value"].as_str().unwrap()
    );

    let choices = response["choices"].as_array().unwrap();
    assert_eq!(
        choices.len(),
        schema["choices_length"].as_u64().unwrap() as usize
    );

    for field in schema["choice_fields"].as_array().unwrap() {
        let field_name = field.as_str().unwrap();
        assert!(
            choices[0].get(field_name).is_some(),
            "choice missing field: {}",
            field_name
        );
    }

    let usage = response.get("usage").expect("missing usage field");
    for field in schema["usage_fields"].as_array().unwrap() {
        let field_name = field.as_str().unwrap();
        assert!(
            usage.get(field_name).is_some(),
            "usage missing field: {}",
            field_name
        );
    }
}

#[tokio::test]
async fn compat_self_consistency_content_voting() {
    let fixture = load_fixture("self_consistency_content");
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    let majority_response = &fixture["mock_responses"][0];
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(majority_response))
        .mount(&backend)
        .await;

    let mut config = fixture["config"].clone();
    config["endpoint"] = Value::String(format!("{}/v1", backend.uri()));

    let configure_resp = client
        .post(format!("{}/configure", base_url))
        .json(&config)
        .send()
        .await
        .unwrap();
    assert_eq!(configure_resp.status(), 200);

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&fixture["request"])
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    validate_response_schema(&body, &fixture["expected"]["response_schema"]);

    let selected_content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap();
    let expected_content = fixture["expected"]["selected_content"].as_str().unwrap();
    assert_eq!(
        selected_content, expected_content,
        "expected '{}', got '{}'",
        expected_content, selected_content
    );
}

#[tokio::test]
async fn compat_best_of_n_scoring() {
    let fixture = load_fixture("best_of_n_scoring");
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    let majority_response = &fixture["mock_responses"][0];
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(majority_response))
        .mount(&backend)
        .await;

    let orm = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/score"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"scores": [0.95]})),
        )
        .mount(&orm)
        .await;

    let mut config = fixture["config"].clone();
    config["endpoint"] = Value::String(format!("{}/v1", backend.uri()));
    config["rm_endpoint"] = Value::String(orm.uri());

    let configure_resp = client
        .post(format!("{}/configure", base_url))
        .json(&config)
        .send()
        .await
        .unwrap();
    assert_eq!(configure_resp.status(), 200);

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&fixture["request"])
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    validate_response_schema(&body, &fixture["expected"]["response_schema"]);

    let selected_content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap();
    let expected_content = fixture["expected"]["selected_content"].as_str().unwrap();
    assert_eq!(
        selected_content, expected_content,
        "expected '{}', got '{}'",
        expected_content, selected_content
    );
}

#[tokio::test]
async fn compat_self_consistency_tool_calls() {
    let fixture = load_fixture("self_consistency_tool_calls");
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    let majority_response = &fixture["mock_responses"][0];
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(majority_response))
        .mount(&backend)
        .await;

    let mut config = fixture["config"].clone();
    config["endpoint"] = Value::String(format!("{}/v1", backend.uri()));

    let configure_resp = client
        .post(format!("{}/configure", base_url))
        .json(&config)
        .send()
        .await
        .unwrap();
    assert_eq!(configure_resp.status(), 200);

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&fixture["request"])
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    validate_response_schema(&body, &fixture["expected"]["response_schema"]);

    let message = &body["choices"][0]["message"];
    let tool_calls = message["tool_calls"].as_array().unwrap();
    assert!(!tool_calls.is_empty(), "expected tool_calls in response");

    let selected_tool_name = tool_calls[0]["function"]["name"].as_str().unwrap();
    let expected_tool_name = fixture["expected"]["selected_tool_name"].as_str().unwrap();
    assert_eq!(
        selected_tool_name, expected_tool_name,
        "expected tool '{}', got '{}'",
        expected_tool_name, selected_tool_name
    );
}

#[tokio::test]
async fn compat_response_types_are_correct() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-xyz",
            "object": "chat.completion",
            "created": 1700000000u64,
            "model": "test-model",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hello"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 1, "total_tokens": 6}
        })))
        .mount(&backend)
        .await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "hi"}],
            "budget": 1
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    assert!(body["id"].is_string());
    assert!(body["object"].is_string());
    assert!(body["created"].is_u64());
    assert!(body["model"].is_string());
    assert!(body["choices"].is_array());
    assert!(body["usage"].is_object());

    let choice = &body["choices"][0];
    assert!(choice["index"].is_u64());
    assert!(choice["message"].is_object());
    assert!(choice["finish_reason"].is_string());

    let usage = &body["usage"];
    assert!(usage["prompt_tokens"].is_u64());
    assert!(usage["completion_tokens"].is_u64());
    assert!(usage["total_tokens"].is_u64());
}

#[tokio::test]
async fn compat_metadata_returned_when_not_response_only() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-meta",
            "object": "chat.completion",
            "created": 1700000000u64,
            "model": "test-model",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "42"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 1, "total_tokens": 6}
        })))
        .mount(&backend)
        .await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "What is 6*7?"}],
            "budget": 3,
            "return_response_only": false
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    assert!(
        body.get("metadata").is_some(),
        "metadata should be present when return_response_only=false"
    );
    let metadata = &body["metadata"];
    assert_eq!(metadata["algorithm"], "self-consistency");
    assert!(metadata.get("all_responses").is_some());
    assert!(metadata.get("selected_index").is_some());
}

#[tokio::test]
async fn compat_health_endpoint_schema() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    assert!(body["status"].is_string());
    assert_eq!(body["status"], "ok");
    assert!(body.get("algorithm").is_some());
    assert!(body.get("models").is_some());
    assert!(body["models"].is_u64());
}

#[tokio::test]
async fn compat_models_endpoint_schema() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!("{}/v1/models", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    assert!(body["data"].is_array());
    let models = body["data"].as_array().unwrap();
    assert_eq!(models.len(), 1);
    assert!(models[0]["id"].is_string());
    assert!(models[0]["object"].is_string());
    assert_eq!(models[0]["object"], "model");
    assert!(models[0]["owned_by"].is_string());
}

#[tokio::test]
async fn compat_configure_endpoint_schema() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    assert_eq!(body["status"], "success");
    assert!(body["message"].is_string());
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("test-model"));
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("self-consistency"));
}

#[tokio::test]
async fn compat_error_response_schema() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: Value = resp.json().await.unwrap();

    assert!(body["error"].is_object());
    assert!(body["error"]["message"].is_string());
    assert!(body["error"]["type"].is_string());
}
