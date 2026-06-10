use std::collections::HashMap;

use async_trait::async_trait;
use rand::Rng;
use regex::{Regex, RegexBuilder};
use serde_json::Value;
use tracing::warn;

use crate::api::{AbstractLanguageModel, AlgorithmOutput, ScalingAlgorithm};
use crate::api::types::{extract_content_from_lm_response, ChatMessage};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectedValue {
    Single(Option<String>),
    Tuple(Vec<Option<String>>),
}

#[derive(Debug, Clone)]
pub enum ToolVoteStrategy {
    Name,
    Args { exclude: Vec<String> },
    Hierarchical { exclude: Vec<String> },
}

pub enum Projection {
    Default,
    Regex(Vec<Regex>),
    Custom(Box<dyn Fn(&Value) -> ProjectedValue + Send + Sync>),
}

pub struct SelfConsistency {
    projection: Projection,
    tool_vote: Option<ToolVoteStrategy>,
    replace_error_with_message: Option<String>,
}

impl SelfConsistency {
    pub fn new(
        regex_patterns: Option<Vec<String>>,
        tool_vote: Option<ToolVoteStrategy>,
    ) -> Result<Self, anyhow::Error> {
        Self::with_error_replacement(regex_patterns, tool_vote, None)
    }

    pub fn with_custom_projection(
        projection_func: Box<dyn Fn(&Value) -> ProjectedValue + Send + Sync>,
        tool_vote: Option<ToolVoteStrategy>,
        replace_error_with_message: Option<String>,
    ) -> Self {
        Self {
            projection: Projection::Custom(projection_func),
            tool_vote,
            replace_error_with_message,
        }
    }

    pub fn with_error_replacement(
        regex_patterns: Option<Vec<String>>,
        tool_vote: Option<ToolVoteStrategy>,
        replace_error_with_message: Option<String>,
    ) -> Result<Self, anyhow::Error> {
        let projection = match regex_patterns {
            Some(patterns) if !patterns.is_empty() => {
                let compiled: Result<Vec<Regex>, _> = patterns
                    .iter()
                    .map(|p| {
                        RegexBuilder::new(p)
                            .case_insensitive(true)
                            .dot_matches_new_line(true)
                            .build()
                    })
                    .collect();
                Projection::Regex(compiled?)
            }
            _ => Projection::Default,
        };

        Ok(Self {
            projection,
            tool_vote,
            replace_error_with_message,
        })
    }

    fn project_content(&self, response: &Value) -> ProjectedValue {
        match &self.projection {
            Projection::Custom(f) => f(response),
            _ => {
                let content = extract_content_from_lm_response(response);
                match &self.projection {
                    Projection::Default => {
                        ProjectedValue::Single(Some(content.trim().to_string()))
                    }
                    Projection::Regex(patterns) => {
                        let results: Vec<Option<String>> = patterns
                            .iter()
                            .map(|pattern| {
                                pattern.captures(&content).and_then(|caps| {
                                    if caps.len() > 1 {
                                        caps.get(1).map(|m| m.as_str().trim().to_string())
                                    } else {
                                        caps.get(0).map(|m| m.as_str().trim().to_string())
                                    }
                                })
                            })
                            .collect();
                        ProjectedValue::Tuple(results)
                    }
                    Projection::Custom(_) => unreachable!(),
                }
            }
        }
    }

    fn extract_tool_call_features(&self, response: &Value) -> ProjectedValue {
        let strategy = match &self.tool_vote {
            Some(s) => s,
            None => return ProjectedValue::Single(None),
        };

        let tool_calls = response.get("tool_calls").and_then(|tc| tc.as_array());
        let first_tc = match tool_calls.and_then(|tcs| tcs.first()) {
            Some(tc) => tc,
            None => match strategy {
                ToolVoteStrategy::Name => return ProjectedValue::Single(None),
                _ => return ProjectedValue::Tuple(vec![None, None]),
            },
        };

        let function_name = first_tc
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());

        let raw_args = first_tc
            .get("function")
            .and_then(|f| f.get("arguments"))
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));

        let parsed_args = match &raw_args {
            Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Object(serde_json::Map::new())),
            Value::Object(_) => raw_args,
            _ => Value::Object(serde_json::Map::new()),
        };

        let exclude = match strategy {
            ToolVoteStrategy::Args { exclude } | ToolVoteStrategy::Hierarchical { exclude } => {
                exclude
            }
            ToolVoteStrategy::Name => &vec![],
        };

        let filtered_args = filter_args(&parsed_args, exclude);
        let canonical_args = make_hashable(&filtered_args);

        match strategy {
            ToolVoteStrategy::Name => ProjectedValue::Single(function_name),
            ToolVoteStrategy::Args { .. } => {
                let mut pairs: Vec<Option<String>> = Vec::new();
                if let Value::Object(map) = &filtered_args {
                    let mut sorted_keys: Vec<&String> = map.keys().collect();
                    sorted_keys.sort();
                    for key in sorted_keys {
                        let val = &map[key];
                        pairs.push(Some(format!("{}={}", key, canonical_json(val))));
                    }
                }
                if pairs.is_empty() {
                    ProjectedValue::Tuple(vec![])
                } else {
                    ProjectedValue::Tuple(pairs)
                }
            }
            ToolVoteStrategy::Hierarchical { .. } => {
                ProjectedValue::Tuple(vec![function_name, Some(canonical_args)])
            }
        }
    }

    pub fn process_responses(
        &self,
        responses: Vec<Value>,
        return_response_only: bool,
    ) -> Result<AlgorithmOutput, anyhow::Error> {
        if responses.is_empty() {
            anyhow::bail!("No responses to process");
        }

        let tool_call_count = responses
            .iter()
            .filter(|r| has_tool_calls(r))
            .count();
        let required_majority = responses.len().div_ceil(2);
        let has_majority_tool_calls = tool_call_count >= required_majority;

        if tool_call_count > 0 && self.tool_vote.is_none() {
            warn!(
                tool_call_count,
                total = responses.len(),
                "responses contain tool calls but tool_vote is not set"
            );
        }

        let (eligible_indices, projected): (Vec<usize>, Vec<ProjectedValue>) =
            if has_majority_tool_calls && self.tool_vote.is_some() {
                let indices: Vec<usize> = responses
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| has_tool_calls(r))
                    .map(|(i, _)| i)
                    .collect();
                let projected: Vec<ProjectedValue> = indices
                    .iter()
                    .map(|&i| self.extract_tool_call_features(&responses[i]))
                    .collect();
                (indices, projected)
            } else {
                let indices: Vec<usize> = responses
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| !has_tool_calls(r))
                    .map(|(i, _)| i)
                    .collect();
                let projected: Vec<ProjectedValue> = indices
                    .iter()
                    .map(|&i| self.project_content(&responses[i]))
                    .collect();
                (indices, projected)
            };

        if eligible_indices.is_empty() {
            anyhow::bail!(
                "No eligible responses found after filtering. \
                 Total responses: {}, responses with tool calls: {}. \
                 This typically happens when tool_vote is not set but all responses contain tool calls.",
                responses.len(),
                tool_call_count
            );
        }

        let is_hierarchical = matches!(projected.first(), Some(ProjectedValue::Tuple(_)));

        let (counts, filtered_selected_index) = if is_hierarchical {
            select_hierarchical_most_common_or_random(&projected)?
        } else {
            select_most_common_or_random(&projected)
        };

        let selected_index = eligible_indices[filtered_selected_index];

        if return_response_only {
            Ok(AlgorithmOutput::ResponseOnly(
                responses[selected_index].clone(),
            ))
        } else {
            let counts_value = counts_to_json(&counts);
            let metadata = serde_json::json!({
                "algorithm": "self-consistency",
                "all_responses": responses,
                "response_counts": counts_value,
                "selected_index": selected_index,
            });
            Ok(AlgorithmOutput::Full {
                selected: responses[selected_index].clone(),
                metadata,
            })
        }
    }
}

#[async_trait]
impl ScalingAlgorithm for SelfConsistency {
    async fn infer(
        &self,
        client: &dyn AbstractLanguageModel,
        messages: &[ChatMessage],
        budget: u32,
        return_response_only: bool,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error> {
        let results = client
            .fan_out(messages, budget, temperature, max_tokens, tools, tool_choice)
            .await;

        let fallback_msg = self
            .replace_error_with_message
            .as_deref()
            .unwrap_or("Error during generation");

        let mut responses = Vec::new();
        let mut all_failed = true;
        for result in results {
            match result {
                Ok(msg) => {
                    all_failed = false;
                    responses.push(msg);
                }
                Err(e) => {
                    warn!(error = %e, "fan-out request failed, substituting error response");
                    responses.push(serde_json::json!({
                        "role": "assistant",
                        "content": format!("{}: {}", fallback_msg, e)
                    }));
                }
            }
        }

        if all_failed {
            anyhow::bail!("all fan-out requests failed");
        }

        self.process_responses(responses, return_response_only)
    }
}

fn has_tool_calls(response: &Value) -> bool {
    response
        .get("tool_calls")
        .map(|tc| match tc {
            Value::Array(arr) => !arr.is_empty(),
            Value::Null => false,
            _ => false,
        })
        .unwrap_or(false)
}

fn filter_args(args: &Value, exclude: &[String]) -> Value {
    match args {
        Value::Object(map) => {
            let filtered: serde_json::Map<String, Value> = map
                .iter()
                .filter(|(k, _)| !exclude.contains(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Value::Object(filtered)
        }
        other => other.clone(),
    }
}

fn make_hashable(value: &Value) -> String {
    canonical_json(value)
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut pairs: Vec<(&String, &Value)> = map.iter().collect();
            pairs.sort_by_key(|(k, _)| *k);
            let entries: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical_json(v)))
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", items.join(","))
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Build a `Projection::Regex` from a list of pattern strings.
///
/// Convenience factory so callers can construct the projection without
/// going through `SelfConsistency::new`.
pub fn create_regex_projection_function(
    patterns: Vec<String>,
) -> Result<Projection, regex::Error> {
    let compiled: Result<Vec<Regex>, _> = patterns
        .iter()
        .map(|p| {
            RegexBuilder::new(p)
                .case_insensitive(true)
                .dot_matches_new_line(true)
                .build()
        })
        .collect();
    Ok(Projection::Regex(compiled?))
}

pub fn select_most_common_or_random(
    values: &[ProjectedValue],
) -> (HashMap<ProjectedValue, usize>, usize) {
    let mut counts: HashMap<ProjectedValue, usize> = HashMap::new();
    for v in values {
        *counts.entry(v.clone()).or_default() += 1;
    }

    let max_count = *counts.values().max().unwrap();

    let most_common_indices: Vec<usize> = values
        .iter()
        .enumerate()
        .filter(|(_, v)| counts[v] == max_count)
        .map(|(i, _)| i)
        .collect();

    let selected = most_common_indices[rand::thread_rng().gen_range(0..most_common_indices.len())];

    (counts, selected)
}

pub fn select_hierarchical_most_common_or_random(
    values: &[ProjectedValue],
) -> Result<(HashMap<ProjectedValue, usize>, usize), anyhow::Error> {
    if values.is_empty() {
        anyhow::bail!("Cannot select from empty list");
    }

    let tuples: Vec<&Vec<Option<String>>> = values
        .iter()
        .map(|v| match v {
            ProjectedValue::Tuple(t) => Ok(t),
            ProjectedValue::Single(_) => Err(anyhow::anyhow!(
                "hierarchical voting requires Tuple projected values"
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if tuples.iter().all(|t| t.len() == 1) {
        let flat: Vec<ProjectedValue> = tuples
            .iter()
            .map(|t| ProjectedValue::Single(t[0].clone()))
            .collect();
        let (_, selected_index) = select_most_common_or_random(&flat);
        let tuple_counts = count_values(values);
        return Ok((tuple_counts, selected_index));
    }

    let max_depth = tuples.iter().map(|t| t.len()).max().unwrap_or(0);
    let mut candidate_indices: Vec<usize> = (0..values.len()).collect();

    for level in 0..max_depth {
        let mut level_values: Vec<Option<String>> = Vec::new();
        let mut valid_indices: Vec<usize> = Vec::new();

        for &idx in &candidate_indices {
            if level < tuples[idx].len() {
                level_values.push(tuples[idx][level].clone());
                valid_indices.push(idx);
            }
        }

        if level_values.is_empty() {
            break;
        }

        let mut level_counts: HashMap<Option<String>, usize> = HashMap::new();
        for v in &level_values {
            *level_counts.entry(v.clone()).or_default() += 1;
        }

        let max_count = *level_counts.values().max().unwrap();

        let new_candidates: Vec<usize> = valid_indices
            .iter()
            .enumerate()
            .filter(|(i, _)| level_counts[&level_values[*i]] == max_count)
            .map(|(_, &idx)| idx)
            .collect();

        candidate_indices = new_candidates;

        if candidate_indices.len() == 1 {
            break;
        }
    }

    let selected =
        candidate_indices[rand::thread_rng().gen_range(0..candidate_indices.len())];

    let tuple_counts = count_values(values);
    Ok((tuple_counts, selected))
}

fn count_values(values: &[ProjectedValue]) -> HashMap<ProjectedValue, usize> {
    let mut counts = HashMap::new();
    for v in values {
        *counts.entry(v.clone()).or_default() += 1;
    }
    counts
}

fn counts_to_json(counts: &HashMap<ProjectedValue, usize>) -> Value {
    let map: serde_json::Map<String, Value> = counts
        .iter()
        .map(|(k, v)| {
            let key = match k {
                ProjectedValue::Single(Some(s)) => s.clone(),
                ProjectedValue::Single(None) => "null".to_string(),
                ProjectedValue::Tuple(items) => format!("{:?}", items),
            };
            (key, serde_json::json!(v))
        })
        .collect();
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn content_response(content: &str) -> Value {
        json!({"role": "assistant", "content": content})
    }

    fn tool_call_response(name: &str, args: Value) -> Value {
        json!({
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
        })
    }

    #[test]
    fn flat_vote_clear_majority() {
        let values = vec![
            ProjectedValue::Single(Some("42".into())),
            ProjectedValue::Single(Some("42".into())),
            ProjectedValue::Single(Some("42".into())),
            ProjectedValue::Single(Some("7".into())),
            ProjectedValue::Single(Some("7".into())),
        ];
        let (counts, selected) = select_most_common_or_random(&values);
        assert_eq!(counts[&ProjectedValue::Single(Some("42".into()))], 3);
        assert_eq!(counts[&ProjectedValue::Single(Some("7".into()))], 2);
        assert_eq!(values[selected], ProjectedValue::Single(Some("42".into())));
    }

    #[test]
    fn flat_vote_tie_randomness() {
        let values = vec![
            ProjectedValue::Single(Some("a".into())),
            ProjectedValue::Single(Some("a".into())),
            ProjectedValue::Single(Some("b".into())),
            ProjectedValue::Single(Some("b".into())),
        ];

        let mut saw_a = false;
        let mut saw_b = false;
        for _ in 0..100 {
            let (_, selected) = select_most_common_or_random(&values);
            match &values[selected] {
                ProjectedValue::Single(Some(s)) if s == "a" => saw_a = true,
                ProjectedValue::Single(Some(s)) if s == "b" => saw_b = true,
                _ => panic!("unexpected selection"),
            }
            if saw_a && saw_b {
                break;
            }
        }
        assert!(saw_a && saw_b, "tiebreak should produce both values");
    }

    #[test]
    fn regex_projection_boxed_answer() {
        let sc = SelfConsistency::new(
            Some(vec![r"\\boxed\{([^}]+)\}".into()]),
            None,
        )
        .unwrap();

        let response = content_response(r"The answer is \boxed{42}");
        let projected = sc.project_content(&response);
        assert_eq!(
            projected,
            ProjectedValue::Tuple(vec![Some("42".into())])
        );
    }

    #[test]
    fn regex_projection_no_match() {
        let sc = SelfConsistency::new(
            Some(vec![r"\\boxed\{([^}]+)\}".into()]),
            None,
        )
        .unwrap();

        let response = content_response("No boxed answer here");
        let projected = sc.project_content(&response);
        assert_eq!(projected, ProjectedValue::Tuple(vec![None]));
    }

    #[test]
    fn regex_projection_multiple_patterns() {
        let sc = SelfConsistency::new(
            Some(vec![
                r"Method:\s*(\w+)".into(),
                r"\\boxed\{([^}]+)\}".into(),
            ]),
            None,
        )
        .unwrap();

        let response = content_response(r"Method: algebra\nSo the answer is \boxed{42}");
        let projected = sc.project_content(&response);
        assert_eq!(
            projected,
            ProjectedValue::Tuple(vec![Some("algebra".into()), Some("42".into())])
        );
    }

    #[test]
    fn regex_projection_no_capture_group() {
        let sc = SelfConsistency::new(Some(vec![r"\d+".into()]), None).unwrap();

        let response = content_response("The answer is 42 or maybe 7");
        let projected = sc.project_content(&response);
        assert_eq!(
            projected,
            ProjectedValue::Tuple(vec![Some("42".into())])
        );
    }

    #[test]
    fn default_projection_strips_whitespace() {
        let sc = SelfConsistency::new(None, None).unwrap();
        let response = content_response("  hello world  ");
        let projected = sc.project_content(&response);
        assert_eq!(
            projected,
            ProjectedValue::Single(Some("hello world".into()))
        );
    }

    #[test]
    fn tool_name_voting() {
        let sc = SelfConsistency::new(None, Some(ToolVoteStrategy::Name)).unwrap();

        let responses = vec![
            tool_call_response("get_weather", json!({"city": "London"})),
            tool_call_response("get_weather", json!({"city": "Paris"})),
            tool_call_response("get_weather", json!({"city": "Berlin"})),
            tool_call_response("get_time", json!({"timezone": "UTC"})),
            tool_call_response("get_time", json!({"timezone": "EST"})),
        ];

        let result = sc.process_responses(responses.clone(), true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                let name = selected["tool_calls"][0]["function"]["name"]
                    .as_str()
                    .unwrap();
                assert_eq!(name, "get_weather");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[test]
    fn tool_args_voting() {
        let sc = SelfConsistency::new(
            None,
            Some(ToolVoteStrategy::Args {
                exclude: vec![],
            }),
        )
        .unwrap();

        let responses = vec![
            tool_call_response("fn", json!({"x": 1, "y": 2})),
            tool_call_response("fn", json!({"x": 1, "y": 2})),
            tool_call_response("fn", json!({"x": 1, "y": 2})),
            tool_call_response("fn", json!({"x": 3, "y": 4})),
            tool_call_response("fn", json!({"x": 5, "y": 6})),
        ];

        let result = sc.process_responses(responses, true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                let args = &selected["tool_calls"][0]["function"]["arguments"];
                assert_eq!(args["x"], 1);
                assert_eq!(args["y"], 2);
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[test]
    fn tool_args_with_exclude() {
        let sc = SelfConsistency::new(
            None,
            Some(ToolVoteStrategy::Args {
                exclude: vec!["timestamp".into()],
            }),
        )
        .unwrap();

        let r1 = tool_call_response("fn", json!({"city": "London", "timestamp": "t1"}));
        let r2 = tool_call_response("fn", json!({"city": "London", "timestamp": "t2"}));
        let r3 = tool_call_response("fn", json!({"city": "Paris", "timestamp": "t3"}));

        let pv1 = sc.extract_tool_call_features(&r1);
        let pv2 = sc.extract_tool_call_features(&r2);
        let pv3 = sc.extract_tool_call_features(&r3);

        assert_eq!(pv1, pv2, "excluded timestamp should make these equal");
        assert_ne!(pv1, pv3, "different city should differ");

        match &pv1 {
            ProjectedValue::Tuple(pairs) => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0], Some("city=\"London\"".to_string()));
            }
            _ => panic!("tool_args should return Tuple"),
        }
    }

    #[test]
    fn tool_hierarchical_voting() {
        let sc = SelfConsistency::new(
            None,
            Some(ToolVoteStrategy::Hierarchical {
                exclude: vec![],
            }),
        )
        .unwrap();

        let responses = vec![
            tool_call_response("get_weather", json!({"city": "London"})),
            tool_call_response("get_weather", json!({"city": "London"})),
            tool_call_response("get_weather", json!({"city": "Paris"})),
            tool_call_response("get_time", json!({"tz": "UTC"})),
            tool_call_response("get_time", json!({"tz": "EST"})),
        ];

        let result = sc.process_responses(responses, true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                let name = selected["tool_calls"][0]["function"]["name"]
                    .as_str()
                    .unwrap();
                assert_eq!(name, "get_weather");
                let city = selected["tool_calls"][0]["function"]["arguments"]["city"]
                    .as_str()
                    .unwrap();
                assert_eq!(city, "London");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[test]
    fn eligible_responses_majority_tool_calls() {
        let sc = SelfConsistency::new(None, Some(ToolVoteStrategy::Name)).unwrap();

        let responses = vec![
            tool_call_response("get_weather", json!({})),
            tool_call_response("get_weather", json!({})),
            tool_call_response("get_time", json!({})),
            content_response("The weather is sunny"),
            content_response("It's raining"),
        ];

        let result = sc.process_responses(responses, true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert!(has_tool_calls(&selected), "should select from tool-call responses");
                let name = selected["tool_calls"][0]["function"]["name"]
                    .as_str()
                    .unwrap();
                assert_eq!(name, "get_weather");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[test]
    fn eligible_responses_minority_tool_calls_uses_content() {
        let sc = SelfConsistency::new(None, Some(ToolVoteStrategy::Name)).unwrap();

        let responses = vec![
            tool_call_response("get_weather", json!({})),
            tool_call_response("get_time", json!({})),
            content_response("42"),
            content_response("42"),
            content_response("7"),
        ];

        let result = sc.process_responses(responses, true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert!(!has_tool_calls(&selected), "should select from content responses");
                assert_eq!(selected["content"].as_str().unwrap(), "42");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[test]
    fn no_eligible_responses_error() {
        let sc = SelfConsistency::new(None, None).unwrap();

        let responses = vec![
            tool_call_response("fn1", json!({})),
            tool_call_response("fn2", json!({})),
            tool_call_response("fn3", json!({})),
        ];

        let result = sc.process_responses(responses, true);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No eligible responses"));
    }

    #[test]
    fn empty_responses_error() {
        let sc = SelfConsistency::new(None, None).unwrap();
        let result = sc.process_responses(vec![], true);
        assert!(result.is_err());
    }

    #[test]
    fn all_identical_responses() {
        let sc = SelfConsistency::new(None, None).unwrap();

        let responses = vec![
            content_response("same answer"),
            content_response("same answer"),
            content_response("same answer"),
        ];

        let result = sc.process_responses(responses, true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert_eq!(selected["content"].as_str().unwrap(), "same answer");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[test]
    fn return_full_includes_metadata() {
        let sc = SelfConsistency::new(None, None).unwrap();

        let responses = vec![
            content_response("42"),
            content_response("42"),
            content_response("7"),
        ];

        let result = sc.process_responses(responses, false).unwrap();
        match result {
            AlgorithmOutput::Full { selected, metadata } => {
                assert_eq!(selected["content"].as_str().unwrap(), "42");
                assert_eq!(metadata["algorithm"], "self-consistency");
                assert!(metadata.get("all_responses").is_some());
                assert!(metadata.get("response_counts").is_some());
                assert!(metadata.get("selected_index").is_some());
            }
            _ => panic!("expected Full"),
        }
    }

    #[test]
    fn has_tool_calls_detection() {
        assert!(has_tool_calls(&json!({"tool_calls": [{"id": "1"}]})));
        assert!(!has_tool_calls(&json!({"tool_calls": []})));
        assert!(!has_tool_calls(&json!({"tool_calls": null})));
        assert!(!has_tool_calls(&json!({"content": "hi"})));
        assert!(!has_tool_calls(&json!({})));
    }

    #[test]
    fn canonical_json_sorts_keys() {
        let v1 = json!({"b": 2, "a": 1});
        let v2 = json!({"a": 1, "b": 2});
        assert_eq!(canonical_json(&v1), canonical_json(&v2));
    }

    #[test]
    fn canonical_json_nested() {
        let v = json!({"a": 1, "b": [2, 3], "c": {"d": 4}});
        let result = canonical_json(&v);
        assert_eq!(result, r#"{"a":1,"b":[2,3],"c":{"d":4}}"#);
    }

    #[test]
    fn tool_call_with_string_arguments() {
        let sc = SelfConsistency::new(
            None,
            Some(ToolVoteStrategy::Args { exclude: vec![] }),
        )
        .unwrap();

        let response = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{\"city\":\"London\"}"
                }
            }]
        });

        let projected = sc.extract_tool_call_features(&response);
        let expected_from_object = {
            let response2 = tool_call_response("get_weather", json!({"city": "London"}));
            sc.extract_tool_call_features(&response2)
        };
        assert_eq!(projected, expected_from_object);
    }

    #[test]
    fn hierarchical_vote_empty_list_error() {
        let result = select_hierarchical_most_common_or_random(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn hierarchical_vote_single_element_tuples_fallback() {
        let values = vec![
            ProjectedValue::Tuple(vec![Some("a".into())]),
            ProjectedValue::Tuple(vec![Some("a".into())]),
            ProjectedValue::Tuple(vec![Some("b".into())]),
        ];

        let (counts, selected) = select_hierarchical_most_common_or_random(&values).unwrap();
        assert_eq!(counts[&values[0]], 2);
        assert!(selected == 0 || selected == 1);
    }

    #[test]
    fn hierarchical_vote_narrows_by_level() {
        let values = vec![
            ProjectedValue::Tuple(vec![Some("weather".into()), Some("London".into())]),
            ProjectedValue::Tuple(vec![Some("weather".into()), Some("London".into())]),
            ProjectedValue::Tuple(vec![Some("weather".into()), Some("Paris".into())]),
            ProjectedValue::Tuple(vec![Some("time".into()), Some("UTC".into())]),
        ];

        let (_, selected) = select_hierarchical_most_common_or_random(&values).unwrap();
        let selected_val = &values[selected];
        match selected_val {
            ProjectedValue::Tuple(t) => {
                assert_eq!(t[0], Some("weather".into()));
                assert_eq!(t[1], Some("London".into()));
            }
            _ => panic!("expected Tuple"),
        }
    }

    #[test]
    fn filter_args_removes_excluded() {
        let args = json!({"city": "London", "timestamp": "t1", "request_id": "abc"});
        let exclude = vec!["timestamp".into(), "request_id".into()];
        let filtered = filter_args(&args, &exclude);
        assert_eq!(filtered, json!({"city": "London"}));
    }

    #[test]
    fn filter_args_non_object_passthrough() {
        let args = json!("just a string");
        let filtered = filter_args(&args, &["foo".into()]);
        assert_eq!(filtered, json!("just a string"));
    }

    #[test]
    fn tool_call_no_tool_calls_returns_none() {
        let sc = SelfConsistency::new(None, Some(ToolVoteStrategy::Name)).unwrap();
        let response = content_response("hello");
        let projected = sc.extract_tool_call_features(&response);
        assert_eq!(projected, ProjectedValue::Single(None));
    }

    #[test]
    fn tool_call_hierarchical_no_tool_calls_returns_none_tuple() {
        let sc = SelfConsistency::new(
            None,
            Some(ToolVoteStrategy::Hierarchical { exclude: vec![] }),
        )
        .unwrap();
        let response = content_response("hello");
        let projected = sc.extract_tool_call_features(&response);
        assert_eq!(
            projected,
            ProjectedValue::Tuple(vec![None, None])
        );
    }

    #[test]
    fn single_response_returns_it() {
        let sc = SelfConsistency::new(None, None).unwrap();
        let responses = vec![content_response("only one")];

        let result = sc.process_responses(responses, true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert_eq!(selected["content"].as_str().unwrap(), "only one");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[test]
    fn majority_threshold_exact_half() {
        let sc = SelfConsistency::new(None, Some(ToolVoteStrategy::Name)).unwrap();

        let responses = vec![
            tool_call_response("fn1", json!({})),
            tool_call_response("fn1", json!({})),
            content_response("hello"),
            content_response("world"),
        ];

        let result = sc.process_responses(responses, true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert!(
                    has_tool_calls(&selected),
                    "2/4 tool calls meets ceil(4/2)=2 threshold"
                );
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[test]
    fn invalid_regex_returns_error() {
        let result = SelfConsistency::new(Some(vec!["[invalid".into()]), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_regex_case_insensitive() {
        let sc = SelfConsistency::new(
            Some(vec![r"answer:\s*(\w+)".into()]),
            None,
        )
        .unwrap();

        let lower = content_response("answer: yes");
        let upper = content_response("ANSWER: YES");
        let mixed = content_response("Answer: Yes");

        let p_lower = sc.project_content(&lower);
        let p_upper = sc.project_content(&upper);
        let p_mixed = sc.project_content(&mixed);

        assert_eq!(p_lower, ProjectedValue::Tuple(vec![Some("yes".into())]));
        assert_eq!(p_upper, ProjectedValue::Tuple(vec![Some("YES".into())]));
        assert_eq!(p_mixed, ProjectedValue::Tuple(vec![Some("Yes".into())]));
    }

    #[test]
    fn test_regex_dotall() {
        let sc = SelfConsistency::new(
            Some(vec![r"<answer>(.*)</answer>".into()]),
            None,
        )
        .unwrap();

        let response = content_response("<answer>line1\nline2\nline3</answer>");
        let projected = sc.project_content(&response);

        assert_eq!(
            projected,
            ProjectedValue::Tuple(vec![Some("line1\nline2\nline3".into())])
        );
    }

    #[test]
    fn custom_projection_function() {
        let sc = SelfConsistency::with_custom_projection(
            Box::new(|response| {
                let content = extract_content_from_lm_response(response);
                let num: Option<String> = content
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u64>()
                    .ok()
                    .map(|n| (n % 10).to_string());
                ProjectedValue::Single(num)
            }),
            None,
            None,
        );

        let r1 = content_response("The answer is 42");
        let r2 = content_response("I think 12");
        let r3 = content_response("Result: 52");

        let p1 = sc.project_content(&r1);
        let p2 = sc.project_content(&r2);
        let p3 = sc.project_content(&r3);

        assert_eq!(p1, ProjectedValue::Single(Some("2".into())));
        assert_eq!(p2, ProjectedValue::Single(Some("2".into())));
        assert_eq!(p3, ProjectedValue::Single(Some("2".into())));
        assert_eq!(p1, p2);
    }

    #[test]
    fn custom_projection_process_responses() {
        let sc = SelfConsistency::with_custom_projection(
            Box::new(|response| {
                let content = extract_content_from_lm_response(response);
                ProjectedValue::Single(Some(content.to_uppercase()))
            }),
            None,
            None,
        );

        let responses = vec![
            content_response("hello"),
            content_response("hello"),
            content_response("world"),
        ];

        let result = sc.process_responses(responses, true).unwrap();
        match result {
            AlgorithmOutput::ResponseOnly(selected) => {
                assert_eq!(selected["content"].as_str().unwrap(), "hello");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }
}
