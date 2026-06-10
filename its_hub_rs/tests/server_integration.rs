use std::sync::Arc;

use reqwest::header::HeaderValue;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use its_hub_rs::server;
use its_hub_rs::server::state::AppState;

// Helper to configure self-consistency with a mock backend
async fn configure_sc(client: &reqwest::Client, base_url: &str, backend_uri: &str) {
    configure_sc_with(client, base_url, backend_uri, json!({})).await;
}

async fn configure_sc_with(
    client: &reqwest::Client,
    base_url: &str,
    backend_uri: &str,
    extra: serde_json::Value,
) {
    let mut payload = json!({
        "endpoint": format!("{}/v1", backend_uri),
        "model": "test-model",
        "alg": "self-consistency"
    });
    if let (Some(base), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, v) in extra {
            base.insert(k.clone(), v.clone());
        }
    }
    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "configure should succeed");
}

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

#[tokio::test]
async fn test_tool_args_vote_config() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "self-consistency",
            "tool_vote": "tool_args",
            "exclude_tool_args": ["timestamp"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_tool_hierarchical_vote_config() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "self-consistency",
            "tool_vote": "tool_hierarchical"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_invalid_tool_vote_defaults_to_name() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "self-consistency",
            "tool_vote": "invalid_value"
        }))
        .send()
        .await
        .unwrap();

    // Current handler defaults invalid tool_vote to Name strategy (200)
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_empty_messages_400() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    configure_sc(&client, &base_url, &backend.uri()).await;

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": []
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_temperature_out_of_range_400() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    configure_sc(&client, &base_url, &backend.uri()).await;

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "hi"}],
            "temperature": 5.0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_system_prompt_in_config() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("ok")))
        .mount(&backend)
        .await;

    configure_sc_with(
        &client,
        &base_url,
        &backend.uri(),
        json!({"system_prompt": "You are a math tutor."}),
    )
    .await;

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "What is 2+2?"}],
            "budget": 1
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["choices"][0]["message"]["content"], "ok");
}

// --- Configuration validation: missing required fields ---

#[tokio::test]
async fn test_configuration_validation_missing_fields() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": "http://localhost:8100/v1"
        }))
        .send()
        .await
        .unwrap();

    assert!(
        resp.status() == 400 || resp.status() == 422,
        "missing model/alg should fail"
    );
}

// --- Chat completions with system message ---

#[tokio::test]
async fn test_chat_completions_with_system_message() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("system aware response")))
        .mount(&backend)
        .await;

    configure_sc(&client, &base_url, &backend.uri()).await;

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [
                {"role": "system", "content": "You are a math tutor"},
                {"role": "user", "content": "Explain algebra"}
            ],
            "budget": 1
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["choices"][0]["message"]["content"], "system aware response");
}

// --- Chat completions algorithm error (passthrough disabled) ---

#[tokio::test]
async fn test_chat_completions_algorithm_error() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal server error"))
        .mount(&backend)
        .await;

    // Must disable passthrough so it returns 500 instead of retrying via passthrough
    configure_sc_with(
        &client,
        &base_url,
        &backend.uri(),
        json!({"passthrough_on_error": false}),
    )
    .await;

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

    assert_eq!(resp.status(), 500);
}

// --- Configure beam-search algorithm ---

#[tokio::test]
async fn test_config_beam_search() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;
    let prm = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/score"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"scores": [0.5]})))
        .mount(&prm)
        .await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "beam-search",
            "step_token": "\n",
            "prm_endpoint": prm.uri(),
            "beam_width": 2
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "success");
    assert!(body["message"].as_str().unwrap().contains("beam-search"));
}

// --- Configure particle-filtering algorithm ---

#[tokio::test]
async fn test_config_particle_filtering() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;
    let prm = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "particle-filtering",
            "step_token": "\n",
            "stop_token": "<end>",
            "prm_endpoint": prm.uri()
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "success");
    assert!(body["message"].as_str().unwrap().contains("particle-filtering"));
}

// --- Configure planning-wrapper with inner algorithm ---

#[tokio::test]
async fn test_config_planning_wrapper() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "planning-wrapper",
            "inner_alg": "self-consistency"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "success");
    assert!(body["message"].as_str().unwrap().contains("planning-wrapper"));

    let health: serde_json::Value = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["algorithm"], "planning-wrapper");
}

// --- Configure planning-wrapper missing inner_alg returns 400 ---

#[tokio::test]
async fn test_config_planning_wrapper_missing_inner_alg() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "planning-wrapper"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

// --- Configure beam-search missing step_token returns 400 ---

#[tokio::test]
async fn test_config_beam_search_missing_step_token() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "beam-search"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

// --- Configure particle-filtering missing prm_endpoint returns 400 ---

#[tokio::test]
async fn test_config_particle_filtering_missing_prm() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "particle-filtering",
            "step_token": "\n"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

// --- Configure planning-wrapper with best-of-n inner algorithm ---

#[tokio::test]
async fn test_config_planning_wrapper_with_bon() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;
    let orm = MockServer::start().await;

    let resp = client
        .post(format!("{}/configure", base_url))
        .json(&json!({
            "endpoint": format!("{}/v1", backend.uri()),
            "model": "test-model",
            "alg": "planning-wrapper",
            "inner_alg": "best-of-n",
            "rm_endpoint": orm.uri()
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "success");
}

#[tokio::test]
async fn test_replace_error_with_message_config() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("ok")))
        .mount(&backend)
        .await;

    configure_sc_with(
        &client,
        &base_url,
        &backend.uri(),
        json!({"replace_error_with_message": "Custom error msg"}),
    )
    .await;

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "test"}],
            "budget": 1
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

// =========================================================================
// Gateway feature tests
// =========================================================================

// --- Passthrough on algorithm error ---

#[tokio::test]
async fn test_passthrough_on_algorithm_error() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Backend that fails on first 3 requests (the fan-out), then succeeds (the passthrough)
    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .up_to_n_times(3)
        .mount(&backend)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(make_chat_response("passthrough response")),
        )
        .mount(&backend)
        .await;

    // passthrough_on_error defaults to true
    configure_sc(&client, &base_url, &backend.uri()).await;

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
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "passthrough response"
    );
}

#[tokio::test]
async fn test_passthrough_disabled() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&backend)
        .await;

    configure_sc_with(
        &client,
        &base_url,
        &backend.uri(),
        json!({"passthrough_on_error": false}),
    )
    .await;

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

    assert_eq!(resp.status(), 500);
}

// --- Token cache tests ---

#[tokio::test]
async fn test_cache_hit() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(make_chat_response("cached answer")),
        )
        .mount(&backend)
        .await;

    configure_sc_with(
        &client,
        &base_url,
        &backend.uri(),
        json!({"cache_enabled": true, "cache_ttl_seconds": 300}),
    )
    .await;

    let payload = json!({
        "model": "test-model",
        "messages": [{"role": "user", "content": "What is 2+2?"}],
        "budget": 1
    });

    // First request: cache miss
    let resp1 = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), 200);
    assert_eq!(
        resp1.headers().get("x-its-cache-hit").unwrap(),
        &HeaderValue::from_static("false")
    );

    // Second request: cache hit (same payload)
    let resp2 = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), 200);
    assert_eq!(
        resp2.headers().get("x-its-cache-hit").unwrap(),
        &HeaderValue::from_static("true")
    );
    let body: serde_json::Value = resp2.json().await.unwrap();
    assert_eq!(body["choices"][0]["message"]["content"], "cached answer");
}

#[tokio::test]
async fn test_cache_miss() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("response")))
        .mount(&backend)
        .await;

    configure_sc_with(
        &client,
        &base_url,
        &backend.uri(),
        json!({"cache_enabled": true, "cache_ttl_seconds": 300}),
    )
    .await;

    // Two different requests: both should be cache misses
    let resp1 = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "question 1"}],
            "budget": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp1.headers().get("x-its-cache-hit").unwrap(),
        &HeaderValue::from_static("false")
    );

    let resp2 = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "question 2"}],
            "budget": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp2.headers().get("x-its-cache-hit").unwrap(),
        &HeaderValue::from_static("false")
    );
}

#[tokio::test]
async fn test_cache_ttl_expiry() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("expired")))
        .mount(&backend)
        .await;

    // TTL = 0 means immediate expiry
    configure_sc_with(
        &client,
        &base_url,
        &backend.uri(),
        json!({"cache_enabled": true, "cache_ttl_seconds": 0}),
    )
    .await;

    let payload = json!({
        "model": "test-model",
        "messages": [{"role": "user", "content": "test"}],
        "budget": 1
    });

    // First request
    let resp1 = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), 200);

    // Short sleep so entry expires
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Second request should be a miss because TTL expired
    let resp2 = client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp2.headers().get("x-its-cache-hit").unwrap(),
        &HeaderValue::from_static("false")
    );
}

#[tokio::test]
async fn test_cache_stats_in_health() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("ok")))
        .mount(&backend)
        .await;

    configure_sc_with(
        &client,
        &base_url,
        &backend.uri(),
        json!({"cache_enabled": true, "cache_ttl_seconds": 300}),
    )
    .await;

    // Make a request to populate cache stats
    client
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "test"}],
            "budget": 1
        }))
        .send()
        .await
        .unwrap();

    let health: serde_json::Value = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(health["cache"].is_object());
    assert!(health["cache"]["entries"].is_number());
    assert!(health["cache"]["max_entries"].is_number());
    assert!(health["cache"]["hits"].is_number());
    assert!(health["cache"]["misses"].is_number());
    assert!(health["cache"]["hit_rate"].is_number());
    assert!(health["cache"]["ttl_seconds"].is_number());
}

// --- Envoy header tests ---

#[tokio::test]
async fn test_envoy_headers_override() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("42")))
        .mount(&backend)
        .await;

    configure_sc(&client, &base_url, &backend.uri()).await;

    // Override algorithm via header (self-consistency matches configured, so it uses it)
    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header("x-its-algorithm", "self-consistency")
        .header("x-its-budget", "2")
        .json(&json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "What is 6*7?"}],
            "budget": 5
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("x-its-algorithm-used").unwrap(),
        &HeaderValue::from_static("self-consistency")
    );
}

#[tokio::test]
async fn test_envoy_response_headers() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let backend = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_chat_response("hello")))
        .mount(&backend)
        .await;

    configure_sc(&client, &base_url, &backend.uri()).await;

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
    assert!(resp.headers().contains_key("x-its-algorithm-used"));
    assert!(resp.headers().contains_key("x-its-latency-ms"));
    assert!(resp.headers().contains_key("x-its-cache-hit"));

    let latency_str = resp
        .headers()
        .get("x-its-latency-ms")
        .unwrap()
        .to_str()
        .unwrap();
    let latency: u64 = latency_str.parse().unwrap();
    assert!(latency < 60000, "latency should be reasonable");
}

// --- Health probe tests ---

#[tokio::test]
async fn test_liveness_always_200() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Even before configure, liveness should return 200
    let resp = client
        .get(format!("{}/health/live", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "alive");
}

#[tokio::test]
async fn test_readiness_503_when_unconfigured() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/health/ready", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["algorithm_configured"], false);
}

#[tokio::test]
async fn test_readiness_200_when_configured() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let backend = MockServer::start().await;

    configure_sc(&client, &base_url, &backend.uri()).await;

    let resp = client
        .get(format!("{}/health/ready", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ready");
}
