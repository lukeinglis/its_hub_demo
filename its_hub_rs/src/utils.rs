/// System prompt for step-by-step reasoning.
/// Taken from <https://github.com/huggingface/search-and-learn>.
pub const SAL_STEP_BY_STEP_SYSTEM_PROMPT: &str = "\
Solve the following math problem efficiently and clearly:\n\n\
- For simple problems (2 steps or fewer):\n\
Provide a concise solution with minimal explanation.\n\n\
- For complex problems (3 steps or more):\n\
Use this step-by-step format:\n\n\
## Step 1: [Concise description]\n\
[Brief explanation and calculations]\n\n\
## Step 2: [Concise description]\n\
[Brief explanation and calculations]\n\n\
...\n\n\
Regardless of the approach, always conclude with:\n\n\
Therefore, the final answer is: $\\boxed{answer}$. I hope it is correct.\n\n\
Where [answer] is just the final number or expression that solves the problem.";

/// Qwen model system prompt for step-by-step reasoning.
pub const QWEN_SYSTEM_PROMPT: &str =
    "Please reason step by step, and put your final answer within \\boxed{}.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sal_prompt_contains_key_phrases() {
        assert!(SAL_STEP_BY_STEP_SYSTEM_PROMPT.contains("step-by-step"));
        assert!(SAL_STEP_BY_STEP_SYSTEM_PROMPT.contains("\\boxed{answer}"));
        assert!(SAL_STEP_BY_STEP_SYSTEM_PROMPT.contains("## Step 1:"));
    }

    #[test]
    fn test_qwen_prompt_contains_boxed() {
        assert!(QWEN_SYSTEM_PROMPT.contains("\\boxed{}"));
        assert!(QWEN_SYSTEM_PROMPT.contains("step by step"));
    }
}
