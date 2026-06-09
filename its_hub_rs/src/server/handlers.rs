use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::Json;
use serde_json::json;
use tracing::info;

use crate::algorithms::best_of_n::{BestOfN, HttpOrmClient};
use crate::algorithms::self_consistency::{SelfConsistency, ToolVoteStrategy};
use crate::algorithms::AlgorithmOutput;
use crate::client::LmClient;
use crate::types::{
    ChatCompletionChoice, ChatCompletionRequest, ChatCompletionResponse, ChatCompletionUsage,
    ConfigRequest, ConfigResponse, ModelInfo, ModelsResponse,
};

use super::error::AppError;
use super::state::{AlgorithmConfig, AppState};

pub async fn configure(
    State(state): State<Arc<AppState>>,
    Json(config): Json<ConfigRequest>,
) -> Result<Json<ConfigResponse>, AppError> {
    let alg_name = config.alg.clone();

    let algorithm: Box<dyn crate::algorithms::ScalingAlgorithm> = match alg_name.as_str() {
        "self-consistency" => {
            let tool_vote = config.tool_vote.as_deref().map(|tv| match tv {
                "tool_name" => ToolVoteStrategy::Name,
                "tool_args" => ToolVoteStrategy::Args {
                    exclude: config.exclude_tool_args.clone().unwrap_or_default(),
                },
                "tool_hierarchical" => ToolVoteStrategy::Hierarchical {
                    exclude: config.exclude_tool_args.clone().unwrap_or_default(),
                },
                _ => ToolVoteStrategy::Name,
            });

            let sc = SelfConsistency::new(config.regex_patterns.clone(), tool_vote)
                .map_err(|e| AppError::BadRequest(format!("invalid regex pattern: {}", e)))?;
            Box::new(sc)
        }
        "best-of-n" => {
            let rm_endpoint = config
                .rm_endpoint
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("rm_endpoint required for best-of-n".into()))?;

            let orm = HttpOrmClient::new(rm_endpoint);
            Box::new(BestOfN::new(Box::new(orm)))
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported algorithm: {}. Supported: self-consistency, best-of-n",
                other
            )));
        }
    };

    let client = LmClient::new(
        &config.endpoint,
        config.api_key.as_deref(),
        &config.model,
        64,
        8,
    )
    .map_err(|e| AppError::BadRequest(format!("failed to create LM client: {}", e)))?;

    let model_name = config.model.clone();

    {
        let mut alg_lock = state.algorithm.write().await;
        *alg_lock = Some(AlgorithmConfig {
            name: alg_name.clone(),
            algorithm,
        });
    }

    {
        let mut clients_lock = state.clients.write().await;
        clients_lock.insert(model_name.clone(), Arc::new(client));
    }

    info!(model = %model_name, algorithm = %alg_name, "configured");

    Ok(Json(ConfigResponse {
        status: "success".to_string(),
        message: format!("Initialized {} with {} algorithm", model_name, alg_name),
    }))
}

pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, AppError> {
    if request.stream == Some(true) {
        return Err(AppError::StreamingNotSupported);
    }

    if request.budget < 1 || request.budget > 1000 {
        return Err(AppError::BadRequest(format!(
            "budget must be between 1 and 1000, got {}",
            request.budget
        )));
    }

    let alg_lock = state.algorithm.read().await;
    let alg_config = alg_lock.as_ref().ok_or(AppError::NotConfigured)?;

    let clients_lock = state.clients.read().await;
    let client = clients_lock
        .get(&request.model)
        .ok_or_else(|| AppError::ModelNotFound(request.model.clone()))?;

    let tools_value = request
        .tools
        .as_ref()
        .map(|t| serde_json::Value::Array(t.clone()));

    let output = alg_config
        .algorithm
        .infer(
            client,
            &request.messages,
            request.budget,
            request.return_response_only,
            request.temperature,
            request.max_tokens,
            tools_value.as_ref(),
            request.tool_choice.as_ref(),
        )
        .await
        .map_err(|e| AppError::Algorithm(e.to_string()))?;

    let (selected, metadata) = match output {
        AlgorithmOutput::ResponseOnly(msg) => (msg, None),
        AlgorithmOutput::Full { selected, metadata } => (selected, Some(metadata)),
    };

    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created,
        model: request.model,
        choices: vec![ChatCompletionChoice {
            index: 0,
            message: selected,
            finish_reason: "stop".to_string(),
        }],
        usage: ChatCompletionUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
        metadata,
    };

    Ok(Json(response))
}

pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<ModelsResponse> {
    let clients_lock = state.clients.read().await;
    let data: Vec<ModelInfo> = clients_lock
        .keys()
        .map(|model_id| ModelInfo {
            id: model_id.clone(),
            object: "model".to_string(),
            owned_by: "its-hub-rs".to_string(),
        })
        .collect();

    Json(ModelsResponse { data })
}

pub async fn health(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let alg_lock = state.algorithm.read().await;
    let clients_lock = state.clients.read().await;

    let algorithm = alg_lock.as_ref().map(|a| a.name.clone());
    let models = clients_lock.len();

    Json(json!({
        "status": "ok",
        "algorithm": algorithm,
        "models": models,
    }))
}
