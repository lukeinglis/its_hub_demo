/**
 * Guided Demo Flow - Step-by-step ITS demonstration
 *
 * Flow: Goal → Method → Scenario → Prompt/Submit → Responses/Trace → Performance
 *
 * WHERE TO PLUG IN REAL DATA LATER:
 * ──────────────────────────────────
 * 1. GUIDED_MOCK_QUESTIONS  — Replace placeholder questions per scenario+method
 * 2. getMockResponse()      — Replace the fallback generator with real captured data
 * 3. guidedRunTraceAnimation() — Enhance with real algorithm trace output
 * 4. getMockPerformance()   — Replace with real cost/latency/token metrics
 *
 * The GUIDED_SCENARIOS object defines the branching structure (goals → scenarios).
 * You can add/remove scenarios there without changing any other code.
 */

// ============================================================
// STATE
// ============================================================

function guidedFormatLatency(ms) {
    if (ms == null) return 'N/A';
    if (ms >= 1000) return (ms / 1000).toFixed(1) + 's';
    return ms + 'ms';
}

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
        title: 'Frontier Model',
        subtitle: 'Enhance a leading commercial model',
        icon: '🌟',
        model: 'GPT-4o',
        provider: 'OpenAI',
        description: 'Even frontier models benefit from ITS — multiple reasoning paths reduce errors.',
    },
    improve_opensource: {
        id: 'improve_opensource',
        goal: 'improve_performance',
        title: 'Open Source Model',
        subtitle: 'Enhance an open-source model',
        icon: '🔓',
        model: 'Llama 3 8B',
        provider: 'Meta',
        description: 'Open-source models see significant accuracy gains from ITS techniques.',
    },
    match_same_family: {
        id: 'match_same_family',
        goal: 'match_frontier',
        title: 'Same Model Family',
        subtitle: 'Small model matching a larger model in the same family',
        icon: '👨‍👦',
        smallModel: 'GPT-4o mini',
        frontierModel: 'GPT-4o',
        provider: 'OpenAI',
        description: 'GPT-4o mini + ITS can match GPT-4o quality at a fraction of the cost.',
    },
    match_cross_family: {
        id: 'match_cross_family',
        goal: 'match_frontier',
        title: 'Cross-Family Match',
        subtitle: 'Small open-source model matching a larger frontier model',
        icon: '🔀',
        smallModel: 'Llama 3 8B',
        frontierModel: 'GPT-4o',
        provider: 'Meta / OpenAI',
        description: 'A small open-source model + ITS competing with a large frontier model.',
    },
};

// ============================================================
// MOCK QUESTIONS — Replace with real demo questions later
// Key format: `${scenarioId}_${method}`
// ============================================================

const GUIDED_MOCK_QUESTIONS = {
    'improve_frontier_self_consistency': 'What is 144 / 12 + 7 * 3 - 5?',
    'improve_frontier_best_of_n': 'Explain the key differences between TCP and UDP protocols, and when you would choose each one.',
    'improve_opensource_self_consistency': 'If a train travels 120 km in 1.5 hours, what is its average speed in km/h?',
    'improve_opensource_best_of_n': 'What are the three laws of thermodynamics? Explain each briefly.',
    'match_same_family_self_consistency': 'A store has a 20% off sale. If an item originally costs $85, what is the sale price?',
    'match_same_family_best_of_n': 'Compare and contrast supervised and unsupervised machine learning. Give one example of each.',
    'match_cross_family_self_consistency': 'What is the sum of the first 10 prime numbers?',
    'match_cross_family_best_of_n': 'Explain why the sky is blue using the concept of Rayleigh scattering.',
};

// ============================================================
// MOCK RESPONSES — Replace with real captured responses later
// ============================================================

function getMockResponse(scenarioId, method) {
    const scenario = GUIDED_SCENARIOS[scenarioId];
    const isMatchFrontier = scenario.goal === 'match_frontier';

    // --- Self-Consistency mocks ---
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

    // --- Best-of-N mocks ---
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
// MOCK PERFORMANCE DATA — Replace with real metrics later
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
            quality: [60, 92, 95],
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
        quality: [70, 94],
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
    wizard.classList.remove('hidden');
    wizard.style.display = 'block';

    // Hide all other sections
    ['useCaseSection', 'scenarioSection', 'configSection', 'questionSection',
     'errorContainer', 'expectedAnswerContainer', 'resultsContainer',
     'performance-visualization-container'].forEach(id => {
        const el = document.getElementById(id);
        if (el) el.style.display = 'none';
    });

    // Clean up dynamic elements from previous runs
    ['performanceDetailsContainer', 'wizardResultsHeadline', 'wizardPromptDisplay'].forEach(id => {
        const el = document.getElementById(id);
        if (el) {
            if (id === 'wizardPromptDisplay') el.style.display = 'none';
            else el.remove();
        }
    });

    // Hide old guided badge and back button (new wizard has its own UI)
    const badge = document.getElementById('guidedDemoBadge');
    if (badge) { badge.classList.add('hidden'); badge.style.display = ''; }

    const backBtn = document.getElementById('wizardBackBtn');
    if (backBtn) backBtn.style.display = 'none';

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
        if (el) { el.classList.add('hidden'); el.style.display = ''; }
    }

    // Show current step
    const currentEl = document.getElementById('guidedStep' + stepNumber);
    if (currentEl) {
        currentEl.classList.remove('hidden');
        currentEl.style.display = 'block';
        // Re-trigger animation
        currentEl.style.animation = 'none';
        currentEl.offsetHeight; // force reflow
        currentEl.style.animation = '';
    }

    // Update progress bar
    const progressBar = document.getElementById('guidedProgressBar');
    if (progressBar) {
        progressBar.style.width = ((stepNumber / 6) * 100) + '%';
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
// STEP 1: GOAL SELECTION
// ============================================================

function guidedSelectGoal(goal) {
    guidedDemoState.goal = goal;

    // Update card selection
    document.querySelectorAll('#guidedStep1 .guided-card').forEach(card => {
        card.classList.toggle('selected', card.dataset.goal === goal);
    });

    setTimeout(() => guidedShowStep(2), 250);
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

    if (goal === 'improve_performance') {
        subtitle.textContent = 'Which type of model do you want to improve?';
        renderScenarioCard(container, GUIDED_SCENARIOS.improve_frontier);
        renderScenarioCard(container, GUIDED_SCENARIOS.improve_opensource);
    } else {
        subtitle.textContent = 'Choose a model-matching scenario';
        renderScenarioCard(container, GUIDED_SCENARIOS.match_same_family);
        renderScenarioCard(container, GUIDED_SCENARIOS.match_cross_family);
    }
}

function renderScenarioCard(container, scenario) {
    const card = document.createElement('div');
    card.className = 'guided-card';
    card.dataset.scenario = scenario.id;

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
        <div class="guided-card-icon">${scenario.icon}</div>
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
    const goalLabel = guidedDemoState.goal === 'improve_performance'
        ? 'Improve Model Performance'
        : 'Match Frontier Model';
    const methodLabel = method === 'self_consistency' ? 'Self-Consistency' : 'Best-of-N';

    // Summary tags
    const summaryEl = document.getElementById('guidedScenarioSummary');
    let modelTag = '';
    if (scenario.goal === 'match_frontier') {
        modelTag = `
            <span class="guided-summary-tag">
                <span class="tag-label">Small:</span>
                <span class="tag-value">${scenario.smallModel}</span>
            </span>
            <span class="guided-summary-tag">
                <span class="tag-label">Frontier:</span>
                <span class="tag-value">${scenario.frontierModel}</span>
            </span>
        `;
    } else {
        modelTag = `
            <span class="guided-summary-tag">
                <span class="tag-label">Model:</span>
                <span class="tag-value">${scenario.model}</span>
            </span>
        `;
    }

    summaryEl.innerHTML = `
        <span class="guided-summary-tag">
            <span class="tag-label">Goal:</span>
            <span class="tag-value">${goalLabel}</span>
        </span>
        <span class="guided-summary-tag">
            <span class="tag-label">Method:</span>
            <span class="tag-value">${methodLabel}</span>
        </span>
        ${modelTag}
    `;

    // Prompt text
    const key = `${guidedDemoState.scenario}_${method}`;
    const question = GUIDED_MOCK_QUESTIONS[key] || 'Placeholder question — replace with real demo question';
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

    // Simulate processing delay
    setTimeout(() => {
        guidedRenderResponses();
        guidedShowStep(5);
    }, 800);
}

function guidedRenderResponses() {
    const scenario = GUIDED_SCENARIOS[guidedDemoState.scenario];
    const method = guidedDemoState.method;
    const mockData = getMockResponse(guidedDemoState.scenario, method);
    const isMatchFrontier = scenario.goal === 'match_frontier';

    const container = document.getElementById('guidedResponses');
    container.className = 'guided-responses ' + (isMatchFrontier ? 'three-col' : 'two-col');
    container.innerHTML = '';

    // Baseline pane
    const baselineLabel = isMatchFrontier
        ? (scenario.smallModel + ' (Baseline)')
        : ((scenario.model || scenario.smallModel) + ' (Baseline)');
    container.innerHTML += guidedBuildResponsePane(
        baselineLabel, 'baseline', mockData.baseline
    );

    // ITS pane
    const itsLabel = isMatchFrontier
        ? (scenario.smallModel + ' + ITS')
        : ((scenario.model || scenario.smallModel) + ' + ITS');
    container.innerHTML += guidedBuildResponsePane(
        itsLabel, 'its', mockData.its
    );

    // Frontier pane (match_frontier only)
    if (isMatchFrontier && mockData.frontier) {
        container.innerHTML += guidedBuildResponsePane(
            scenario.frontierModel + ' (Frontier)', 'frontier', mockData.frontier
        );
    }

    // Show trace button area
    const traceArea = document.getElementById('guidedTraceArea');
    traceArea.classList.remove('hidden');
    traceArea.style.display = 'block';

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
    document.getElementById('guidedNextArea').classList.add('hidden');
    document.getElementById('guidedNextArea').style.display = '';
}

function guidedBuildResponsePane(title, type, data) {
    const indicatorClass = type === 'its' ? 'its' : type === 'frontier' ? 'frontier' : 'baseline';
    const paneClass = type === 'its' ? ' its-pane' : type === 'frontier' ? ' frontier-pane' : '';

    const costFmt = data.cost_usd < 0.0001
        ? '$' + data.cost_usd.toExponential(2)
        : '$' + data.cost_usd.toFixed(4);

    // Use formatAsHTML if available (from the main inline script)
    const responseHtml = typeof formatAsHTML === 'function'
        ? formatAsHTML(data.response)
        : '<p>' + data.response.replace(/\n\n/g, '</p><p>').replace(/\n/g, '<br>') + '</p>';

    return `
        <div class="guided-response-pane${paneClass}">
            <div class="guided-pane-header">
                <div class="guided-pane-title">
                    <span class="guided-pane-indicator ${indicatorClass}"></span>
                    ${title}
                </div>
                <div class="guided-pane-badges">
                    <span class="guided-pane-badge">${guidedFormatLatency(data.latency_ms)}</span>
                    <span class="guided-pane-badge">${costFmt}</span>
                </div>
            </div>
            <div class="guided-pane-response">${responseHtml}</div>
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

    // Build the staged trace visualization
    let html = '<div class="guided-trace-animation">';

    // Phase 1: Generate candidates
    html += `
        <div class="guided-trace-phase" id="tracePhase1">
            <div class="guided-trace-phase-label">
                <span class="phase-number">1</span>
                Generate Multiple Candidates
            </div>
            <div style="padding: 12px; background: var(--bg-primary); border: 1px solid var(--border-color);">
                <p style="font-size: 13px; color: var(--text-secondary); margin-bottom: 12px;">
                    ${mockData.trace.candidates.length} candidate responses generated in parallel
                </p>
                <div style="display: flex; flex-wrap: wrap; gap: 6px;">
    `;
    mockData.trace.candidates.forEach((c, i) => {
        const isWinner = c.is_selected;
        html += `<span style="
            display: inline-block; padding: 4px 10px; font-size: 11px;
            background: ${isWinner ? 'rgba(163, 190, 140, 0.15)' : 'var(--bg-secondary)'};
            border: 1px solid ${isWinner ? 'var(--success)' : 'var(--border-color)'};
            color: ${isWinner ? 'var(--success)' : 'var(--text-secondary)'};
        ">Candidate ${i + 1}</span>`;
    });
    html += '</div></div></div>';

    // Phase 2: Evaluate / Compare
    html += `
        <div class="guided-trace-phase" id="tracePhase2">
            <div class="guided-trace-phase-label">
                <span class="phase-number">2</span>
                ${method === 'self_consistency' ? 'Vote on Answers' : 'Score by LLM Judge'}
            </div>
            <div style="padding: 12px; background: var(--bg-primary); border: 1px solid var(--border-color);">
    `;

    if (method === 'self_consistency' && mockData.trace.vote_counts) {
        // Render voting bars
        const sortedVotes = Object.entries(mockData.trace.vote_counts).sort((a, b) => b[1] - a[1]);
        const maxVotes = sortedVotes[0][1];
        sortedVotes.forEach(([answer, count], i) => {
            const pct = (count / maxVotes) * 100;
            const isWinner = i === 0;
            html += `
                <div style="display: grid; grid-template-columns: 100px 1fr 60px; gap: 8px; align-items: center; margin-bottom: 8px;">
                    <span style="font-size: 12px; color: ${isWinner ? 'var(--success)' : 'var(--text-secondary)'}; font-weight: ${isWinner ? '700' : '400'}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">${answer}</span>
                    <div style="height: 20px; background: var(--bg-secondary); overflow: hidden;">
                        <div style="height: 100%; width: ${pct}%; background: ${isWinner ? 'var(--success)' : 'var(--bg-elevated)'}; transition: width 0.6s ease;"></div>
                    </div>
                    <span style="font-size: 12px; font-weight: 700; text-align: right; color: ${isWinner ? 'var(--success)' : 'var(--text-secondary)'};">${count} vote${count !== 1 ? 's' : ''}</span>
                </div>
            `;
        });
    } else if (mockData.trace.scores) {
        // Render score bars
        const sorted = mockData.trace.candidates.map((c, i) => ({ ...c, score: mockData.trace.scores[i] }))
            .sort((a, b) => b.score - a.score);
        const maxScore = mockData.trace.max_score;
        const minScore = mockData.trace.min_score;
        const range = maxScore - minScore || 1;
        sorted.forEach((c, i) => {
            const pct = ((c.score - minScore) / range) * 100;
            const isWinner = i === 0;
            html += `
                <div style="display: grid; grid-template-columns: 90px 1fr 60px; gap: 8px; align-items: center; margin-bottom: 6px;">
                    <span style="font-size: 12px; color: ${isWinner ? 'var(--warning)' : 'var(--text-secondary)'}; font-weight: ${isWinner ? '700' : '400'};">Candidate ${c.index + 1}${isWinner ? ' ★' : ''}</span>
                    <div style="height: 16px; background: var(--bg-secondary); overflow: hidden;">
                        <div style="height: 100%; width: ${pct}%; background: ${isWinner ? 'var(--warning)' : 'var(--bg-elevated)'}; transition: width 0.6s ease;"></div>
                    </div>
                    <span style="font-size: 12px; font-weight: 700; text-align: right; font-family: 'IBM Plex Mono', monospace; color: ${isWinner ? 'var(--warning)' : 'var(--text-secondary)'};">${c.score.toFixed(2)}</span>
                </div>
            `;
        });
    }
    html += '</div></div>';

    // Phase 3: Select best
    html += `
        <div class="guided-trace-phase" id="tracePhase3">
            <div class="guided-trace-phase-label">
                <span class="phase-number">3</span>
                Select Best Answer
            </div>
            <div style="padding: 16px; background: rgba(163, 190, 140, 0.08); border: 2px solid var(--success);">
                <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 8px;">
                    <span style="font-size: 20px;">✓</span>
                    <span style="font-size: 14px; font-weight: 700; color: var(--success); text-transform: uppercase; letter-spacing: 0.05em;">
                        ${method === 'self_consistency' ? 'Majority Vote Winner' : 'Highest Scored Response'}
                    </span>
                </div>
                <p style="font-size: 13px; color: var(--text-secondary); line-height: 1.6;">
                    ${method === 'self_consistency'
                        ? 'The answer with the most votes was selected, ensuring reliability through consensus.'
                        : 'The response with the highest quality score from the LLM judge was selected.'}
                </p>
            </div>
        </div>
    `;

    html += '</div>';
    traceContent.innerHTML = html;

    // Animate phases sequentially
    const phases = ['tracePhase1', 'tracePhase2', 'tracePhase3'];
    phases.forEach((id, i) => {
        setTimeout(() => {
            const phase = document.getElementById(id);
            if (phase) phase.classList.add('visible');

            // After last phase, show the performance button
            if (i === phases.length - 1) {
                setTimeout(() => {
                    const nextArea = document.getElementById('guidedNextArea');
                    nextArea.classList.remove('hidden');
                    nextArea.style.display = 'block';
                }, 600);
            }
        }, (i + 1) * 800);
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
    const methodLabel = method === 'self_consistency' ? 'Self-Consistency' : 'Best-of-N';

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
    summaryItems += `
        <div class="guided-perf-summary-item">
            <div class="guided-perf-summary-label">ITS Method</div>
            <div class="guided-perf-summary-value">${methodLabel}</div>
        </div>
        <div class="guided-perf-summary-item">
            <div class="guided-perf-summary-label">Budget</div>
            <div class="guided-perf-summary-value">8 candidates</div>
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

    // Quality chart
    chartsEl.innerHTML += guidedBuildChart('Quality Score', perf.columns, perf.quality, 'higher',
        v => v + '%'
    );

    // Latency chart
    chartsEl.innerHTML += guidedBuildChart('Latency', perf.columns, perf.latency, 'lower',
        v => Math.round(v).toLocaleString() + ' ms'
    );

    // Tokens chart
    chartsEl.innerHTML += guidedBuildChart('Total Tokens', perf.columns, perf.tokens, 'lower',
        v => v.toLocaleString()
    );

    // Restart button
    chartsEl.innerHTML += `
        <div class="guided-restart-area" style="grid-column: 1 / -1;">
            <button class="btn-secondary" onclick="initGuidedWizard()" style="margin-right: 12px;">
                ← Start New Demo
            </button>
            <button class="btn-secondary" onclick="returnToLanding()">
                Back to Home
            </button>
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
// CLEANUP for returnToLanding — patch the existing function
// ============================================================

const _originalReturnToLanding = typeof returnToLanding === 'function' ? returnToLanding : null;

// We'll patch returnToLanding after the inline script has defined it.
// This is done in the DOMContentLoaded handler below.
document.addEventListener('DOMContentLoaded', function() {
    if (typeof window.returnToLanding === 'function') {
        const original = window.returnToLanding;
        window.returnToLanding = function() {
            // Reset guided demo state
            guidedDemoState.goal = null;
            guidedDemoState.method = null;
            guidedDemoState.scenario = null;
            guidedDemoState.currentStep = 1;

            // Call original cleanup
            original();
        };
    }
});
