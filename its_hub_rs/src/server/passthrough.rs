//! Passthrough handler: forwards requests directly to the upstream vLLM backend
//! without running any ITS algorithm. Used when:
//! - The gateway is not configured (no algorithm set)
//! - The algorithm fails and `passthrough_on_error` is enabled
//! - The gateway is in degraded mode

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::api::types::{
    ChatCompletionChoice, ChatCompletionRequest, ChatCompletionResponse, ChatCompletionUsage,
};
use crate::core::lms::LmClient;

use super::error::AppError;

/// Forward the request as-is to the upstream LM and return the raw response
/// without any ITS processing.
pub async fn passthrough_to_upstream(
    client: &LmClient,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, AppError> {
    let result = client
        .chat_completion(
            &request.messages,
            request.temperature,
            request.max_tokens,
            None,
            request.tools.as_ref().map(|t| {
                serde_json::Value::Array(t.clone())
            }).as_ref(),
            request.tool_choice.as_ref(),
        )
        .await
        .map_err(AppError::Upstream)?;

    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let usage = result.usage.unwrap_or(ChatCompletionUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    });

    Ok(ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created,
        model: request.model.clone(),
        choices: vec![ChatCompletionChoice {
            index: 0,
            message: result.message,
            finish_reason: "stop".to_string(),
        }],
        usage,
        metadata: Some(json!({"passthrough": true})),
    })
}
