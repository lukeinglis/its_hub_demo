"""
Correctness evaluation for ITS demo responses.

Two evaluation paths:
- Math questions: regex extraction + normalization + comparison (zero API cost)
- General questions: LLM judge via litellm (GPT-4.1 Mini)
"""

import json
import logging
import re
from fractions import Fraction

import litellm

logger = logging.getLogger(__name__)

JUDGE_MODEL = "gpt-4.1-mini"


# ---------------------------------------------------------------------------
# Math answer extraction
# ---------------------------------------------------------------------------

def extract_math_answer(response: str) -> str | None:
    """Extract a final numeric/math answer from a model response.

    Tries in order:
    1. \\boxed{...} with nested brace handling
    2. "Final Answer:" / "the answer is" patterns
    3. Last standalone numeric value
    """
    if not response:
        return None

    # 1. \boxed{...} — handle nested braces
    boxed_match = _extract_boxed(response)
    if boxed_match is not None:
        return boxed_match

    # 2. Common "final answer" patterns
    patterns = [
        r"[Ff]inal\s+[Aa]nswer\s*[:=]\s*(.+?)(?:\n|$)",
        r"[Tt]herefore[,:]?\s*(.+?)(?:\n|$)",
        r"[Tt]he\s+answer\s+is\s*[:=]?\s*(.+?)(?:\n|$|\.|,)",
    ]
    for pat in patterns:
        m = re.search(pat, response)
        if m:
            ans = m.group(1).strip().rstrip(".")
            if ans:
                return ans

    # 3. Last numeric value (integer, decimal, or fraction like 5/14)
    nums = re.findall(r"[-+]?\d+(?:/\d+)?(?:\.\d+)?", response)
    if nums:
        return nums[-1]

    return None


def _extract_boxed(text: str) -> str | None:
    """Extract content from the last \\boxed{...}, handling nested braces."""
    idx = text.rfind("\\boxed{")
    if idx == -1:
        return None
    start = idx + len("\\boxed{")
    depth = 1
    i = start
    while i < len(text) and depth > 0:
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
        i += 1
    if depth == 0:
        return text[start : i - 1].strip()
    return None


# ---------------------------------------------------------------------------
# Math answer normalization
# ---------------------------------------------------------------------------

def normalize_math_answer(answer: str) -> str:
    """Normalize a math answer for comparison.

    Strips whitespace, dollar signs, converts \\frac{a}{b} → a/b,
    removes \\text{} wrappers, etc.
    """
    s = answer.strip()
    # Remove surrounding $ signs
    s = s.strip("$")
    # Remove \text{...} wrappers
    s = re.sub(r"\\text\{([^}]*)\}", r"\1", s)
    # Convert \frac{a}{b} → a/b
    s = re.sub(r"\\frac\{([^}]*)\}\{([^}]*)\}", r"\1/\2", s)
    # Remove remaining LaTeX commands like \, \; \! \quad
    s = re.sub(r"\\[,;!quad]+", "", s)
    # Remove commas in numbers (e.g. "1,000" → "1000")
    s = re.sub(r"(\d),(\d)", r"\1\2", s)
    # Collapse whitespace
    s = re.sub(r"\s+", " ", s).strip()
    return s


# ---------------------------------------------------------------------------
# Math answer comparison
# ---------------------------------------------------------------------------

def math_answers_equal(a: str, b: str) -> bool:
    """Compare two math answers with tolerance.

    Tries: float comparison (1e-6 tolerance), Fraction comparison,
    then case-insensitive string match.
    """
    na = normalize_math_answer(a)
    nb = normalize_math_answer(b)

    # Try float comparison
    try:
        fa = float(na)
        fb = float(nb)
        if abs(fa - fb) < 1e-6:
            return True
    except (ValueError, OverflowError):
        pass

    # Try Fraction comparison (handles "5/14" == "5/14")
    try:
        fra = Fraction(na)
        frb = Fraction(nb)
        if fra == frb:
            return True
    except (ValueError, ZeroDivisionError):
        pass

    # Case-insensitive string match
    return na.lower() == nb.lower()


# ---------------------------------------------------------------------------
# LLM judge correctness check
# ---------------------------------------------------------------------------

async def _llm_judge_correctness(
    question: str,
    response: str,
    expected_answer: str,
) -> bool | None:
    """Use an LLM to judge whether a response is correct.

    Returns True/False on success, None on failure.
    """
    prompt = (
        "You are an answer-correctness judge. Given a question, the expected correct answer, "
        "and a model's response, determine whether the response contains or conveys the correct answer.\n\n"
        f"Question: {question}\n\n"
        f"Expected Answer: {expected_answer}\n\n"
        f"Model Response: {response}\n\n"
        "Does the model's response contain or convey the correct answer? "
        'Respond with ONLY a JSON object: {"is_correct": true} or {"is_correct": false}'
    )

    try:
        result = await litellm.acompletion(
            model=JUDGE_MODEL,
            messages=[{"role": "user", "content": prompt}],
            temperature=0,
            max_tokens=150,
        )
        content = result.choices[0].message.content.strip()
        # Parse JSON from response
        parsed = json.loads(content)
        return bool(parsed.get("is_correct"))
    except Exception as e:
        logger.warning(f"LLM judge correctness check failed: {e}")
        return None


# ---------------------------------------------------------------------------
# Top-level evaluation function
# ---------------------------------------------------------------------------

async def evaluate_correctness(
    question: str,
    response: str,
    expected_answer: str,
    question_type: str = "general",
) -> tuple[bool | None, str | None]:
    """Evaluate whether a response is correct.

    Args:
        question: The original question
        response: The model's response text
        expected_answer: The expected correct answer
        question_type: "math" or "general"

    Returns:
        (is_correct, eval_method) where eval_method is "exact_match",
        "llm_judge", or None (on failure).
    """
    if not expected_answer:
        return None, None

    # Math path: extract, normalize, compare
    if question_type == "math":
        extracted = extract_math_answer(response)
        if extracted is not None:
            is_correct = math_answers_equal(extracted, expected_answer)
            return is_correct, "exact_match"
        # Fall back to LLM judge if extraction failed
        logger.info("Math answer extraction failed, falling back to LLM judge")

    # General path (or math fallback): LLM judge
    result = await _llm_judge_correctness(question, response, expected_answer)
    if result is not None:
        return result, "llm_judge"

    return None, None
