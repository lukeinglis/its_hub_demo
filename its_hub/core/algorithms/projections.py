from collections.abc import Callable

# Above this operation count, sympy.simplify() on a rationalized expression can
# take pathologically long; skip simplification and fall back to the
# rationalized-but-unsimplified expression instead.
MATH_VERIFY_SIMPLIFY_MAX_OPS = 200

# math-verify's extraction regex exhibits catastrophic backtracking on strings
# that mix deeply nested parentheses with multiple operators (e.g. a continued
# fraction like "1/(1+1/(1+1/(...)))"): parse() can hang for seconds to
# indefinitely, and math-verify's own signal-based internal timeout does not
# reliably interrupt it because regex backtracking blocks signal delivery.
# Skip parse() entirely above this nesting depth and fall back to exact-match
# on the raw string instead.
MATH_VERIFY_MAX_PAREN_DEPTH = 15


def _max_paren_depth(s: str) -> int:
    """Return the maximum nesting depth of parentheses in a string."""
    depth = 0
    max_depth = 0
    for ch in s:
        if ch == "(":
            depth += 1
            max_depth = max(max_depth, depth)
        elif ch == ")":
            depth = max(depth - 1, 0)
    return max_depth


def create_math_verify_projection() -> Callable[[str], str]:
    """Create a projection function that canonicalizes mathematical expressions.

    Uses the math-verify library (optional, install via `pip install its_hub[research]`)
    to parse mathematical expressions into SymPy objects, then canonicalizes them
    (via rationalization and simplification) so that equivalent forms
    (e.g., "1/2" and "0.5") map to the same voting key in Self-Consistency.

    Returns:
        A projection function that takes a response string and returns a
        canonicalized string. Falls back to stripped input when the response
        cannot be parsed as a mathematical expression.

    Raises:
        ImportError: If math-verify is not installed.
    """
    try:
        import sympy  # pyright: ignore[reportMissingImports]
        from math_verify import parse  # pyright: ignore[reportMissingImports]
    except ImportError as err:
        raise ImportError(
            "math-verify is required for equivalence-aware projection. "
            "Install it with: pip install its_hub[research]"
        ) from err

    def projection(response: str) -> str:
        if not isinstance(response, str):
            response = str(response)
        stripped = response.strip()
        # guard against math-verify's parse() hanging on pathologically nested
        # input before we ever get an expression to apply the op-count guard to
        if _max_paren_depth(stripped) > MATH_VERIFY_MAX_PAREN_DEPTH:
            return stripped
        try:
            parsed = parse(stripped)
            if not parsed:
                return stripped
            expr = parsed[0]
            try:
                # skip both nsimplify and simplify on pathologically complex
                # expressions (e.g. deep continued fractions) — equivalence
                # grouping degrades to exact-match for these, which is correct
                if sympy.count_ops(expr) > MATH_VERIFY_SIMPLIFY_MAX_OPS:
                    canonical = expr
                else:
                    rationalized = sympy.nsimplify(expr, rational=True)
                    canonical = sympy.simplify(rationalized)
            except Exception:
                canonical = expr
            return str(canonical)
        except Exception:
            return stripped

    return projection
