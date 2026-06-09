use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use its_hub_rs::server;
use its_hub_rs::server::state::AppState;

async fn start_test_server() -> (String, Arc<AppState>) {
    let state = Arc::new(AppState::new());
    let app = server::app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (base_url, state)
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
async fn health_returns_ok_before_configure() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["algorithm"], json!(null));
    assert_eq!(body["models"], 0);
}

#[tokio::test]
async fn chat_completions_before_configure_returns_503() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
}

#[tokio::test]
async fn configure_self_consistency_returns_success() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let mock_server = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", mock_server.uri()),
            "model": "test-model",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "success");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("self-consistency"));
}

#[tokio::test]
async fn configure_invalid_algorithm_returns_400() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": "http://localhost:8100/v1",
            "model": "test-model",
            "alg": "invalid-algorithm"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn models_after_configure_lists_model() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let mock_server = MockServer::start().await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", mock_server.uri()),
            "model": "my-model",
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
    let body: serde_json::Value = resp.json().await.unwrap();
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["id"], "my-model");
    assert_eq!(data[0]["object"], "model");
}

#[tokio::test]
async fn health_after_configure_shows_algorithm() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let mock_server = MockServer::start().await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", mock_server.uri()),
            "model": "my-model",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["algorithm"], "self-consistency");
    assert_eq!(body["models"], 1);
}

#[tokio::test]
async fn streaming_returns_501() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let mock_server = MockServer::start().await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", mock_server.uri()),
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
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 501);
}

#[tokio::test]
async fn invalid_model_returns_404() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let mock_server = MockServer::start().await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", mock_server.uri()),
            "model": "test-model",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "nonexistent-model",
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn invalid_budget_returns_400() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let mock_server = MockServer::start().await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", mock_server.uri()),
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
            "messages": [{"role": "user", "content": "hello"}],
            "budget": 0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "hello"}],
            "budget": 1001
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn end_to_end_self_consistency() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("42")))
        .mount(&backend)
        .await;

    let configure_resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(configure_resp.status(), 200);

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "What is 6*7?"}],
            "budget": 3
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["id"].as_str().unwrap().starts_with("chatcmpl-"));
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["model"], "test-model");

    let choices = body["choices"].as_array().unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0]["index"], 0);
    assert_eq!(choices[0]["finish_reason"], "stop");
    assert_eq!(choices[0]["message"]["content"], "42");

    assert_eq!(body["usage"]["prompt_tokens"], 0);
    assert_eq!(body["usage"]["completion_tokens"], 0);
    assert_eq!(body["usage"]["total_tokens"], 0);
}

#[tokio::test]
async fn end_to_end_best_of_n() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("answer")))
        .mount(&backend)
        .await;

    let orm = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/score"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"scores": [0.9]})))
        .mount(&orm)
        .await;

    let configure_resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "best-of-n",
            "rm_endpoint": orm.uri(),
            "rm_name": "test-rm"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(configure_resp.status(), 200);

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "test"}],
            "budget": 3
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["choices"][0]["message"]["content"], "answer");
}

#[tokio::test]
async fn reconfigure_changes_algorithm() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("ok")))
        .mount(&backend)
        .await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "m1",
            "alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    let health1: serde_json::Value = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health1["algorithm"], "self-consistency");

    let orm = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/score"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"scores": [0.9]})))
        .mount(&orm)
        .await;

    client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "m2",
            "alg": "best-of-n",
            "rm_endpoint": orm.uri(),
            "rm_name": "rm"
        }))
        .send()
        .await
        .unwrap();

    let health2: serde_json::Value = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health2["algorithm"], "best-of-n");
    assert_eq!(health2["models"], 2);
}

#[tokio::test]
async fn best_of_n_without_rm_endpoint_returns_400() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": "http://localhost:8100/v1",
            "model": "test-model",
            "alg": "best-of-n"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}
