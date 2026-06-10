use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use tracing::{info, warn};

use crate::api::algorithm::{AlgorithmOutput, ScalingAlgorithm};
use crate::api::types::{
    ChatCompletionChoice, ChatCompletionRequest, ChatCompletionResponse, ChatCompletionUsage,
    ConfigRequest, ConfigResponse, ModelInfo, ModelsResponse,
};
use crate::core::algorithms::beam_search::BeamSearch;
use crate::core::algorithms::best_of_n::{BestOfN, HttpOrmClient};
use crate::core::algorithms::particle_gibbs::{
    EntropicParticleFiltering, ParticleFiltering, ParticleGibbs, ResamplingMethod,
    SelectionMethod, TemperatureMethod,
};
use crate::core::algorithms::planning_wrapper::PlanningWrapper;
use crate::core::algorithms::self_consistency::{SelfConsistency, ToolVoteStrategy};
use crate::core::cache::{CacheKey, TokenCache};
use crate::core::lms::step_generation::{StepGeneration, StepToken};
use crate::core::lms::LmClient;
use crate::core::reward_models::{HttpProcessRewardModel, LlmJudgeRewardModel};

use super::error::AppError;
use super::passthrough::passthrough_to_upstream;
use super::state::{AlgorithmConfig, AppState};

/// Build an algorithm from its name using the provided config.
/// Extracted to allow reuse for both /configure and header-based override.
fn build_algorithm(
    alg_name: &str,
    config: &ConfigRequest,
) -> Result<Arc<dyn ScalingAlgorithm>, AppError> {
    match alg_name {
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
            Ok(Arc::new(sc))
        }
        "best-of-n" => {
            let rm_name = config.rm_name.as_deref().unwrap_or("http");

            if rm_name == "llm-judge" {
                let judge_model = config.judge_model.as_deref().ok_or_else(|| {
                    AppError::BadRequest("judge_model required when rm_name is llm-judge".into())
                })?;
                let criterion = config
                    .judge_criterion
                    .as_deref()
                    .unwrap_or("overall_quality");
                let judge_temp = config.judge_temperature.unwrap_or(0.0);
                let judge_max = config.judge_max_tokens.unwrap_or(4096);

                let judge_base_url = config
                    .judge_base_url
                    .as_deref()
                    .unwrap_or(&config.endpoint);

                let judge_client = LmClient::new(
                    judge_base_url,
                    config
                        .judge_api_key
                        .as_deref()
                        .or(config.api_key.as_deref()),
                    judge_model,
                    8,
                    3,
                    None,
                    None,
                )
                .map_err(|e| {
                    AppError::BadRequest(format!("failed to create judge client: {}", e))
                })?;

                let judge = LlmJudgeRewardModel::new(
                    judge_client,
                    criterion.to_string(),
                    judge_temp,
                    judge_max,
                );
                Ok(Arc::new(BestOfN::with_error_replacement(
                    Box::new(judge),
                    config.replace_error_with_message.clone(),
                )))
            } else {
                let rm_endpoint = config.rm_endpoint.as_deref().ok_or_else(|| {
                    AppError::BadRequest("rm_endpoint required for best-of-n".into())
                })?;

                let orm = HttpOrmClient::new(rm_endpoint);
                Ok(Arc::new(BestOfN::with_error_replacement(
                    Box::new(orm),
                    config.replace_error_with_message.clone(),
                )))
            }
        }
        "beam-search" => {
            let step_token_str = config.step_token.as_deref().ok_or_else(|| {
                AppError::BadRequest("step_token required for beam-search".into())
            })?;
            let prm_endpoint = config.prm_endpoint.as_deref().ok_or_else(|| {
                AppError::BadRequest("prm_endpoint required for beam-search".into())
            })?;
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
            Ok(Arc::new(BeamSearch::new(sg, prm, beam_width)))
        }
        "particle-filtering" => {
            let step_token_str = config.step_token.as_deref().ok_or_else(|| {
                AppError::BadRequest("step_token required for particle-filtering".into())
            })?;
            let prm_endpoint = config.prm_endpoint.as_deref().ok_or_else(|| {
                AppError::BadRequest("prm_endpoint required for particle-filtering".into())
            })?;

            let sg = StepGeneration::with_step_token(
                StepToken::Single(step_token_str.to_string()),
                config.n.unwrap_or(10),
                config.stop_token.clone(),
                config.temperature.unwrap_or(0.8),
                config.include_stop_str_in_output.unwrap_or(false),
                None,
            );
            let prm = Arc::new(HttpProcessRewardModel::new(prm_endpoint));
            Ok(Arc::new(ParticleFiltering::new(
                sg,
                prm,
                SelectionMethod::Argmax,
                ResamplingMethod::Systematic,
            )))
        }
        "particle-gibbs" => {
            let step_token_str = config.step_token.as_deref().ok_or_else(|| {
                AppError::BadRequest("step_token required for particle-gibbs".into())
            })?;
            let prm_endpoint = config.prm_endpoint.as_deref().ok_or_else(|| {
                AppError::BadRequest("prm_endpoint required for particle-gibbs".into())
            })?;
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
            Ok(Arc::new(ParticleGibbs::new(
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
            )))
        }
        "entropic-particle-filtering" => {
            let step_token_str = config.step_token.as_deref().ok_or_else(|| {
                AppError::BadRequest(
                    "step_token required for entropic-particle-filtering".into(),
                )
            })?;
            let prm_endpoint = config.prm_endpoint.as_deref().ok_or_else(|| {
                AppError::BadRequest(
                    "prm_endpoint required for entropic-particle-filtering".into(),
                )
            })?;

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
            Ok(Arc::new(EntropicParticleFiltering::new(
                sg,
                prm,
                SelectionMethod::Argmax,
                ResamplingMethod::Systematic,
                temp_method,
                0.5,
                0.5,
            )))
        }
        "planning-wrapper" => {
            let inner_alg_name = config.inner_alg.as_deref().ok_or_else(|| {
                AppError::BadRequest("inner_alg required for planning-wrapper".into())
            })?;

            let inner: Box<dyn ScalingAlgorithm> = match inner_alg_name {
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
                    .map_err(|e| {
                        AppError::BadRequest(format!("invalid regex pattern: {}", e))
                    })?;
                    Box::new(sc)
                }
                "best-of-n" => {
                    let rm_endpoint =
                        config.rm_endpoint.as_deref().ok_or_else(|| {
                            AppError::BadRequest(
                                "rm_endpoint required for best-of-n inner algorithm".into(),
                            )
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
            Ok(Arc::new(PlanningWrapper::new(inner)))
        }
        other => Err(AppError::BadRequest(format!(
            "unsupported algorithm: {}. Supported: self-consistency, best-of-n, beam-search, \
             particle-filtering, particle-gibbs, entropic-particle-filtering, planning-wrapper",
            other
        ))),
    }
}

pub async fn configure(
    State(state): State<Arc<AppState>>,
    Json(config): Json<ConfigRequest>,
) -> Result<Json<ConfigResponse>, AppError> {
    let alg_name = config.alg.clone();

    let algorithm = build_algorithm(&alg_name, &config)?;

    let max_concurrency = config.max_concurrent_requests.unwrap_or(64);

    let lm_client = LmClient::new(
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
        clients_lock.insert(model_name.clone(), Arc::new(lm_client));
    }

    // Update passthrough_on_error (default true)
    if let Some(passthrough) = config.passthrough_on_error {
        let mut pt_lock = state.passthrough_on_error.write().await;
        *pt_lock = passthrough;
    }

    // Configure cache
    let cache_enabled = config.cache_enabled.unwrap_or(false);
    {
        let mut ce_lock = state.cache_enabled.write().await;
        *ce_lock = cache_enabled;
    }
    if cache_enabled {
        let max_entries = config.cache_max_entries.unwrap_or(10000);
        let ttl = config.cache_ttl_seconds.unwrap_or(300);
        let mut cache_lock = state.cache.write().await;
        *cache_lock = Some(Arc::new(TokenCache::new(max_entries, ttl)));
    }

    info!(model = %model_name, algorithm = %alg_name, "configured");

    Ok(Json(ConfigResponse {
        status: "success".to_string(),
        message: format!("Initialized {} with {} algorithm", model_name, alg_name),
    }))
}

pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<impl IntoResponse, AppError> {
    let start = Instant::now();

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

    // Read Envoy header overrides
    let header_algorithm = headers
        .get("x-its-algorithm")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let header_budget = headers
        .get("x-its-budget")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok());

    let budget = header_budget.unwrap_or(request.budget);
    if !(1..=1000).contains(&budget) {
        return Err(AppError::BadRequest(format!(
            "budget must be between 1 and 1000, got {}",
            budget
        )));
    }

    // Check cache before anything else
    let cache_enabled = *state.cache_enabled.read().await;
    let cache_key = if cache_enabled {
        let messages_val = serde_json::to_value(&request.messages).unwrap_or_default();
        Some(CacheKey::new(
            &request.model,
            &messages_val,
            request.temperature,
            request.max_tokens,
        ))
    } else {
        None
    };

    if let Some(ref key) = cache_key {
        let cache_lock = state.cache.read().await;
        if let Some(ref cache) = *cache_lock {
            if let Some(cached_response) = cache.get(key).await {
                let elapsed = start.elapsed().as_millis();
                let response: ChatCompletionResponse =
                    serde_json::from_value(cached_response).map_err(|e| {
                        AppError::Algorithm(format!("failed to deserialize cached response: {}", e))
                    })?;
                let mut response_headers = HeaderMap::new();
                response_headers
                    .insert("x-its-cache-hit", "true".parse().unwrap());
                response_headers.insert(
                    "x-its-latency-ms",
                    elapsed.to_string().parse().unwrap(),
                );
                if let Some(ref alg_name) = header_algorithm {
                    response_headers.insert(
                        "x-its-algorithm-used",
                        alg_name.parse().unwrap(),
                    );
                } else {
                    let alg_lock = state.algorithm.read().await;
                    if let Some(ref alg_config) = *alg_lock {
                        response_headers.insert(
                            "x-its-algorithm-used",
                            alg_config.name.parse().unwrap(),
                        );
                    }
                }
                return Ok((response_headers, Json(response)));
            }
        }
    }

    // Resolve which algorithm to use
    let (algorithm, alg_name_used) = if let Some(ref alg_override) = header_algorithm {
        // Header override: build a temporary self-consistency (or whatever) from name alone.
        // For simplicity, only "self-consistency" can be overridden via header without
        // full config. Others need the full /configure path. If the override matches the
        // configured algorithm, just use that.
        let alg_lock = state.algorithm.read().await;
        if let Some(ref alg_config) = *alg_lock {
            if alg_config.name == *alg_override {
                (Arc::clone(&alg_config.algorithm), alg_config.name.clone())
            } else if alg_override == "self-consistency" {
                let sc = SelfConsistency::new(None, None)
                    .map_err(|e| AppError::BadRequest(format!("invalid regex: {}", e)))?;
                (Arc::new(sc) as Arc<dyn ScalingAlgorithm>, alg_override.clone())
            } else {
                // Fall back to the configured algorithm if we can't build the override
                warn!(
                    requested = %alg_override,
                    configured = %alg_config.name,
                    "cannot build header-override algorithm, using configured"
                );
                (Arc::clone(&alg_config.algorithm), alg_config.name.clone())
            }
        } else {
            return Err(AppError::NotConfigured);
        }
    } else {
        let alg_lock = state.algorithm.read().await;
        let alg_config = alg_lock.as_ref().ok_or(AppError::NotConfigured)?;
        (
            Arc::clone(&alg_config.algorithm),
            alg_config.name.clone(),
        )
    };

    let client = {
        let clients_lock = state.clients.read().await;
        Arc::clone(
            clients_lock
                .get(&request.model)
                .ok_or_else(|| AppError::ModelNotFound(request.model.clone()))?,
        )
    };

    let tools_value = request
        .tools
        .as_ref()
        .map(|t| serde_json::Value::Array(t.clone()));

    let result = algorithm
        .infer(
            &client,
            &request.messages,
            budget,
            request.return_response_only,
            request.temperature,
            request.max_tokens,
            tools_value.as_ref(),
            request.tool_choice.as_ref(),
        )
        .await;

    // Handle algorithm failure with passthrough fallback
    let output = match result {
        Ok(output) => output,
        Err(e) => {
            let passthrough_enabled = *state.passthrough_on_error.read().await;
            if passthrough_enabled {
                warn!(error = %e, "algorithm failed, falling back to passthrough");
                let passthrough_response = passthrough_to_upstream(&client, &request).await?;
                let elapsed = start.elapsed().as_millis();
                let mut response_headers = HeaderMap::new();
                response_headers
                    .insert("x-its-cache-hit", "false".parse().unwrap());
                response_headers.insert(
                    "x-its-algorithm-used",
                    "passthrough".parse().unwrap(),
                );
                response_headers.insert(
                    "x-its-latency-ms",
                    elapsed.to_string().parse().unwrap(),
                );
                return Ok((response_headers, Json(passthrough_response)));
            } else {
                return Err(AppError::Algorithm(e.to_string()));
            }
        }
    };

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

    // Store in cache
    if let Some(ref key) = cache_key {
        let cache_lock = state.cache.read().await;
        if let Some(ref cache) = *cache_lock {
            if let Ok(response_val) = serde_json::to_value(&response) {
                cache.put(key.clone(), response_val).await;
            }
        }
    }

    let elapsed = start.elapsed().as_millis();
    let mut response_headers = HeaderMap::new();
    response_headers.insert("x-its-cache-hit", "false".parse().unwrap());
    response_headers.insert(
        "x-its-algorithm-used",
        alg_name_used.parse().unwrap(),
    );
    response_headers.insert(
        "x-its-latency-ms",
        elapsed.to_string().parse().unwrap(),
    );

    Ok((response_headers, Json(response)))
}

pub async fn list_models(State(state): State<Arc<AppState>>) -> Json<ModelsResponse> {
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

/// Combined health endpoint (existing behavior, extended with cache stats).
pub async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let alg_lock = state.algorithm.read().await;
    let clients_lock = state.clients.read().await;

    let algorithm = alg_lock.as_ref().map(|a| a.name.clone());
    let models = clients_lock.len();
    let passthrough_on_error = *state.passthrough_on_error.read().await;

    let cache_stats = {
        let cache_lock = state.cache.read().await;
        if let Some(ref cache) = *cache_lock {
            Some(cache.stats().await)
        } else {
            None
        }
    };

    Json(json!({
        "status": "ok",
        "algorithm": algorithm,
        "models": models,
        "passthrough_on_error": passthrough_on_error,
        "cache": cache_stats,
    }))
}

/// Liveness probe: always returns 200 if the process is running.
pub async fn health_live() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "alive"})))
}

/// Readiness probe: returns 200 only if an algorithm is configured and
/// at least one model client is registered.
pub async fn health_ready(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let alg_lock = state.algorithm.read().await;
    let clients_lock = state.clients.read().await;

    let has_algorithm = alg_lock.is_some();
    let has_models = !clients_lock.is_empty();

    if has_algorithm && has_models {
        (StatusCode::OK, Json(json!({"status": "ready"})))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "algorithm_configured": has_algorithm,
                "models_connected": clients_lock.len(),
            })),
        )
    }
}
