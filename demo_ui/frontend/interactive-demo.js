/**
 * Interactive Demo Flow — Live ITS demonstration with provider detection
 *
 * Flow: Provider Access → Detection → Scenario → Configure & Run → Results
 *
 * WHERE TO PLUG IN REAL DATA LATER:
 * ──────────────────────────────────
 * 1. IW_PROVIDER_DOCS       — Update setup instructions per provider
 * 2. IW_CURATED_PROMPTS     — Replace placeholder questions with real curated ones
 * 3. iwBuildRequest()       — Adjust request body for new use cases or parameters
 * 4. iwRenderResultPane()   — Customize answer presentation / judge integration
 * 5. iwRenderPerformance()  — Add real quality/judge scores
 */

function iwEscapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = String(text);
    return div.innerHTML;
}

// iwFormatLatency is now the global formatLatency() in app.js
function iwFormatLatency(ms) { return formatLatency(ms); }

// ============================================================
// STATE
// ============================================================

const iwState = {
    currentStep: 1,
    providers: {},       // from /providers API
    models: [],          // from /models API
    scenario: null,      // 'improve_performance' or 'match_frontier'
    modelId: null,
    frontierModelId: null,
    algorithm: 'self_consistency',
    budget: 4,
    question: null,
    expectedAnswer: null,
    isRunning: false,
    lastResults: null,
};

// ============================================================
// CURATED PROMPTS — Replace with real curated questions later
// Key format: `${scenario}_${algorithm}`
// ============================================================

const IW_CURATED_PROMPTS = {
    'improve_performance_self_consistency': [
        { q: 'What is 144 / 12 + 7 * 3 - 5?', a: '28' },
        { q: 'If a train travels 120 km in 1.5 hours, what is its average speed in km/h?', a: '80 km/h' },
        { q: 'What is the probability of rolling a sum of 7 with two standard dice?', a: '6/36 = 1/6' },
    ],
    'improve_performance_best_of_n': [
        { q: 'Explain the key differences between TCP and UDP protocols.', a: null },
        { q: 'What are the three laws of thermodynamics? Explain each briefly.', a: null },
        { q: 'Compare and contrast REST and GraphQL APIs.', a: null },
    ],
    'match_frontier_self_consistency': [
        { q: 'A store has a 20% off sale. If an item costs $85, what is the sale price?', a: '$68' },
        { q: 'What is the sum of the first 10 prime numbers?', a: '129' },
        { q: 'Solve for x: 3x + 7 = 22', a: 'x = 5' },
    ],
    'match_frontier_best_of_n': [
        { q: 'Explain why the sky is blue using Rayleigh scattering.', a: null },
        { q: 'Compare supervised and unsupervised machine learning with examples.', a: null },
        { q: 'What is the difference between a stack and a queue? Give a real-world analogy for each.', a: null },
    ],
    'improve_performance_beam_search': [
        { q: 'Find all integer solutions to the equation x² + y² = 25.', a: '(0,±5), (±3,±4), (±4,±3), (±5,0)' },
        { q: 'A sequence is defined by a₁ = 2 and aₙ = 3aₙ₋₁ - 1. What is a₄?', a: '41' },
        { q: 'How many distinct ways can you partition the number 7 into positive integers?', a: '15' },
    ],
    'improve_performance_particle_filtering': [
        { q: 'Compute the derivative of f(x) = x³ ln(x) and evaluate it at x = 1.', a: '1' },
        { q: 'What is the remainder when 2⁵⁰ is divided by 7?', a: '4' },
        { q: 'Solve the system: 2x + 3y = 12 and 4x - y = 5.', a: 'x = 27/14, y = 25/7' },
    ],
    'improve_performance_entropic_particle_filtering': [
        { q: 'How many 4-digit numbers have digits that sum to 9?', a: '165' },
        { q: 'If log₂(x) + log₂(x-2) = 3, find x.', a: '4' },
        { q: 'What is the sum of all even numbers between 1 and 100 inclusive?', a: '2550' },
    ],
    'improve_performance_particle_gibbs': [
        { q: 'Find the area enclosed by y = x² and y = 2x + 3.', a: '32/3' },
        { q: 'A bag has 5 red and 3 blue balls. Two are drawn without replacement. What is P(both red)?', a: '5/14' },
        { q: 'What is the greatest common divisor of 252 and 198?', a: '18' },
    ],
    'match_frontier_beam_search': [
        { q: 'Evaluate the integral ∫₀¹ x·eˣ dx.', a: '1' },
        { q: 'How many trailing zeros does 50! have?', a: '12' },
        { q: 'Solve for x: |2x - 5| = 3.', a: 'x = 1 or x = 4' },
    ],
    'match_frontier_particle_filtering': [
        { q: 'What is the determinant of the matrix [[1,2,3],[4,5,6],[7,8,9]]?', a: '0' },
        { q: 'Find the coefficient of x³ in the expansion of (2x + 1)⁵.', a: '80' },
        { q: 'If f(x) = sin(x)/x, what is the limit as x → 0?', a: '1' },
    ],
    'match_frontier_entropic_particle_filtering': [
        { q: 'How many non-negative integer solutions are there to x + y + z = 10?', a: '66' },
        { q: 'What is the sum of the infinite geometric series 1 + 1/3 + 1/9 + 1/27 + ...?', a: '3/2' },
        { q: 'Find the inverse of the function f(x) = (2x + 1)/(x - 3).', a: 'f⁻¹(x) = (3x + 1)/(x - 2)' },
    ],
    'match_frontier_particle_gibbs': [
        { q: 'Compute 15 choose 4.', a: '1365' },
        { q: 'What is the radius of convergence of the power series Σ xⁿ/n! ?', a: '∞' },
        { q: 'Solve the recurrence relation T(n) = 2T(n/2) + n with T(1) = 1 for n = 8.', a: '24' },
    ],
};

// ============================================================
// LIFECYCLE EVENT LISTENERS — replaces function patching
// ============================================================

// Initialize interactive wizard when interactive mode is selected
document.addEventListener('experience:selected', function(e) {
    if (e.detail && e.detail.experience === 'interactive') {
        iwInit();
    }
});

// ============================================================
// INITIALIZATION
// ============================================================

function iwInit() {
    iwState.currentStep = 1;
    iwState.providers = {};
    iwState.models = [];
    iwState.scenario = null;
    iwState.modelId = null;
    iwState.frontierModelId = null;
    iwState.algorithm = 'self_consistency';
    iwState.budget = 4;
    iwState.question = null;
    iwState.expectedAnswer = null;
    iwState.questionType = null;
    iwState.isRunning = false;
    iwState.lastResults = null;

    const wizard = document.getElementById('interactiveWizard');
    setVisible(wizard, true);

    // Hide all other sections — the wizard is the entire experience
    ['useCaseSection', 'scenarioSection', 'configSection', 'questionSection',
     'errorContainer', 'expectedAnswerContainer', 'resultsContainer',
     'performance-visualization-container'].forEach(id => {
        setVisible(document.getElementById(id), false);
    });

    // Hide elements from the original interactive UI
    setVisible(document.getElementById('expertModeToggle'), false);

    // Clear Step 1 error
    const step1Err = document.getElementById('iwStep1Error');
    if (step1Err) { step1Err.innerHTML = ''; setVisible(step1Err, false); }

    // Reset provider card visuals before detection
    document.querySelectorAll('.iw-provider-card[data-provider]').forEach(card => {
        card.classList.remove('iw-provider-active');
        const hint = card.querySelector('.iw-copy-hint');
        if (hint) { hint.textContent = 'Click to copy'; hint.classList.remove('copied'); }
    });

    iwShowStep(1);

    // Silently detect providers in the background to light up active cards
    iwDetectProvidersForCards();
}

// ============================================================
// SILENT PROVIDER DETECTION (lights up cards on step 1)
// ============================================================

async function iwDetectProvidersForCards() {
    const dot = document.getElementById('iwBackendDot');
    try {
        const resp = await fetch(API_BASE_URL + '/providers', { signal: AbortSignal.timeout(3000) });
        if (!resp.ok) {
            if (dot) dot.classList.add('offline');
            return;
        }
        const data = await resp.json();

        // Backend is reachable — update status dot
        if (dot) { dot.classList.remove('offline'); dot.classList.add('online'); }

        for (const [key, prov] of Object.entries(data.providers)) {
            const card = document.querySelector(`.iw-provider-card[data-provider="${key}"]`);
            if (!card) continue;

            if (prov.enabled) {
                card.classList.add('iw-provider-active');
                const hint = card.querySelector('.iw-copy-hint');
                if (hint) { hint.textContent = 'Active'; hint.classList.add('active-label'); }
            }
        }
    } catch (_) {
        // Backend not running — cards stay neutral, show offline dot
        if (dot) { dot.classList.remove('online'); dot.classList.add('offline'); }
    }
}

// ============================================================
// COPY SETUP SNIPPET
// ============================================================

function iwCopySetup(card) {
    const text = card.dataset.copy;
    if (!text) return;

    navigator.clipboard.writeText(text).then(() => {
        const hint = card.querySelector('.iw-copy-hint');
        if (hint) {
            hint.textContent = 'Copied!';
            hint.classList.add('copied');
            setTimeout(() => {
                hint.textContent = 'Click to copy';
                hint.classList.remove('copied');
            }, 2000);
        }
    });
}

// ============================================================
// STEP NAVIGATION
// ============================================================

function iwShowStep(n) {
    iwState.currentStep = n;
    for (let i = 1; i <= 5; i++) {
        const el = document.getElementById('iwStep' + i);
        setVisible(el, false);
    }
    const cur = document.getElementById('iwStep' + n);
    if (cur) {
        setVisible(cur, true);
        cur.style.animation = 'none';
        cur.offsetHeight;
        cur.style.animation = '';
    }

    // Progress bar
    const bar = document.getElementById('iwProgressBar');
    if (bar) {
        bar.style.width = ((n / 5) * 100) + '%';
        bar.setAttribute('aria-valuenow', n);
    }

    // Breadcrumbs
    document.querySelectorAll('.iw-crumb').forEach(c => {
        const s = parseInt(c.dataset.step);
        c.classList.remove('active', 'completed');
        if (s < n) c.classList.add('completed');
        else if (s === n) c.classList.add('active');
    });
}

function iwGoBack(toStep) {
    if (toStep <= 3) { iwState.scenario = null; }
    if (toStep <= 4) { iwState.modelId = null; iwState.frontierModelId = null; iwState.question = null; iwState.expectedAnswer = null; }
    iwShowStep(toStep);
}

// Breadcrumb click
document.addEventListener('click', function(e) {
    const crumb = e.target.closest('.iw-crumb.completed');
    if (crumb) {
        const step = parseInt(crumb.dataset.step);
        if (step && step < iwState.currentStep) iwGoBack(step);
    }
});

// ============================================================
// STEP 1: PROVIDER ACCESS PAGE
// ============================================================

// (Content is static HTML — see index.html)

// ============================================================
// STEP 2: PROVIDER DETECTION
// ============================================================

async function iwCheckProviders() {
    const step1Err = document.getElementById('iwStep1Error');
    const statusEl = document.getElementById('iwProviderStatus');
    const modelListEl = document.getElementById('iwModelList');
    const proceedBtn = document.getElementById('iwProceedBtn');

    // Clear any previous Step 1 error
    if (step1Err) { step1Err.innerHTML = ''; setVisible(step1Err, false); }

    // Health check BEFORE navigating away from Step 1
    try {
        const healthResp = await fetch(API_BASE_URL + '/health', { signal: AbortSignal.timeout(5000) });
        if (!healthResp.ok) throw new Error('Backend returned status ' + healthResp.status);
    } catch (err) {
        // Show error inline on Step 1 — do NOT navigate to Step 2
        if (step1Err) {
            step1Err.innerHTML = `
                <strong>Could not connect to the backend.</strong><br>
                Make sure the backend server is running:<br>
                <code style="display:block;margin-top:8px;padding:8px;background:var(--bg-tertiary);font-size:12px;">cd demo_ui && uvicorn backend.main:app --host 0.0.0.0 --port 8000 --reload</code>
                <div style="margin-top:8px;font-size:12px;color:var(--text-tertiary);">Error: ${iwEscapeHtml(err.message)}</div>
            `;
            setVisible(step1Err, true);
        }
        return;
    }

    // Backend is healthy — proceed to Step 2
    iwShowStep(2);
    statusEl.innerHTML = '<div class="iw-loading"><div class="spinner"></div><div class="iw-loading-text">Detecting provider credentials and available models...</div></div>';
    modelListEl.innerHTML = '';
    proceedBtn.disabled = true;

    try {
        // Provider check
        const provResp = await fetch(API_BASE_URL + '/providers');
        const provData = await provResp.json();
        iwState.providers = provData.providers;

        // Model list
        const modelsResp = await fetch(API_BASE_URL + '/models');
        const modelsData = await modelsResp.json();
        iwState.models = modelsData.models;

        // Render status
        let statusHtml = '<div class="iw-status-summary">';
        for (const [key, prov] of Object.entries(iwState.providers)) {
            const icon = prov.enabled ? '✓' : '✗';
            const cls = prov.enabled ? 'enabled' : 'disabled';
            statusHtml += `
                <div class="iw-status-item">
                    <span class="iw-status-icon" style="color: var(--${prov.enabled ? 'success' : 'text-tertiary'})">${icon}</span>
                    <span>${iwEscapeHtml(prov.name)}</span>
                    <span class="iw-provider-badge ${cls}">${prov.enabled ? 'Active' : 'Not configured'}</span>
                </div>
            `;
        }
        statusHtml += '</div>';
        statusEl.innerHTML = statusHtml;

        // Render model list
        if (iwState.models.length > 0) {
            let modelsHtml = '<h4 style="font-size:14px; margin-bottom:12px; color:var(--text-secondary);">' +
                iwState.models.length + ' models available</h4>';
            modelsHtml += '<div class="iw-model-list">';
            const providerLabels = { openai: 'OpenAI', openrouter: 'OpenRouter', vertex_ai: 'Vertex AI', local: 'Local' };
            iwState.models.forEach(m => {
                const pLabel = providerLabels[m.provider] || m.provider;
                modelsHtml += `
                    <div class="iw-model-chip">
                        <span class="iw-model-chip-provider">${iwEscapeHtml(pLabel)}</span>
                        <span class="iw-model-chip-name">${iwEscapeHtml(m.description)}</span>
                        <span class="iw-model-chip-size">${iwEscapeHtml(m.size)}</span>
                    </div>
                `;
            });
            modelsHtml += '</div>';
            modelListEl.innerHTML = modelsHtml;
            proceedBtn.disabled = false;
        } else {
            modelListEl.innerHTML = '<div class="iw-error">No models available. Please configure at least one provider in <code>demo_ui/.env</code> and restart the backend.</div>';
        }

    } catch (err) {
        statusEl.innerHTML = `
            <div class="iw-error">
                <strong>Error fetching provider data.</strong><br>
                Error: ${iwEscapeHtml(err.message)}
            </div>
        `;
        modelListEl.innerHTML = '';
    }
}

// ============================================================
// STEP 3: SCENARIO SELECTION
// ============================================================

function iwSelectScenario(scenario) {
    iwState.scenario = scenario;
    document.querySelectorAll('#iwStep3 .iw-card').forEach(c => {
        c.classList.toggle('selected', c.dataset.scenario === scenario);
    });
    setTimeout(() => {
        iwPopulateConfig();
        iwPopulatePrompts();
        iwShowStep(4);
    }, 250);
}

// ============================================================
// STEP 4: CONFIGURATION — Model, Algorithm, Budget
// ============================================================

function iwPopulateConfig() {
    const modelSelect = document.getElementById('iwModelSelect');
    const frontierGroup = document.getElementById('iwFrontierGroup');
    const frontierSelect = document.getElementById('iwFrontierSelect');
    const isMatch = iwState.scenario === 'match_frontier';

    // Populate model dropdown
    let modelHtml = '';
    const providerLabels = { openai: 'OpenAI', openrouter: 'OpenRouter', vertex_ai: 'Vertex AI', local: 'Local' };
    const grouped = {};
    iwState.models.forEach(m => {
        const g = providerLabels[m.provider] || m.provider;
        if (!grouped[g]) grouped[g] = [];
        grouped[g].push(m);
    });

    for (const [group, models] of Object.entries(grouped)) {
        modelHtml += `<optgroup label="${group}">`;
        models.forEach(m => {
            const sizeLabel = m.size ? ` [${iwEscapeHtml(m.size)}]` : '';
            modelHtml += `<option value="${iwEscapeHtml(m.id)}">${iwEscapeHtml(m.description)}${sizeLabel}</option>`;
        });
        modelHtml += '</optgroup>';
    }
    modelSelect.innerHTML = modelHtml;

    // For match_frontier, show frontier model dropdown
    if (isMatch) {
        setVisible(frontierGroup, true);
        frontierSelect.innerHTML = modelHtml;
        // Pre-select a large model if available
        const gpt4o = Array.from(frontierSelect.options).find(o => o.value === 'gpt-4o');
        if (gpt4o) frontierSelect.value = 'gpt-4o';
    } else {
        setVisible(frontierGroup, false);
    }

    // Set labels
    const modelLabel = document.getElementById('iwModelLabel');
    modelLabel.textContent = isMatch ? 'Small Model' : 'Model';

    // Reset budget
    const budgetSlider = document.getElementById('iwBudget');
    const budgetValue = document.getElementById('iwBudgetValue');
    budgetSlider.value = 4;
    budgetValue.textContent = '4';
}

function iwBudgetChanged(slider) {
    document.getElementById('iwBudgetValue').textContent = slider.value;
}

// Re-populate curated prompts when algorithm changes
function iwAlgorithmChanged() {
    iwState.algorithm = document.getElementById('iwAlgorithm').value;
    iwPopulatePrompts();
}

function iwPopulatePrompts() {
    const select = document.getElementById('iwCuratedSelect');
    const key = `${iwState.scenario}_${iwState.algorithm}`;
    const prompts = IW_CURATED_PROMPTS[key] || [];

    let html = '<option value="">Select a curated question...</option>';
    prompts.forEach((p, i) => {
        const preview = p.q.length > 80 ? p.q.substring(0, 80) + '...' : p.q;
        html += `<option value="${i}">${preview}</option>`;
    });
    select.innerHTML = html;

    // Clear custom textarea
    document.getElementById('iwCustomTextarea').value = '';
    iwState.question = null;
    iwState.expectedAnswer = null;

    // Reset submit button
    const btn = document.getElementById('iwSubmitBtn');
    btn.disabled = false;
    btn.innerHTML = '<span>▶</span><span>Run Comparison</span>';
}

function iwCuratedChanged(select) {
    const key = `${iwState.scenario}_${iwState.algorithm}`;
    const prompts = IW_CURATED_PROMPTS[key] || [];
    const idx = parseInt(select.value);

    if (!isNaN(idx) && prompts[idx]) {
        iwState.question = prompts[idx].q;
        iwState.expectedAnswer = prompts[idx].a;
        document.getElementById('iwCustomTextarea').value = prompts[idx].q;
    }
}

function iwCustomChanged(textarea) {
    if (textarea.value.trim()) {
        iwState.question = textarea.value.trim();
        iwState.expectedAnswer = null; // No expected answer for custom prompts
        document.getElementById('iwCuratedSelect').value = '';
    }
}

// ============================================================
// STEP 6: LIVE EXECUTION
// ============================================================

async function iwSubmit() {
    const question = document.getElementById('iwCustomTextarea').value.trim();
    if (!question) {
        alert('Please enter or select a question.');
        return;
    }
    iwState.question = question;

    // Read config from the combined step 4 form
    iwState.modelId = document.getElementById('iwModelSelect').value;
    iwState.algorithm = document.getElementById('iwAlgorithm').value;
    iwState.budget = parseInt(document.getElementById('iwBudget').value);
    if (iwState.scenario === 'match_frontier') {
        iwState.frontierModelId = document.getElementById('iwFrontierSelect').value;
    }

    const btn = document.getElementById('iwSubmitBtn');
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner" style="width:18px;height:18px;border-width:2px;margin:0;"></span><span>Running...</span>';

    // Show step 5 (results) with loading
    iwShowStep(5);
    const resultsEl = document.getElementById('iwResultsArea');
    resultsEl.innerHTML = '<div class="iw-loading"><div class="spinner"></div><div class="iw-loading-text">Running live comparison... This may take a few seconds.</div></div>';

    // Hide trace/perf sections
    setVisible(document.getElementById('iwTraceSection'), false);
    setVisible(document.getElementById('iwPerfSection'), false);

    try {
        const requestBody = {
            question: iwState.question,
            model_id: iwState.modelId,
            algorithm: iwState.algorithm,
            budget: iwState.budget,
            use_case: iwState.scenario === 'match_frontier' ? 'match_frontier' : 'improve_model',
        };

        if (iwState.scenario === 'match_frontier') {
            requestBody.frontier_model_id = iwState.frontierModelId;
        }

        const response = await fetch(API_BASE_URL + '/compare', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(requestBody),
        });

        if (!response.ok) {
            const errorData = await response.json().catch(() => ({}));
            throw new Error(errorData.detail || `Comparison failed (${response.status})`);
        }

        const data = await response.json();
        iwState.lastResults = data;
        iwRenderResults(data);

    } catch (err) {
        resultsEl.innerHTML = `<div class="iw-error"><strong>Error:</strong> ${iwEscapeHtml(err.message)}</div>`;
    } finally {
        btn.disabled = false;
        btn.innerHTML = '<span>▶</span><span>Run Comparison</span>';
        iwState.isRunning = false;
    }
}

// ============================================================
// RESULTS RENDERING
// ============================================================

function iwRenderResults(data) {
    const isMatch = iwState.scenario === 'match_frontier' && data.small_baseline;
    const resultsEl = document.getElementById('iwResultsArea');

    // Store question_type from response meta
    if (data.meta && data.meta.question_type) {
        iwState.questionType = data.meta.question_type;
    }

    try {
        // Determine visual indicator badges
        const allPanes = [];
        if (isMatch) {
            allPanes.push({ key: 'small_baseline', data: data.small_baseline });
            allPanes.push({ key: 'its', data: data.its });
            allPanes.push({ key: 'frontier', data: data.baseline });
        } else {
            allPanes.push({ key: 'baseline', data: data.baseline });
            allPanes.push({ key: 'its', data: data.its });
        }

        // Find cheapest and fastest
        let minCost = Infinity, minLatency = Infinity;
        allPanes.forEach(p => {
            if (p.data && p.data.cost_usd != null && p.data.cost_usd < minCost) minCost = p.data.cost_usd;
            if (p.data && p.data.latency_ms != null && p.data.latency_ms < minLatency) minLatency = p.data.latency_ms;
        });

        // Build question display block
        let questionHtml = '';
        if (iwState.question) {
            questionHtml = `
                <div class="iw-question-display">
                    <div class="iw-question-label">Question</div>
                    <div class="iw-question-text">${iwEscapeHtml(iwState.question)}</div>
                    ${iwState.expectedAnswer ? `
                        <div class="iw-expected-inline">
                            <span class="iw-expected-label">Expected Answer:</span>
                            <span class="iw-expected-value">${iwEscapeHtml(iwState.expectedAnswer)}</span>
                        </div>
                    ` : ''}
                </div>
            `;
        }

        const colClass = isMatch ? 'three-col' : 'two-col';
        let html = questionHtml + `<div class="iw-results-grid ${colClass}">`;

        if (isMatch) {
            html += iwBuildResultPaneSafe(data.small_baseline, 'baseline', iwGetModelDesc(iwState.modelId) + ' (Baseline)', minCost, minLatency);
            html += iwBuildResultPaneSafe(data.its, 'its', iwGetModelDesc(iwState.modelId) + ' + ITS', minCost, minLatency);
            html += iwBuildResultPaneSafe(data.baseline, 'frontier', iwGetModelDesc(iwState.frontierModelId) + ' (Frontier)', minCost, minLatency);
        } else {
            html += iwBuildResultPaneSafe(data.baseline, 'baseline', 'Baseline', minCost, minLatency);
            html += iwBuildResultPaneSafe(data.its, 'its', 'ITS Enhanced', minCost, minLatency);
        }

        html += '</div>';
        resultsEl.innerHTML = html;

        // Render math in results (including question display)
        if (typeof renderMath === 'function') renderMath(resultsEl);

    } catch (err) {
        console.error('iwRenderResults error:', err);
        resultsEl.innerHTML = `<div class="iw-error">Error rendering results: ${err.message}</div>`;
    }

    // Show trace section if ITS result has trace
    try {
        if (data.its && data.its.trace) {
            iwRenderTrace(data.its.trace);
        }
    } catch (e) { console.error('Trace section error:', e); }

    // Show performance section
    try {
        iwRenderPerformance(data);
    } catch (e) { console.error('Performance section error:', e); }
}

// Wrapper that catches errors per-pane so one bad pane doesn't break all results
function iwBuildResultPaneSafe(data, type, title, minCost, minLatency) {
    try {
        return iwBuildResultPane(data, type, title, minCost, minLatency);
    } catch (err) {
        console.error('Error building pane "' + title + '":', err);
        return `<div class="iw-result-pane"><div class="iw-pane-header"><div class="iw-pane-title">${title}</div></div><div class="iw-pane-body"><div class="iw-error">Error rendering this pane: ${err.message}</div></div></div>`;
    }
}

function iwGetModelDesc(modelId) {
    const m = iwState.models.find(m => m.id === modelId);
    return m ? m.description : modelId;
}

function iwBuildResultPane(data, type, title, minCost, minLatency) {
    if (!data) return `<div class="iw-result-pane"><div class="iw-pane-body"><p>No data available</p></div></div>`;

    const indicatorClass = type;
    const paneClass = type === 'its' ? ' its-pane' : type === 'frontier' ? ' frontier-pane' : '';

    // Badges — guard against null/undefined values
    let badges = '';
    const cost = (data.cost_usd !== null && data.cost_usd !== undefined) ? Number(data.cost_usd) : null;
    const latency = (data.latency_ms !== null && data.latency_ms !== undefined) ? Number(data.latency_ms) : null;
    if (cost != null && cost <= minCost) badges += '<span class="iw-pane-badge cheapest">Cheapest</span>';
    if (latency != null && latency <= minLatency) badges += '<span class="iw-pane-badge fastest">Fastest</span>';

    // Format cost
    const isEstimated = !!data.tokens_estimated;
    const costFmt = cost != null
        ? (isEstimated ? '~' : '') + (cost < 0.0001 ? '$' + cost.toExponential(2) : '$' + cost.toFixed(4))
        : 'N/A';

    // Response content
    const fullResponse = data.answer || data.response || '';
    const questionType = iwState.questionType || 'general';

    // Try to extract a concise final answer (works best for math)
    let finalAnswer = null;
    if (questionType === 'math' && typeof extractFinalAnswer === 'function') {
        finalAnswer = extractFinalAnswer(fullResponse);
    }

    // Build final answer callout if we found one
    let finalAnswerHtml = '';
    if (finalAnswer) {
        const hasLatex = finalAnswer.includes('$') || /[\\{}_^]/.test(finalAnswer);
        const displayAnswer = hasLatex
            ? finalAnswer
            : (finalAnswer.includes('=') || /^[-+]?\d/.test(finalAnswer))
                ? '$' + finalAnswer + '$'
                : iwEscapeHtml(finalAnswer);
        finalAnswerHtml = `
            <div class="iw-final-answer">
                <div class="iw-final-answer-label">Final Answer</div>
                <div class="iw-final-answer-content">${displayAnswer}</div>
            </div>
        `;
    }

    // Build response content (conclusion for long responses, full for short)
    const conclusion = typeof extractConclusion === 'function'
        ? extractConclusion(fullResponse) : fullResponse;
    const responseHtml = typeof formatAsHTML === 'function'
        ? formatAsHTML(finalAnswer ? conclusion : fullResponse)
        : '<p>' + fullResponse + '</p>';

    // Full reasoning (for expandable section)
    const fullHtml = typeof formatReasoningSteps === 'function' ? formatReasoningSteps(fullResponse) : '';
    const hasFullReasoning = (finalAnswer || conclusion !== fullResponse) && fullHtml;

    // Trace expandable (for ITS results)
    let traceExpandable = '';
    if (data.trace && typeof renderAlgorithmTrace === 'function') {
        try {
            // Parse trace if it arrived as a string
            const trace = typeof data.trace === 'string' ? JSON.parse(data.trace) : data.trace;
            if (trace && trace.algorithm) {
                const traceHtml = renderAlgorithmTrace(trace, true);
                const count = trace.algorithm === 'particle_gibbs'
                    ? (trace.iterations ? trace.iterations.length + ' iterations' : 'details')
                    : (trace.candidates ? trace.candidates.length + ' candidates' : 'details');
                traceExpandable = `
                    <div class="iw-expandable" aria-expanded="false" onclick="this.classList.toggle('expanded'); this.setAttribute('aria-expanded', this.classList.contains('expanded'))">
                        <button class="iw-expand-btn">
                            <span class="iw-expand-icon">▶</span>
                            Algorithm Trace (${count})
                        </button>
                        <div class="iw-expand-content">${traceHtml}</div>
                    </div>
                `;
            }
        } catch (e) {
            console.error('Trace render error in pane:', e);
        }
    }

    // Tool calls expandable
    let toolsExpandable = '';
    if (data.tool_calls && data.tool_calls.length > 0 && typeof renderToolCalls === 'function') {
        try {
            toolsExpandable = `
                <div class="iw-expandable" aria-expanded="false" onclick="this.classList.toggle('expanded'); this.setAttribute('aria-expanded', this.classList.contains('expanded'))">
                    <button class="iw-expand-btn">
                        <span class="iw-expand-icon">▶</span>
                        Tool Calls (${data.tool_calls.length})
                    </button>
                    <div class="iw-expand-content">${renderToolCalls(data.tool_calls)}</div>
                </div>
            `;
        } catch (e) {
            console.error('Tool calls render error:', e);
        }
    }

    return `
        <div class="iw-result-pane${paneClass}">
            <div class="iw-pane-header">
                <div class="iw-pane-title">
                    <span class="iw-pane-indicator ${indicatorClass}"></span>
                    ${title}
                </div>
                <div class="iw-pane-badges">${badges}</div>
            </div>
            <div class="iw-pane-body">
                <div class="iw-response-time">
                    <span class="response-time-value">${iwFormatLatency(latency)}</span>
                    <span class="response-time-label">response time</span>
                </div>
                ${finalAnswerHtml}
                <div class="iw-pane-response">${responseHtml}</div>
                <div class="iw-pane-meta">
                    <span class="iw-meta-tag"><span class="meta-label">Latency:</span><span class="meta-value">${iwFormatLatency(latency)}</span></span>
                    <span class="iw-meta-tag"><span class="meta-label">Cost${isEstimated ? ' (est.)' : ''}:</span><span class="meta-value">${costFmt}</span></span>
                    <span class="iw-meta-tag"><span class="meta-label">Tokens${isEstimated ? ' (est.)' : ''}:</span><span class="meta-value">${isEstimated ? '~' : ''}${(data.input_tokens || 0) + (data.output_tokens || 0)}</span></span>
                </div>
            </div>
            ${hasFullReasoning ? `
                <div class="iw-expandable" aria-expanded="false" onclick="this.classList.toggle('expanded'); this.setAttribute('aria-expanded', this.classList.contains('expanded'))">
                    <button class="iw-expand-btn">
                        <span class="iw-expand-icon">▶</span>
                        View Full Reasoning
                    </button>
                    <div class="iw-expand-content">${fullHtml}</div>
                </div>
            ` : ''}
            <div class="iw-expandable" aria-expanded="false" onclick="this.classList.toggle('expanded'); this.setAttribute('aria-expanded', this.classList.contains('expanded'))">
                <button class="iw-expand-btn">
                    <span class="iw-expand-icon">▶</span>
                    Performance Details
                </button>
                <div class="iw-expand-content">
                    <div style="display:grid; grid-template-columns:1fr 1fr; gap:8px; font-size:13px;">
                        <span style="color:var(--text-tertiary)">Latency</span><span style="font-family:'IBM Plex Mono',monospace">${iwFormatLatency(latency)}</span>
                        <span style="color:var(--text-tertiary)">Cost${isEstimated ? ' (est.)' : ''}</span><span style="font-family:'IBM Plex Mono',monospace">${costFmt}</span>
                        <span style="color:var(--text-tertiary)">Input Tokens${isEstimated ? ' (est.)' : ''}</span><span style="font-family:'IBM Plex Mono',monospace">${isEstimated ? '~' : ''}${(data.input_tokens || 0).toLocaleString()}</span>
                        <span style="color:var(--text-tertiary)">Output Tokens${isEstimated ? ' (est.)' : ''}</span><span style="font-family:'IBM Plex Mono',monospace">${isEstimated ? '~' : ''}${(data.output_tokens || 0).toLocaleString()}</span>
                        ${data.model_size ? `<span style="color:var(--text-tertiary)">Model Size</span><span>${iwEscapeHtml(data.model_size)}</span>` : ''}
                    </div>
                    ${isEstimated ? `<div class="iw-estimate-note">Token counts and cost are estimates. ITS algorithms make multiple internal LLM calls whose usage is not individually tracked. Latency is actual wall-clock time.</div>` : ''}
                </div>
            </div>
            ${traceExpandable}
            ${toolsExpandable}
        </div>
    `;
}

// ============================================================
// TRACE RENDERING
// ============================================================

function iwRenderTrace(traceRaw) {
    const section = document.getElementById('iwTraceSection');
    const content = document.getElementById('iwTraceContent');

    // Parse if string
    let trace = traceRaw;
    try {
        if (typeof trace === 'string') trace = JSON.parse(trace);
    } catch (_) {}

    if (!trace || !trace.algorithm) { setVisible(section, false); return; }

    setVisible(section, true);
    const algName = (ALGORITHM_DESCRIPTIONS[trace.algorithm] && ALGORITHM_DESCRIPTIONS[trace.algorithm].name) || trace.algorithm;

    const subtitle = trace.algorithm === 'particle_gibbs'
        ? `${algName} ran ${trace.iterations ? trace.iterations.length : '?'} iterations of particle filtering`
        : `${algName} evaluated ${trace.candidates ? trace.candidates.length : '?'} candidates`;
    document.getElementById('iwTraceSubtitle').textContent = subtitle;

    if (typeof renderAlgorithmTrace === 'function') {
        try {
            content.innerHTML = renderAlgorithmTrace(trace, true);
        } catch (e) {
            console.error('renderAlgorithmTrace error:', e);
            content.innerHTML = '<div class="iw-error">Could not render trace visualization.</div>';
        }
    }
}

// ============================================================
// PERFORMANCE VISUALIZATION
// ============================================================

function iwRenderPerformance(data) {
    const section = document.getElementById('iwPerfSection');
    const container = document.getElementById('iwPerfContainer');
    setVisible(section, true);

    // Use PerformanceVizV2 if available
    if (typeof PerformanceVizV2 !== 'undefined') {
        try {
            const viz = new PerformanceVizV2('iwPerfContainer');
            viz.render(data);
            return;
        } catch (e) {
            console.error('PerformanceVizV2 error:', e);
        }
    }

    // Fallback: simple table
    const isMatch = !!data.small_baseline;
    const columns = isMatch
        ? [{ label: 'Small Baseline', d: data.small_baseline }, { label: 'Small + ITS', d: data.its }, { label: 'Frontier', d: data.baseline }]
        : [{ label: 'Baseline', d: data.baseline }, { label: 'ITS Enhanced', d: data.its }];

    let html = '<table style="width:100%;border-collapse:collapse;font-size:13px;">';
    html += '<thead><tr><th style="text-align:left;padding:8px;border-bottom:2px solid var(--border-color)">Metric</th>';
    columns.forEach(c => { html += `<th style="text-align:right;padding:8px;border-bottom:2px solid var(--border-color)">${c.label}</th>`; });
    html += '</tr></thead><tbody>';

    const fmtCost = d => d.cost_usd != null ? (d.cost_usd < 0.0001 ? '$' + d.cost_usd.toExponential(2) : '$' + d.cost_usd.toFixed(4)) : 'N/A';
    const rows = [
        { label: 'Cost', fn: fmtCost },
        { label: 'Latency', fn: d => d.latency_ms != null ? Math.round(d.latency_ms) + 'ms' : 'N/A' },
        { label: 'Total Tokens', fn: d => ((d.input_tokens || 0) + (d.output_tokens || 0)).toLocaleString() },
    ];

    rows.forEach(row => {
        html += `<tr><td style="padding:8px;color:var(--text-secondary)">${row.label}</td>`;
        columns.forEach(c => {
            html += `<td style="text-align:right;padding:8px;font-family:'IBM Plex Mono',monospace">${row.fn(c.d)}</td>`;
        });
        html += '</tr>';
    });

    html += '</tbody></table>';
    container.innerHTML = html;
}

// Reset interactive wizard state when returning to landing
document.addEventListener('experience:teardown', function() {
    iwState.currentStep = 1;
    iwState.providers = {};
    iwState.models = [];
    iwState.scenario = null;
    iwState.lastResults = null;

    const wizard = document.getElementById('interactiveWizard');
    setVisible(wizard, false);
});
