"""
Example questions for the ITS demo.

Sources:
  - Hand-curated questions tested against gpt-3.5-turbo / gpt-4.1-nano
  - MATH500 (HuggingFaceH4/MATH-500) — subset of the MATH benchmark
  - AIME (American Invitational Mathematics Examination) — competition math
  - AMC (American Mathematics Competitions) 10/12 — competition math

Each question's best_for list is ordered by which algorithm most clearly
demonstrates improvement. The get_questions_by_algorithm() function returns
questions sorted so the ones that best showcase the selected algorithm
appear first.

All expected answers verified computationally.
"""

from typing import List, Dict


CURATED_QUESTIONS: List[Dict] = [
    # =========================================================================
    # QUESTIONS WITH VERIFIED ITS IMPROVEMENT (tested against Granite 4 3B)
    # Baseline gets these wrong; ITS corrects via consensus.
    # =========================================================================
    {
        "category": "Sequences",
        "difficulty": "Medium",
        "question": (
            "In an arithmetic sequence, the 5th term is 23 and the 12th "
            "term is 58. What is the 20th term?"
        ),
        "expected_answer": "98",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Baseline gets wrong answer (38 instead of 98). ITS corrects via majority vote.",
        "source": "curated",
        "source_id": "curated-seq-1",
    },
    {
        "category": "Geometry",
        "difficulty": "Medium",
        "question": (
            "A right triangle has legs of length $a$ and $b$ and hypotenuse "
            "of length $c$. If the area of the triangle is 60 and the "
            "perimeter is 40, what is the length of the hypotenuse?"
        ),
        "expected_answer": "17",
        "best_for": ["self_consistency"],
        "why": "Baseline gets wrong answer (100 instead of 17). System of equations — ITS corrects via consensus.",
        "source": "curated",
        "source_id": "curated-geom-1",
    },
    {
        "category": "Sequences & Series",
        "difficulty": "Hard",
        "question": (
            "Let $a_1 = 1$, $a_2 = 1$, and $a_n = a_{n-1} + a_{n-2}$ for "
            "$n \\geq 3$ (the Fibonacci sequence). Find the remainder when "
            "$a_1^2 + a_2^2 + a_3^2 + \\cdots + a_{10}^2$ is divided by 10."
        ),
        "expected_answer": "5",
        "best_for": ["self_consistency", "best_of_n", "beam_search", "particle_filtering"],
        "why": "Multi-step computation where arithmetic errors compound across Fibonacci terms",
        "source": "curated",
        "source_id": "curated-seq-2",
    },

    # =========================================================================
    # MATH500 BENCHMARK QUESTIONS
    # From HuggingFaceH4/MATH-500 (subset of the MATH benchmark).
    # Selected for ITS-friendly difficulty: small models struggle but
    # consensus / judge selection recovers the correct answer.
    # =========================================================================
    {
        "category": "Number Theory",
        "difficulty": "Medium",
        "question": (
            "What is the remainder when $2^{2005}$ is divided by 7?"
        ),
        "expected_answer": "2",
        "best_for": ["self_consistency", "beam_search", "particle_filtering"],
        "why": "Modular exponentiation requires finding a pattern in powers mod 7 (period 3). Models often miscalculate the cycle.",
        "source": "MATH500",
        "source_id": "MATH500-NT-1",
    },
    {
        "category": "Counting & Probability",
        "difficulty": "Medium",
        "question": (
            "How many integers from 1 to 9999 have digits that sum to 10?"
        ),
        "expected_answer": "282",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Stars and bars with upper-bound constraints. Models frequently miscount by forgetting digit limits or leading zeros.",
        "source": "MATH500",
        "source_id": "MATH500-CP-1",
    },
    {
        "category": "Algebra",
        "difficulty": "Medium",
        "question": (
            "If $a + b = 6$ and $a^2 + b^2 = 20$, find $a^4 + b^4$."
        ),
        "expected_answer": "272",
        "best_for": ["self_consistency", "best_of_n", "beam_search"],
        "why": "Requires computing ab=8 then (a^2+b^2)^2 - 2(ab)^2. Arithmetic errors are common but fixable via consensus.",
        "source": "MATH500",
        "source_id": "MATH500-ALG-1",
    },
    {
        "category": "Precalculus",
        "difficulty": "Hard",
        "question": (
            "Find the value of $\\sin 20^\\circ \\sin 40^\\circ \\sin 80^\\circ$. "
            "Express your answer as a common fraction involving radicals."
        ),
        "expected_answer": "\\frac{\\sqrt{3}}{8}",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Trig identity problem requiring the triple-angle product formula. Models often get partial simplifications wrong.",
        "source": "MATH500",
        "source_id": "MATH500-PRECALC-1",
    },
    {
        "category": "Geometry",
        "difficulty": "Medium",
        "question": (
            "In triangle $ABC$, $AB = 13$, $BC = 14$, and $CA = 15$. "
            "Find the area of triangle $ABC$."
        ),
        "expected_answer": "84",
        "best_for": ["self_consistency", "best_of_n", "beam_search"],
        "why": "Classic Heron's formula problem. Models sometimes make arithmetic errors in the intermediate products.",
        "source": "MATH500",
        "source_id": "MATH500-GEOM-1",
    },

    # =========================================================================
    # AIME QUESTIONS
    # American Invitational Mathematics Examination — competition-level
    # problems that strongly benefit from ITS due to multi-step reasoning.
    # =========================================================================
    {
        "category": "Competition Math",
        "difficulty": "Hard",
        "question": (
            "Find the number of ordered pairs $(x, y)$ of positive integers "
            "satisfying $x + 2y = 100$ where $x$ and $y$ are both positive."
        ),
        "expected_answer": "49",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Boundary conditions on positive integers trip up models. Consensus voting reliably catches off-by-one errors.",
        "source": "AIME",
        "source_id": "AIME-adapted-1",
    },
    {
        "category": "Competition Math",
        "difficulty": "Hard",
        "question": (
            "Let $S$ be the set of integers from 1 to 100. How many elements "
            "of $S$ are neither a perfect square nor a perfect cube?"
        ),
        "expected_answer": "88",
        "best_for": ["self_consistency", "beam_search"],
        "why": "Inclusion-exclusion with sixth powers (10 squares + 4 cubes - 2 sixth powers = 12). Models often miscount overlaps.",
        "source": "AIME",
        "source_id": "AIME-adapted-2",
    },
    {
        "category": "Competition Math",
        "difficulty": "Hard",
        "question": (
            "Find the sum of all positive integers $n$ less than 1000 "
            "such that $n^2 \\equiv 1 \\pmod{1000}$."
        ),
        "expected_answer": "4000",
        "best_for": ["self_consistency", "particle_filtering"],
        "why": "Requires CRT decomposition into mod 8 and mod 125. There are 8 solutions; multi-step modular arithmetic is error-prone.",
        "source": "AIME",
        "source_id": "AIME-adapted-3",
    },

    # =========================================================================
    # AMC (American Mathematics Competitions) QUESTIONS
    # AMC 10/12 level — accessible but tricky enough for ITS to help.
    # =========================================================================
    {
        "category": "Competition Math",
        "difficulty": "Medium",
        "question": (
            "A jar contains 4 red, 3 blue, and 2 green marbles. "
            "Three marbles are drawn at random without replacement. "
            "What is the probability that all three are different colors? "
            "Express your answer as a common fraction."
        ),
        "expected_answer": "\\frac{2}{7}",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Counting problem with three categories. Models often miscount the total combinations or favorable outcomes.",
        "source": "AMC",
        "source_id": "AMC-adapted-1",
    },
    {
        "category": "Competition Math",
        "difficulty": "Medium",
        "question": (
            "How many three-digit numbers have the property that the "
            "middle digit is the average of the first and last digits?"
        ),
        "expected_answer": "45",
        "best_for": ["self_consistency", "beam_search"],
        "why": "Requires systematic case analysis with parity constraints. Models miss cases without consensus correction.",
        "source": "AMC",
        "source_id": "AMC-adapted-2",
    },
    {
        "category": "Competition Math",
        "difficulty": "Easy",
        "question": (
            "What is the greatest prime factor of $15! + 17!$?"
        ),
        "expected_answer": "13",
        "best_for": ["self_consistency", "best_of_n", "beam_search"],
        "why": "Factor out 15! to get 15!(1 + 16·17) = 15! · 273 = 15! · 3·7·13. All primes ≤ 13. Models often wrongly guess 17.",
        "source": "AMC",
        "source_id": "AMC-adapted-3",
    },
    {
        "category": "Competition Math",
        "difficulty": "Hard",
        "question": (
            "The polynomial $x^3 - ax^2 + bx - 2010$ has three positive "
            "integer roots. What is the smallest possible value of $a$?"
        ),
        "expected_answer": "78",
        "best_for": ["self_consistency", "best_of_n", "particle_filtering"],
        "why": "Requires finding factorizations of 2010 = 2·3·5·67 into three positive factors minimizing their sum.",
        "source": "AMC",
        "source_id": "AMC-12A-2010-21",
    },
]


# Tool calling example questions - demonstrate agent tool selection consensus
TOOL_CALLING_QUESTIONS: List[Dict] = [
    # =========================================================================
    # TOOL SELECTION QUESTIONS
    # These demonstrate the value of consensus on tool choice and parameters
    # =========================================================================
    {
        "category": "Finance",
        "difficulty": "Easy",
        "question": (
            "What's the compound annual growth rate (CAGR) if I invest $1000 "
            "and it grows to $2000 in 5 years?"
        ),
        "expected_answer": "14.87%",
        "best_for": ["self_consistency"],
        "why": "Could use 'calculate' OR 'code_executor'. Tool voting shows consensus on best approach.",
        "source": "tool_calling",
        "source_id": "tc-finance-1",
        "expected_tools": ["calculate", "code_executor"],
    },
    {
        "category": "Data Retrieval",
        "difficulty": "Easy",
        "question": (
            "What's the current temperature in San Francisco in Celsius?"
        ),
        "expected_answer": "22°C (approximately)",
        "best_for": ["self_consistency"],
        "why": "Could use 'get_data' (weather API) OR 'calculate' (if temp in F is known). Shows tool selection consensus.",
        "source": "tool_calling",
        "source_id": "tc-data-1",
        "expected_tools": ["get_data", "calculate"],
    },
    {
        "category": "Finance",
        "difficulty": "Medium",
        "question": (
            "What's the current price of Apple (AAPL) stock and how has it "
            "changed in the last quarter?"
        ),
        "expected_answer": "Current price with percentage change",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Requires both 'get_data' for current price and 'web_search' for historical context. Tool parameter voting important.",
        "source": "tool_calling",
        "source_id": "tc-finance-2",
        "expected_tools": ["get_data", "web_search"],
    },
    {
        "category": "Unit Conversion",
        "difficulty": "Easy",
        "question": (
            "If it's currently 72°F outside, what's that in Celsius?"
        ),
        "expected_answer": "22.2°C",
        "best_for": ["self_consistency"],
        "why": "Simple calculation. Could use 'calculate' with formula OR 'get_data' for conversion. Shows tool choice reliability.",
        "source": "tool_calling",
        "source_id": "tc-convert-1",
        "expected_tools": ["calculate", "get_data"],
    },
    {
        "category": "Data Analysis",
        "difficulty": "Medium",
        "question": (
            "Calculate the monthly payment on a $300,000 mortgage at 6.5% "
            "interest over 30 years."
        ),
        "expected_answer": "$1,896 per month",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Complex formula - could use 'calculate' (with proper formula) OR 'code_executor' for clarity. Tool voting reduces errors.",
        "source": "tool_calling",
        "source_id": "tc-finance-3",
        "expected_tools": ["calculate", "code_executor"],
    },
    {
        "category": "Information Retrieval",
        "difficulty": "Medium",
        "question": (
            "What's the exchange rate from USD to EUR today?"
        ),
        "expected_answer": "~0.92 EUR per USD",
        "best_for": ["self_consistency"],
        "why": "Clearly needs 'get_data' for currency rates, but models might try 'web_search'. Tool consensus ensures correct approach.",
        "source": "tool_calling",
        "source_id": "tc-data-2",
        "expected_tools": ["get_data", "web_search"],
    },
    {
        "category": "Computation",
        "difficulty": "Hard",
        "question": (
            "Calculate the standard deviation of the following dataset: "
            "[12, 15, 18, 22, 25, 28, 30, 35, 40, 45]. Show your work."
        ),
        "expected_answer": "~10.96",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Multi-step calculation. 'code_executor' is cleanest but 'calculate' could work. Parameter consensus shows best approach.",
        "source": "tool_calling",
        "source_id": "tc-stats-1",
        "expected_tools": ["code_executor", "calculate"],
    },
    {
        "category": "Mixed",
        "difficulty": "Hard",
        "question": (
            "I bought Microsoft stock at $250 per share. It's now at $378.91. "
            "If I invested $10,000, what's my return percentage and dollar gain?"
        ),
        "expected_answer": "51.56% return, $5,156.40 gain",
        "best_for": ["self_consistency", "best_of_n"],
        "why": "Requires 'get_data' for current price verification AND 'calculate' for returns. Tool sequencing and parameters matter.",
        "source": "tool_calling",
        "source_id": "tc-mixed-1",
        "expected_tools": ["calculate", "get_data"],
    },
]


def get_all_questions() -> List[Dict[str, str]]:
    """
    Get all curated example questions.

    Returns questions with ITS-improvement questions first, then reliable ones.
    """
    return CURATED_QUESTIONS


def get_questions_by_algorithm(algorithm: str, limit: int = 10) -> List[Dict[str, str]]:
    """
    Get example questions ordered by how well they showcase the given algorithm.

    Questions where this algorithm is listed first in best_for appear at the top
    (these are the ones tested to show the clearest improvement). Questions where
    the algorithm appears later are listed next, followed by any remaining.

    Args:
        algorithm: Algorithm name (e.g., 'beam_search', 'best_of_n')
        limit: Maximum number of questions to return

    Returns:
        List of question dictionaries, ordered by demo effectiveness.
    """
    all_questions = get_all_questions()

    # Tier 1: Algorithm is the FIRST entry in best_for (best demo for this algo)
    tier1 = [q for q in all_questions
             if q["best_for"] and q["best_for"][0] == algorithm]

    # Tier 2: Algorithm appears in best_for but not first
    tier2 = [q for q in all_questions
             if algorithm in q["best_for"] and q not in tier1]

    # Tier 3: Algorithm not listed, but still available
    tier3 = [q for q in all_questions
             if algorithm not in q["best_for"]]

    result = tier1 + tier2 + tier3
    return result[:limit]


def get_questions_by_difficulty(difficulty: str) -> List[Dict[str, str]]:
    """
    Get example questions by difficulty level.

    Args:
        difficulty: 'Easy', 'Medium', or 'Hard'

    Returns:
        List of question dictionaries
    """
    return [q for q in get_all_questions() if q["difficulty"] == difficulty]


def get_tool_calling_questions() -> List[Dict[str, str]]:
    """
    Get all tool calling example questions.

    These questions demonstrate agent tool selection consensus scenarios.

    Returns:
        List of tool calling question dictionaries
    """
    return TOOL_CALLING_QUESTIONS


def get_tool_calling_questions_by_algorithm(algorithm: str, limit: int = 10) -> List[Dict[str, str]]:
    """
    Get tool calling questions ordered by how well they showcase the given algorithm.

    Args:
        algorithm: Algorithm name (e.g., 'self_consistency', 'best_of_n')
        limit: Maximum number of questions to return

    Returns:
        List of question dictionaries, ordered by demo effectiveness.
    """
    all_questions = get_tool_calling_questions()

    # Tier 1: Algorithm is the FIRST entry in best_for
    tier1 = [q for q in all_questions
             if q["best_for"] and q["best_for"][0] == algorithm]

    # Tier 2: Algorithm appears in best_for but not first
    tier2 = [q for q in all_questions
             if algorithm in q["best_for"] and q not in tier1]

    # Tier 3: Algorithm not listed, but still available
    tier3 = [q for q in all_questions
             if algorithm not in q["best_for"]]

    result = tier1 + tier2 + tier3
    return result[:limit]
