/**
 * Guided Demo Flow - Step-by-step ITS demonstration
 *
 * Flow: Goal → Method → Scenario → Prompt/Submit → Responses/Trace → Performance
 *
 * Data loading:
 *   On startup, fetches guided-demo-data.json (captured via capture_guided_scenarios.py).
 *   If the file is present, real API responses are used for all 8 scenario+method combos.
 *   If the file is missing or fails to load, hardcoded mock data is used as fallback.
 *
 * The GUIDED_SCENARIOS object defines the branching structure (goals → scenarios).
 * You can add/remove scenarios there without changing any other code.
 */

// ============================================================
// CAPTURED DATA LOADING
// ============================================================

let GUIDED_CAPTURED_DATA = null;
let _guidedDataPromise = fetch('guided-demo-data.json')
    .then(r => r.json())
    .then(data => { GUIDED_CAPTURED_DATA = data; })
    .catch(err => console.warn('Guided demo data not loaded, using mock fallback:', err));

// ============================================================
// STATE
// ============================================================

// guidedFormatLatency removed — use global formatLatency() from app.js directly

const guidedDemoState = {
    goal: null,        // 'improve_performance' or 'match_frontier'
    method: null,      // 'self_consistency' or 'best_of_n'
    scenario: null,    // scenario key from GUIDED_SCENARIOS
    currentStep: 1,    // 1-6
};

// ============================================================
// SCENARIO DEFINITIONS
// ============================================================

const GUIDED_SCENARIOS = {
    improve_frontier: {
        id: 'improve_frontier',
        goal: 'improve_performance',
        title: 'Small Commercial Model',
        subtitle: 'Fix errors and improve reliability',
        icon: '🌟',
        model: 'GPT-4.1 Nano',
        provider: 'OpenAI',
        description: 'Small models occasionally make calculation errors. ITS generates multiple reasoning paths and votes on the correct answer, catching mistakes that would slip through in a single pass.',
    },
    improve_opensource: {
        id: 'improve_opensource',
        goal: 'improve_performance',
        title: 'Open Source Model',
        subtitle: 'Correct errors at lower cost than upgrading',
        icon: '🔓',
        model: 'Llama 3.2 3B',
        provider: 'Open Source',
        description: 'Small open-source models make mistakes on complex reasoning. ITS corrects errors through consensus voting — achieving better accuracy without switching to a larger model.',
    },
    match_same_family: {
        id: 'match_same_family',
        goal: 'match_frontier',
        title: 'Same Model Family',
        subtitle: 'Match quality while reducing costs',
        icon: '👨‍👦',
        smallModel: 'GPT-3.5 Turbo',
        frontierModel: 'GPT-4o',
        provider: 'OpenAI',
        description: 'GPT-3.5 Turbo costs 97% less per token than GPT-4o. With ITS, the small model matches frontier quality while maintaining significant cost savings.',
    },
    match_cross_family: {
        id: 'match_cross_family',
        goal: 'match_frontier',
        title: 'Cross-Family Match',
        subtitle: 'Open-source quality matching proprietary models',
        icon: '🔀',
        smallModel: 'Llama 3.2 3B',
        frontierModel: 'GPT-4o',
        provider: 'Open Source / OpenAI',
        description: 'A tiny open-source model with ITS can match expensive frontier model quality — enabling cost-effective alternatives to proprietary APIs.',
    },
    tool_stock: {
        id: 'tool_stock',
        goal: 'tool_calling',
        title: 'Stock Price Lookup',
        subtitle: 'Correct tool selection via consensus',
        icon: '📈',
        model: 'GPT-4.1 Nano',
        provider: 'OpenAI',
        description: 'Given 4 available tools, the model must pick the right one to look up a stock price. The baseline sometimes picks web_search instead of the structured get_data tool. ITS votes across attempts to select the correct tool.',
        availableTools: ['web_search', 'calculate', 'get_data', 'code_executor'],
        source: 'Adapted from BFCL multiple_56 (Berkeley Function Calling Leaderboard)',
        expectedTool: {
            name: 'get_data',
            arguments: { data_type: 'stock_price', parameters: { symbol: 'AAPL' } },
        },
        correctExplanation: 'The get_data tool with data_type="stock_price" returns structured, machine-readable data (price, change, volume) from a reliable data source. web_search returns unstructured HTML snippets that require parsing and may contain stale or inconsistent information. In production agentic pipelines, structured APIs are always preferred for data retrieval.',
        baselineExplanation: 'The baseline picked web_search — a general-purpose tool that returns search result snippets. While it may contain the price, extracting it requires parsing free text, and the data may be delayed or inconsistent across sources.',
    },
    tool_currency: {
        id: 'tool_currency',
        goal: 'tool_calling',
        title: 'Currency Exchange Rate',
        subtitle: 'Structured API vs general search',
        icon: '💱',
        model: 'GPT-4.1 Nano',
        provider: 'OpenAI',
        description: 'The model must retrieve an exchange rate using the structured get_data tool rather than web_search or calculate. ITS votes on both the tool and the correct parameters.',
        availableTools: ['web_search', 'calculate', 'get_data', 'code_executor'],
        source: 'Adapted from BFCL multiple_52 (Berkeley Function Calling Leaderboard)',
        expectedTool: {
            name: 'get_data',
            arguments: { data_type: 'currency_rate', parameters: { from: 'USD', to: 'JPY' } },
        },
        correctExplanation: 'The get_data tool with data_type="currency_rate" returns the precise, current exchange rate as structured data. This can be directly used for calculation (200 × rate) without any text parsing. In agentic workflows, using the structured API ensures reliable, programmatic access to the data.',
        baselineExplanation: 'The baseline picked web_search, which returns search snippets like "1 USD = 149.50 JPY". The rate is buried in free text, may be stale, and requires regex or LLM extraction to use programmatically.',
    },
};

// ============================================================
// MOCK QUESTIONS — Replace with real demo questions later
// Key format: `${scenarioId}_${method}`
// ============================================================

const GUIDED_MOCK_QUESTIONS = {
    'improve_frontier_self_consistency': 'A palindrome is a number that reads the same forwards and backwards. How many 5-digit palindromes are divisible by 3?',
    'improve_frontier_best_of_n': 'An investment of $10,000 earns 8% annual interest compounded quarterly. After 3 years, how much total interest has been earned? Round to the nearest cent.',
    'improve_opensource_self_consistency': 'In how many ways can 5 letters be placed in 5 addressed envelopes so that no letter is in its correct envelope?',
    'improve_opensource_best_of_n': 'In how many ways can 5 letters be placed in 5 addressed envelopes so that no letter is in its correct envelope?',
    'match_same_family_self_consistency': 'In how many ways can 5 letters be placed in 5 addressed envelopes so that no letter is in its correct envelope?',
    'match_same_family_best_of_n': 'In how many ways can 5 letters be placed in 5 addressed envelopes so that no letter is in its correct envelope?',
    'match_cross_family_self_consistency': 'In how many ways can 5 letters be placed in 5 addressed envelopes so that no letter is in its correct envelope?',
    'match_cross_family_best_of_n': 'In how many ways can 5 letters be placed in 5 addressed envelopes so that no letter is in its correct envelope?',
    'tool_stock_self_consistency': 'What is the current stock price of Apple (AAPL)?',
    'tool_currency_self_consistency': 'How many Japanese yen can I get for 200 US dollars?',
};

// ============================================================
// RESPONSE DATA — uses captured JSON when available, mock fallback otherwise
// ============================================================

function getMockResponse(scenarioId, method) {
    const scenario = GUIDED_SCENARIOS[scenarioId];
    const isMatchFrontier = scenario.goal === 'match_frontier';

    // --- Try captured data first ---
    const key = `${scenarioId}_${method}`;
    const captured = GUIDED_CAPTURED_DATA && GUIDED_CAPTURED_DATA[key];
    if (captured) {
        const result = {
            baseline: captured.baseline,
            its: captured.its,
            trace: captured.trace || null,
        };
        if (isMatchFrontier && captured.frontier) {
            result.frontier = captured.frontier;
        }
        return result;
    }

    // --- Tool calling mock data ---
    if (scenario.goal === 'tool_calling') {
        return getToolCallingMockResponse(scenarioId);
    }

    // --- Fallback: hardcoded mocks ---

    // Self-Consistency mocks
    if (method === 'self_consistency') {
        const result = {
            baseline: {
                response: '[Placeholder] The baseline model gave a single response without verification.\n\nThis is where the standard model output would appear — a single inference pass with no ITS enhancement.',
                latency_ms: 480,
                input_tokens: 28,
                output_tokens: 35,
                cost_usd: 0.000025,
            },
            its: {
                response: '[Placeholder] The ITS-enhanced response was selected by majority voting across 8 candidates.\n\nThe most common answer was chosen, improving reliability over a single pass.',
                latency_ms: 1150,
                input_tokens: 224,
                output_tokens: 52,
                cost_usd: 0.000195,
            },
            trace: {
                algorithm: 'self_consistency',
                candidates: [
                    { index: 0, content: 'Candidate 1: Answer A — placeholder reasoning', is_selected: false, tool_calls: null },
                    { index: 1, content: 'Candidate 2: Answer A — different reasoning path', is_selected: false, tool_calls: null },
                    { index: 2, content: 'Candidate 3: Answer A — selected as representative', is_selected: true, tool_calls: null },
                    { index: 3, content: 'Candidate 4: Answer B — alternative answer', is_selected: false, tool_calls: null },
                    { index: 4, content: 'Candidate 5: Answer A — consistent', is_selected: false, tool_calls: null },
                    { index: 5, content: 'Candidate 6: Answer C — outlier', is_selected: false, tool_calls: null },
                    { index: 6, content: 'Candidate 7: Answer A — majority', is_selected: false, tool_calls: null },
                    { index: 7, content: 'Candidate 8: Answer B — minority', is_selected: false, tool_calls: null },
                ],
                vote_counts: { 'Answer A': 5, 'Answer B': 2, 'Answer C': 1 },
                total_votes: 8,
                tool_voting: null,
            },
        };
        if (isMatchFrontier) {
            result.frontier = {
                response: '[Placeholder] The frontier model produced a high-quality response in a single pass.\n\nThis represents the quality target that the small model + ITS aims to match.',
                latency_ms: 620,
                input_tokens: 28,
                output_tokens: 65,
                cost_usd: 0.00250,
            };
        }
        return result;
    }

    // Best-of-N mocks
    const scores = [0.72, 0.85, 0.95, 0.61, 0.88, 0.77, 0.91, 0.68];
    const result = {
        baseline: {
            response: '[Placeholder] The baseline model produced a single response without quality evaluation.\n\nWith Best-of-N, multiple candidates would be generated and scored by an LLM judge.',
            latency_ms: 520,
            input_tokens: 32,
            output_tokens: 48,
            cost_usd: 0.000032,
        },
        its: {
            response: '[Placeholder] The highest-scoring response was selected by an LLM judge from 8 candidates.\n\nThe judge scored each response for quality, selecting the best one (score: 0.95).',
            latency_ms: 2800,
            input_tokens: 256,
            output_tokens: 384,
            cost_usd: 0.000480,
        },
        trace: {
            algorithm: 'best_of_n',
            candidates: scores.map((s, i) => ({
                index: i,
                content: `[Candidate ${i + 1}] Placeholder response — scored ${s.toFixed(2)} by LLM judge`,
                is_selected: i === 2,
                tool_calls: null,
            })),
            scores: scores,
            max_score: 0.95,
            min_score: 0.61,
        },
    };
    if (isMatchFrontier) {
        result.frontier = {
            response: '[Placeholder] The frontier model baseline — the quality target for the small model + ITS.',
            latency_ms: 650,
            input_tokens: 32,
            output_tokens: 72,
            cost_usd: 0.00280,
        };
    }
    return result;
}

// ============================================================
// TOOL CALLING MOCK DATA
// ============================================================

// Questions adapted from the Berkeley Function Calling Leaderboard (BFCL)
// https://gorilla.cs.berkeley.edu/leaderboard.html
// Stock scenario: adapted from BFCL multiple_56
// Currency scenario: adapted from BFCL multiple_52
function getToolCallingMockResponse(scenarioId) {
    if (scenarioId === 'tool_stock') {
        return {
            baseline: {
                response: 'Let me search the web for the current Apple stock price.',
                latency_ms: 520,
                input_tokens: 45,
                output_tokens: 38,
                cost_usd: 0.000033,
                tool_call: {
                    name: 'web_search',
                    arguments: { query: 'AAPL Apple stock price today' },
                },
            },
            its: {
                response: 'I\'ll retrieve the current AAPL stock price from the data API.',
                latency_ms: 1280,
                input_tokens: 360,
                output_tokens: 52,
                cost_usd: 0.000210,
                tool_call: {
                    name: 'get_data',
                    arguments: { data_type: 'stock_price', parameters: { symbol: 'AAPL' } },
                },
            },
            trace: {
                algorithm: 'self_consistency',
                candidates: [
                    { index: 0, content: 'I\'ll use get_data to retrieve the AAPL stock price.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'stock_price', parameters: { symbol: 'AAPL' } } }] },
                    { index: 1, content: 'Let me search the web for Apple\'s current stock price.', is_selected: false,
                      tool_calls: [{ name: 'web_search', arguments: { query: 'AAPL Apple stock price today' } }] },
                    { index: 2, content: 'I\'ll query get_data for AAPL stock data.', is_selected: true,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'stock_price', parameters: { symbol: 'AAPL' } } }] },
                    { index: 3, content: 'I\'ll use the get_data API for stock price info.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'stock_price', parameters: { symbol: 'AAPL' } } }] },
                    { index: 4, content: 'Let me search for Apple stock information online.', is_selected: false,
                      tool_calls: [{ name: 'web_search', arguments: { query: 'Apple AAPL stock price current' } }] },
                    { index: 5, content: 'I\'ll retrieve stock data via the get_data tool.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'stock_price', parameters: { symbol: 'AAPL' } } }] },
                    { index: 6, content: 'Using get_data for AAPL stock price retrieval.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'stock_price', parameters: { symbol: 'AAPL' } } }] },
                    { index: 7, content: 'I\'ll look up Apple stock price using get_data.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'stock_price', parameters: { symbol: 'AAPL' } } }] },
                ],
                vote_counts: { 'get_data': 6, 'web_search': 2 },
                total_votes: 8,
                tool_voting: {
                    tool_vote_type: 'tool_name',
                    tool_counts: { 'get_data': 6, 'web_search': 2 },
                    winning_tool: 'get_data',
                    total_tool_calls: 8,
                },
            },
        };
    }

    if (scenarioId === 'tool_currency') {
        return {
            baseline: {
                response: 'Let me search the web for the current USD to JPY exchange rate.',
                latency_ms: 540,
                input_tokens: 48,
                output_tokens: 42,
                cost_usd: 0.000035,
                tool_call: {
                    name: 'web_search',
                    arguments: { query: 'current USD to JPY exchange rate' },
                },
            },
            its: {
                response: 'I\'ll retrieve the current USD to JPY exchange rate using the structured data API.',
                latency_ms: 1320,
                input_tokens: 384,
                output_tokens: 50,
                cost_usd: 0.000220,
                tool_call: {
                    name: 'get_data',
                    arguments: { data_type: 'currency_rate', parameters: { from: 'USD', to: 'JPY' } },
                },
            },
            trace: {
                algorithm: 'self_consistency',
                candidates: [
                    { index: 0, content: 'I\'ll use get_data to retrieve the USD/JPY exchange rate.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'currency_rate', parameters: { from: 'USD', to: 'JPY' } } }] },
                    { index: 1, content: 'Let me search for the current exchange rate online.', is_selected: false,
                      tool_calls: [{ name: 'web_search', arguments: { query: 'USD to JPY exchange rate today' } }] },
                    { index: 2, content: 'I\'ll query the currency rate data source.', is_selected: true,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'currency_rate', parameters: { from: 'USD', to: 'JPY' } } }] },
                    { index: 3, content: 'Let me search for USD to yen conversion.', is_selected: false,
                      tool_calls: [{ name: 'web_search', arguments: { query: 'US dollar to Japanese yen conversion rate' } }] },
                    { index: 4, content: 'Using the get_data tool for currency conversion.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'currency_rate', parameters: { from: 'USD', to: 'JPY' } } }] },
                    { index: 5, content: 'I\'ll look up the currency rate via get_data.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'currency_rate', parameters: { from: 'USD', to: 'JPY' } } }] },
                    { index: 6, content: 'I\'ll use get_data for the exchange rate.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'currency_rate', parameters: { from: 'USD', to: 'JPY' } } }] },
                    { index: 7, content: 'I\'ll retrieve the exchange rate from the data API.', is_selected: false,
                      tool_calls: [{ name: 'get_data', arguments: { data_type: 'currency_rate', parameters: { from: 'USD', to: 'JPY' } } }] },
                ],
                vote_counts: { 'get_data': 6, 'web_search': 2 },
                total_votes: 8,
                tool_voting: {
                    tool_vote_type: 'tool_name',
                    tool_counts: { 'get_data': 6, 'web_search': 2 },
                    winning_tool: 'get_data',
                    total_tool_calls: 8,
                },
            },
        };
    }

    // Fallback
    return getMockResponse('improve_frontier', 'self_consistency');
}

// ============================================================
// PERFORMANCE DATA — derived from response data (real or mock)
// ============================================================

function getMockPerformance(scenarioId, method) {
    const scenario = GUIDED_SCENARIOS[scenarioId];
    const isMatchFrontier = scenario.goal === 'match_frontier';
    const mockResp = getMockResponse(scenarioId, method);

    if (isMatchFrontier) {
        return {
            columns: ['Small Baseline', 'Small + ITS', 'Frontier'],
            cost:    [mockResp.baseline.cost_usd, mockResp.its.cost_usd, mockResp.frontier.cost_usd],
            latency: [mockResp.baseline.latency_ms, mockResp.its.latency_ms, mockResp.frontier.latency_ms],
            tokens:  [
                mockResp.baseline.input_tokens + mockResp.baseline.output_tokens,
                mockResp.its.input_tokens + mockResp.its.output_tokens,
                mockResp.frontier.input_tokens + mockResp.frontier.output_tokens,
            ],
        };
    }
    return {
        columns: ['Baseline', 'ITS Enhanced'],
        cost:    [mockResp.baseline.cost_usd, mockResp.its.cost_usd],
        latency: [mockResp.baseline.latency_ms, mockResp.its.latency_ms],
        tokens:  [
            mockResp.baseline.input_tokens + mockResp.baseline.output_tokens,
            mockResp.its.input_tokens + mockResp.its.output_tokens,
        ],
    };
}

// ============================================================
// INITIALIZATION — overrides the old initGuidedWizard
// ============================================================

function initGuidedWizard() {
    // Reset state
    guidedDemoState.goal = null;
    guidedDemoState.method = null;
    guidedDemoState.scenario = null;
    guidedDemoState.currentStep = 1;

    // Show wizard
    const wizard = document.getElementById('guidedWizard');
    setVisible(wizard, true);

    // Hide summary bar
    setVisible(document.getElementById('guidedScenarioSummary'), false);

    // Hide all other sections
    ['useCaseSection', 'scenarioSection', 'configSection', 'questionSection',
     'errorContainer', 'expectedAnswerContainer', 'resultsContainer',
     'performance-visualization-container'].forEach(id => {
        setVisible(document.getElementById(id), false);
    });

    // Clean up dynamic elements from previous runs
    ['performanceDetailsContainer', 'wizardResultsHeadline', 'wizardPromptDisplay'].forEach(id => {
        const el = document.getElementById(id);
        if (el) {
            if (id === 'wizardPromptDisplay') setVisible(el, false);
            else el.remove();
        }
    });

    // Hide old guided badge and back button (new wizard has its own UI)
    const badge = document.getElementById('guidedDemoBadge');
    setVisible(badge, false);

    const backBtn = document.getElementById('wizardBackBtn');
    setVisible(backBtn, false);

    // Show step 1
    guidedShowStep(1);
}

// ============================================================
// STEP NAVIGATION
// ============================================================

function guidedShowStep(stepNumber) {
    guidedDemoState.currentStep = stepNumber;

    // Hide all steps
    for (let i = 1; i <= 6; i++) {
        const el = document.getElementById('guidedStep' + i);
        setVisible(el, false);
    }

    // Show current step
    const currentEl = document.getElementById('guidedStep' + stepNumber);
    if (currentEl) {
        setVisible(currentEl, true);
        // Re-trigger animation
        currentEl.style.animation = 'none';
        currentEl.offsetHeight; // force reflow
        currentEl.style.animation = '';
    }

    // Update progress bar
    const progressBar = document.getElementById('guidedProgressBar');
    if (progressBar) {
        progressBar.style.width = ((stepNumber / 6) * 100) + '%';
        progressBar.setAttribute('aria-valuenow', stepNumber);
    }

    // Update breadcrumbs
    document.querySelectorAll('.g-crumb').forEach(crumb => {
        const crumbStep = parseInt(crumb.dataset.step);
        crumb.classList.remove('active', 'completed');
        if (crumbStep < stepNumber) {
            crumb.classList.add('completed');
        } else if (crumbStep === stepNumber) {
            crumb.classList.add('active');
        }
    });
}

function guidedGoBack(toStep) {
    // Clear state for steps after target
    if (toStep <= 1) {
        guidedDemoState.goal = null;
        guidedDemoState.method = null;
        guidedDemoState.scenario = null;
    } else if (toStep <= 2) {
        guidedDemoState.method = null;
        guidedDemoState.scenario = null;
    } else if (toStep <= 3) {
        guidedDemoState.scenario = null;
    }

    // Deselect cards in steps after target
    for (let i = toStep; i <= 6; i++) {
        const stepEl = document.getElementById('guidedStep' + i);
        if (stepEl) {
            stepEl.querySelectorAll('.guided-card.selected').forEach(c => c.classList.remove('selected'));
        }
    }

    guidedUpdateSummaryBar();
    guidedShowStep(toStep);
}

// Breadcrumb click navigation
document.addEventListener('click', function(e) {
    const crumb = e.target.closest('.g-crumb.completed');
    if (crumb) {
        const step = parseInt(crumb.dataset.step);
        if (step && step < guidedDemoState.currentStep) {
            guidedGoBack(step);
        }
    }
});

// ============================================================
// SELECTION SUMMARY BAR — progressively builds as choices are made
// ============================================================

function guidedUpdateSummaryBar() {
    const summaryEl = document.getElementById('guidedScenarioSummary');
    const { goal, method, scenario } = guidedDemoState;

    // Hide on step 1 (no selections yet)
    if (!goal) {
        setVisible(summaryEl, false);
        return;
    }

    let tags = '';
    const goalLabels = { improve_performance: 'Improve Model Performance', match_frontier: 'Match Frontier Model', tool_calling: 'Improve Tool Calling' };
    const goalLabel = goalLabels[goal] || goal;
    tags += `
        <span class="guided-summary-tag">
            <span class="tag-label">Goal:</span>
            <span class="tag-value">${goalLabel}</span>
        </span>
    `;

    if (method) {
        const methodLabel = goal === 'tool_calling' ? 'Self-Consistency (Tool Voting)' : (method === 'self_consistency' ? 'Self-Consistency' : 'Best-of-N');
        tags += `
            <span class="guided-summary-tag">
                <span class="tag-label">Method:</span>
                <span class="tag-value">${methodLabel}</span>
            </span>
        `;
    }

    if (scenario) {
        const sc = GUIDED_SCENARIOS[scenario];
        if (sc.goal === 'match_frontier') {
            tags += `
                <span class="guided-summary-tag">
                    <span class="tag-label">Small:</span>
                    <span class="tag-value">${sc.smallModel}</span>
                </span>
                <span class="guided-summary-tag">
                    <span class="tag-label">Frontier:</span>
                    <span class="tag-value">${sc.frontierModel}</span>
                </span>
            `;
        } else {
            tags += `
                <span class="guided-summary-tag">
                    <span class="tag-label">Model:</span>
                    <span class="tag-value">${sc.model}</span>
                </span>
            `;
        }
    }

    summaryEl.innerHTML = tags;
    setVisible(summaryEl, true);
}

// ============================================================
// STEP 1: GOAL SELECTION
// ============================================================

function guidedSelectGoal(goal) {
    guidedDemoState.goal = goal;

    // Update card selection
    document.querySelectorAll('#guidedStep1 .guided-card').forEach(card => {
        card.classList.toggle('selected', card.dataset.goal === goal);
    });

    guidedUpdateSummaryBar();

    if (goal === 'tool_calling') {
        // Auto-select self_consistency and skip method selection
        guidedDemoState.method = 'self_consistency';
        setTimeout(() => {
            guidedPopulateScenarios();
            guidedShowStep(3);
        }, 250);
    } else {
        setTimeout(() => guidedShowStep(2), 250);
    }
}

// ============================================================
// STEP 2: ITS METHOD SELECTION
// ============================================================

function guidedSelectMethod(method) {
    guidedDemoState.method = method;

    // Update card selection
    document.querySelectorAll('#guidedStep2 .guided-card').forEach(card => {
        card.classList.toggle('selected', card.dataset.method === method);
    });

    guidedUpdateSummaryBar();

    // Populate scenario cards for step 3
    setTimeout(() => {
        guidedPopulateScenarios();
        guidedShowStep(3);
    }, 250);
}

// ============================================================
// STEP 3: SCENARIO SELECTION
// ============================================================

function guidedPopulateScenarios() {
    const container = document.getElementById('guidedScenarioCards');
    const subtitle = document.getElementById('guidedStep3Subtitle');
    container.innerHTML = '';

    const goal = guidedDemoState.goal;

    // Hide benchmark callout unless tool_calling
    const existingCallout = document.getElementById('guidedBenchmarkCallout');
    if (existingCallout) setVisible(existingCallout, goal === 'tool_calling');

    if (goal === 'improve_performance') {
        subtitle.textContent = 'Which type of model do you want to improve?';
        renderScenarioCard(container, GUIDED_SCENARIOS.improve_frontier);
        renderScenarioCard(container, GUIDED_SCENARIOS.improve_opensource);
    } else if (goal === 'tool_calling') {
        subtitle.textContent = 'Choose a tool-calling scenario';

        // Benchmark attribution callout — insert before the cards grid
        let callout = document.getElementById('guidedBenchmarkCallout');
        if (!callout) {
            callout = document.createElement('div');
            callout.id = 'guidedBenchmarkCallout';
            callout.className = 'guided-benchmark-callout';
            container.parentNode.insertBefore(callout, container);
        }
        callout.innerHTML = `
            <div class="guided-benchmark-badge">BFCL</div>
            <div class="guided-benchmark-text">
                <strong>Scenarios sourced from the Berkeley Function Calling Leaderboard</strong>
                <span>BFCL is a standardised benchmark for evaluating LLM tool/function calling accuracy, maintained by UC Berkeley. These questions test whether a model selects the correct tool from multiple options — a common failure point in agentic AI pipelines.</span>
            </div>
        `;
        setVisible(callout, true);

        renderScenarioCard(container, GUIDED_SCENARIOS.tool_stock);
        renderScenarioCard(container, GUIDED_SCENARIOS.tool_currency);
        // Back button should go to Step 1 (Step 2 was skipped)
        const backBtn = document.querySelector('#guidedStep3 .guided-back-btn');
        if (backBtn) backBtn.onclick = () => guidedGoBack(1);
    } else {
        subtitle.textContent = 'Choose a model-matching scenario';
        renderScenarioCard(container, GUIDED_SCENARIOS.match_same_family);
        renderScenarioCard(container, GUIDED_SCENARIOS.match_cross_family);
        // Ensure back button goes to Step 2 (default)
        const backBtn = document.querySelector('#guidedStep3 .guided-back-btn');
        if (backBtn) backBtn.onclick = () => guidedGoBack(2);
    }
}

function renderScenarioCard(container, scenario) {
    const card = document.createElement('div');
    card.className = 'wizard-card guided-card';
    card.dataset.scenario = scenario.id;
    card.setAttribute('role', 'button');
    card.setAttribute('tabindex', '0');

    let modelInfo = '';
    if (scenario.goal === 'match_frontier') {
        modelInfo = `
            <div class="guided-card-detail">
                ${scenario.smallModel} → matching ${scenario.frontierModel}
            </div>
        `;
    } else {
        modelInfo = `
            <div class="guided-card-detail">
                Model: ${scenario.model} (${scenario.provider})
            </div>
        `;
    }

    card.innerHTML = `
        <div class="wizard-card-icon guided-card-icon">${scenario.icon}</div>
        <h4>${scenario.title}</h4>
        <p>${scenario.description}</p>
        ${modelInfo}
    `;

    card.addEventListener('click', () => guidedSelectScenario(scenario.id));
    container.appendChild(card);
}

function guidedSelectScenario(scenarioId) {
    guidedDemoState.scenario = scenarioId;

    // Update card selection
    document.querySelectorAll('#guidedStep3 .guided-card').forEach(card => {
        card.classList.toggle('selected', card.dataset.scenario === scenarioId);
    });

    guidedUpdateSummaryBar();

    // Populate step 4 (prompt)
    setTimeout(() => {
        guidedPopulatePrompt();
        guidedShowStep(4);
    }, 250);
}

// ============================================================
// STEP 4: PROMPT DISPLAY + SUBMIT
// ============================================================

function guidedPopulatePrompt() {
    const scenario = GUIDED_SCENARIOS[guidedDemoState.scenario];
    const method = guidedDemoState.method;

    // Inference callout — explain that ITS doesn't touch the model
    const calloutEl = document.getElementById('guidedInferenceCallout');
    const modelName = scenario.model || scenario.smallModel;
    const mechanismText = scenario.goal === 'tool_calling'
        ? `<strong>tool consensus voting</strong> to select the most-agreed-upon tool and arguments`
        : method === 'self_consistency'
        ? `<strong>majority voting</strong> to pick the most consistent answer`
        : `an <strong>LLM judge</strong> to score and select the highest-quality response`;
    calloutEl.innerHTML = `
        <div class="guided-inference-callout-icon">ℹ</div>
        <div class="guided-inference-callout-text">
            <strong>No training or finetuning involved.</strong>
            ITS sends the same question to the same unchanged model (${modelName}) multiple times at inference, then uses ${mechanismText}. You're paying for extra inference calls, not a different model.
        </div>
    `;

    // Prompt text — prefer captured data, fall back to mock questions
    const key = `${guidedDemoState.scenario}_${method}`;
    const capturedEntry = GUIDED_CAPTURED_DATA && GUIDED_CAPTURED_DATA[key];
    const question = (capturedEntry && capturedEntry.question)
        || GUIDED_MOCK_QUESTIONS[key]
        || 'Placeholder question — replace with real demo question';
    document.getElementById('guidedPromptText').textContent = question;

    // Reset submit button
    const btn = document.getElementById('guidedSubmitBtn');
    btn.disabled = false;
    btn.innerHTML = '<span>▶</span><span>Submit</span>';
}

// ============================================================
// STEP 5: SUBMIT + RESPONSES + TRACE
// ============================================================

function guidedSubmit() {
    const btn = document.getElementById('guidedSubmitBtn');
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner" style="width:18px;height:18px;border-width:2px;margin:0;"></span><span>Running...</span>';

    // Wait for captured data to load, then render
    _guidedDataPromise.finally(() => {
        setTimeout(() => {
            guidedRenderResponses();
            guidedShowStep(5);
        }, 400);
    });
}

function guidedRenderResponses() {
    const scenario = GUIDED_SCENARIOS[guidedDemoState.scenario];
    const method = guidedDemoState.method;
    const mockData = getMockResponse(guidedDemoState.scenario, method);
    const isMatchFrontier = scenario.goal === 'match_frontier';

    // --- Question display at top of results ---
    const key = `${guidedDemoState.scenario}_${method}`;
    const capturedEntry = GUIDED_CAPTURED_DATA && GUIDED_CAPTURED_DATA[key];
    const question = (capturedEntry && capturedEntry.question)
        || GUIDED_MOCK_QUESTIONS[key] || '';

    const container = document.getElementById('guidedResponses');
    const isToolCalling = scenario.goal === 'tool_calling';
    container.className = 'guided-responses ' + (isMatchFrontier ? 'three-col' : 'two-col');

    // Build question banner (spans full width above response columns)
    const expectedAnswer = capturedEntry && capturedEntry.expected_answer;
    const dataSourceBadge = capturedEntry
        ? '<span class="guided-data-source guided-data-source--captured" title="Results from real API calls captured via capture_guided_scenarios.py">Using captured results</span>'
        : '<span class="guided-data-source guided-data-source--mock" title="Pre-captured data not available — showing example data">Using example data</span>';

    let questionHtml = `
        <div class="guided-question-banner" style="grid-column: 1 / -1;">
            <div class="guided-question-label">Question ${dataSourceBadge}</div>
            <div class="guided-question-text">${question}</div>
    `;
    // Show available tools for tool calling scenarios
    if (isToolCalling && scenario.availableTools) {
        questionHtml += `
            <div class="guided-available-tools">
                <span class="guided-tool-label" style="line-height: 26px;">Available tools:</span>
                ${scenario.availableTools.map(t => `<span class="guided-available-tool-tag">${t}</span>`).join('')}
            </div>
        `;
    }
    questionHtml += '</div>';
    container.innerHTML = questionHtml;

    const modelName = scenario.model || scenario.smallModel;

    if (isToolCalling) {
        // Tool calling: always 2-column, use tool call panes
        container.innerHTML += guidedBuildToolCallPane(
            modelName + ' (Baseline)', 'baseline', mockData.baseline, guidedDemoState.scenario
        );
        container.innerHTML += guidedBuildToolCallPane(
            modelName + ' + ITS', 'its', mockData.its, guidedDemoState.scenario
        );
    } else {
        // Baseline pane
        const baselineLabel = isMatchFrontier
            ? (scenario.smallModel + ' (Baseline)')
            : (modelName + ' (Baseline)');
        container.innerHTML += guidedBuildResponsePane(
            baselineLabel, 'baseline', mockData.baseline
        );

        // ITS pane
        const itsLabel = isMatchFrontier
            ? (scenario.smallModel + ' + ITS')
            : (modelName + ' + ITS');
        container.innerHTML += guidedBuildResponsePane(
            itsLabel, 'its', mockData.its
        );

        // Frontier pane (match_frontier only)
        if (isMatchFrontier && mockData.frontier) {
            container.innerHTML += guidedBuildResponsePane(
                scenario.frontierModel + ' (Frontier)', 'frontier', mockData.frontier
            );
        }
    }

    // Expected answer — prominent, spanning full width for easy comparison
    if (expectedAnswer) {
        container.innerHTML += `
            <div class="guided-expected-answer-bar" style="grid-column: 1 / -1;">
                <span class="guided-expected-label">Expected Answer:</span>
                <span class="guided-expected-value">${expectedAnswer}</span>
            </div>
        `;
    }

    // Tool calling: expected tool + correctness explanation
    if (isToolCalling && scenario.expectedTool) {
        const expectedJson = JSON.stringify(scenario.expectedTool.arguments, null, 2);
        const baselineTool = mockData.baseline.tool_call ? mockData.baseline.tool_call.name : '';
        const itsTool = mockData.its.tool_call ? mockData.its.tool_call.name : '';
        const baselineCorrect = baselineTool === scenario.expectedTool.name;
        const itsCorrect = itsTool === scenario.expectedTool.name;

        container.innerHTML += `
            <div class="guided-tool-expected" style="grid-column: 1 / -1;">
                <div class="guided-tool-expected-header">
                    <span class="guided-tool-expected-label">BFCL Ground Truth</span>
                    <span class="guided-tool-expected-tool">${scenario.expectedTool.name}</span>
                </div>
                <pre class="guided-tool-args-json guided-tool-expected-args">${escapeHtml(expectedJson)}</pre>
                <div class="guided-tool-verdict">
                    <div class="guided-tool-verdict-item ${itsCorrect ? 'correct' : 'incorrect'}">
                        <span class="guided-tool-verdict-icon">${itsCorrect ? '✓' : '✗'}</span>
                        <div>
                            <strong>ITS: ${itsTool}</strong>
                            <p>${scenario.correctExplanation}</p>
                        </div>
                    </div>
                    <div class="guided-tool-verdict-item ${baselineCorrect ? 'correct' : 'incorrect'}">
                        <span class="guided-tool-verdict-icon">${baselineCorrect ? '✓' : '✗'}</span>
                        <div>
                            <strong>Baseline: ${baselineTool}</strong>
                            <p>${scenario.baselineExplanation}</p>
                        </div>
                    </div>
                </div>
            </div>
        `;
    }

    // Add "Why is this better?" insight box for certain scenarios
    const insightBox = buildScenarioInsight(guidedDemoState.scenario, method);
    if (insightBox) {
        container.innerHTML += insightBox;
    }

    // Render LaTeX math in response panes (after all innerHTML is finalized)
    if (typeof renderMath === 'function') {
        renderMath(container);
    }

    // Sync all reasoning toggles — clicking one opens/closes all
    const allToggles = container.querySelectorAll('.guided-reasoning-toggle');
    allToggles.forEach(details => {
        details.addEventListener('toggle', function() {
            const isOpen = this.open;
            allToggles.forEach(d => {
                if (d !== details) d.open = isOpen;
            });
            // Attach scroll sync after opening
            if (isOpen) {
                guidedSyncReasoningScroll(container);
            }
        });
    });

    // Show trace button area
    const traceArea = document.getElementById('guidedTraceArea');
    setVisible(traceArea, true);

    // Render the trace button
    const traceContent = document.getElementById('guidedTraceContent');
    traceContent.innerHTML = `
        <div class="guided-trace-btn-area">
            <button class="guided-trace-btn" onclick="guidedRunTraceAnimation()">
                Show ITS Trace →
            </button>
        </div>
    `;

    // Hide the performance button until trace is shown
    setVisible(document.getElementById('guidedNextArea'), false);
}

/**
 * Consolidate multi-line LaTeX blocks so KaTeX can find matching delimiters.
 * Converts  \[\n...\n\]  →  $$...$$ (single line)
 * and       \(\n...\n\)  →  $...$  (single line)
 */
function guidedPreprocessLatex(text) {
    // Display math: \[ ... \] (possibly spanning multiple lines)
    text = text.replace(/\\\[\s*\n([\s\S]*?)\n\s*\\\]/g, function(_, inner) {
        return '$$' + inner.replace(/\n/g, ' ').trim() + '$$';
    });
    // Also handle single-line \[...\]
    text = text.replace(/\\\[(.+?)\\\]/g, '$$$1$$');
    // Inline math: \( ... \)
    text = text.replace(/\\\((.+?)\\\)/g, '$$$1$$');
    return text;
}

/**
 * Build insight box explaining why ITS is better for specific scenarios
 */
function buildScenarioInsight(scenarioId, method) {
    const key = `${scenarioId}_${method}`;
    const insights = {
        'improve_frontier_self_consistency': {
            title: 'What to Look For',
            content: 'Compare the final answers and reasoning. ITS uses majority voting across multiple candidates to catch errors and improve reliability. Look for cases where the baseline makes calculation mistakes that ITS corrects through consensus.'
        },
        'improve_opensource_self_consistency': {
            title: 'What to Look For',
            content: 'Small models often make mistakes on complex reasoning. ITS generates multiple attempts and votes on the most common answer. Check if the baseline got the wrong answer and ITS corrected it — this shows how ITS can improve accuracy without upgrading to a larger model.'
        },
        'improve_frontier_best_of_n': {
            title: 'What to Look For',
            content: 'Best-of-N generates multiple responses and uses an LLM judge to select the highest quality. Compare response completeness, clarity, and usefulness. The ITS answer should be more comprehensive and better structured than the baseline.'
        },
        'improve_opensource_best_of_n': {
            title: 'What to Look For',
            content: 'Look for differences in completeness and accuracy. The baseline may give incomplete or incorrect information, while ITS selects the best response from multiple attempts. This shows how quality improves with inference-time scaling.'
        },
        'match_same_family_self_consistency': {
            title: 'Value Proposition',
            content: 'The goal is to show that a small model + ITS can match the quality of its larger sibling at lower cost. Compare the small baseline vs small+ITS vs frontier. Check if small+ITS gets the same answer as frontier while maintaining cost savings.'
        },
        'match_cross_family_self_consistency': {
            title: 'Value Proposition',
            content: 'Demonstrates that a tiny open-source model with ITS can match expensive proprietary model quality. Compare costs: the open-source model with ITS should be dramatically cheaper while achieving similar correctness.'
        },
        'match_same_family_best_of_n': {
            title: 'Value Proposition',
            content: 'Small model + ITS should produce responses comparable in quality to the larger frontier model. Look at response depth, completeness, and accuracy. The cost should still favor the small model even with ITS overhead.'
        },
        'match_cross_family_best_of_n': {
            title: 'Value Proposition',
            content: 'Open-source model with ITS competing with proprietary frontier quality. Compare response comprehensiveness and accuracy. The dramatic cost difference makes this an attractive alternative for production use cases.'
        },
        'tool_stock_self_consistency': {
            title: 'What to Look For (BFCL multiple_56)',
            content: 'Compare the tool chosen by the baseline vs ITS. The baseline picked web_search (a general-purpose search) when get_data with data_type="stock_price" returns precise, structured data. ITS voting across 8 candidates corrects this by reaching consensus on the right tool.'
        },
        'tool_currency_self_consistency': {
            title: 'What to Look For (BFCL multiple_52)',
            content: 'The baseline used web_search to look up the exchange rate, when get_data with data_type="currency_rate" returns structured, reliable data. Notice how one candidate even tried calculate with a hardcoded rate — ITS consensus filters these out.'
        }
    };

    if (!insights[key]) return null;

    const insight = insights[key];
    return `
        <div class="guided-insight-box" style="grid-column: 1 / -1; margin-top: 16px;">
            <div class="guided-insight-header">
                <span class="guided-insight-icon">💡</span>
                <span class="guided-insight-title">${insight.title}</span>
            </div>
            <div class="guided-insight-content">${insight.content}</div>
        </div>
    `;
}

/**
 * Sync scroll position across all reasoning panes.
 * Scrolls by percentage so panes of different heights stay in sync.
 */
function guidedSyncReasoningScroll(container) {
    const panes = container.querySelectorAll('.guided-reasoning-toggle .guided-pane-response');
    if (panes.length < 2) return;

    let syncing = false;
    panes.forEach(pane => {
        // Remove any existing listener by replacing the node (simple approach)
        if (pane._scrollSynced) return;
        pane._scrollSynced = true;

        pane.addEventListener('scroll', function() {
            if (syncing) return;
            syncing = true;
            const scrollPct = this.scrollHeight > this.clientHeight
                ? this.scrollTop / (this.scrollHeight - this.clientHeight)
                : 0;
            panes.forEach(other => {
                if (other !== pane) {
                    const maxScroll = other.scrollHeight - other.clientHeight;
                    other.scrollTop = scrollPct * maxScroll;
                }
            });
            syncing = false;
        });
    });
}

/**
 * Extract a final answer from a model response.
 * For math: looks for \boxed{...} or "Final answer: ..." patterns.
 * For general: uses the last substantive paragraph.
 * Returns { answer: string, hasBoxed: boolean }
 */
function guidedExtractFinalAnswer(responseText) {
    // Try \boxed{...} (LaTeX math answer)
    const boxedMatches = responseText.match(/\\boxed\{([^}]+)\}/g);
    if (boxedMatches) {
        const lastBoxed = boxedMatches[boxedMatches.length - 1];
        const inner = lastBoxed.replace(/\\boxed\{/, '').replace(/\}$/, '');
        return { answer: '$$\\boxed{' + inner + '}$$', hasBoxed: true };
    }

    // Try "The final answer is: X" or "Final answer: X"
    const finalMatch = responseText.match(/(?:the\s+)?final\s+answer\s*(?:is)?[:\s]*(.+?)(?:\n|$)/i);
    if (finalMatch) {
        return { answer: finalMatch[1].trim(), hasBoxed: false };
    }

    // Try "Therefore, ... is X" or "The answer is X" near the end
    const lines = responseText.trim().split('\n').filter(l => l.trim());
    for (let i = lines.length - 1; i >= Math.max(0, lines.length - 5); i--) {
        const line = lines[i].trim();
        if (line.match(/^(?:therefore|thus|so|hence|the answer|answer:)/i) && line.length < 300) {
            return { answer: line, hasBoxed: false };
        }
    }

    // Fallback: last non-empty paragraph (truncated)
    const lastLine = lines[lines.length - 1] || '';
    if (lastLine.length <= 300) {
        return { answer: lastLine, hasBoxed: false };
    }
    return { answer: lastLine.substring(0, 200) + '...', hasBoxed: false };
}

function guidedBuildResponsePane(title, type, data) {
    const indicatorClass = type === 'its' ? 'its' : type === 'frontier' ? 'frontier' : 'baseline';
    const paneClass = type === 'its' ? ' its-pane' : type === 'frontier' ? ' frontier-pane' : '';

    const costFmt = data.cost_usd < 0.0001
        ? '$' + data.cost_usd.toExponential(2)
        : '$' + data.cost_usd.toFixed(4);

    // Extract final answer
    const extracted = guidedExtractFinalAnswer(data.response);

    // Format the final answer for display
    const answerProcessed = guidedPreprocessLatex(extracted.answer);
    const answerHtml = typeof formatAsHTML === 'function'
        ? formatAsHTML(answerProcessed)
        : '<p>' + answerProcessed + '</p>';

    // Format full reasoning for the collapsible dropdown
    const processedText = guidedPreprocessLatex(data.response);
    const responseHtml = typeof formatAsHTML === 'function'
        ? formatAsHTML(processedText)
        : '<p>' + processedText.replace(/\n\n/g, '</p><p>').replace(/\n/g, '<br>') + '</p>';

    // Unique ID for the collapsible
    const collapseId = 'reasoning_' + type + '_' + Math.random().toString(36).slice(2, 8);

    return `
        <div class="guided-response-pane${paneClass}">
            <div class="guided-pane-header">
                <div class="guided-pane-title">
                    <span class="guided-pane-indicator ${indicatorClass}"></span>
                    ${title}
                </div>
                <div class="guided-pane-badges">
                    <span class="guided-pane-badge">${formatLatency(data.latency_ms)}</span>
                    <span class="guided-pane-badge">${costFmt}</span>
                </div>
            </div>
            <div class="guided-pane-final-answer">
                <div class="guided-final-answer-label">Final Answer</div>
                <div class="guided-final-answer-value">${answerHtml}</div>
            </div>
            <details class="guided-reasoning-toggle">
                <summary class="guided-reasoning-summary">Show Reasoning</summary>
                <div class="guided-pane-response">${responseHtml}</div>
            </details>
        </div>
    `;
}

// Simulated tool results — shows what each tool would return for the query.
// These match the mock tool implementations in backend/tools.py.
const TOOL_RESULT_MOCKS = {
    'get_data:stock_price': {
        symbol: 'AAPL', price: 178.42, change: 2.34,
        change_percent: 1.32, volume: 75000000,
    },
    'get_data:currency_rate': {
        from: 'USD', to: 'JPY', rate: 149.50,
    },
    'get_data:weather': {
        location: 'Hanoi', temperature_f: 72, temperature_c: 22,
        condition: 'Partly Cloudy', humidity: 65, wind_mph: 8,
    },
    'web_search:stock': {
        results: [
            { title: 'Apple Inc. (AAPL) Stock Price Today',
              snippet: 'AAPL stock is trading at $178.42, up 2.3% today. Market cap: $2.8T' },
            { title: 'Apple Reports Q4 Earnings Beat',
              snippet: 'Apple reported earnings of $1.46 per share, beating estimates of $1.39' },
        ],
    },
    'web_search:currency': {
        results: [
            { title: 'USD to EUR Exchange Rate',
              snippet: '1 USD = 0.92 EUR. Updated 5 minutes ago.' },
        ],
    },
};

function guidedGetToolResult(tc, scenarioId) {
    if (tc.name === 'get_data' && tc.arguments.data_type) {
        return TOOL_RESULT_MOCKS['get_data:' + tc.arguments.data_type] || null;
    }
    if (tc.name === 'web_search') {
        if (scenarioId.includes('stock')) return TOOL_RESULT_MOCKS['web_search:stock'];
        if (scenarioId.includes('currency')) return TOOL_RESULT_MOCKS['web_search:currency'];
    }
    return null;
}

function guidedBuildToolCallPane(title, type, data, scenarioId) {
    const indicatorClass = type === 'its' ? 'its' : 'baseline';
    const paneClass = type === 'its' ? ' its-pane' : '';
    const costFmt = data.cost_usd < 0.0001
        ? '$' + data.cost_usd.toExponential(2)
        : '$' + data.cost_usd.toFixed(4);

    const tc = data.tool_call;
    const argsJson = JSON.stringify(tc.arguments, null, 2);

    // Show what the tool would return
    const toolResult = guidedGetToolResult(tc, scenarioId || '');
    const resultJson = toolResult ? JSON.stringify(toolResult, null, 2) : null;

    return `
        <div class="guided-response-pane${paneClass}">
            <div class="guided-pane-header">
                <div class="guided-pane-title">
                    <span class="guided-pane-indicator ${indicatorClass}"></span>
                    ${title}
                </div>
                <div class="guided-pane-badges">
                    <span class="guided-pane-badge">${formatLatency(data.latency_ms)}</span>
                    <span class="guided-pane-badge">${costFmt}</span>
                </div>
            </div>
            <div class="guided-tool-call-display">
                <div class="guided-tool-call-name">
                    <span class="guided-tool-label">Tool Called:</span>
                    <span class="guided-tool-value">${tc.name}</span>
                </div>
                <div class="guided-tool-call-args">
                    <span class="guided-tool-label">Arguments:</span>
                    <pre class="guided-tool-args-json">${escapeHtml(argsJson)}</pre>
                </div>
            </div>
            ${resultJson ? `
            <details class="guided-reasoning-toggle">
                <summary class="guided-reasoning-summary">Show Tool Result</summary>
                <div class="guided-tool-result">
                    <pre class="guided-tool-args-json">${escapeHtml(resultJson)}</pre>
                </div>
            </details>
            ` : ''}
        </div>
    `;
}

// ============================================================
// TRACE ANIMATION
// ============================================================

function guidedRunTraceAnimation() {
    const method = guidedDemoState.method;
    const mockData = getMockResponse(guidedDemoState.scenario, method);
    const traceContent = document.getElementById('guidedTraceContent');
    const numCandidates = mockData.trace.candidates.length;

    const isSC = method === 'self_consistency';
    const isToolVoting = !!(mockData.trace.tool_voting);
    const scenario = GUIDED_SCENARIOS[guidedDemoState.scenario];
    let explainerText, conclusionText;

    if (isToolVoting) {
        const toolVoting = mockData.trace.tool_voting;
        explainerText = `The same question was sent to the same model ${numCandidates} times with access to ${scenario.availableTools.length} tools. Each call independently chose which tool to use and what arguments to pass. The most-voted tool selection wins.`;
        conclusionText = `Tool "${toolVoting.winning_tool}" won with ${toolVoting.tool_counts[toolVoting.winning_tool]} out of ${toolVoting.total_tool_calls} votes. By voting across multiple attempts, ITS ensures the correct tool is selected even when individual calls are unreliable.`;
    } else if (isSC) {
        explainerText = `The same question was sent to the same model ${numCandidates} separate times. Each call may reason differently and arrive at a different answer. The most common answer wins — errors tend to be random, but correct reasoning converges.`;
        conclusionText = 'The answer with the most votes is selected as the final output. This is the same model, with no changes — just smarter use of inference.';
    } else {
        explainerText = `The same question was sent to the same model ${numCandidates} separate times. A separate LLM judge scored each response on quality criteria, and the highest-scored response was selected.`;
        conclusionText = 'The highest-scored response becomes the final output. The model was never retrained — ITS simply chose the best of several attempts.';
    }

    // Build candidate rows with answer/score and expandable reasoning
    let candidateRowsHtml = '';

    if (isToolVoting) {
        // Tool voting trace — show tool name votes
        const toolVoting = mockData.trace.tool_voting;
        const winningTool = toolVoting.winning_tool;

        mockData.trace.candidates.forEach((c, i) => {
            const toolName = c.tool_calls && c.tool_calls[0] ? c.tool_calls[0].name : '(no tool)';
            const toolArgs = c.tool_calls && c.tool_calls[0]
                ? JSON.stringify(c.tool_calls[0].arguments) : '';
            const isWinner = c.is_selected;
            const isWinningTool = toolName === winningTool;
            const votesForTool = toolVoting.tool_counts[toolName] || 0;
            const argsPreview = toolArgs.length > 50 ? toolArgs.substring(0, 50) + '...' : toolArgs;

            candidateRowsHtml += `
                <div class="guided-trace-row${isWinner ? ' selected' : ''}${isWinningTool ? ' winning-answer' : ''}" data-candidate-idx="${i}">
                    <div class="guided-trace-row-summary">
                        <span class="guided-trace-row-label">Candidate ${i + 1}${isWinner ? ' ✓' : ''}</span>
                        <span class="guided-trace-row-answer guided-tool-trace-name">${toolName}</span>
                        <span class="guided-trace-row-metric${isWinningTool ? ' winner' : ''}">${votesForTool} vote${votesForTool !== 1 ? 's' : ''}</span>
                    </div>
                </div>
            `;
        });
    } else if (isSC) {
        // For SC: extract final answer from each candidate and show vote-style
        const voteMap = {}; // answer → [candidate indices]
        mockData.trace.candidates.forEach((c, i) => {
            const extracted = guidedExtractFinalAnswer(c.content);
            const answerText = extracted.answer.replace(/\$\$/g, '').replace(/\\boxed\{|\}/g, '').trim() || '(no answer)';
            if (!voteMap[answerText]) voteMap[answerText] = [];
            voteMap[answerText].push(i);
        });
        // Sort by vote count descending
        const sortedAnswers = Object.entries(voteMap).sort((a, b) => b[1].length - a[1].length);
        const maxVotes = sortedAnswers[0][1].length;
        const winnerAnswer = sortedAnswers[0][0];

        mockData.trace.candidates.forEach((c, i) => {
            const extracted = guidedExtractFinalAnswer(c.content);
            const answerText = extracted.answer.replace(/\$\$/g, '').replace(/\\boxed\{|\}/g, '').trim() || '(no answer)';
            const isWinner = c.is_selected;
            const votesForThis = voteMap[answerText] ? voteMap[answerText].length : 0;
            const isWinningAnswer = answerText === winnerAnswer;

            candidateRowsHtml += `
                <div class="guided-trace-row${isWinner ? ' selected' : ''}${isWinningAnswer ? ' winning-answer' : ''}" data-candidate-idx="${i}">
                    <div class="guided-trace-row-summary">
                        <span class="guided-trace-row-label">Candidate ${i + 1}${isWinner ? ' ✓' : ''}</span>
                        <span class="guided-trace-row-answer">${answerText}</span>
                        <span class="guided-trace-row-metric${isWinningAnswer ? ' winner' : ''}">${votesForThis} vote${votesForThis !== 1 ? 's' : ''}</span>
                    </div>
                </div>
            `;
        });
    } else if (mockData.trace.scores) {
        // For BoN: show score for each candidate
        const maxScore = Math.max(...mockData.trace.scores);

        mockData.trace.candidates.forEach((c, i) => {
            const score = mockData.trace.scores[i];
            const isWinner = c.is_selected;
            const isTopScore = score === maxScore;

            candidateRowsHtml += `
                <div class="guided-trace-row${isWinner ? ' selected' : ''}" data-candidate-idx="${i}">
                    <div class="guided-trace-row-summary">
                        <span class="guided-trace-row-label">Candidate ${i + 1}${isWinner ? ' ✓' : ''}</span>
                        <div class="guided-trace-row-bar-container">
                            <div class="guided-trace-row-bar${isTopScore ? ' top-score' : ''}" style="width: ${(score / 10) * 100}%"></div>
                        </div>
                        <span class="guided-trace-row-metric${isTopScore ? ' winner' : ''}">${score.toFixed(1)}/10</span>
                    </div>
                </div>
            `;
        });
    }

    let html = `
        <div class="guided-trace-animation">
            <div class="guided-trace-phase" id="tracePhase1">
                <p class="guided-trace-explainer" style="margin-bottom: 16px;">${explainerText}</p>
                <div class="guided-trace-candidates-list">
                    ${candidateRowsHtml}
                </div>
                <div class="guided-trace-candidate-detail" id="traceCandidateDetail">
                    <p style="font-size: 12px; color: var(--text-tertiary); font-style: italic; padding: 12px;">Click any candidate above to view its full reasoning</p>
                </div>
            </div>
            <div class="guided-trace-phase" id="tracePhase2">
                <div style="padding: 16px; background: rgba(163, 190, 140, 0.08); border: 2px solid #a3be8c;">
                    <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 8px;">
                        <span style="font-size: 20px;">✓</span>
                        <span style="font-size: 14px; font-weight: 700; color: #a3be8c; text-transform: uppercase; letter-spacing: 0.05em;">
                            ${isToolVoting ? 'Tool Consensus Winner' : (isSC ? 'Majority Vote Winner' : 'Highest Scored Response')}
                        </span>
                    </div>
                    <p style="font-size: 13px; color: var(--text-secondary); line-height: 1.6;">${conclusionText}</p>
                </div>
            </div>
        </div>
    `;

    traceContent.innerHTML = html;

    // Candidate row click handler — show reasoning
    traceContent.querySelectorAll('.guided-trace-row').forEach(row => {
        row.addEventListener('click', function() {
            const idx = parseInt(this.dataset.candidateIdx);
            const candidate = mockData.trace.candidates[idx];
            const detailEl = document.getElementById('traceCandidateDetail');
            if (!candidate || !detailEl) return;

            // Highlight selected row
            traceContent.querySelectorAll('.guided-trace-row').forEach(r => r.classList.remove('active'));
            this.classList.add('active');

            // Format candidate content
            const processed = guidedPreprocessLatex(candidate.content);
            const candidateHtml = typeof formatAsHTML === 'function'
                ? formatAsHTML(processed)
                : '<p>' + processed.replace(/\n\n/g, '</p><p>').replace(/\n/g, '<br>') + '</p>';

            const selectedLabel = candidate.is_selected
                ? '<span style="color: #a3be8c; font-weight: 700; margin-left: 8px;">← Selected by ITS</span>'
                : '';

            detailEl.innerHTML = `
                <div class="guided-trace-candidate-header">
                    Candidate ${idx + 1} ${selectedLabel}
                </div>
                <div class="guided-trace-candidate-content">${candidateHtml}</div>
            `;

            if (typeof renderMath === 'function') renderMath(detailEl);
        });
    });

    // Animate: show candidates list, then conclusion
    const phases = ['tracePhase1', 'tracePhase2'];
    phases.forEach((id, i) => {
        setTimeout(() => {
            const phase = document.getElementById(id);
            if (phase) phase.classList.add('visible');

            if (i === phases.length - 1) {
                setTimeout(() => {
                    const nextArea = document.getElementById('guidedNextArea');
                    setVisible(nextArea, true);
                }, 600);
            }
        }, (i + 1) * 600);
    });
}

// ============================================================
// STEP 6: PERFORMANCE PAGE
// ============================================================

function guidedShowPerformance() {
    guidedShowStep(6);
    guidedRenderPerformance();
}

function guidedRenderPerformance() {
    const scenario = GUIDED_SCENARIOS[guidedDemoState.scenario];
    const method = guidedDemoState.method;
    const perf = getMockPerformance(guidedDemoState.scenario, method);
    const isMatchFrontier = scenario.goal === 'match_frontier';
    const methodLabel = scenario.goal === 'tool_calling' ? 'Self-Consistency (Tool Voting)' : (method === 'self_consistency' ? 'Self-Consistency' : 'Best-of-N');

    // --- Summary bar ---
    const summaryEl = document.getElementById('guidedPerfSummary');
    let summaryItems = '';
    if (isMatchFrontier) {
        summaryItems = `
            <div class="guided-perf-summary-item">
                <div class="guided-perf-summary-label">Small Model</div>
                <div class="guided-perf-summary-value">${scenario.smallModel}</div>
            </div>
            <div class="guided-perf-summary-item">
                <div class="guided-perf-summary-label">Frontier Model</div>
                <div class="guided-perf-summary-value">${scenario.frontierModel}</div>
            </div>
        `;
    } else {
        summaryItems = `
            <div class="guided-perf-summary-item">
                <div class="guided-perf-summary-label">Model</div>
                <div class="guided-perf-summary-value">${scenario.model}</div>
            </div>
        `;
    }
    // Determine budget from trace data
    const mockResp = getMockResponse(guidedDemoState.scenario, method);
    const budget = mockResp.trace?.candidates?.length || 8;
    const budgetValue = `${budget} candidate${budget !== 1 ? 's' : ''}`;

    summaryItems += `
        <div class="guided-perf-summary-item">
            <div class="guided-perf-summary-label">ITS Method</div>
            <div class="guided-perf-summary-value">${methodLabel}</div>
        </div>
        <div class="guided-perf-summary-item">
            <div class="guided-perf-summary-label">Budget</div>
            <div class="guided-perf-summary-value">${budgetValue}</div>
        </div>
    `;
    summaryEl.innerHTML = summaryItems;

    // --- Charts ---
    const chartsEl = document.getElementById('guidedPerfCharts');
    chartsEl.innerHTML = '';

    // Cost chart
    chartsEl.innerHTML += guidedBuildChart('Cost', perf.columns, perf.cost, 'lower',
        v => v < 0.0001 ? '$' + v.toExponential(2) : '$' + v.toFixed(4)
    );

    // Latency chart
    chartsEl.innerHTML += guidedBuildChart('Latency', perf.columns, perf.latency, 'lower',
        v => Math.round(v).toLocaleString() + ' ms'
    );

    // Tokens chart
    chartsEl.innerHTML += guidedBuildChart('Total Tokens', perf.columns, perf.tokens, 'lower',
        v => v.toLocaleString()
    );

    // Takeaway statement
    chartsEl.innerHTML += guidedBuildTakeaway(guidedDemoState.scenario, method);

    // Raw data viewer + restart buttons
    const rawData = getMockResponse(guidedDemoState.scenario, method);
    const rawJson = JSON.stringify(rawData, null, 2);

    chartsEl.innerHTML += `
        <div class="guided-restart-area" style="grid-column: 1 / -1;">
            <div class="guided-raw-data-section">
                <details class="guided-raw-data-toggle">
                    <summary class="guided-raw-data-btn">
                        View Raw API Response
                    </summary>
                    <div class="guided-raw-data-content">
                        <div class="guided-raw-data-header">
                            <span>Scenario: <strong>${guidedDemoState.scenario}_${method}</strong></span>
                            <button class="guided-raw-data-copy" onclick="navigator.clipboard.writeText(this.closest('.guided-raw-data-content').querySelector('pre').textContent).then(() => { this.textContent = 'Copied!'; setTimeout(() => this.textContent = 'Copy JSON', 1500); })">Copy JSON</button>
                        </div>
                        <pre class="guided-raw-data-json">${escapeHtml(rawJson)}</pre>
                    </div>
                </details>
            </div>
            <div style="margin-top: 24px;">
                <button class="btn-secondary" onclick="initGuidedWizard()" style="margin-right: 12px;">
                    ← Start New Demo
                </button>
                <button class="btn-secondary" onclick="returnToLanding()">
                    Back to Home
                </button>
            </div>
        </div>
    `;
}

function guidedBuildChart(title, columns, values, betterWhen, formatter) {
    const maxVal = Math.max(...values);
    const minVal = Math.min(...values);
    const barColors = ['baseline', 'its', 'frontier'];

    let barsHtml = '';
    columns.forEach((label, i) => {
        const pct = maxVal > 0 ? (values[i] / maxVal) * 100 : 0;
        const isBest = betterWhen === 'lower'
            ? values[i] === minVal
            : values[i] === maxVal;

        barsHtml += `
            <div class="guided-chart-bar-row">
                <span class="guided-bar-label">${label}</span>
                <div class="guided-bar-track">
                    <div class="guided-bar-fill ${barColors[i] || 'baseline'}${isBest ? ' best' : ''}"
                         style="width: ${pct}%"></div>
                </div>
                <span class="guided-bar-value${isBest ? ' best' : ''}">${isBest ? '✓ ' : ''}${formatter(values[i])}</span>
            </div>
        `;
    });

    return `
        <div class="guided-perf-chart">
            <div class="guided-chart-title">${title}</div>
            <div class="guided-chart-bars">${barsHtml}</div>
        </div>
    `;
}

// ============================================================
// PERFORMANCE TAKEAWAY
// ============================================================

function guidedBuildTakeaway(scenarioId, method) {
    const scenario = GUIDED_SCENARIOS[scenarioId];
    const mockResp = getMockResponse(scenarioId, method);
    const goal = scenario.goal;

    let message = '';

    if (goal === 'tool_calling') {
        const bTool = mockResp.baseline.tool_call?.name || 'unknown';
        const iTool = mockResp.its.tool_call?.name || 'unknown';
        if (bTool !== iTool) {
            message = `The baseline selected <strong>${bTool}</strong>, a general-purpose tool that returns unstructured results. ITS consensus voting corrected this to <strong>${iTool}</strong>, the structured API that returns reliable, machine-readable data. This demonstrates how ITS improves agent reliability in tool selection.`;
        } else {
            message = `Both baseline and ITS selected <strong>${iTool}</strong>. ITS consensus voting confirmed the correct tool choice with high confidence.`;
        }
    } else if (goal === 'match_frontier') {
        const itsCost = mockResp.its.cost_usd;
        const frontierCost = mockResp.frontier.cost_usd;
        const savings = Math.round((1 - itsCost / frontierCost) * 100);
        const smallModel = scenario.smallModel;
        const frontierModel = scenario.frontierModel;

        message = `<strong>${smallModel} + ITS</strong> matched <strong>${frontierModel}</strong> quality while costing <strong>${savings}% less</strong>. `;
        message += `This shows you can achieve frontier-level results using a smaller, cheaper model enhanced with inference-time scaling — without upgrading to a more expensive model.`;
    } else {
        // improve_performance
        const model = scenario.model;
        const costRatio = (mockResp.its.cost_usd / mockResp.baseline.cost_usd).toFixed(1);
        const methodName = method === 'self_consistency' ? 'Self-Consistency voting' : 'Best-of-N selection';

        message = `ITS corrected <strong>${model}</strong>'s wrong answer using ${methodName} at <strong>${costRatio}x</strong> the baseline cost. `;
        message += `The baseline produced an incorrect result in a single pass, but generating multiple candidates and selecting the best one recovered the correct answer — a small cost increase for a significant accuracy improvement.`;
    }

    return `
        <div class="guided-takeaway" style="grid-column: 1 / -1;">
            <div class="guided-takeaway-label">KEY TAKEAWAY</div>
            <p class="guided-takeaway-text">${message}</p>
        </div>
    `;
}

// ============================================================
// LIFECYCLE EVENT LISTENERS — replaces function patching
// ============================================================

// Reset guided demo state when returning to landing
document.addEventListener('experience:teardown', function() {
    guidedDemoState.goal = null;
    guidedDemoState.method = null;
    guidedDemoState.scenario = null;
    guidedDemoState.currentStep = 1;
});

// Initialize guided wizard when guided mode is selected
document.addEventListener('experience:selected', function(e) {
    if (e.detail && e.detail.experience === 'guided') {
        initGuidedWizard();
    }
});
