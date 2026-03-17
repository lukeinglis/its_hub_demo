/**
 * app.js — Core state management for the ITS Demo
 *
 * ARCHITECTURE
 * ────────────
 * State ownership:
 *   - app.js owns global state (selectedExperience, currentUseCase)
 *   - guided-demo.js owns guidedDemoState
 *   - interactive-demo.js owns iwState
 *
 * Lifecycle events (dispatched on document):
 *   - 'experience:selected'  — detail: { experience: 'guided'|'interactive' }
 *     Fired after selectExperience() configures the main UI.
 *     Demo modules listen to initialize when their mode is selected.
 *
 *   - 'experience:teardown'  — no detail
 *     Fired at the start of returnToLanding().
 *     Demo modules listen to reset their state before the main UI tears down.
 *
 * Visibility contract:
 *   All show/hide uses setVisible(el, bool) which toggles the .hidden CSS class.
 *   No code should set el.style.display directly.
 */

const API_BASE_URL = 'http://localhost:8000';

// Shared utilities (setVisible, escapeHtml, formatLatency, formatCost, etc.)
// are defined in utils.js which is loaded before this file.

let currentExpectedAnswer = null;
let selectedAlgorithm = 'best_of_n';
let lastResults = null;
let currentUseCase = 'improve_model';
let isRunning = false; // Track if a comparison is currently running
let selectedExperience = null; // 'guided' or 'interactive'


// Algorithm descriptions
const ALGORITHM_DESCRIPTIONS = {
    best_of_n: {
        type: 'outcome',
        category: 'production',
        name: 'Best-of-N',
        description: 'Generates multiple complete responses and uses an LLM judge to select the highest quality answer.',
        useCase: 'Best for: Open-ended questions, creative tasks, general Q&A'
    },
    self_consistency: {
        type: 'outcome',
        category: 'production',
        name: 'Self-Consistency',
        description: 'Generates multiple complete responses and selects the most frequent answer through majority voting.',
        useCase: 'Best for: Questions with clear correct answers, factual queries'
    },
    beam_search: {
        type: 'process',
        category: 'research',
        name: 'Beam Search',
        description: 'Explores multiple reasoning paths simultaneously, keeping only the top-k most promising paths at each step. Requires a Process Reward Model (PRM) for step-level scoring.',
        useCase: 'Research: Requires dedicated PRM infrastructure for optimal performance'
    },
    particle_filtering: {
        type: 'process',
        category: 'research',
        name: 'Particle Filtering',
        description: 'Uses Sequential Monte Carlo sampling to maintain and evolve multiple reasoning paths, resampling based on quality scores. Requires a Process Reward Model (PRM).',
        useCase: 'Research: Best results with local PRM on GPU for fast step scoring'
    },
    entropic_particle_filtering: {
        type: 'process',
        category: 'research',
        name: 'Entropic Particle Filtering',
        description: 'Particle filtering enhanced with temperature annealing to balance exploration and exploitation over time. Requires a Process Reward Model (PRM).',
        useCase: 'Research: Experimental algorithm for hard reasoning tasks'
    },
    particle_gibbs: {
        type: 'process',
        category: 'research',
        name: 'Particle Gibbs',
        description: 'Iteratively refines solutions using particle filtering with Gibbs sampling for multiple refinement passes. Requires a Process Reward Model (PRM).',
        useCase: 'Research: Most compute-intensive, best with dedicated PRM hardware'
    }
};

// Theme management
function toggleTheme() {
    const currentTheme = document.documentElement.getAttribute('data-theme');
    const newTheme = currentTheme === 'dark' ? 'light' : 'dark';
    document.documentElement.setAttribute('data-theme', newTheme);
    localStorage.setItem('theme', newTheme);
}

// Load saved theme
const savedTheme = localStorage.getItem('theme') || 'dark';
document.documentElement.setAttribute('data-theme', savedTheme);

// Delegated keyboard handler — Enter/Space triggers click on [role="button"] elements
document.addEventListener('keydown', function(e) {
    if (e.key === 'Enter' || e.key === ' ') {
        const target = e.target;
        if (target.getAttribute('role') === 'button' && target.tagName !== 'BUTTON') {
            e.preventDefault();
            target.click();
        }
    }
});

// NOTE: The guided wizard is implemented in guided-demo.js (initGuidedWizard, guidedDemoState).

// LANDING PAGE & EXPERIENCE SELECTION

function selectExperience(experience) {
    selectedExperience = experience;
    localStorage.setItem('selectedExperience', experience);

    // Hide landing page
    setVisible(document.getElementById('landingPage'), false);

    // Show demo container
    setVisible(document.getElementById('demoContainer'), true);

    // Show back to home button
    document.getElementById('backToHomeBtn').classList.add('visible');

    // Wizard initialization is handled by experience:selected event in guided-demo.js / interactive-demo.js

    // Initialize the demo
    initializeDemo();

    // Notify demo modules
    document.dispatchEvent(new CustomEvent('experience:selected', { detail: { experience } }));
}

function returnToLanding() {
    // Notify demo modules to tear down before we reset
    document.dispatchEvent(new CustomEvent('experience:teardown'));
    // Clear selection
    selectedExperience = null;
    localStorage.removeItem('selectedExperience');

    // Hide results and guided wizard UI
    setVisible(document.getElementById('resultsContainer'), false);
    setVisible(document.getElementById('guidedWizard'), false);

    // Hide demo container
    setVisible(document.getElementById('demoContainer'), false);

    // Show landing page
    setVisible(document.getElementById('landingPage'), true);

    // Hide back button
    document.getElementById('backToHomeBtn').classList.remove('visible');

    // Clear any running state
    isRunning = false;
}

function initializeDemo() {
    // This will be called after experience selection
    showAlgorithmInfo(selectedAlgorithm);
    loadModels();
    loadExampleQuestions();
    updateUIForUseCase();

    // Skip old scenario initialization in guided mode
    // (Wizard handles its own initialization via initGuidedWizard)

    // Initialize budget slider gradient
    const budgetSlider = document.getElementById('budget');
    budgetSlider.addEventListener('input', updateBudgetSliderGradient);
    updateBudgetSliderGradient.call(budgetSlider);
}

function updateBudgetSliderGradient() {
    const value = this.value;
    const max = this.max;
    const percentage = (value / max) * 100;
    this.style.background = `linear-gradient(to right, var(--primary) 0%, var(--primary) ${percentage}%, var(--border-color) ${percentage}%, var(--border-color) 100%)`;
}

// Check if user has a saved experience preference
function checkSavedExperience() {
    const savedExperience = localStorage.getItem('selectedExperience');
    if (savedExperience) {
        // Auto-select the previously chosen experience
        selectExperience(savedExperience);
    }
    // Otherwise, show landing page (default state)
}

// Show algorithm info
function showAlgorithmInfo(algorithmKey) {
    const algo = ALGORITHM_DESCRIPTIONS[algorithmKey];
    const container = document.getElementById('algorithmInfo');

    const researchNote = algo.category === 'research'
        ? '<div style="margin-top:8px;padding:8px 12px;background:rgba(255,152,0,0.1);border-left:3px solid #ff9800;border-radius:4px;font-size:12px;color:#e65100;">Research algorithm — requires a local Process Reward Model (PRM) for optimal latency and cost. Results may vary without dedicated PRM hardware.</div>'
        : '';

    container.innerHTML = `
        <div class="algorithm-info">
            <div class="algorithm-info-header">
                <span class="algorithm-type-badge ${algo.type}">
                    ${algo.type === 'outcome' ? 'Outcome-Based' : 'Process-Based'}
                </span>
                ${algo.category === 'research' ? '<span class="algorithm-type-badge" style="background:#ff9800;">Research</span>' : ''}
                <h3>${algo.name}</h3>
            </div>
            <p>${algo.description}</p>
            <div class="use-case">${algo.useCase}</div>
            ${researchNote}
        </div>
    `;
}

// Handle use case change
function onUseCaseChange() {
    const selectedRadio = document.querySelector('input[name="useCase"]:checked');
    currentUseCase = selectedRadio.value;

    // Update UI based on use case
    updateUIForUseCase();

    // Clear results when switching use cases
    clearResults();
}

// Update UI elements based on selected use case
function updateUIForUseCase() {
    const modelGroup = document.getElementById('modelGroup');
    const frontierModelGroup = document.getElementById('frontierModelGroup');
    const modelLabel = document.getElementById('modelLabel');
    const configDescription = document.getElementById('configDescription');
    const headerSubtitle = document.getElementById('headerSubtitle');
    const resultsContainer = document.getElementById('resultsContainer');
    const smallBaselinePane = document.getElementById('smallBaselinePane');
    const middlePaneIndicator = document.getElementById('middlePaneIndicator');
    const middlePaneTitle = document.getElementById('middlePaneTitle');
    const rightPaneIndicator = document.getElementById('rightPaneIndicator');
    const rightPaneTitle = document.getElementById('rightPaneTitle');

    if (currentUseCase === 'match_frontier') {
        // Use Case 2: 3-column layout - Small, Small+ITS, Frontier
        modelLabel.textContent = 'Small Model';
        setVisible(frontierModelGroup, true);
        configDescription.textContent = 'Choose a small model to enhance with ITS and a frontier model to compare against';
        headerSubtitle.textContent = 'Demonstrate how ITS can make smaller models competitive with large frontier models';

        // Show 3-column layout
        resultsContainer.classList.add('three-column');
        setVisible(smallBaselinePane, true);

        // Update pane titles and indicators for 3-column layout
        // Left: Small Model baseline (gray)
        // Middle: Small Model + ITS (blue)
        // Right: Frontier Model baseline (green)
        middlePaneIndicator.className = 'pane-indicator its';
        middlePaneTitle.textContent = 'Small Model + ITS';
        rightPaneIndicator.className = 'pane-indicator frontier';
        rightPaneTitle.textContent = 'Frontier Model';

        // Adjust grid to show both models
        document.getElementById('modelSelectionGrid').style.gridTemplateColumns = '1fr 1fr';

        // Reload all models (no filter)
        loadModels();
    } else if (currentUseCase === 'tool_consensus') {
        // Use Case 3: 2-column layout - Single Tool Call, Tool Voting
        modelLabel.textContent = 'Model';
        setVisible(frontierModelGroup, false);
        configDescription.textContent = 'Compare single agent call vs ITS with tool voting for reliable tool selection';
        headerSubtitle.textContent = 'Demonstrate how ITS creates consensus on tool selection for reliable agent behavior';

        // Show 2-column layout
        resultsContainer.classList.remove('three-column');
        setVisible(smallBaselinePane, false);

        // Update pane titles and indicators for 2-column layout
        // Middle: Single Call (gray)
        // Right: Tool Voting (blue)
        middlePaneIndicator.className = 'pane-indicator baseline';
        middlePaneTitle.textContent = 'Single Agent Call';
        rightPaneIndicator.className = 'pane-indicator its';
        rightPaneTitle.textContent = 'ITS + Tool Voting';

        // Reset grid
        document.getElementById('modelSelectionGrid').style.gridTemplateColumns = '';

        // Reload models to show only tool-compatible models
        loadModels('tool_consensus');
    } else {
        // Use Case 1: 2-column layout - Baseline, ITS
        modelLabel.textContent = 'Model';
        setVisible(frontierModelGroup, false);
        configDescription.textContent = 'Choose your model, algorithm, and compute budget';
        headerSubtitle.textContent = 'Compare baseline inference with ITS algorithms side by side';

        // Show 2-column layout
        resultsContainer.classList.remove('three-column');
        setVisible(smallBaselinePane, false);

        // Update pane titles and indicators for 2-column layout
        // Middle: Baseline (gray)
        // Right: ITS Result (blue)
        middlePaneIndicator.className = 'pane-indicator baseline';
        middlePaneTitle.textContent = 'Baseline';
        rightPaneIndicator.className = 'pane-indicator its';
        rightPaneTitle.textContent = 'ITS Result';

        // Reset grid
        document.getElementById('modelSelectionGrid').style.gridTemplateColumns = '';

        // Reload all models (no filter)
        loadModels();
    }
}

// Handle algorithm change
function onAlgorithmChange() {
    selectedAlgorithm = document.getElementById('algorithm').value;
    showAlgorithmInfo(selectedAlgorithm);
    loadExampleQuestions();
}

// Copy to clipboard
async function copyToClipboard(type) {
    let element;
    if (type === 'smallBaseline') {
        element = document.getElementById('smallBaselineContent');
    } else if (type === 'middlePane') {
        element = document.getElementById('middlePaneContent');
    } else if (type === 'rightPane') {
        element = document.getElementById('rightPaneContent');
    }

    // Get plain text content (strips HTML tags)
    const content = element.textContent || element.innerText;

    try {
        await navigator.clipboard.writeText(content);
        // Visual feedback - find the button that triggered this
        const btn = document.querySelector(`#${type}Actions .action-btn`);
        if (!btn) return;
        const originalText = btn.textContent;
        btn.textContent = '✓';
        btn.style.background = 'var(--success)';
        btn.style.color = 'white';
        btn.style.borderColor = 'var(--success)';
        setTimeout(() => {
            btn.textContent = originalText;
            btn.style.background = '';
            btn.style.color = '';
            btn.style.borderColor = '';
        }, 1500);
    } catch (err) {
        console.error('Failed to copy:', err);
        alert('Failed to copy to clipboard');
    }
}

// Clear results
function clearResults() {
    // Clear small baseline pane
    document.getElementById('smallBaselineContent').innerHTML = `
        <div class="empty-state">
            <div class="empty-state-icon">💭</div>
            <div>Run a comparison to see results</div>
        </div>
    `;
    setVisible(document.getElementById('smallBaselineLatency'), false);
    setVisible(document.getElementById('smallBaselineActions'), false);
    setVisible(document.getElementById('smallBaselineSize'), false);
    setVisible(document.getElementById('smallBaselineCost'), false);

    // Clear middle pane
    document.getElementById('middlePaneContent').innerHTML = `
        <div class="empty-state">
            <div class="empty-state-icon">💭</div>
            <div>Run a comparison to see results</div>
        </div>
    `;
    setVisible(document.getElementById('middlePaneLatency'), false);
    setVisible(document.getElementById('middlePaneActions'), false);
    setVisible(document.getElementById('middlePaneSize'), false);
    setVisible(document.getElementById('middlePaneCost'), false);

    // Clear right pane
    document.getElementById('rightPaneContent').innerHTML = `
        <div class="empty-state">
            <div class="empty-state-icon">🚀</div>
            <div>Run a comparison to see results</div>
        </div>
    `;
    setVisible(document.getElementById('rightPaneLatency'), false);
    setVisible(document.getElementById('rightPaneActions'), false);
    setVisible(document.getElementById('rightPaneSize'), false);
    setVisible(document.getElementById('rightPaneCost'), false);

    document.getElementById('expectedAnswerContainer').classList.remove('visible');

    // Hide performance visualization
    setVisible(document.getElementById('performance-visualization-container'), false);
}

// Update budget value and slider gradient
document.getElementById('budget').addEventListener('input', (e) => {
    const value = e.target.value;
    const max = e.target.max;
    const percentage = (value / max) * 100;

    document.getElementById('budgetValue').textContent = value;

    // Update gradient
    e.target.style.background = `linear-gradient(to right, var(--primary) 0%, var(--primary) ${percentage}%, var(--border-color) ${percentage}%, var(--border-color) 100%)`;
});

// Handle example question selection
document.getElementById('exampleQuestions').addEventListener('change', (e) => {
    const selectedIndex = e.target.selectedIndex;
    if (selectedIndex > 0) {
        const selectedOption = e.target.options[selectedIndex];
        const question = selectedOption.dataset.question;
        const expectedAnswer = selectedOption.dataset.expected;

        if (question) {
            document.getElementById('question').value = question;
            currentExpectedAnswer = expectedAnswer;
            clearResults();
        }
    } else {
        currentExpectedAnswer = null;
        document.getElementById('expectedAnswerContainer').classList.remove('visible');
    }
});

// Load models
async function loadModels(useCase = null) {
    try {
        const url = useCase ? `${API_BASE_URL}/models?use_case=${useCase}` : `${API_BASE_URL}/models`;
        const response = await fetch(url);
        if (!response.ok) throw new Error('Failed to load models');

        const data = await response.json();

        // Filter models for tool_consensus to only show those supporting tools
        let models = data.models;
        if (useCase === 'tool_consensus') {
            models = models.filter(m => m.supports_tools);
        }

        if (models.length === 0 && useCase === 'tool_consensus') {
            const errorMsg = '<option value="">No models support tool calling - use OpenAI models</option>';
            document.getElementById('model').innerHTML = errorMsg;
            document.getElementById('frontierModel').innerHTML = errorMsg;
            return;
        }

        const modelHTML = models.map(model => {
            const sizeLabel = model.size ? ` [${model.size}]` : '';
            return `<option value="${model.id}">${model.description}${sizeLabel}</option>`;
        }).join('');

        // Populate both model selects
        document.getElementById('model').innerHTML = modelHTML;
        document.getElementById('frontierModel').innerHTML = modelHTML;

        // Pre-select a good default for frontier model (e.g., gpt-4o if available)
        const frontierSelect = document.getElementById('frontierModel');
        const gpt4oOption = Array.from(frontierSelect.options).find(opt => opt.value === 'gpt-4o');
        if (gpt4oOption) {
            frontierSelect.value = 'gpt-4o';
        }
    } catch (error) {
        console.error('Error loading models:', error);
        document.getElementById('model').innerHTML = '<option value="">Error loading models</option>';
        document.getElementById('frontierModel').innerHTML = '<option value="">Error loading models</option>';
    }
}

// Load example questions
async function loadExampleQuestions() {
    const exampleSelect = document.getElementById('exampleQuestions');

    try {
        const url = `${API_BASE_URL}/examples?algorithm=${selectedAlgorithm}&use_case=${currentUseCase}`;
        const response = await fetch(url);
        if (!response.ok) throw new Error('Failed to load examples');

        const data = await response.json();

        const byDifficulty = { 'Easy': [], 'Medium': [], 'Hard': [] };
        data.examples.forEach(ex => byDifficulty[ex.difficulty].push(ex));

        let optionsHTML = '<option value="">Select an example...</option>';

        ['Easy', 'Medium', 'Hard'].forEach(difficulty => {
            if (byDifficulty[difficulty].length > 0) {
                optionsHTML += `<optgroup label="${difficulty} Questions">`;
                byDifficulty[difficulty].forEach(ex => {
                    const label = `${ex.category}: ${ex.question.substring(0, 60)}${ex.question.length > 60 ? '...' : ''}`;
                    const questionAttr = ex.question.replace(/"/g, '&quot;');
                    const expectedAttr = ex.expected_answer.replace(/"/g, '&quot;');
                    optionsHTML += `<option value="${ex.question}" data-question="${questionAttr}" data-expected="${expectedAttr}" title="${ex.why}">${label}</option>`;
                });
                optionsHTML += '</optgroup>';
            }
        });

        exampleSelect.innerHTML = optionsHTML;
    } catch (error) {
        console.error('Error loading examples:', error);
        exampleSelect.innerHTML = '<option value="">Error loading examples</option>';
    }
}

// Show error
function showError(message) {
    const errorContainer = document.getElementById('errorContainer');
    errorContainer.innerHTML = `<div class="error-message">${message}</div>`;
    setTimeout(() => errorContainer.innerHTML = '', 5000);
}

// Show loading
function showLoading() {
    const loadingHTML = `
        <div class="loading-skeleton">
            <div class="skeleton-line"></div>
            <div class="skeleton-line"></div>
            <div class="skeleton-line"></div>
            <div class="skeleton-line"></div>
            <div class="skeleton-line"></div>
            <div class="skeleton-line"></div>
        </div>
    `;

    // Show loading in all visible panes
    if (currentUseCase === 'match_frontier') {
        document.getElementById('smallBaselineContent').innerHTML = loadingHTML;
        setVisible(document.getElementById('smallBaselineLatency'), false);
        setVisible(document.getElementById('smallBaselineActions'), false);
        setVisible(document.getElementById('smallBaselineCost'), false);
    }

    document.getElementById('middlePaneContent').innerHTML = loadingHTML;
    setVisible(document.getElementById('middlePaneLatency'), false);
    setVisible(document.getElementById('middlePaneActions'), false);
    setVisible(document.getElementById('middlePaneCost'), false);

    document.getElementById('rightPaneContent').innerHTML = loadingHTML;
    setVisible(document.getElementById('rightPaneLatency'), false);
    setVisible(document.getElementById('rightPaneActions'), false);
    setVisible(document.getElementById('rightPaneCost'), false);
}

// Metrics moved to Performance Details within each result pane

// Formatting/rendering utilities (extractFinalAnswer, formatAnswer,
// formatReasoningSteps, extractConclusion, formatAsHTML, renderMath,
// toggleReasoning, toggleCandidateContent, etc.) are in utils.js.

// Helper to set size badge
function setSizeBadge(elementId, size) {
    const badge = document.getElementById(elementId);
    if (size) {
        badge.textContent = size;
        badge.className = 'size-badge ' + size.toLowerCase();
        setVisible(badge, true);
    } else {
        setVisible(badge, false);
    }
}

// Helper to set cost badge
function setCostBadge(elementId, cost_usd, threshold = 0.01) {
    const badge = document.getElementById(elementId);
    if (cost_usd !== null && cost_usd !== undefined && cost_usd > 0) {
        badge.textContent = formatCost(cost_usd);
        badge.className = cost_usd > threshold ? 'cost-badge expensive' : 'cost-badge';
        setVisible(badge, true);
    } else {
        setVisible(badge, false);
    }
}

// Display results
function toggleSection(button) {
    const section = button.parentElement;
    section.classList.toggle('expanded');
}

// --- Algorithm Trace Rendering ---

function renderCandidateCard(candidate, metricHtml) {
    const winnerClass = candidate.is_selected ? ' winner' : '';
    const winnerBadge = candidate.is_selected ? '<span class="winner-badge">Winner</span>' : '';
    return `
        <div class="candidate-card${winnerClass}">
            <div class="candidate-header">
                <span class="candidate-label">Candidate ${candidate.index + 1}</span>
                ${winnerBadge}
            </div>
            <div class="candidate-content">${candidate.content.replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/\n/g, '<br>')}</div>
            <button class="candidate-expand-btn" onclick="toggleCandidateContent(this)">Show more</button>
            ${metricHtml}
        </div>
    `;
}

function renderSelfConsistencyTrace(trace) {
    const totalVotes = trace.total_votes || trace.candidates.length;
    let html = '<div class="voting-arena">';

    // Header
    html += `
        <div class="voting-arena-header">
            <span class="arena-icon">&#9745;</span>
            <span>Majority Vote &mdash; ${trace.candidates.length} candidates, ${totalVotes} votes</span>
        </div>
    `;

    // Tool voting section if present
    if (trace.tool_voting) {
        const tv = trace.tool_voting;
        html += `
            <div class="tool-voting-section" style="margin: 0 0 8px 0; padding: 12px 16px; background: var(--primary-light); border-radius: var(--radius); border-left: 4px solid var(--primary);">
                <div style="font-weight: 600; color: var(--primary); margin-bottom: 6px; font-size: 13px;">
                    Tool Voting Consensus
                </div>
                <div style="font-size: 12px; margin-bottom: 10px; color: var(--text-secondary);">
                    Type: <strong>${tv.tool_vote_type}</strong> | Calls: <strong>${tv.total_tool_calls}</strong>
                </div>
                <div class="vote-chart">
        `;

        const sortedTools = Object.entries(tv.tool_counts).sort((a, b) => b[1] - a[1]);
        const maxToolVotes = sortedTools.length > 0 ? sortedTools[0][1] : 1;

        sortedTools.forEach(([tool, count], i) => {
            const pct = (count / maxToolVotes) * 100;
            const isWinner = tool === tv.winning_tool;
            const displayTool = tool.length > 30 ? tool.substring(0, 30) + '...' : tool;
            html += `
                <div class="vote-chart-row ${isWinner ? 'is-winner' : ''}">
                    <span class="vote-chart-rank">${i + 1}</span>
                    <span class="vote-chart-answer" title="${tool.replace(/"/g, '&quot;')}">${displayTool}</span>
                    <div class="vote-chart-bar-wrap">
                        <div class="vote-chart-bar" style="width: ${pct}%; ${!isWinner ? 'background: var(--primary);' : ''}"></div>
                    </div>
                    <span class="vote-chart-count">${count}${isWinner ? ' <span class="vote-chart-winner-badge">WINNER</span>' : ''}</span>
                </div>
            `;
        });

        html += `
                </div>
            </div>
        `;
    }

    // Vote distribution bar chart — the main visualization
    const sortedVotes = Object.entries(trace.vote_counts).sort((a, b) => b[1] - a[1]);
    const maxVotes = sortedVotes.length > 0 ? sortedVotes[0][1] : 1;
    const winningAnswer = sortedVotes.length > 0 ? sortedVotes[0][0] : '';

    html += '<div class="vote-chart">';
    sortedVotes.forEach(([answer, count], i) => {
        const pct = (count / maxVotes) * 100;
        const isWinner = answer === winningAnswer;
        const displayAnswer = answer.length > 40 ? answer.substring(0, 40) + '...' : answer;
        html += `
            <div class="vote-chart-row ${isWinner ? 'is-winner' : ''}">
                <span class="vote-chart-rank">${i + 1}</span>
                <span class="vote-chart-answer" title="${answer.replace(/"/g, '&quot;')}">${displayAnswer}</span>
                <div class="vote-chart-bar-wrap">
                    <div class="vote-chart-bar" style="width: ${pct}%"></div>
                </div>
                <span class="vote-chart-count">
                    ${count} vote${count !== 1 ? 's' : ''}${isWinner ? ' <span class="vote-chart-winner-badge">WINNER</span>' : ''}
                </span>
            </div>
        `;
    });
    html += '</div>';

    // Candidate chips grouped by answer
    html += '<div class="candidate-chips">';
    for (const candidate of trace.candidates) {
        const preview = candidate.content ? candidate.content.substring(0, 50).replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/\n/g, ' ') : '';
        const isWinner = candidate.is_selected;
        html += `
            <span class="candidate-chip ${isWinner ? 'is-winner' : ''}" title="${candidate.content ? candidate.content.substring(0, 200).replace(/"/g, '&quot;').replace(/\n/g, ' ') : ''}">
                <span class="chip-index">#${candidate.index + 1}</span>
                ${isWinner ? '&#10003; ' : ''}${preview}${candidate.content && candidate.content.length > 50 ? '...' : ''}
            </span>
        `;
    }
    html += '</div>';

    // Expandable full candidate details
    const traceId = 'sc_candidates_' + Date.now();
    html += `
        <div class="trace-candidates-detail">
            <button class="trace-candidates-toggle" onclick="document.getElementById('${traceId}').classList.toggle('expanded'); this.textContent = document.getElementById('${traceId}').classList.contains('expanded') ? 'Hide full responses' : 'Show full responses'">Show full responses</button>
            <div id="${traceId}" class="trace-candidates-list">
    `;
    for (const candidate of trace.candidates) {
        html += renderCandidateCard(candidate, '');
    }
    html += '</div></div>';

    html += '</div>'; // close .voting-arena
    return html;
}

function renderBestOfNTrace(trace) {
    // Sort candidates by score descending
    const sorted = trace.candidates.map((c, i) => ({ ...c, score: trace.scores[i] }))
        .sort((a, b) => b.score - a.score);

    const range = trace.max_score - trace.min_score || 1;
    let html = '<div class="score-leaderboard">';

    // Header
    html += `
        <div class="score-leaderboard-header">
            <span>&#9733;</span>
            <span>Score Leaderboard &mdash; ${trace.candidates.length} candidates scored by LLM judge</span>
        </div>
    `;

    sorted.forEach((candidate, i) => {
        const pct = ((candidate.score - trace.min_score) / range) * 100;
        const isWinner = i === 0;
        const preview = candidate.content
            ? candidate.content.substring(0, 60).replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/\n/g, ' ')
            : '';
        const hasMore = candidate.content && candidate.content.length > 60;

        // Color gradient: red (low) to green (high) based on percentage
        const r = Math.round(220 - (pct / 100) * 140);
        const g = Math.round(80 + (pct / 100) * 100);
        const b = Math.round(80 + (pct / 100) * 20);
        const barColor = isWinner ? 'var(--warning)' : `rgb(${r}, ${g}, ${b})`;

        const fullTextId = 'bon_full_' + (candidate.index || i) + '_' + Date.now();

        html += `
            <div class="leaderboard-row ${isWinner ? 'is-winner' : ''}">
                <span class="leaderboard-rank">${isWinner ? '&#9733;' : i + 1}</span>
                <div class="leaderboard-content">
                    <div class="leaderboard-preview">
                        ${isWinner ? '<span class="leaderboard-winner-tag">BEST</span> ' : ''}${preview}${hasMore ? '...' : ''}
                        ${hasMore ? `<button class="leaderboard-expand-btn" onclick="document.getElementById('${fullTextId}').classList.toggle('expanded'); this.textContent = document.getElementById('${fullTextId}').classList.contains('expanded') ? 'Show less' : 'Show full'">Show full</button>` : ''}
                    </div>
                    <div class="leaderboard-bar-wrap">
                        <div class="leaderboard-bar" style="width: ${pct}%; background: ${barColor}"></div>
                    </div>
                    ${hasMore ? `<div id="${fullTextId}" class="leaderboard-full-text">${candidate.content.replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/\n/g, '<br>')}</div>` : ''}
                </div>
                <span class="leaderboard-score" style="color: ${barColor}">${candidate.score.toFixed(2)}</span>
            </div>
        `;

        // Separator after winner
        if (isWinner && sorted.length > 1) {
            html += '<div class="leaderboard-separator"></div>';
        }
    });

    html += '</div>';
    return html;
}

function renderBeamSearchTrace(trace) {
    let html = '<div class="trace-summary">Beam Search: explored ' + trace.candidates.length + ' beams with PRM scoring</div>';

    // Sort by score descending
    const sorted = trace.candidates.map((c, i) => ({ ...c, score: trace.scores[i], steps: trace.steps_used[i] }))
        .sort((a, b) => b.score - a.score);

    const maxScore = Math.max(...trace.scores);
    const minScore = Math.min(...trace.scores);
    const range = maxScore - minScore || 1;

    for (const candidate of sorted) {
        const pct = ((candidate.score - minScore) / range) * 100;
        const metricHtml = `
            <div class="candidate-metric">
                <span class="metric-label">PRM</span>
                <div class="metric-bar-container">
                    <div class="metric-bar score-bar" style="width: ${pct}%"></div>
                </div>
                <span class="metric-value">${candidate.score.toFixed(3)}</span>
            </div>
            <div class="candidate-metric">
                <span class="metric-label">Steps</span>
                <span class="metric-value">${candidate.steps}</span>
            </div>
        `;
        html += renderCandidateCard(candidate, metricHtml);
    }
    return html;
}

function renderParticleFilteringTrace(trace, containerId) {
    let html = '<div class="trace-summary">Particle Filtering: maintained ' + trace.candidates.length + ' particles with importance weighting</div>';

    // Sort by normalized weight descending
    const sorted = trace.candidates.map((c, i) => ({
        ...c,
        logWeight: trace.log_weights[i],
        normWeight: trace.normalized_weights[i],
        steps: trace.steps_used[i],
    })).sort((a, b) => b.normWeight - a.normWeight);

    const maxWeight = Math.max(...trace.normalized_weights);

    for (const candidate of sorted) {
        const pct = maxWeight > 0 ? (candidate.normWeight / maxWeight) * 100 : 0;
        const metricHtml = `
            <div class="candidate-metric">
                <span class="metric-label">Weight</span>
                <div class="metric-bar-container">
                    <div class="metric-bar weight-bar" style="width: ${pct}%"></div>
                </div>
                <span class="metric-value">${(candidate.normWeight * 100).toFixed(1)}%</span>
            </div>
            <div class="candidate-metric">
                <span class="metric-label">Steps</span>
                <span class="metric-value">${candidate.steps}</span>
            </div>
        `;
        html += renderCandidateCard(candidate, metricHtml);
    }
    return html;
}

function switchIteration(tabBtn, iterIdx, containerId) {
    // Update tab active state
    const tabContainer = tabBtn.parentElement;
    tabContainer.querySelectorAll('.iteration-tab').forEach(t => t.classList.remove('active'));
    tabBtn.classList.add('active');

    // Show/hide iteration content
    const parent = tabContainer.parentElement;
    parent.querySelectorAll('.iteration-content').forEach(c => c.classList.remove('active'));
    const target = parent.querySelector('[data-iteration="' + iterIdx + '"]');
    if (target) target.classList.add('active');
}

function renderParticleGibbsTrace(trace, containerId) {
    let html = '<div class="trace-summary">Particle Gibbs: ' + trace.num_iterations + ' iterations of particle filtering with reference particle</div>';

    // Iteration tabs
    html += '<div class="iteration-tabs">';
    for (let i = 0; i < trace.num_iterations; i++) {
        const activeClass = i === trace.num_iterations - 1 ? ' active' : '';
        html += `<button class="iteration-tab${activeClass}" onclick="switchIteration(this, ${i}, '${containerId}')">Iteration ${i + 1}</button>`;
    }
    html += '</div>';

    // Iteration content
    for (let i = 0; i < trace.num_iterations; i++) {
        const activeClass = i === trace.num_iterations - 1 ? ' active' : '';
        html += `<div class="iteration-content${activeClass}" data-iteration="${i}">`;
        html += renderParticleFilteringTrace(trace.iterations[i], containerId + '_iter' + i);
        html += '</div>';
    }

    return html;
}

function renderToolCalls(toolCalls) {
    if (!toolCalls || toolCalls.length === 0) return '';

    let html = `
        <div class="expandable-section">
            <button class="expand-button" onclick="toggleSection(this)">
                <svg class="expand-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path d="M4 6L8 10L12 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                <span>Tool Calls (${toolCalls.length})</span>
            </button>
            <div class="expandable-content">
                <div class="trace-section">
    `;

    toolCalls.forEach((tc, idx) => {
        const args = JSON.stringify(tc.arguments, null, 2);
        html += `
            <div class="tool-call-item" style="margin-bottom: 12px; padding: 12px; background: var(--bg-tertiary); border-radius: 4px;">
                <div style="font-weight: 600; color: var(--primary); margin-bottom: 8px;">
                    🔧 ${tc.name}
                </div>
                <div style="font-size: 0.9em; color: var(--text-secondary); margin-bottom: 8px;">
                    <strong>Arguments:</strong>
                    <pre style="margin: 4px 0; padding: 8px; background: var(--bg-primary); border-radius: 4px; overflow-x: auto;">${args}</pre>
                </div>
                ${tc.result ? `
                <div style="font-size: 0.9em; color: var(--text-secondary);">
                    <strong>Result:</strong>
                    <pre style="margin: 4px 0; padding: 8px; background: var(--bg-primary); border-radius: 4px; overflow-x: auto;">${tc.result}</pre>
                </div>
                ` : ''}
            </div>
        `;
    });

    html += `
                </div>
            </div>
        </div>
    `;

    return html;
}

function renderAlgorithmTrace(trace, directDisplay = false) {
    if (!trace || !trace.algorithm) return '';

    let traceHtml = '';
    switch (trace.algorithm) {
        case 'self_consistency':
            traceHtml = renderSelfConsistencyTrace(trace);
            break;
        case 'best_of_n':
            traceHtml = renderBestOfNTrace(trace);
            break;
        case 'beam_search':
            traceHtml = renderBeamSearchTrace(trace);
            break;
        case 'entropic_particle_filtering':
        case 'particle_filtering':
            traceHtml = renderParticleFilteringTrace(trace, 'pf_trace');
            break;
        case 'particle_gibbs':
            traceHtml = renderParticleGibbsTrace(trace, 'pg_trace');
            break;
        default:
            return '';
    }

    if (directDisplay) {
        return `
            <div class="trace-direct-display">
                ${traceHtml}
            </div>
        `;
    }

    return `
        <div class="expandable-section">
            <button class="expand-button" onclick="toggleSection(this)">
                <svg class="expand-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path d="M4 6L8 10L12 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                <span>Algorithm Trace (${trace.candidates ? trace.candidates.length + ' candidates' : trace.num_iterations + ' iterations'})</span>
            </button>
            <div class="expandable-content">
                <div class="trace-section">
                    ${traceHtml}
                </div>
            </div>
        </div>
    `;
}

function renderAnswerBox(containerId, data, comparisonData = null) {
    const container = document.getElementById(containerId);

    // The full response from the model
    const fullResponse = data.answer;

    // Extract just the concluding answer for the main display
    const conclusion = extractConclusion(fullResponse);

    let html = '';

    // Main response area — show just the conclusion/answer
    html += `<div class="chat-response">${formatAsHTML(conclusion)}</div>`;

    // Expandable section with full detailed reasoning
    html += `
        <div class="expandable-section">
            <button class="expand-button" onclick="toggleSection(this)">
                <svg class="expand-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path d="M4 6L8 10L12 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                <span>View Detailed Response</span>
            </button>
            <div class="expandable-content">
                <div class="reasoning-section">
                    <div class="reasoning-content">
                        ${formatReasoningSteps(fullResponse)}
                    </div>
                </div>
            </div>
        </div>
    `;

    // Add expandable performance details section
    html += `
        <div class="expandable-section">
            <button class="expand-button" onclick="toggleSection(this)">
                <svg class="expand-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path d="M4 6L8 10L12 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                <span>Performance Details</span>
            </button>
            <div class="expandable-content">
                <div class="details-section">
                    <div class="detail-row">
                        <span class="detail-label">Latency</span>
                        <span class="detail-value">${data.latency_ms}ms</span>
                    </div>
    `;

    if (data.model_size) {
        html += `
            <div class="detail-row">
                <span class="detail-label">Model Size</span>
                <span class="detail-value">${data.model_size}</span>
            </div>
        `;
    }

    if (data.input_tokens) {
        html += `
            <div class="detail-row">
                <span class="detail-label">Input Tokens</span>
                <span class="detail-value">${data.input_tokens.toLocaleString()}</span>
            </div>
        `;
    }

    if (data.output_tokens) {
        html += `
            <div class="detail-row">
                <span class="detail-label">Output Tokens</span>
                <span class="detail-value">${data.output_tokens.toLocaleString()}</span>
            </div>
        `;
    }

    if (data.cost_usd !== null && data.cost_usd !== undefined) {
        const costFormatted = data.cost_usd < 0.0001
            ? `$${data.cost_usd.toExponential(2)}`
            : `$${data.cost_usd.toFixed(4)}`;
        html += `
            <div class="detail-row">
                <span class="detail-label">Cost</span>
                <span class="detail-value">${costFormatted}</span>
            </div>
        `;
    }

    // Add comparison data if provided
    if (comparisonData) {
        html += `
            <div class="detail-row">
                <span class="detail-label">${comparisonData.label}</span>
                <span class="detail-value">${comparisonData.value}</span>
            </div>
        `;
    }

    html += `
                </div>
            </div>
        </div>
    `;

    // Add tool calls section if present
    if (data.tool_calls && data.tool_calls.length > 0) {
        html += renderToolCalls(data.tool_calls);
    }

    // Add algorithm trace section (only for ITS results that have trace data)
    if (data.trace) {
        html += renderAlgorithmTrace(data.trace);
    }

    container.innerHTML = html;
    container.classList.add('fade-in');

    // Render math in the final answer and reasoning
    renderMath(container);

    // Hide the old badges and latency display
    const paneId = containerId.replace('Content', '');
    const latencyEl = document.getElementById(paneId + 'Latency');
    const actionsEl = document.getElementById(paneId + 'Actions');
    setVisible(latencyEl, false);
    setVisible(actionsEl, false);

    setTimeout(() => {
        container.classList.remove('fade-in');
    }, 300);
}

function displayResults(data) {
    lastResults = data;

    if (currentUseCase === 'match_frontier' && data.small_baseline) {
        // 3-column layout: Small baseline, Small+ITS, Frontier
        const smallMs = data.small_baseline.latency_ms;
        const itsMs = data.its.latency_ms;
        const frontierMs = data.baseline.latency_ms;

        renderAnswerBox('smallBaselineContent', data.small_baseline);
        renderAnswerBox('middlePaneContent', data.its, {
            label: 'vs Small Model',
            value: `${itsMs - smallMs > 0 ? '+' : ''}${(itsMs - smallMs).toFixed(0)}ms`
        });
        renderAnswerBox('rightPaneContent', data.baseline, {
            label: 'vs ITS Result',
            value: `${frontierMs - itsMs > 0 ? '+' : ''}${(frontierMs - itsMs).toFixed(0)}ms`
        });
    } else {
        // 2-column layout: Baseline, ITS
        const baselineMs = data.baseline.latency_ms;
        const itsMs = data.its.latency_ms;

        renderAnswerBox('middlePaneContent', data.baseline);
        renderAnswerBox('rightPaneContent', data.its, {
            label: 'vs Baseline',
            value: `${itsMs - baselineMs > 0 ? '+' : ''}${(itsMs - baselineMs).toFixed(0)}ms`
        });
    }

    // Expected answer - show simply without the complex formatting
    if (currentExpectedAnswer) {
        const expectedContent = document.getElementById('expectedAnswerContent');
        expectedContent.innerHTML = formatExpectedAnswer(currentExpectedAnswer);
        renderMath(expectedContent);
        document.getElementById('expectedAnswerContainer').classList.add('visible');
    }

    // Render enhanced performance visualization
    renderPerformanceVisualization(data);
}

// Render enhanced performance visualization
function renderPerformanceVisualization(data) {
    console.log('🎨 renderPerformanceVisualization called with data:', data);

    const container = document.getElementById('performance-visualization-container');

    if (!container) {
        console.error('❌ Container element not found!');
        return;
    }

    // Show the container
    setVisible(container, true);
    console.log('✅ Container display set to block');

    // Initialize and render the visualization with V2
    if (typeof PerformanceVizV2 !== 'undefined') {
        try {
            const perfViz = new PerformanceVizV2('performance-visualization');
            perfViz.render(data);
            console.log('✅ Performance visualization V2 rendered successfully!');

            // Scroll to visualization
            setTimeout(() => {
                container.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }, 500);
        } catch (error) {
            console.error('❌ Error rendering visualization:', error);
            console.error('Stack trace:', error.stack);
        }
    } else {
        console.error('❌ PerformanceVizV2 class not loaded. Make sure performance-viz-v2.js is included.');
    }
}

// Run comparison
async function runComparison() {
    const question = document.getElementById('question').value.trim();
    const model_id = document.getElementById('model').value;
    const budget = parseInt(document.getElementById('budget').value);

    if (!question) {
        showError('Please enter a question');
        return;
    }
    if (!model_id) {
        showError('Please select a model');
        return;
    }

    // Additional validation for match_frontier use case
    if (currentUseCase === 'match_frontier') {
        const frontier_model_id = document.getElementById('frontierModel').value;
        if (!frontier_model_id) {
            showError('Please select a frontier model');
            return;
        }
    }

    const runButton = document.getElementById('runButton');
    runButton.disabled = true;
    runButton.textContent = 'Running...';
    isRunning = true; // Set running flag
    showLoading();

    try {
        const requestBody = {
            question,
            model_id,
            algorithm: selectedAlgorithm,
            budget,
            use_case: currentUseCase,
        };

        // Add frontier model if using match_frontier use case
        if (currentUseCase === 'match_frontier') {
            requestBody.frontier_model_id = document.getElementById('frontierModel').value;
        }

        // Add tool calling parameters for tool_consensus use case
        if (currentUseCase === 'tool_consensus') {
            requestBody.enable_tools = true;
            requestBody.tool_vote = 'tool_name';  // Default to tool_name voting
            requestBody.exclude_args = [];
        }

        const response = await fetch(`${API_BASE_URL}/compare`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(requestBody),
        });

        if (!response.ok) {
            const errorData = await response.json();
            throw new Error(errorData.detail || 'Comparison failed');
        }

        const data = await response.json();
        displayResults(data);
    } catch (error) {
        console.error('Error running comparison:', error);
        showError(`Error: ${error.message}`);

        if (currentUseCase === 'match_frontier') {
            document.getElementById('smallBaselineContent').innerHTML = '<div class="empty-state">Error occurred</div>';
        }
        document.getElementById('middlePaneContent').innerHTML = '<div class="empty-state">Error occurred</div>';
        document.getElementById('rightPaneContent').innerHTML = '<div class="empty-state">Error occurred</div>';
    } finally {
        runButton.disabled = false;
        runButton.textContent = 'Run Comparison';
        isRunning = false; // Reset running flag
    }
}

// Wait for KaTeX to load
function initializeApp() {
    checkSavedExperience();
}

// Initialize when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initializeApp);
} else {
    initializeApp();
}
