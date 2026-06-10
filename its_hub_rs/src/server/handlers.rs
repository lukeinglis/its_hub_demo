use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::Json;
use serde_json::json;
use tracing::info;

use crate::algorithms::beam_search::BeamSearch;
use crate::algorithms::best_of_n::{BestOfN, HttpOrmClient};
use crate::algorithms::particle_gibbs::{
    EntropicParticleFiltering, ParticleFiltering, ParticleGibbs, ResamplingMethod,
    SelectionMethod, TemperatureMethod,
};
use crate::algorithms::planning_wrapper::PlanningWrapper;
use crate::algorithms::self_consistency::{SelfConsistency, ToolVoteStrategy};
use crate::algorithms::AlgorithmOutput;
use crate::client::LmClient;
use crate::integration::reward_models::HttpProcessRewardModel;
use crate::step_generation::{StepGeneration, StepToken};
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

            let sc = SelfConsistency::with_error_replacement(
                config.regex_patterns.clone(),
                tool_vote,
                config.replace_error_with_message.clone(),
            )
            .map_err(|e| AppError::BadRequest(format!("invalid regex pattern: {}", e)))?;
            Box::new(sc)
        }
        "best-of-n" => {
            let rm_endpoint = config
                .rm_endpoint
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("rm_endpoint required for best-of-n".into()))?;

            let orm = HttpOrmClient::new(rm_endpoint);
            Box::new(BestOfN::with_error_replacement(
                Box::new(orm),
                config.replace_error_with_message.clone(),
            ))
        }
        "beam-search" => {
            let step_token_str = config
                .step_token
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("step_token required for beam-search".into()))?;
            let prm_endpoint = config
                .prm_endpoint
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("prm_endpoint required for beam-search".into()))?;
            let beam_width = config.beam_width.unwrap_or(2) as usize;

            let sg = StepGeneration::with_step_token(
                StepToken::Single(step_token_str.to_string()),
                config.n.unwrap_or(10),
                config.stop_token.clone(),
                config.temperature.unwrap_or(0.8),
                config.include_stop_str_in_output.unwrap_or(false),
                None,
            );
            let prm = Arc::new(HttpProcessRewardModel::new(prm_endpoint));
            Box::new(BeamSearch::new(sg, prm, beam_width))
        }
        "particle-filtering" => {
            let step_token_str = config
                .step_token
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("step_token required for particle-filtering".into()))?;
            let prm_endpoint = config
                .prm_endpoint
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("prm_endpoint required for particle-filtering".into()))?;

            let sg = StepGeneration::with_step_token(
                StepToken::Single(step_token_str.to_string()),
                config.n.unwrap_or(10),
                config.stop_token.clone(),
                config.temperature.unwrap_or(0.8),
                config.include_stop_str_in_output.unwrap_or(false),
                None,
            );
            let prm = Arc::new(HttpProcessRewardModel::new(prm_endpoint));
            Box::new(ParticleFiltering::new(
                sg,
                prm,
                SelectionMethod::Argmax,
                ResamplingMethod::Systematic,
            ))
        }
        "particle-gibbs" => {
            let step_token_str = config
                .step_token
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("step_token required for particle-gibbs".into()))?;
            let prm_endpoint = config
                .prm_endpoint
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("prm_endpoint required for particle-gibbs".into()))?;
            let num_iterations = config.num_iterations.unwrap_or(2) as usize;

            let sg = StepGeneration::with_step_token(
                StepToken::Single(step_token_str.to_string()),
                config.n.unwrap_or(10),
                config.stop_token.clone(),
                config.temperature.unwrap_or(0.8),
                config.include_stop_str_in_output.unwrap_or(false),
                None,
            );
            let prm = Arc::new(HttpProcessRewardModel::new(prm_endpoint));
            Box::new(ParticleGibbs::new(
                sg,
                prm,
                num_iterations,
                SelectionMethod::Argmax,
                1,
                false,
                0.5,
                0.5,
                ResamplingMethod::Systematic,
                TemperatureMethod::Ess,
            ))
        }
        "entropic-particle-filtering" => {
            let step_token_str = config
                .step_token
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("step_token required for entropic-particle-filtering".into()))?;
            let prm_endpoint = config
                .prm_endpoint
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("prm_endpoint required for entropic-particle-filtering".into()))?;

            let temp_method = match config.temperature_method.as_deref() {
                Some("entropy") => TemperatureMethod::Entropy,
                Some("base") => TemperatureMethod::Base,
                _ => TemperatureMethod::Ess,
            };

            let sg = StepGeneration::with_step_token(
                StepToken::Single(step_token_str.to_string()),
                config.n.unwrap_or(10),
                config.stop_token.clone(),
                config.temperature.unwrap_or(0.8),
                config.include_stop_str_in_output.unwrap_or(false),
                None,
            );
            let prm = Arc::new(HttpProcessRewardModel::new(prm_endpoint));
            Box::new(EntropicParticleFiltering::new(
                sg,
                prm,
                SelectionMethod::Argmax,
                ResamplingMethod::Systematic,
                temp_method,
                0.5,
                0.5,
            ))
        }
        "planning-wrapper" => {
            let inner_alg_name = config
                .inner_alg
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("inner_alg required for planning-wrapper".into()))?;

            let inner: Box<dyn crate::algorithms::ScalingAlgorithm> = match inner_alg_name {
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
                    let sc = SelfConsistency::with_error_replacement(
                        config.regex_patterns.clone(),
                        tool_vote,
                        config.replace_error_with_message.clone(),
                    )
                    .map_err(|e| AppError::BadRequest(format!("invalid regex pattern: {}", e)))?;
                    Box::new(sc)
                }
                "best-of-n" => {
                    let rm_endpoint = config.rm_endpoint.as_deref().ok_or_else(|| {
                        AppError::BadRequest("rm_endpoint required for best-of-n inner algorithm".into())
                    })?;
                    let orm = HttpOrmClient::new(rm_endpoint);
                    Box::new(BestOfN::with_error_replacement(
                        Box::new(orm),
                        config.replace_error_with_message.clone(),
                    ))
                }
                other => {
                    return Err(AppError::BadRequest(format!(
                        "unsupported inner algorithm for planning-wrapper: {}",
                        other
                    )));
                }
            };
            Box::new(PlanningWrapper::new(inner))
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported algorithm: {}. Supported: self-consistency, best-of-n, beam-search, \
                 particle-filtering, particle-gibbs, entropic-particle-filtering, planning-wrapper",
                other
            )));
        }
    };

    let max_concurrency = config.max_concurrent_requests.unwrap_or(64);

    let client = LmClient::new(
        &config.endpoint,
        config.api_key.as_deref(),
        &config.model,
        max_concurrency,
        8,
        config.system_prompt.clone(),
        config.include_stop_str_in_output,
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

    if request.messages.is_empty() {
        return Err(AppError::BadRequest("messages must not be empty".into()));
    }

    if let Some(t) = request.temperature {
        if !(0.0..=2.0).contains(&t) {
            return Err(AppError::BadRequest(format!(
                "temperature must be between 0.0 and 2.0, got {}",
                t
            )));
        }
    }

    if let Some(m) = request.max_tokens {
        if m == 0 {
            return Err(AppError::BadRequest(
                "max_tokens must be greater than 0".into(),
            ));
        }
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
