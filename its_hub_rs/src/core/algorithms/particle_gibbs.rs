use std::sync::Arc;

use async_trait::async_trait;
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use serde_json::Value;

use crate::api::{AlgorithmOutput, ProcessRewardModel, ScalingAlgorithm};
use crate::core::lms::step_generation::StepGeneration;
use crate::core::lms::LmClient;
use crate::api::types::ChatMessage;

/// Convert a slice of ChatMessage into a single prompt string for step-generation.
fn messages_to_prompt(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.extract_text_content()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionMethod {
    Sample,
    Argmax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResamplingMethod {
    Systematic,
    Multinomial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemperatureMethod {
    Ess,
    Entropy,
    Base,
}

#[derive(Debug, Clone)]
pub struct Particle {
    pub steps: Vec<String>,
    pub is_stopped: bool,
    pub partial_log_weights: Vec<f64>,
}

impl Particle {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            is_stopped: false,
            partial_log_weights: Vec::new(),
        }
    }

    pub fn log_weight(&self) -> f64 {
        self.partial_log_weights.last().copied().unwrap_or(0.0)
    }

    pub fn deepcopy(&self) -> Self {
        Self {
            steps: self.steps.clone(),
            is_stopped: self.is_stopped,
            partial_log_weights: self.partial_log_weights.clone(),
        }
    }
}

impl Default for Particle {
    fn default() -> Self {
        Self::new()
    }
}

pub fn softmax(logits: &[f64]) -> Vec<f64> {
    if logits.is_empty() {
        return Vec::new();
    }
    let max_val = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = logits.iter().map(|&x| (x - max_val).exp()).collect();
    let sum: f64 = exps.iter().sum();
    exps.iter().map(|&e| e / sum).collect()
}

pub fn inv_sigmoid(x: f64) -> f64 {
    assert!((0.0..=1.0).contains(&x), "x must be between 0 and 1");
    let clamped = x.clamp(1e-7, 1.0 - 1e-7);
    (clamped / (1.0 - clamped)).ln()
}

pub fn entropy_n(probabilities: &[f64]) -> f64 {
    let p: Vec<f64> = probabilities
        .iter()
        .map(|&v| v.clamp(1e-7, 1.0 - 1e-7))
        .collect();
    let entropy: f64 = p.iter().map(|&pi| -pi * pi.ln()).sum();
    let n = p.len();
    if n > 2 && (n as f64).ln() > 0.0 {
        entropy / (n as f64).ln()
    } else {
        entropy
    }
}

pub fn effective_sample_size(probabilities: &[f64]) -> f64 {
    let p: Vec<f64> = probabilities
        .iter()
        .map(|&v| v.clamp(1e-7, 1.0 - 1e-7))
        .collect();
    let sum_sq: f64 = p.iter().map(|&pi| pi * pi).sum();
    1.0 / sum_sq
}

pub fn systematic_resampling(probabilities: &[f64], n: usize) -> Vec<usize> {
    let mut rng = thread_rng();
    let u: f64 = rng.gen();
    let positions: Vec<f64> = (0..n).map(|i| (i as f64 + u) / n as f64).collect();

    let cumsum: Vec<f64> = probabilities
        .iter()
        .scan(0.0, |acc, &w| {
            *acc += w;
            Some(*acc)
        })
        .collect();

    let mut indices = vec![0usize; n];
    let mut j = 0;
    for i in 0..n {
        while j < cumsum.len() - 1 && positions[i] >= cumsum[j] {
            j += 1;
        }
        indices[i] = j;
    }
    indices
}

pub fn multinomial_resampling(probabilities: &[f64], n: usize) -> Vec<usize> {
    let mut rng = thread_rng();
    let dist = WeightedIndex::new(probabilities).expect("invalid weights for multinomial resampling");
    (0..n).map(|_| dist.sample(&mut rng)).collect()
}

pub struct ParticleGibbs {
    sg: StepGeneration,
    prm: Arc<dyn ProcessRewardModel>,
    num_iterations: usize,
    final_response_selection: SelectionMethod,
    num_ref_particles: usize,
    does_entropic_annealing: bool,
    ess_threshold: f64,
    early_phase: f64,
    resampling_method: ResamplingMethod,
    temperature_method: TemperatureMethod,
}

impl ParticleGibbs {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sg: StepGeneration,
        prm: Arc<dyn ProcessRewardModel>,
        num_iterations: usize,
        final_response_selection: SelectionMethod,
        num_ref_particles: usize,
        does_entropic_annealing: bool,
        ess_threshold: f64,
        early_phase: f64,
        resampling_method: ResamplingMethod,
        temperature_method: TemperatureMethod,
    ) -> Self {
        Self {
            sg,
            prm,
            num_iterations,
            final_response_selection,
            num_ref_particles,
            does_entropic_annealing,
            ess_threshold,
            early_phase,
            resampling_method,
            temperature_method,
        }
    }

    fn max_steps(&self) -> usize {
        self.sg.max_steps as usize
    }

    #[allow(clippy::too_many_arguments)]
    async fn propagate(
        &self,
        client: &LmClient,
        particles: &mut [Particle],
        prompt: &str,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<(), anyhow::Error> {
        let was_stopped: Vec<bool> = particles.iter().map(|p| p.is_stopped).collect();

        let mut active_indices: Vec<usize> = Vec::new();
        for (i, &stopped) in was_stopped.iter().enumerate() {
            if !stopped {
                active_indices.push(i);
            }
        }

        let mut sg_results: Vec<(String, bool)> = Vec::with_capacity(active_indices.len());
        let max_steps = self.max_steps();

        for &idx in &active_indices {
            let result = self
                .sg
                .forward_step(
                    client,
                    prompt,
                    &particles[idx].steps,
                    temperature,
                    max_tokens,
                    tools,
                    tool_choice,
                )
                .await?;
            sg_results.push(result);
        }

        for (j, &idx) in active_indices.iter().enumerate() {
            let (next_step, is_stopped) = &sg_results[j];
            particles[idx].steps.push(next_step.clone());
            particles[idx].is_stopped = *is_stopped || particles[idx].steps.len() >= max_steps;
        }

        let prompt_messages = &[ChatMessage {
            role: "user".to_string(),
            content: Some(crate::api::types::Content::Text(prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];
        for &idx in active_indices.iter() {
            let processed = self.sg.post_process(&particles[idx].steps, true);
            let steps = vec![processed];
            let scores = self.prm.score(prompt_messages, &steps).await?;
            let score = scores.last().copied().unwrap_or(0.5);
            particles[idx].partial_log_weights.push(inv_sigmoid(score));
        }

        Ok(())
    }

    fn temperature_base(&self, value_max: f64, progress: f64) -> f64 {
        let value = value_max - progress;
        value.max(1.0)
    }

    fn temperature_entropy(&self, entropy_n_val: f64, progress: f64) -> f64 {
        let beta = entropy_n_val + (1.0 - entropy_n_val) * progress;
        let value = 1.0 / beta;
        value.max(1.0)
    }

    fn temperature_ess(&self, ess_ratio: f64, progress: f64) -> f64 {
        if ess_ratio <= 0.0 {
            return 1.0;
        }
        let value = 1.0 / ess_ratio * (1.0 - progress);
        value.max(1.0)
    }

    fn temperature_annealing(
        &self,
        probabilities: &[f64],
        current_step: usize,
        num_particles: usize,
        value_max: f64,
    ) -> f64 {
        if num_particles <= 1 {
            return 1.0;
        }

        let progress = current_step as f64 / self.max_steps() as f64;
        let entropy_n_val = entropy_n(probabilities);
        let ess = effective_sample_size(probabilities);
        let ess_ratio = ess / num_particles as f64;

        let mut temperature = 1.0;
        if ess_ratio < self.ess_threshold && progress < self.early_phase {
            temperature = match self.temperature_method {
                TemperatureMethod::Ess => self.temperature_ess(ess_ratio, progress),
                TemperatureMethod::Entropy => self.temperature_entropy(entropy_n_val, progress),
                TemperatureMethod::Base => self.temperature_base(value_max, progress),
            };
        }
        temperature
    }

    fn resampling_systematic(&self, particles: &[Particle], probabilities: &[f64], n: usize) -> Vec<Particle> {
        let indices = systematic_resampling(probabilities, n);
        indices.iter().map(|&i| particles[i].clone()).collect()
    }

    fn resampling_multinomial(&self, particles: &[Particle], probabilities: &[f64], n: usize) -> Vec<Particle> {
        let indices = multinomial_resampling(probabilities, n);
        indices.iter().map(|&i| particles[i].clone()).collect()
    }

    fn resample(&self, particles: &[Particle], probabilities: &[f64], n: usize) -> Vec<Particle> {
        match self.resampling_method {
            ResamplingMethod::Systematic => self.resampling_systematic(particles, probabilities, n),
            ResamplingMethod::Multinomial => self.resampling_multinomial(particles, probabilities, n),
        }
    }
}

#[async_trait]
impl ScalingAlgorithm for ParticleGibbs {
    async fn infer(
        &self,
        client: &LmClient,
        messages: &[ChatMessage],
        budget: u32,
        return_response_only: bool,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error> {
        let budget = budget as usize;
        if !budget.is_multiple_of(self.num_iterations) {
            anyhow::bail!("budget must be divisible by num_iterations");
        }

        let num_particles = budget / self.num_iterations;
        let prompt = messages_to_prompt(messages);

        let mut ref_particles: Vec<Particle> = Vec::new();
        let mut responses_lst: Vec<Vec<Value>> = Vec::new();
        let mut log_weights_lst: Vec<Vec<f64>> = Vec::new();
        let mut ref_indices_lst: Vec<Vec<usize>> = Vec::new();
        let mut steps_used_lst: Vec<Vec<usize>> = Vec::new();

        for _ in 0..self.num_iterations {
            let num_free = num_particles - ref_particles.len();
            let mut particles: Vec<Particle> = (0..num_free).map(|_| Particle::new()).collect();
            particles.extend(ref_particles.iter().map(|p| p.deepcopy()));

            let mut current_step: usize = 0;

            while !particles.iter().all(|p| p.is_stopped) {
                self.propagate(client, &mut particles, &prompt, temperature, max_tokens, tools, tool_choice)
                    .await?;

                current_step += 1;

                let log_weights: Vec<f64> = particles
                    .iter()
                    .map(|p| {
                        if p.is_stopped {
                            p.log_weight()
                        } else if current_step > 0 && current_step <= p.partial_log_weights.len() {
                            p.partial_log_weights[current_step - 1]
                        } else {
                            p.log_weight()
                        }
                    })
                    .collect();

                let mut probabilities = softmax(&log_weights);

                if self.does_entropic_annealing {
                    let temp = self.temperature_annealing(&probabilities, current_step, num_free, 2.0);
                    let tempered: Vec<f64> = log_weights.iter().map(|&w| w / temp).collect();
                    probabilities = softmax(&tempered);
                }

                let mut resampled = self.resample(&particles, &probabilities, num_free);
                resampled = resampled.iter().map(|p| p.deepcopy()).collect();

                for p in &mut resampled {
                    if p.steps.len() > current_step {
                        p.steps.truncate(current_step);
                        p.partial_log_weights.truncate(current_step);
                        p.is_stopped = false;
                    }
                }

                particles = resampled;
                for rp in &ref_particles {
                    particles.push(rp.deepcopy());
                }
            }

            let final_log_weights: Vec<f64> = particles.iter().map(|p| p.log_weight()).collect();
            let final_probs = softmax(&final_log_weights);

            let mut rng = thread_rng();
            let ref_dist = WeightedIndex::new(&final_probs).unwrap_or_else(|_| {
                WeightedIndex::new(vec![1.0; particles.len()]).unwrap()
            });
            let ref_indices: Vec<usize> = (0..self.num_ref_particles)
                .map(|_| ref_dist.sample(&mut rng))
                .collect();
            ref_particles = ref_indices.iter().map(|&i| particles[i].deepcopy()).collect();

            let responses: Vec<Value> = particles
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "role": "assistant",
                        "content": self.sg.post_process(&p.steps, true),
                    })
                })
                .collect();

            responses_lst.push(responses);
            log_weights_lst.push(final_log_weights.clone());
            ref_indices_lst.push(ref_indices);
            steps_used_lst.push(particles.iter().map(|p| p.steps.len()).collect());
        }

        let last_log_weights = log_weights_lst.last().unwrap();
        let last_probs = softmax(last_log_weights);
        let last_responses = responses_lst.last().unwrap();

        let selected_index = match self.final_response_selection {
            SelectionMethod::Sample => {
                let mut rng = thread_rng();
                let dist = WeightedIndex::new(&last_probs).unwrap_or_else(|_| {
                    WeightedIndex::new(vec![1.0; last_responses.len()]).unwrap()
                });
                dist.sample(&mut rng)
            }
            SelectionMethod::Argmax => last_log_weights
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(0),
        };

        if return_response_only {
            Ok(AlgorithmOutput::ResponseOnly(
                last_responses[selected_index].clone(),
            ))
        } else {
            let metadata = serde_json::json!({
                "algorithm": "particle-gibbs",
                "responses_lst": responses_lst,
                "log_weights_lst": log_weights_lst,
                "ref_indices_lst": ref_indices_lst,
                "selected_index": selected_index,
                "steps_used_lst": steps_used_lst,
            });
            Ok(AlgorithmOutput::Full {
                selected: last_responses[selected_index].clone(),
                metadata,
            })
        }
    }
}

pub struct ParticleFiltering {
    inner: ParticleGibbs,
}

impl ParticleFiltering {
    pub fn new(
        sg: StepGeneration,
        prm: Arc<dyn ProcessRewardModel>,
        final_response_selection: SelectionMethod,
        resampling_method: ResamplingMethod,
    ) -> Self {
        Self {
            inner: ParticleGibbs::new(
                sg,
                prm,
                1,
                final_response_selection,
                0,
                false,
                0.5,
                0.5,
                resampling_method,
                TemperatureMethod::Ess,
            ),
        }
    }
}

#[async_trait]
impl ScalingAlgorithm for ParticleFiltering {
    async fn infer(
        &self,
        client: &LmClient,
        messages: &[ChatMessage],
        budget: u32,
        return_response_only: bool,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error> {
        let result = self
            .inner
            .infer(client, messages, budget, false, temperature, max_tokens, tools, tool_choice)
            .await?;

        match result {
            AlgorithmOutput::Full { metadata, .. } => {
                let responses = metadata["responses_lst"][0].as_array().unwrap().clone();
                let log_weights: Vec<f64> = metadata["log_weights_lst"][0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect();
                let selected_index = metadata["selected_index"].as_u64().unwrap() as usize;
                let steps_used: Vec<u64> = metadata["steps_used_lst"][0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect();

                if return_response_only {
                    Ok(AlgorithmOutput::ResponseOnly(responses[selected_index].clone()))
                } else {
                    let flat_metadata = serde_json::json!({
                        "algorithm": "particle-filtering",
                        "responses": responses,
                        "log_weights": log_weights,
                        "selected_index": selected_index,
                        "steps_used": steps_used,
                    });
                    Ok(AlgorithmOutput::Full {
                        selected: responses[selected_index].clone(),
                        metadata: flat_metadata,
                    })
                }
            }
            other => Ok(other),
        }
    }
}

pub struct EntropicParticleFiltering {
    inner: ParticleGibbs,
}

impl EntropicParticleFiltering {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sg: StepGeneration,
        prm: Arc<dyn ProcessRewardModel>,
        final_response_selection: SelectionMethod,
        resampling_method: ResamplingMethod,
        temperature_method: TemperatureMethod,
        ess_threshold: f64,
        early_phase: f64,
    ) -> Self {
        Self {
            inner: ParticleGibbs::new(
                sg,
                prm,
                1,
                final_response_selection,
                0,
                true,
                ess_threshold,
                early_phase,
                resampling_method,
                temperature_method,
            ),
        }
    }
}

#[async_trait]
impl ScalingAlgorithm for EntropicParticleFiltering {
    async fn infer(
        &self,
        client: &LmClient,
        messages: &[ChatMessage],
        budget: u32,
        return_response_only: bool,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error> {
        let result = self
            .inner
            .infer(client, messages, budget, false, temperature, max_tokens, tools, tool_choice)
            .await?;

        match result {
            AlgorithmOutput::Full { metadata, .. } => {
                let responses = metadata["responses_lst"][0].as_array().unwrap().clone();
                let log_weights: Vec<f64> = metadata["log_weights_lst"][0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect();
                let selected_index = metadata["selected_index"].as_u64().unwrap() as usize;
                let steps_used: Vec<u64> = metadata["steps_used_lst"][0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect();

                if return_response_only {
                    Ok(AlgorithmOutput::ResponseOnly(responses[selected_index].clone()))
                } else {
                    let flat_metadata = serde_json::json!({
                        "algorithm": "entropic-particle-filtering",
                        "responses": responses,
                        "log_weights": log_weights,
                        "selected_index": selected_index,
                        "steps_used": steps_used,
                    });
                    Ok(AlgorithmOutput::Full {
                        selected: responses[selected_index].clone(),
                        metadata: flat_metadata,
                    })
                }
            }
            other => Ok(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockPRM {
        scores: Vec<f64>,
        call_count: AtomicUsize,
    }

    impl MockPRM {
        fn new(scores: Vec<f64>) -> Self {
            Self {
                scores,
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl ProcessRewardModel for MockPRM {
        async fn score(
            &self,
            _prompt_messages: &[ChatMessage],
            _steps: &[String],
        ) -> Result<Vec<f64>, anyhow::Error> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let score = self.scores[idx % self.scores.len()];
            Ok(vec![score])
        }
    }

    fn make_client(server_uri: &str) -> LmClient {
        LmClient::new(
            &format!("{}/v1", server_uri),
            Some("test-key"),
            "test-model",
            8,
            3,
            None,
            None,
        )
        .unwrap()
    }

    fn user_message(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: Some(crate::api::types::Content::Text(text.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn system_message(text: &str) -> ChatMessage {
        ChatMessage {
            role: "system".to_string(),
            content: Some(crate::api::types::Content::Text(text.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn mock_chat_response(content: &str) -> serde_json::Value {
        serde_json::json!({
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

    #[test]
    fn test_particle_init_and_clone() {
        let p = Particle {
            steps: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            is_stopped: false,
            partial_log_weights: vec![0.3, 0.6, 1.0],
        };
        let copy = p.deepcopy();

        assert_eq!(copy.steps, vec!["a", "b", "c"]);
        assert!(!copy.is_stopped);
        assert!((copy.log_weight() - 1.0).abs() < f64::EPSILON);
        assert_eq!(copy.partial_log_weights, vec![0.3, 0.6, 1.0]);

        let mut p2 = p.deepcopy();
        p2.steps.push("d".to_string());
        p2.partial_log_weights.push(1.2);
        assert_eq!(p.steps.len(), 3);
        assert_eq!(p.partial_log_weights.len(), 3);
    }

    #[test]
    fn test_particle_default_log_weight() {
        let p = Particle::new();
        assert!((p.log_weight() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_softmax_basic() {
        let logits = vec![1.0, 2.0, 3.0];
        let probs = softmax(&logits);
        assert_eq!(probs.len(), 3);
        let sum: f64 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
        assert!(probs[2] > probs[1]);
        assert!(probs[1] > probs[0]);
    }

    #[test]
    fn test_softmax_numerical_stability() {
        let logits = vec![1000.0, 1001.0, 1002.0];
        let probs = softmax(&logits);
        let sum: f64 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
        assert!(probs.iter().all(|&p| p.is_finite()));

        let logits_neg = vec![-1000.0, -1001.0, -1002.0];
        let probs_neg = softmax(&logits_neg);
        let sum_neg: f64 = probs_neg.iter().sum();
        assert!((sum_neg - 1.0).abs() < 1e-10);
        assert!(probs_neg.iter().all(|&p| p.is_finite()));
    }

    #[test]
    fn test_softmax_equal_logits() {
        let logits = vec![0.0, 0.0, 0.0, 0.0];
        let probs = softmax(&logits);
        for p in &probs {
            assert!((*p - 0.25).abs() < 1e-10);
        }
    }

    #[test]
    fn test_softmax_empty() {
        let probs = softmax(&[]);
        assert!(probs.is_empty());
    }

    #[test]
    fn test_inv_sigmoid() {
        let v = inv_sigmoid(0.5);
        assert!(v.abs() < 1e-10);

        let v2 = inv_sigmoid(0.7);
        assert!(v2 > 0.0);

        let v3 = inv_sigmoid(0.3);
        assert!(v3 < 0.0);
    }

    #[test]
    fn test_inv_sigmoid_boundary() {
        let v0 = inv_sigmoid(0.0);
        assert!(v0.is_finite());
        let v1 = inv_sigmoid(1.0);
        assert!(v1.is_finite());
    }

    #[test]
    #[should_panic(expected = "x must be between 0 and 1")]
    fn test_inv_sigmoid_out_of_range_negative() {
        inv_sigmoid(-0.1);
    }

    #[test]
    #[should_panic(expected = "x must be between 0 and 1")]
    fn test_inv_sigmoid_out_of_range_above() {
        inv_sigmoid(1.1);
    }

    #[test]
    fn test_entropy_normalized() {
        let uniform = vec![0.25, 0.25, 0.25, 0.25];
        let e = entropy_n(&uniform);
        assert!((e - 1.0).abs() < 0.01);

        let peaked = vec![0.9, 0.05, 0.025, 0.025];
        let e_peaked = entropy_n(&peaked);
        assert!(e_peaked < 1.0);
        assert!(e_peaked > 0.0);
    }

    #[test]
    fn test_effective_sample_size() {
        let uniform = vec![0.2, 0.2, 0.2, 0.2, 0.2];
        let ess = effective_sample_size(&uniform);
        let expected = 1.0 / (5.0 * 0.04);
        assert!((ess - expected).abs() < 1e-10);
        assert!((ess - 5.0).abs() < 1e-10);

        let non_uniform = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let ess2 = effective_sample_size(&non_uniform);
        let expected2 = 1.0 / (0.01 + 0.04 + 0.09 + 0.16 + 0.25);
        assert!((ess2 - expected2).abs() < 1e-10);
    }

    #[test]
    fn test_systematic_resampling() {
        let weights = vec![0.1, 0.2, 0.3, 0.4];
        let indices = systematic_resampling(&weights, 100);
        assert_eq!(indices.len(), 100);
        assert!(indices.iter().all(|&i| i < 4));
    }

    #[test]
    fn test_multinomial_resampling() {
        let weights = vec![0.1, 0.2, 0.3, 0.4];
        let indices = multinomial_resampling(&weights, 100);
        assert_eq!(indices.len(), 100);
        assert!(indices.iter().all(|&i| i < 4));
    }

    #[test]
    fn test_temperature_ess() {
        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            3,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.5]));
        let pg = ParticleGibbs::new(
            sg,
            prm,
            1,
            SelectionMethod::Argmax,
            0,
            true,
            0.5,
            0.5,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Ess,
        );

        let t = pg.temperature_ess(0.2, 0.2);
        assert!((t - 4.0).abs() < 1e-10);

        let t2 = pg.temperature_ess(0.5, 0.8);
        assert!((t2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_temperature_entropy() {
        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            3,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.5]));
        let pg = ParticleGibbs::new(
            sg,
            prm,
            1,
            SelectionMethod::Argmax,
            0,
            true,
            0.5,
            0.5,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Entropy,
        );

        let t = pg.temperature_entropy(0.5, 0.3);
        let expected = 1.0 / (0.5 + 0.5 * 0.3);
        assert!((t - expected).abs() < 1e-10);

        let t2 = pg.temperature_entropy(1.0, 0.2);
        assert!((t2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_temperature_base() {
        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            3,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.5]));
        let pg = ParticleGibbs::new(
            sg,
            prm,
            1,
            SelectionMethod::Argmax,
            0,
            true,
            0.5,
            0.5,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Base,
        );

        let t = pg.temperature_base(2.0, 0.5);
        assert!((t - 1.5).abs() < 1e-10);

        let t2 = pg.temperature_base(0.8, 0.5);
        assert!((t2 - 1.0).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_particle_gibbs_budget_validation() {
        use wiremock::MockServer;

        let server = MockServer::start().await;
        let client = make_client(&server.uri());

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.5]));
        let pg = ParticleGibbs::new(
            sg,
            prm,
            3,
            SelectionMethod::Argmax,
            0,
            false,
            0.5,
            0.5,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Ess,
        );

        let messages = vec![user_message("test")];
        let result = pg
            .infer(&client, &messages, 4, true, None, None, None, None)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("budget must be divisible by num_iterations"));
    }

    #[tokio::test]
    async fn test_particle_gibbs_basic() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6]));
        let pg = ParticleGibbs::new(
            sg,
            prm,
            1,
            SelectionMethod::Argmax,
            0,
            false,
            0.5,
            0.5,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Ess,
        );

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = pg
            .infer(&client, &messages, 2, true, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::ResponseOnly(val) => {
                assert!(val.get("content").is_some());
                assert_eq!(val["role"], "assistant");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn test_particle_gibbs_full_output() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6]));
        let pg = ParticleGibbs::new(
            sg,
            prm,
            1,
            SelectionMethod::Argmax,
            0,
            false,
            0.5,
            0.5,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Ess,
        );

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = pg
            .infer(&client, &messages, 2, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { selected, metadata } => {
                assert_eq!(selected["role"], "assistant");
                assert_eq!(metadata["algorithm"], "particle-gibbs");
                assert!(metadata.get("responses_lst").is_some());
                assert!(metadata.get("log_weights_lst").is_some());
                assert!(metadata.get("ref_indices_lst").is_some());
                assert!(metadata.get("selected_index").is_some());
                assert!(metadata.get("steps_used_lst").is_some());
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn test_particle_gibbs_multiple_iterations() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6, 0.8, 0.5]));
        let pg = ParticleGibbs::new(
            sg,
            prm,
            2,
            SelectionMethod::Argmax,
            1,
            false,
            0.5,
            0.5,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Ess,
        );

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = pg
            .infer(&client, &messages, 4, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { metadata, .. } => {
                let rl = metadata["responses_lst"].as_array().unwrap();
                assert_eq!(rl.len(), 2);
                let lwl = metadata["log_weights_lst"].as_array().unwrap();
                assert_eq!(lwl.len(), 2);
                let ril = metadata["ref_indices_lst"].as_array().unwrap();
                assert_eq!(ril.len(), 2);
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn test_particle_gibbs_with_conversation() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.8, 0.5]));
        let pg = ParticleGibbs::new(
            sg,
            prm,
            1,
            SelectionMethod::Argmax,
            0,
            false,
            0.5,
            0.5,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Ess,
        );

        let client = make_client(&server.uri());
        let messages = vec![
            system_message("Solve step by step"),
            user_message("Problem:"),
        ];

        let result = pg
            .infer(&client, &messages, 2, true, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::ResponseOnly(val) => {
                assert_eq!(val["role"], "assistant");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn test_particle_filtering_single_iteration() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6]));
        let pf = ParticleFiltering::new(sg, prm, SelectionMethod::Argmax, ResamplingMethod::Multinomial);

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = pf
            .infer(&client, &messages, 2, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { selected, metadata } => {
                assert_eq!(selected["role"], "assistant");
                assert_eq!(metadata["algorithm"], "particle-filtering");
                let responses = metadata["responses"].as_array().unwrap();
                assert_eq!(responses.len(), 2);
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn test_particle_filtering_response_only() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6]));
        let pf = ParticleFiltering::new(sg, prm, SelectionMethod::Argmax, ResamplingMethod::Multinomial);

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = pf
            .infer(&client, &messages, 2, true, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::ResponseOnly(val) => {
                assert_eq!(val["role"], "assistant");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn test_entropic_particle_filtering() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6]));
        let epf = EntropicParticleFiltering::new(
            sg,
            prm,
            SelectionMethod::Argmax,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Ess,
            0.5,
            0.5,
        );

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = epf
            .infer(&client, &messages, 2, true, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::ResponseOnly(val) => {
                assert_eq!(val["role"], "assistant");
            }
            _ => panic!("expected ResponseOnly"),
        }
    }

    #[tokio::test]
    async fn test_entropic_particle_filtering_full_output() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            1,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6]));
        let epf = EntropicParticleFiltering::new(
            sg,
            prm,
            SelectionMethod::Argmax,
            ResamplingMethod::Systematic,
            TemperatureMethod::Entropy,
            0.5,
            0.5,
        );

        let client = make_client(&server.uri());
        let messages = vec![user_message("Solve this:")];

        let result = epf
            .infer(&client, &messages, 4, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { selected, metadata } => {
                assert_eq!(selected["role"], "assistant");
                assert_eq!(metadata["algorithm"], "entropic-particle-filtering");
                let responses = metadata["responses"].as_array().unwrap();
                assert_eq!(responses.len(), 4);
                let log_weights = metadata["log_weights"].as_array().unwrap();
                assert_eq!(log_weights.len(), 4);
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn test_entropic_with_base_temperature() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            3,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6, 0.5, 0.8]));
        let epf = EntropicParticleFiltering::new(
            sg,
            prm,
            SelectionMethod::Argmax,
            ResamplingMethod::Multinomial,
            TemperatureMethod::Base,
            0.5,
            0.5,
        );

        let client = make_client(&server.uri());
        let messages = vec![user_message("Test prompt")];

        let result = epf
            .infer(&client, &messages, 4, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { metadata, .. } => {
                let responses = metadata["responses"].as_array().unwrap();
                assert_eq!(responses.len(), 4);
            }
            _ => panic!("expected Full"),
        }
    }

    #[tokio::test]
    async fn test_entropic_with_systematic_resampling() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_chat_response("step1")),
            )
            .mount(&server)
            .await;

        let sg = StepGeneration::with_step_token(
            crate::core::lms::step_generation::StepToken::Single("\n".to_string()),
            3,
            None,
            0.8,
            false,
            None,
        ).unwrap();
        let prm = Arc::new(MockPRM::new(vec![0.7, 0.6, 0.5, 0.8]));
        let epf = EntropicParticleFiltering::new(
            sg,
            prm,
            SelectionMethod::Argmax,
            ResamplingMethod::Systematic,
            TemperatureMethod::Ess,
            0.5,
            0.5,
        );

        let client = make_client(&server.uri());
        let messages = vec![user_message("Test prompt")];

        let result = epf
            .infer(&client, &messages, 4, false, None, None, None, None)
            .await
            .unwrap();

        match result {
            AlgorithmOutput::Full { metadata, .. } => {
                let responses = metadata["responses"].as_array().unwrap();
                assert_eq!(responses.len(), 4);
            }
            _ => panic!("expected Full"),
        }
    }
}
