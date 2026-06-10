use serde_json::Value;

use crate::api::lm::AbstractLanguageModel;
use crate::api::types::{ChatMessage, Content};

/// Strips `suffix` from the end of `s` if present.
/// Mirrors Python: `s[:-len(subs)]` when `s.endswith(subs)`.
pub fn rstrip_iff_entire(s: &str, suffix: &str) -> String {
    match s.strip_suffix(suffix) {
        Some(trimmed) => trimmed.to_string(),
        None => s.to_string(),
    }
}

/// How steps are delimited during generation.
#[derive(Debug, Clone)]
pub enum StepMode {
    /// Each step ends at the given delimiter token(s).
    StepToken(StepToken),
    /// Each step is a fixed number of tokens (no delimiter).
    TokensPerStep(u32),
}

/// Step token can be a single string or a list of strings.
#[derive(Debug, Clone)]
pub enum StepToken {
    Single(String),
    Multiple(Vec<String>),
}

/// Temperature switching: after seeing `open_token` without a matching
/// `close_token` in the assistant's partial response, use a different temperature.
#[derive(Debug, Clone)]
pub struct TemperatureSwitch {
    pub temperature: f64,
    pub open_token: String,
    pub close_token: String,
}

/// Enables incremental text generation: instead of generating a full response
/// at once, it generates step-by-step, where each "step" is delimited by a
/// configurable token (e.g. "\n\n" for reasoning steps) or a fixed token count.
#[derive(Debug, Clone)]
pub struct StepGeneration {
    pub step_mode: StepMode,
    pub max_steps: u32,
    pub stop_token: Option<String>,
    pub temperature: f64,
    pub include_stop_str_in_output: bool,
    pub temperature_switch: Option<TemperatureSwitch>,
}

impl StepGeneration {
    /// Create a new StepGeneration with a step token delimiter.
    ///
    /// # Panics
    /// Panics if `include_stop_str_in_output` is false and `step_token` is `Multiple`.
    pub fn with_step_token(
        step_token: StepToken,
        max_steps: u32,
        stop_token: Option<String>,
        temperature: f64,
        include_stop_str_in_output: bool,
        temperature_switch: Option<TemperatureSwitch>,
    ) -> Self {
        if !include_stop_str_in_output {
            assert!(
                matches!(step_token, StepToken::Single(_)),
                "step_token must be a single string if include_stop_str_in_output is false"
            );
        }

        Self {
            step_mode: StepMode::StepToken(step_token),
            max_steps,
            stop_token,
            temperature,
            include_stop_str_in_output,
            temperature_switch,
        }
    }

    /// Create a new StepGeneration with a fixed token count per step.
    ///
    /// # Errors
    /// Returns `Err` if `tokens_per_step` is 0.
    pub fn with_tokens_per_step(
        tokens_per_step: u32,
        max_steps: u32,
        stop_token: Option<String>,
        temperature: f64,
        include_stop_str_in_output: bool,
        temperature_switch: Option<TemperatureSwitch>,
    ) -> Result<Self, String> {
        if tokens_per_step == 0 {
            return Err("tokens_per_step must be a positive integer".to_string());
        }

        Ok(Self {
            step_mode: StepMode::TokensPerStep(tokens_per_step),
            max_steps,
            stop_token,
            temperature,
            include_stop_str_in_output,
            temperature_switch,
        })
    }

    /// Returns the step token if using StepToken mode, None otherwise.
    pub fn step_token(&self) -> Option<&StepToken> {
        match &self.step_mode {
            StepMode::StepToken(st) => Some(st),
            StepMode::TokensPerStep(_) => None,
        }
    }

    /// Returns the tokens_per_step if using TokensPerStep mode, None otherwise.
    pub fn tokens_per_step(&self) -> Option<u32> {
        match &self.step_mode {
            StepMode::TokensPerStep(n) => Some(*n),
            StepMode::StepToken(_) => None,
        }
    }

    /// Reconstruct the full text from accumulated steps.
    ///
    /// When `include_stop_str_in_output` is true:
    ///   - Steps are concatenated directly (the LM already included delimiters).
    ///   - If `stopped` and a `stop_token` is set, strip the stop_token from the
    ///     last step using `rstrip_iff_entire`.
    ///
    /// When `include_stop_str_in_output` is false:
    ///   - If using `tokens_per_step`, simply concatenate.
    ///   - If using a single `step_token`, join steps with that token.
    ///   - If using multiple step tokens, simply concatenate.
    ///   - If not stopped and using a single step_token, append the step_token.
    pub fn post_process(&self, steps: &[String], stopped: bool) -> String {
        if self.include_stop_str_in_output {
            let mut steps = steps.to_vec();
            if stopped {
                if let Some(ref stop_tok) = self.stop_token {
                    if let Some(last) = steps.last_mut() {
                        *last = rstrip_iff_entire(last, stop_tok);
                    }
                }
            }
            steps.join("")
        } else {
            match &self.step_mode {
                StepMode::TokensPerStep(_) => steps.join(""),
                StepMode::StepToken(StepToken::Single(tok)) => {
                    let mut response = steps.join(tok);
                    if !stopped {
                        response.push_str(tok);
                    }
                    response
                }
                StepMode::StepToken(StepToken::Multiple(_)) => steps.join(""),
            }
        }
    }

    /// Get the temperature for this generation step.
    ///
    /// If `temperature_switch` is configured and the last message is an assistant
    /// message containing `open_token` but not `close_token`, returns the switch
    /// temperature. Otherwise returns the base temperature.
    pub fn get_temperature(&self, messages: &[ChatMessage]) -> f64 {
        let Some(ref switch) = self.temperature_switch else {
            return self.temperature;
        };

        if let Some(last) = messages.last() {
            if last.role == "assistant" {
                let text = last.extract_text_content();
                if text.contains(&switch.open_token)
                    && !text.contains(&switch.close_token)
                {
                    return switch.temperature;
                }
            }
        }

        self.temperature
    }

    /// Build the stop string to send to the LM for this step generation config.
    ///
    /// If a step token is set, that is used as the stop string. If not (e.g.
    /// tokens_per_step mode), falls back to the stop_token if present.
    pub fn build_stop_string(&self) -> Option<String> {
        if let Some(st) = self.step_token() {
            match st {
                StepToken::Single(tok) => return Some(tok.clone()),
                StepToken::Multiple(toks) => {
                    if let Some(first) = toks.first() {
                        return Some(first.clone());
                    }
                }
            }
        }
        if let Some(ref stop) = self.stop_token {
            return Some(stop.clone());
        }
        None
    }

    /// Check whether text contains the configured stop_token.
    pub fn contains_stop_token(&self, text: &str) -> bool {
        if let Some(ref stop) = self.stop_token {
            if text.contains(stop.as_str()) {
                return true;
            }
        }
        false
    }

    /// Generate the next step for a single prompt.
    ///
    /// Constructs the full message list by appending accumulated steps as an
    /// assistant continuation, calls the LM, and returns `(step_text, is_stopped)`.
    ///
    /// `is_stopped` is true when the generated text is empty, the stop_token
    /// appears in the output, or `steps_so_far` already reached `max_steps`.
    #[allow(clippy::too_many_arguments)]
    pub async fn forward_step(
        &self,
        client: &dyn AbstractLanguageModel,
        prompt: &str,
        steps_so_far: &[String],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<(String, bool), anyhow::Error> {
        let mut messages: Vec<ChatMessage> = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text(prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        if !steps_so_far.is_empty() {
            let partial = self.post_process(steps_so_far, false);
            messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: Some(Content::Text(partial)),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        let temp = temperature.unwrap_or(self.temperature);
        let stop_str = self.build_stop_string();

        let response = client
            .agenerate_single(
                &messages,
                stop_str.as_deref(),
                max_tokens,
                Some(temp),
                None,
                tools,
                tool_choice,
            )
            .await
            .map_err(|e| anyhow::anyhow!("LM generation failed: {}", e))?;

        let content = response
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let is_stopped = content.is_empty() || self.contains_stop_token(&content);
        Ok((content, is_stopped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ChatMessage, Content};

    fn s(val: &str) -> String {
        val.to_string()
    }

    fn steps(vals: &[&str]) -> Vec<String> {
        vals.iter().map(|v| s(v)).collect()
    }

    #[test]
    fn test_initialization_with_step_token() {
        let cases: Vec<(&str, u32, Option<&str>, f64, bool)> = vec![
            ("\n", 5, None, 0.8, false),
            ("\n", 3, Some("END"), 0.5, true),
            (">>", 10, Some("STOP"), 1.0, false),
        ];

        for (step_tok, max_steps, stop_tok, temp, include_stop) in cases {
            let sg = StepGeneration::with_step_token(
                StepToken::Single(s(step_tok)),
                max_steps,
                stop_tok.map(|t| s(t)),
                temp,
                include_stop,
                None,
            );

            match &sg.step_mode {
                StepMode::StepToken(StepToken::Single(tok)) => assert_eq!(tok, step_tok),
                _ => panic!("expected StepToken::Single"),
            }
            assert_eq!(sg.max_steps, max_steps);
            assert_eq!(sg.stop_token.as_deref(), stop_tok);
            assert_eq!(sg.temperature, temp);
            assert_eq!(sg.include_stop_str_in_output, include_stop);
        }
    }

    #[test]
    fn test_initialization_with_tokens_per_step() {
        let sg1 = StepGeneration::with_tokens_per_step(50, 5, None, 0.8, false, None).unwrap();
        assert!(sg1.step_token().is_none());
        assert_eq!(sg1.tokens_per_step(), Some(50));

        let sg2 = StepGeneration::with_tokens_per_step(100, 5, None, 0.8, true, None).unwrap();
        assert!(sg2.step_token().is_none());
        assert_eq!(sg2.tokens_per_step(), Some(100));
    }

    #[test]
    fn test_initialization_with_tokens_per_step_zero() {
        let err = StepGeneration::with_tokens_per_step(0, 5, None, 0.8, false, None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err(), "tokens_per_step must be a positive integer");
    }

    #[test]
    #[should_panic(expected = "step_token must be a single string")]
    fn test_initialization_validation_multiple_step_token() {
        StepGeneration::with_step_token(
            StepToken::Multiple(vec![s("token1"), s("token2")]),
            5,
            None,
            0.8,
            false,
            None,
        );
    }

    #[test]
    fn test_temperature_switching() {
        let sg = StepGeneration::with_step_token(
            StepToken::Single(s("\n")),
            5,
            None,
            0.8,
            false,
            Some(TemperatureSwitch {
                temperature: 0.2,
                open_token: s("<think>"),
                close_token: s("</think>"),
            }),
        );

        let no_messages: Vec<ChatMessage> = vec![];
        assert_eq!(sg.get_temperature(&no_messages), 0.8);

        let user_only = vec![ChatMessage {
            role: s("user"),
            content: Some(Content::Text(s("hello"))),
            tool_calls: None,
            tool_call_id: None,
        }];
        assert_eq!(sg.get_temperature(&user_only), 0.8);

        let with_open_token = vec![
            ChatMessage {
                role: s("user"),
                content: Some(Content::Text(s("hello"))),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: s("assistant"),
                content: Some(Content::Text(s("Let me think <think> about this"))),
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        assert_eq!(sg.get_temperature(&with_open_token), 0.2);

        let with_both_tokens = vec![
            ChatMessage {
                role: s("user"),
                content: Some(Content::Text(s("hello"))),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: s("assistant"),
                content: Some(Content::Text(
                    s("Let me think <think> about this </think> done"),
                )),
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        assert_eq!(sg.get_temperature(&with_both_tokens), 0.8);

        let sg_no_switch = StepGeneration::with_step_token(
            StepToken::Single(s("\n")),
            5,
            None,
            0.8,
            false,
            None,
        );
        assert_eq!(sg_no_switch.get_temperature(&with_open_token), 0.8);
    }

    #[test]
    fn test_post_process_basic() {
        let sg = StepGeneration::with_step_token(
            StepToken::Single(s("\n")),
            5,
            None,
            0.8,
            false,
            None,
        );

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3"]), false),
            "step1\nstep2\nstep3\n"
        );

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3"]), true),
            "step1\nstep2\nstep3"
        );
    }

    #[test]
    fn test_post_process_with_stop_token() {
        let sg = StepGeneration::with_step_token(
            StepToken::Single(s("\n")),
            5,
            Some(s("END")),
            0.8,
            true,
            None,
        );

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3END"]), true),
            "step1step2step3"
        );

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3END"]), false),
            "step1step2step3END"
        );

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3"]), true),
            "step1step2step3"
        );
    }

    #[test]
    fn test_post_process_strips_trailing_step_token() {
        let sg = StepGeneration::with_step_token(
            StepToken::Single(s("\n")),
            5,
            None,
            0.8,
            false,
            None,
        );

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3"]), false),
            "step1\nstep2\nstep3\n"
        );

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3"]), true),
            "step1\nstep2\nstep3"
        );
    }

    #[test]
    fn test_post_process_preserves_content_matching_step_token() {
        let sg = StepGeneration::with_step_token(
            StepToken::Single(s("\n")),
            5,
            None,
            0.8,
            true,
            None,
        );

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3"]), false),
            "step1step2step3"
        );
    }

    #[test]
    fn test_post_process_with_tokens_per_step() {
        let sg = StepGeneration::with_tokens_per_step(50, 5, None, 0.8, false, None).unwrap();

        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3"]), false),
            "step1step2step3"
        );
        assert_eq!(
            sg.post_process(&steps(&["step1", "step2", "step3"]), true),
            "step1step2step3"
        );

        let sg_include =
            StepGeneration::with_tokens_per_step(50, 5, None, 0.8, true, None).unwrap();

        assert_eq!(
            sg_include.post_process(&steps(&["step1", "step2", "step3"]), false),
            "step1step2step3"
        );
        assert_eq!(
            sg_include.post_process(&steps(&["step1", "step2", "step3"]), true),
            "step1step2step3"
        );
    }

    #[test]
    fn test_rstrip_iff_entire() {
        assert_eq!(rstrip_iff_entire("hello world", "world"), "hello ");
        assert_eq!(rstrip_iff_entire("hello", "world"), "hello");
        assert_eq!(rstrip_iff_entire("world", "world"), "");
        assert_eq!(rstrip_iff_entire("", "world"), "");
        assert_eq!(rstrip_iff_entire("hello worldworld", "world"), "hello world");
        assert_eq!(rstrip_iff_entire("step3END", "END"), "step3");
        assert_eq!(rstrip_iff_entire("END", "END"), "");
    }
}
