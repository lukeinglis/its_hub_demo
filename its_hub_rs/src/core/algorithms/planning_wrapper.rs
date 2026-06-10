use regex::Regex;
use serde_json::Value;

use async_trait::async_trait;

use crate::api::{AlgorithmOutput, ScalingAlgorithm};
use crate::core::lms::LmBackend;
use crate::api::types::{extract_content_from_lm_response, ChatMessage, Content};

const PLANNING_TEMPLATE: &str = r#"Before solving this problem, I want you to first create a plan with different approaches to explore. This will help generate diverse solution strategies.

Problem: {problem}

Please provide a plan with 3 distinct approaches or hypotheses for solving this problem. Format your response as:

APPROACH 1: [Brief description of first method/strategy]
APPROACH 2: [Brief description of second method/strategy]
APPROACH 3: [Brief description of third method/strategy]

Make sure each approach represents a genuinely different mathematical strategy or perspective for tackling this problem."#;

const APPROACH_TEMPLATE: &str = r#"Using the {approach} method from your plan, solve this problem step by step:

Problem: {problem}

Approach to use: {approach}

Please solve the problem following this specific approach and show your work clearly. Make sure to box your final answer using \boxed{{answer}}."#;

const DEFAULT_APPROACHES: &[&str] = &[
    "Direct algebraic approach using standard techniques",
    "Alternative method using different mathematical properties",
    "Geometric or graphical interpretation approach",
];

fn create_planning_prompt(problem: &str) -> String {
    PLANNING_TEMPLATE.replace("{problem}", problem)
}

fn create_approach_prompt(problem: &str, approach: &str) -> String {
    APPROACH_TEMPLATE.replace("{problem}", problem).replace("{approach}", approach)
}

fn extract_approaches(plan: &str) -> Vec<String> {
    let mut approaches = Vec::new();

    let pattern = Regex::new(r"(?i)APPROACH\s+\d+:\s*(.+)").unwrap();
    for cap in pattern.captures_iter(plan) {
        let desc = cap[1].trim().to_string();
        if !desc.is_empty() {
            approaches.push(desc);
        }
    }

    if approaches.is_empty() {
        for line in plan.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix(|c: char| c.is_ascii_digit()) {
                if let Some(rest) = rest.strip_prefix('.') {
                    let approach = rest.trim();
                    if !approach.is_empty() {
                        approaches.push(approach.to_string());
                    }
                }
            }
        }
    }

    if approaches.len() < 2 {
        approaches = DEFAULT_APPROACHES.iter().map(|s| s.to_string()).collect();
    }

    approaches.truncate(3);
    approaches
}

fn get_result_score(metadata: &Value) -> f64 {
    let score_keys = [
        "best_score",
        "max_score",
        "score",
        "confidence",
        "probability",
        "weight",
    ];

    for key in &score_keys {
        if let Some(val) = metadata.get(key) {
            if let Some(n) = val.as_f64() {
                return n;
            }
            if let Some(arr) = val.as_array() {
                if let Some(max_val) = arr.iter().filter_map(|v| v.as_f64()).reduce(f64::max) {
                    return max_val;
                }
            }
        }
    }

    if let Some(scores) = metadata.get("scores").and_then(|v| v.as_array()) {
        if let Some(max_val) = scores.iter().filter_map(|v| v.as_f64()).reduce(f64::max) {
            return max_val;
        }
    }

    if let Some(all_scores) = metadata.get("all_scores").and_then(|v| v.as_array()) {
        if let Some(max_val) = all_scores.iter().filter_map(|v| v.as_f64()).reduce(f64::max) {
            return max_val;
        }
    }

    if let Some(log_weights) = metadata.get("log_weights_lst").and_then(|v| v.as_array()) {
        let all_weights: Vec<f64> = log_weights
            .iter()
            .flat_map(|w| {
                if let Some(arr) = w.as_array() {
                    arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>()
                } else if let Some(n) = w.as_f64() {
                    vec![n]
                } else {
                    vec![]
                }
            })
            .collect();
        if let Some(max_val) = all_weights.into_iter().reduce(f64::max) {
            return max_val;
        }
    }

    0.0
}

pub struct PlanningWrapper {
    base_algorithm: Box<dyn ScalingAlgorithm>,
}

impl PlanningWrapper {
    pub fn new(base_algorithm: Box<dyn ScalingAlgorithm>) -> Self {
        Self { base_algorithm }
    }

    /// Create a PlanningWrapper using SelfConsistency as the base algorithm.
    pub fn planning_self_consistency(
        regex_patterns: Option<Vec<String>>,
        tool_vote: Option<crate::core::algorithms::self_consistency::ToolVoteStrategy>,
    ) -> Result<Self, anyhow::Error> {
        let sc = crate::core::algorithms::self_consistency::SelfConsistency::new(regex_patterns, tool_vote)?;
        Ok(Self::new(Box::new(sc)))
    }

    /// Create a PlanningWrapper using BestOfN as the base algorithm.
    pub fn planning_best_of_n(
        orm: Box<dyn crate::api::OutcomeRewardModel>,
    ) -> Self {
        let bon = crate::core::algorithms::best_of_n::BestOfN::new(orm);
        Self::new(Box::new(bon))
    }

    /// Create a PlanningWrapper using BeamSearch as the base algorithm.
    pub fn planning_beam_search(
        step_generation: crate::step_generation::StepGeneration,
        prm: std::sync::Arc<dyn crate::api::ProcessRewardModel>,
        beam_width: usize,
    ) -> Self {
        let bs = crate::core::algorithms::beam_search::BeamSearch::new(step_generation, prm, beam_width);
        Self::new(Box::new(bs))
    }
}

#[async_trait]
impl ScalingAlgorithm for PlanningWrapper {
    async fn infer(
        &self,
        client: &LmBackend,
        messages: &[ChatMessage],
        budget: u32,
        return_response_only: bool,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error> {
        let problem_text: String = messages
            .iter()
            .map(|m| m.extract_text_content())
            .collect::<Vec<_>>()
            .join("\n");

        let planning_prompt_text = create_planning_prompt(&problem_text);
        let planning_messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text(planning_prompt_text)),
            tool_calls: None,
            tool_call_id: None,
        }];

        let plan_response = client
            .chat_completion(&planning_messages, temperature, max_tokens, None, None, None)
            .await?;
        let plan = extract_content_from_lm_response(&plan_response);

        let approaches = extract_approaches(&plan);

        let remaining_budget = budget.saturating_sub(1);
        let n = approaches.len() as u32;
        let budget_per_approach = remaining_budget.checked_div(n).unwrap_or(1).max(1);

        let mut approach_budgets: Vec<u32> = Vec::new();
        let mut total_allocated: u32 = 0;
        for i in 0..approaches.len() {
            let mut b = budget_per_approach;
            if n > 0 && total_allocated + b < remaining_budget && (i as u32) < (remaining_budget % n) {
                b += 1;
            }
            approach_budgets.push(b);
            total_allocated += b;
        }

        let mut best_approach_idx: Option<usize> = None;
        let mut best_score = f64::NEG_INFINITY;
        let mut best_selected: Option<Value> = None;
        let mut best_metadata: Option<Value> = None;

        let mut all_approach_results: Vec<Value> = Vec::new();

        for (i, approach) in approaches.iter().enumerate() {
            let approach_prompt = create_approach_prompt(&problem_text, approach);
            let approach_messages = vec![ChatMessage {
                role: "user".to_string(),
                content: Some(Content::Text(approach_prompt)),
                tool_calls: None,
                tool_call_id: None,
            }];

            let result = self
                .base_algorithm
                .infer(
                    client,
                    &approach_messages,
                    approach_budgets[i],
                    false,
                    temperature,
                    max_tokens,
                    tools,
                    tool_choice,
                )
                .await?;

            let (selected, metadata) = match result {
                AlgorithmOutput::Full { selected, metadata } => (selected, metadata),
                AlgorithmOutput::ResponseOnly(v) => (v, serde_json::json!({})),
            };

            let score = get_result_score(&metadata);

            all_approach_results.push(serde_json::json!({
                "approach": approach,
                "budget": approach_budgets[i],
                "selected": selected,
                "metadata": metadata,
                "score": score,
            }));

            if score > best_score || best_approach_idx.is_none() {
                best_score = score;
                best_approach_idx = Some(i);
                best_selected = Some(selected);
                best_metadata = Some(metadata);
            }
        }

        let best_idx = best_approach_idx.unwrap_or(0);
        let selected = best_selected.unwrap_or(Value::Null);

        if return_response_only {
            Ok(AlgorithmOutput::ResponseOnly(selected))
        } else {
            let metadata = serde_json::json!({
                "algorithm": "planning-wrapper",
                "plan": plan,
                "approaches": approaches,
                "approach_budgets": approach_budgets,
                "approach_results": all_approach_results,
                "best_approach": approaches[best_idx],
                "best_approach_index": best_idx,
                "best_score": best_score,
                "inner_metadata": best_metadata,
            });
            Ok(AlgorithmOutput::Full { selected, metadata })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_planning_prompt_formatting() {
        let prompt = create_planning_prompt("Solve x + 1 = 3");
        assert!(prompt.contains("Problem: Solve x + 1 = 3"));
        assert!(prompt.contains("APPROACH 1:"));
        assert!(prompt.contains("APPROACH 2:"));
        assert!(prompt.contains("APPROACH 3:"));
    }

    #[test]
    fn test_approach_prompt_formatting() {
        let prompt = create_approach_prompt("Solve x + 1 = 3", "Direct algebra");
        assert!(prompt.contains("Problem: Solve x + 1 = 3"));
        assert!(prompt.contains("Approach to use: Direct algebra"));
        assert!(prompt.contains("Direct algebra"));
    }

    #[test]
    fn test_extract_approaches_structured() {
        let plan = "APPROACH 1: Direct algebraic approach\nAPPROACH 2: Substitution method\nAPPROACH 3: Geometric interpretation";
        let approaches = extract_approaches(plan);
        assert_eq!(approaches.len(), 3);
        assert_eq!(approaches[0], "Direct algebraic approach");
        assert_eq!(approaches[1], "Substitution method");
        assert_eq!(approaches[2], "Geometric interpretation");
    }

    #[test]
    fn test_extract_approaches_numbered_fallback() {
        let plan = "1. Direct method\n2. Alternative method\n3. Graphical method";
        let approaches = extract_approaches(plan);
        assert_eq!(approaches.len(), 3);
        assert_eq!(approaches[0], "Direct method");
        assert_eq!(approaches[1], "Alternative method");
        assert_eq!(approaches[2], "Graphical method");
    }

    #[test]
    fn test_extract_approaches_fallback_defaults() {
        let plan = "This is a general plan without structured approaches.";
        let approaches = extract_approaches(plan);
        assert_eq!(approaches.len(), 3);
        assert_eq!(approaches[0], DEFAULT_APPROACHES[0]);
    }

    #[test]
    fn test_extract_approaches_single_approach_uses_defaults() {
        let plan = "APPROACH 1: Only one approach here";
        let approaches = extract_approaches(plan);
        assert_eq!(approaches.len(), 3);
        assert_eq!(approaches[0], DEFAULT_APPROACHES[0]);
    }

    #[test]
    fn test_extract_approaches_truncates_to_three() {
        let plan = "APPROACH 1: First\nAPPROACH 2: Second\nAPPROACH 3: Third\nAPPROACH 4: Fourth\nAPPROACH 5: Fifth";
        let approaches = extract_approaches(plan);
        assert_eq!(approaches.len(), 3);
    }

    #[test]
    fn test_extract_approaches_case_insensitive() {
        let plan = "approach 1: First method\napproach 2: Second method\napproach 3: Third method";
        let approaches = extract_approaches(plan);
        assert_eq!(approaches.len(), 3);
        assert_eq!(approaches[0], "First method");
    }

    #[test]
    fn test_get_result_score_from_scores_array() {
        let metadata = json!({"scores": [0.3, 0.9, 0.5]});
        assert!((get_result_score(&metadata) - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_result_score_from_best_score() {
        let metadata = json!({"best_score": 0.85});
        assert!((get_result_score(&metadata) - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_result_score_empty_metadata() {
        let metadata = json!({});
        assert!((get_result_score(&metadata) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_result_score_log_weights_flat() {
        let metadata = json!({"log_weights_lst": [0.1, 0.5, 0.3]});
        assert!((get_result_score(&metadata) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_result_score_log_weights_nested() {
        let metadata = json!({"log_weights_lst": [[0.1, 0.2], [0.5, 0.3]]});
        assert!((get_result_score(&metadata) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_result_score_priority() {
        let metadata = json!({"best_score": 0.7, "scores": [0.9]});
        assert!(
            (get_result_score(&metadata) - 0.7).abs() < f64::EPSILON,
            "best_score should take priority over scores"
        );
    }

    #[test]
    fn test_planning_wrapper_creation() {
        use crate::core::algorithms::self_consistency::SelfConsistency;
        let sc = SelfConsistency::new(None, None).unwrap();
        let pw = PlanningWrapper::new(Box::new(sc));
        assert!(std::mem::size_of_val(&pw) > 0);
    }

    #[tokio::test]
    async fn test_planning_wrapper_basic_with_mock_server() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

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
                    "content": "Solving step by step.\n\n2x + 3 = 7\n2x = 4\nx = 2\n\n\\boxed{2}"
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

        let client = LmBackend::OpenAI(
            crate::client::LmClient::new(
                &format!("{}/v1", server.uri()),
                Some("test-key"),
                "test-model",
                8,
                3,
                None,
                None,
            )
            .unwrap(),
        );

        use crate::core::algorithms::self_consistency::SelfConsistency;
        let sc = SelfConsistency::new(None, None).unwrap();
        let pw = PlanningWrapper::new(Box::new(sc));

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text("Solve for x: 2x + 3 = 7".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let result = pw
            .infer(&client, &messages, 4, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { selected, metadata } => {
                assert!(selected.get("content").is_some());
                assert_eq!(metadata["algorithm"], "planning-wrapper");
                assert!(metadata.get("plan").is_some());
                assert!(metadata.get("approaches").is_some());
                assert!(metadata.get("best_approach").is_some());
                let approaches = metadata["approaches"].as_array().unwrap();
                assert!(!approaches.is_empty());
            }
            AlgorithmOutput::ResponseOnly(_) => {
                panic!("expected Full output");
            }
        }
    }

    #[tokio::test]
    async fn test_planning_wrapper_response_only() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        let response = json!({
            "id": "chatcmpl-1",
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
            "usage": {"prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20}
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(&server)
            .await;

        let client = LmBackend::OpenAI(
            crate::client::LmClient::new(
                &format!("{}/v1", server.uri()),
                Some("test-key"),
                "test-model",
                8,
                3,
                None,
                None,
            )
            .unwrap(),
        );

        use crate::core::algorithms::self_consistency::SelfConsistency;
        let sc = SelfConsistency::new(None, None).unwrap();
        let pw = PlanningWrapper::new(Box::new(sc));

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text("Solve x+1=3".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let result = pw
            .infer(&client, &messages, 4, true, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::ResponseOnly(v) => {
                assert!(v.get("content").is_some() || v.get("role").is_some());
            }
            AlgorithmOutput::Full { .. } => {
                panic!("expected ResponseOnly");
            }
        }
    }

    #[tokio::test]
    async fn test_planning_wrapper_result_includes_plan() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        let response = json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1700000000u64,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "APPROACH 1: Algebra\nAPPROACH 2: Geometry\nAPPROACH 3: Calculus"
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20}
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(&server)
            .await;

        let client = LmBackend::OpenAI(
            crate::client::LmClient::new(
                &format!("{}/v1", server.uri()),
                Some("test-key"),
                "test-model",
                8,
                3,
                None,
                None,
            )
            .unwrap(),
        );

        use crate::core::algorithms::self_consistency::SelfConsistency;
        let sc = SelfConsistency::new(None, None).unwrap();
        let pw = PlanningWrapper::new(Box::new(sc));

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text("Problem".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let result = pw
            .infer(&client, &messages, 4, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { metadata, .. } => {
                let plan = metadata["plan"].as_str().unwrap();
                assert!(plan.contains("APPROACH 1:"));
                assert!(plan.contains("Algebra"));

                let approaches = metadata["approaches"].as_array().unwrap();
                assert_eq!(approaches.len(), 3);
                assert_eq!(approaches[0].as_str().unwrap(), "Algebra");
                assert_eq!(approaches[1].as_str().unwrap(), "Geometry");
                assert_eq!(approaches[2].as_str().unwrap(), "Calculus");

                assert!(metadata.get("best_approach").is_some());
                assert!(metadata.get("approach_budgets").is_some());
                assert!(metadata.get("approach_results").is_some());
            }
            _ => panic!("expected Full"),
        }
    }
}
