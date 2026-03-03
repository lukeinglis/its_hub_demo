/**
 * app.js — Core state management for the ITS Demo
 *
 * ARCHITECTURE
 * ────────────
 * State ownership:
 *   - app.js owns global state (selectedExperience, currentUseCase, isExpertMode)
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

// Unified visibility utility — replaces all style.display assignments.
// Uses the .hidden CSS class for consistent show/hide behavior.
function setVisible(el, show) {
    if (!el) return;
    if (show) el.classList.remove('hidden');
    else el.classList.add('hidden');
}

// Global formatting utilities — used by app.js, guided-demo.js, and interactive-demo.js.
function formatLatency(ms) {
    if (ms == null) return 'N/A';
    if (ms >= 1000) return (ms / 1000).toFixed(1) + 's';
    return ms + 'ms';
}

function formatCost(cost_usd) {
    if (cost_usd == null || cost_usd === undefined) return 'N/A';
    if (cost_usd < 0.0001) return '$' + cost_usd.toExponential(2);
    if (cost_usd < 0.01) return '$' + cost_usd.toFixed(4);
    if (cost_usd < 1) return '$' + cost_usd.toFixed(3);
    return '$' + cost_usd.toFixed(2);
}

let currentExpectedAnswer = null;
let selectedAlgorithm = 'best_of_n';
let lastResults = null;
let currentUseCase = 'improve_model';
let isExpertMode = false;
let isRunning = false; // Track if a comparison is currently running
let selectedExperience = null; // 'guided' or 'interactive'

// OFFLINE DEMO SCENARIOS FOR GUIDED MODE (loaded from scenarios.json)
let OFFLINE_SCENARIOS = [];
fetch('scenarios.json').then(r => r.json()).then(data => { OFFLINE_SCENARIOS = data; });

let currentScenarioIndex = 0; // Track which scenario is displayed in guided mode

// Guided mode questions tailored to each algorithm
const GUIDED_MODE_QUESTIONS = {
    best_of_n: [
        "Write a persuasive argument for why cities should invest more in public transportation infrastructure.",
        "Explain the concept of machine learning to a 10-year-old using everyday examples.",
        "Analyze the potential long-term impacts of remote work on urban development.",
        "Create a detailed plan for someone who wants to transition from a corporate job to freelancing."
    ],
    self_consistency: [
        "If a train travels 120 miles in 2 hours, then speeds up and travels 180 miles in the next 2 hours, what is its average speed for the entire journey?",
        "A farmer has 17 sheep. All but 9 die. How many sheep are left alive?",
        "If you have a 3-gallon jug and a 5-gallon jug, how can you measure exactly 4 gallons of water?",
        "What is the next number in this sequence: 2, 6, 12, 20, 30, ?"
    ],
    beam_search: [
        "Solve: A rectangular garden is 3 times as long as it is wide. If the perimeter is 96 feet, find the length and width. Show all steps.",
        "Find the value of x: 3(2x - 5) + 4 = 2(x + 7) - 1. Show your step-by-step reasoning.",
        "A store offers a 20% discount on an item, then adds 8% sales tax. Is this the same as adding 8% tax first and then applying a 20% discount? Prove your answer with calculations.",
        "Three consecutive integers sum to 72. Find these integers using algebraic reasoning."
    ],
    particle_filtering: [
        "A bakery makes three types of pastries: croissants, muffins, and cookies. A croissant uses 3 eggs and 2 cups of flour. A muffin uses 1 egg and 1 cup of flour. A cookie batch uses 2 eggs and 3 cups of flour. If the bakery has 25 eggs and 30 cups of flour, and wants to maximize revenue where croissants sell for $4, muffins for $2, and cookie batches for $5, how many of each should they make?",
        "Find all integer solutions to: x² + y² = 65, where both x and y are positive. Then, for each solution, calculate x³ - y³. What is the sum of all these cubes?",
        "A frog is at the bottom of a 30-foot well. Each day it climbs up 5 feet, but each night it slides back down 2 feet. On which day will the frog reach the top? Show your step-by-step reasoning.",
        "In a tournament, 8 teams play in a single-elimination bracket. Team strengths are: A=8, B=7, C=6, D=5, E=4, F=3, G=2, H=1. A stronger team beats a weaker team with 80% probability. If the bracket is A vs H, B vs G, C vs F, D vs E, what is the probability that team C wins the tournament?"
    ],
    entropic_particle_filtering: [
        "In how many ways can you arrange the letters in the word MISSISSIPPI? Show your complete step-by-step calculation accounting for repeated letters.",
        "A rectangular box has dimensions that are consecutive even integers. If the volume is 960 cubic inches, find the dimensions. Then calculate the surface area.",
        "Solve the system of equations: 2x + 3y - z = 7, x - 2y + 4z = -3, 3x + y + 2z = 10. Show all steps including elimination or substitution process.",
        "A sequence is defined by: a₁ = 2, a₂ = 5, and aₙ = 3aₙ₋₁ - 2aₙ₋₂ for n ≥ 3. Find a₁₀ and derive a general formula for aₙ."
    ],
    particle_gibbs: [
        "Solve this cryptarithmetic puzzle: SEND + MORE = MONEY. Each letter represents a unique digit (0-9). Show the complete solution process.",
        "Eight queens must be placed on a standard 8×8 chessboard so that no two queens threaten each other. Find at least one valid configuration and explain your solving strategy.",
        "Solve the nonlinear system: x² + y² = 25, xy = 12. Find all real solutions (x, y) and verify each one.",
        "A group of 5 friends owes each other money: Alice owes Bob $20, Bob owes Carol $15, Carol owes David $30, David owes Emma $10, Emma owes Alice $25. What is the minimum number of transactions needed to settle all debts, and what should they be?"
    ]
};

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

// GUIDED WIZARD STATE & FUNCTIONS

// Guided Wizard State
let wizardState = {
    useCase: null,        // 'improve_model' or 'match_frontier'
    selectedModel: null,  // e.g., 'GPT-3.5 Turbo'
    frontierModel: null,  // e.g., 'GPT-4o' (for match_frontier only)
    algorithm: null,      // e.g., 'best_of_n'
    currentStep: 1        // 1-4
};

// Model-to-Frontier mapping
const FRONTIER_MODEL_MAP = {
    'GPT-3.5 Turbo': 'GPT-4o',
    'GPT-4o Mini': 'GPT-4o',
    'Claude 3.5 Haiku': 'Claude 3.5 Sonnet',
    'Llama 3.2 3B': 'Llama 3.3 70B'
};

let currentWizardScenario = null;

// Event delegation for the wizard (bound once)
document.addEventListener('DOMContentLoaded', function() {
    const wizardEl = document.getElementById('guidedWizard');
    if (!wizardEl) return;
    wizardEl.addEventListener('click', function(e) {
        const wizardCard = e.target.closest('.wizard-card[data-usecase]');
        if (wizardCard) { selectUseCase(wizardCard.dataset.usecase); return; }

        const breadcrumb = e.target.closest('.breadcrumb.completed');
        if (breadcrumb) { navigateToStep(parseInt(breadcrumb.dataset.step)); return; }

        const backStepBtn = e.target.closest('.wizard-back-step-btn');
        if (backStepBtn) { navigateToStep(parseInt(backStepBtn.dataset.backTo)); return; }
    });
});

function initGuidedWizard() {
    // Reset state
    wizardState = {
        useCase: null,
        selectedModel: null,
        frontierModel: null,
        algorithm: null,
        currentStep: 1
    };
    currentWizardScenario = null;

    // Show wizard
    setVisible(document.getElementById('guidedWizard'), true);

    // Hide all other sections - wizard replaces the entire flow
    const sectionsToHide = [
        'useCaseSection',
        'scenarioSection',
        'configSection',
        'questionSection',
        'errorContainer',
        'expectedAnswerContainer',
        'resultsContainer',
        'performance-visualization-container'
    ];
    sectionsToHide.forEach(id => {
        setVisible(document.getElementById(id), false);
    });

    // Clean up dynamic elements from previous runs
    const perfContainer = document.getElementById('performanceDetailsContainer');
    if (perfContainer) perfContainer.remove();
    const headlineBanner = document.getElementById('wizardResultsHeadline');
    if (headlineBanner) headlineBanner.remove();
    const promptDisplay = document.getElementById('wizardPromptDisplay');
    setVisible(promptDisplay, false);

    // Also hide guided demo badge (wizard is self-explanatory)
    const badge = document.getElementById('guidedDemoBadge');
    setVisible(badge, false);

    // Hide back button if it exists
    const backBtn = document.getElementById('wizardBackBtn');
    setVisible(backBtn, false);

    // Reset question textarea and restore hidden form elements
    const questionEl = document.getElementById('question');
    if (questionEl) questionEl.disabled = false;
    const questionSection = document.getElementById('questionSection');
    if (questionSection) {
        const formGroup = questionSection.querySelector('.form-group');
        setVisible(formGroup, true);
        const actionBtns = questionSection.querySelector('.action-buttons');
        setVisible(actionBtns, true);
        const exampleQs = questionSection.querySelector('.example-questions-container');
        setVisible(exampleQs, true);
    }

    // Reset card selections
    document.querySelectorAll('#guidedWizard .wizard-card').forEach(card => {
        card.classList.remove('selected');
    });

    // Dynamically compute scenario counts
    document.querySelectorAll('#step1Container .wizard-card').forEach(card => {
        const count = OFFLINE_SCENARIOS.filter(s => s.useCase === card.dataset.usecase).length;
        const countEl = card.querySelector('.scenario-count');
        if (countEl) countEl.textContent = `${count} scenarios available`;
    });

    // Show step 1
    showWizardStep(1);
}

function showWizardStep(stepNumber) {
    wizardState.currentStep = stepNumber;

    // Hide all steps
    document.querySelectorAll('.wizard-step').forEach(step => {
        setVisible(step, false);
    });

    // Show current step
    const currentStepEl = document.getElementById(`step${stepNumber}Container`);
    setVisible(currentStepEl, true);

    // Update breadcrumbs
    updateBreadcrumbs();
}

function updateBreadcrumbs() {
    document.querySelectorAll('.breadcrumb').forEach((crumb, index) => {
        const stepNum = parseInt(crumb.dataset.step);
        crumb.classList.remove('active', 'completed');

        if (stepNum < wizardState.currentStep) {
            crumb.classList.add('completed');
        } else if (stepNum === wizardState.currentStep) {
            crumb.classList.add('active');
        }
    });
}

function navigateToStep(stepNumber) {
    // Clear state for steps after the target
    if (stepNumber <= 1) {
        wizardState.useCase = null;
        wizardState.selectedModel = null;
        wizardState.frontierModel = null;
        wizardState.algorithm = null;
    } else if (stepNumber <= 2) {
        wizardState.selectedModel = null;
        wizardState.frontierModel = null;
        wizardState.algorithm = null;
    } else if (stepNumber <= 3) {
        wizardState.algorithm = null;
    }

    // If navigating away from results (step 4), clean up results UI
    if (wizardState.currentStep === 4) {
        const perfContainer = document.getElementById('performanceDetailsContainer');
        if (perfContainer) perfContainer.remove();
        const headlineBanner = document.getElementById('wizardResultsHeadline');
        if (headlineBanner) headlineBanner.remove();
        const resultsContainer = document.getElementById('resultsContainer');
        setVisible(resultsContainer, false);
        const questionSection = document.getElementById('questionSection');
        setVisible(questionSection, false);
        const backBtn = document.getElementById('wizardBackBtn');
        setVisible(backBtn, false);
        const promptDisplay = document.getElementById('wizardPromptDisplay');
        setVisible(promptDisplay, false);
        const formGroup = document.querySelector('#questionSection .form-group');
        setVisible(formGroup, true);
        const q = document.getElementById('question');
        if (q) q.disabled = false;
        currentWizardScenario = null;
    }

    showWizardStep(stepNumber);

    // Re-populate dynamic content if needed
    if (stepNumber === 2 && wizardState.useCase) populateModelSelection();
    if (stepNumber === 3 && wizardState.selectedModel) populateAlgorithmSelection();
}

function selectUseCase(useCase) {
    wizardState.useCase = useCase;

    // Update card selection
    document.querySelectorAll('#guidedWizard .wizard-card').forEach(card => {
        card.classList.toggle('selected', card.dataset.usecase === useCase);
    });

    // Populate model options for step 2
    setTimeout(() => {
        populateModelSelection();
        showWizardStep(2);
    }, 300);
}

function populateModelSelection() {
    const container = document.getElementById('modelSelectionContainer');
    container.innerHTML = '';

    // Get available models for this use case
    const availableScenarios = OFFLINE_SCENARIOS.filter(s => s.useCase === wizardState.useCase);
    const uniqueModels = [...new Set(availableScenarios.map(s => s.model))];

    // Auto-select if only one model available
    if (uniqueModels.length === 1) {
        selectModel(uniqueModels[0]);
        return;
    }

    uniqueModels.forEach(model => {
        const card = document.createElement('div');
        card.className = 'model-card';
        card.dataset.model = model;

        // Get provider for this model
        const scenario = availableScenarios.find(s => s.model === model);
        const provider = scenario.provider;

        let frontierInfo = '';
        if (wizardState.useCase === 'match_frontier' && FRONTIER_MODEL_MAP[model]) {
            frontierInfo = `<div class="frontier-match">→ Matches ${FRONTIER_MODEL_MAP[model]}</div>`;
        }

        card.innerHTML = `
            <div class="model-card-header">
                <span class="model-name">${model}</span>
                <span class="model-provider">${provider}</span>
            </div>
            ${frontierInfo}
        `;

        card.addEventListener('click', () => selectModel(model));
        container.appendChild(card);
    });
}

function selectModel(model) {
    wizardState.selectedModel = model;
    wizardState.frontierModel = FRONTIER_MODEL_MAP[model] || null;

    // Update card selection
    document.querySelectorAll('.model-card').forEach(card => {
        card.classList.toggle('selected', card.dataset.model === model);
    });

    // Populate algorithm options for step 3
    setTimeout(() => {
        populateAlgorithmSelection();
        showWizardStep(3);
    }, 300);
}

function populateAlgorithmSelection() {
    const container = document.getElementById('algorithmSelectionContainer');
    container.innerHTML = '';

    // Get available algorithms for this use case + model combination
    const availableScenarios = OFFLINE_SCENARIOS.filter(s =>
        s.useCase === wizardState.useCase &&
        s.model === wizardState.selectedModel
    );
    const availableAlgorithms = [...new Set(availableScenarios.map(s => s.algorithm))];

    // Show only production algorithms that have available scenarios
    const productionAvailable = [];
    Object.keys(ALGORITHM_DESCRIPTIONS).forEach(algorithm => {
        const info = ALGORITHM_DESCRIPTIONS[algorithm];
        if (info.category === 'research') return;
        if (!availableAlgorithms.includes(algorithm)) return;
        productionAvailable.push(algorithm);
    });

    // Auto-select if only one algorithm available
    if (productionAvailable.length === 1) {
        selectAlgorithm(productionAvailable[0]);
        return;
    }

    productionAvailable.forEach(algorithm => {
        const info = ALGORITHM_DESCRIPTIONS[algorithm];

        const card = document.createElement('div');
        card.className = 'algorithm-card';
        card.dataset.algorithm = algorithm;

        card.innerHTML = `
            <div class="algorithm-name">${info.name}</div>
            <span class="algorithm-type-badge">${info.type}</span>
            <div class="algorithm-description">${info.description}</div>
            <div class="algorithm-usecase">${info.useCase}</div>
        `;

        card.addEventListener('click', () => selectAlgorithm(algorithm));

        container.appendChild(card);
    });
}

function selectAlgorithm(algorithm) {
    wizardState.algorithm = algorithm;

    // Update card selection
    document.querySelectorAll('.algorithm-card').forEach(card => {
        card.classList.toggle('selected', card.dataset.algorithm === algorithm);
    });

    // Load matching scenario
    setTimeout(() => {
        loadWizardScenario();
    }, 300);
}

function loadWizardScenario() {
    // Find all matching scenarios and pick one randomly
    const matches = OFFLINE_SCENARIOS.filter(s =>
        s.useCase === wizardState.useCase &&
        s.model === wizardState.selectedModel &&
        s.algorithm === wizardState.algorithm
    );

    if (matches.length === 0) {
        showError('No matching scenario found for this combination.');
        return;
    }

    const scenario = matches[Math.floor(Math.random() * matches.length)];

    // Store for copy summary
    currentWizardScenario = scenario;

    // Hide wizard step containers but keep breadcrumbs visible
    document.querySelectorAll('.wizard-step').forEach(step => {
        setVisible(step, false);
    });

    // Update breadcrumbs to show step 4 as active
    wizardState.currentStep = 4;
    updateBreadcrumbs();

    // Show the question section with styled prompt display
    const questionSection = document.getElementById('questionSection');
    setVisible(questionSection, true);

    // Hide form elements (textarea, buttons, examples)
    const formGroup = questionSection.querySelector('.form-group');
    setVisible(formGroup, false);
    const actionButtons = questionSection.querySelector('.action-buttons');
    setVisible(actionButtons, false);

    // Show styled prompt display
    let promptDisplay = document.getElementById('wizardPromptDisplay');
    if (!promptDisplay) {
        promptDisplay = document.createElement('div');
        promptDisplay.id = 'wizardPromptDisplay';
        promptDisplay.className = 'wizard-prompt-display';
        questionSection.appendChild(promptDisplay);
    }
    setVisible(promptDisplay, true);
    promptDisplay.innerHTML = `
        <div class="wizard-prompt-label">Prompt</div>
        <div class="wizard-prompt-text">${scenario.prompt.replace(/</g, '&lt;').replace(/>/g, '&gt;')}</div>
    `;

    // Add a "Back to Selection" button in the wizard area
    const wizardEl = document.getElementById('guidedWizard');
    let backBtn = document.getElementById('wizardBackBtn');
    if (!backBtn) {
        backBtn = document.createElement('button');
        backBtn.id = 'wizardBackBtn';
        backBtn.className = 'btn-secondary';
        backBtn.style.cssText = 'margin-bottom: 16px; display: flex; align-items: center; gap: 8px;';
        backBtn.innerHTML = '← Back to Selection';
        backBtn.addEventListener('click', () => {
            initGuidedWizard();
        });
        wizardEl.insertBefore(backBtn, wizardEl.querySelector('.wizard-breadcrumbs'));
    }
    setVisible(backBtn, true);

    // Update algorithm selector to match
    selectedAlgorithm = scenario.algorithm;

    // Render results
    renderWizardResults(scenario);
}

function renderWizardResponsePane(response, container) {
    const conclusion = extractConclusion(response);
    let html = `<div class="chat-response">${formatAsHTML(conclusion)}</div>`;

    // Add expandable detailed response if conclusion differs from full text
    if (conclusion !== response) {
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
                            ${formatReasoningSteps(response)}
                        </div>
                    </div>
                </div>
            </div>
        `;
    }
    container.innerHTML = html;
    renderMath(container);
}

function renderWizardResults(scenario) {
    const algName = ALGORITHM_DESCRIPTIONS[scenario.algorithm]?.name || scenario.algorithm;

    // Build and insert headline banner
    let headline = '';
    if (wizardState.useCase === 'match_frontier') {
        const costRatio = scenario.baseline.cost_usd > 0
            ? ((scenario.its.cost_usd / scenario.baseline.cost_usd) * 100).toFixed(0)
            : '?';
        headline = `${scenario.model} + ${algName} achieved frontier-equivalent quality at ${costRatio}% of ${wizardState.frontierModel}'s cost`;
    } else {
        headline = `${scenario.model} + ${algName}: ${scenario.budget} candidates evaluated to find the best answer`;
    }

    let headlineBanner = document.getElementById('wizardResultsHeadline');
    if (!headlineBanner) {
        headlineBanner = document.createElement('div');
        headlineBanner.id = 'wizardResultsHeadline';
        headlineBanner.className = 'wizard-results-headline';
        const resultsContainer = document.getElementById('resultsContainer');
        resultsContainer.parentNode.insertBefore(headlineBanner, resultsContainer);
    }
    setVisible(headlineBanner, true);
    headlineBanner.innerHTML = `
        <div class="headline-text">${headline}</div>
        <button id="copySummaryBtn" class="btn-secondary copy-summary-btn" onclick="copyWizardSummary()">Copy Summary</button>
    `;

    // Show the results container
    const resultsContainer = document.getElementById('resultsContainer');
    setVisible(resultsContainer, true);

    // Get actual pane elements
    const smallBaselinePane = document.getElementById('smallBaselinePane');
    const middlePaneTitle = document.getElementById('middlePaneTitle');
    const middlePaneContent = document.getElementById('middlePaneContent');
    const rightPaneTitle = document.getElementById('rightPaneTitle');
    const rightPaneContent = document.getElementById('rightPaneContent');

    // For match_frontier, show 3-column layout
    if (wizardState.useCase === 'match_frontier') {
        setVisible(smallBaselinePane, true);

        // Small model baseline (left pane)
        const smallBaselineTitle = document.getElementById('smallBaselineTitle');
        if (smallBaselineTitle) smallBaselineTitle.textContent = scenario.model;
        const smallBaselineContent = document.getElementById('smallBaselineContent');
        renderWizardResponsePane(scenario.smallBaseline?.response || scenario.baseline.response, smallBaselineContent);
        smallBaselineContent.innerHTML += renderPerformanceBadges(scenario.smallBaseline || scenario.baseline, 'baseline');

        // Frontier model baseline (middle pane)
        middlePaneTitle.textContent = wizardState.frontierModel || 'Frontier Model';
        renderWizardResponsePane(scenario.baseline.response, middlePaneContent);
        middlePaneContent.innerHTML += renderPerformanceBadges(scenario.baseline, 'baseline');
        renderMath(middlePaneContent);

        // ITS result (right pane)
        rightPaneTitle.textContent = 'ITS Enhanced';
        renderITSPane(scenario, rightPaneContent);
    } else {
        // improve_model: 2-column layout
        setVisible(smallBaselinePane, false);

        // Baseline (middle pane)
        middlePaneTitle.textContent = `${scenario.model} (Baseline)`;
        renderWizardResponsePane(scenario.baseline.response, middlePaneContent);
        middlePaneContent.innerHTML += renderPerformanceBadges(scenario.baseline, 'baseline');
        renderMath(middlePaneContent);

        // ITS result (right pane)
        rightPaneTitle.textContent = 'ITS Enhanced';
        renderITSPane(scenario, rightPaneContent);
    }

    // Render performance details below results
    renderPerformanceDetails(scenario);
}

function renderITSPane(scenario, container) {
    const algName = ALGORITHM_DESCRIPTIONS[scenario.algorithm]?.name || scenario.algorithm;

    // Why ITS Helped banner
    const whyBanner = `
        <div class="why-its-helped-banner">
            <div class="why-its-helped-icon">💡</div>
            <div class="why-its-helped-content">
                <div class="why-its-helped-title">Why ITS Helped</div>
                <div class="why-its-helped-text">${scenario.whyItsHelped || 'ITS enhanced the model performance through intelligent scaling.'}</div>
            </div>
        </div>
    `;

    // Result content with conclusion extraction
    const conclusion = extractConclusion(scenario.its.response);
    let resultContent = `<div class="chat-response">${formatAsHTML(conclusion)}</div>`;
    if (conclusion !== scenario.its.response) {
        resultContent += `
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
                            ${formatReasoningSteps(scenario.its.response)}
                        </div>
                    </div>
                </div>
            </div>
        `;
    }

    // Performance badges
    const perfBadges = renderPerformanceBadges(scenario.its, 'its');

    // Parse and render trace
    let traceHTML = '';
    try {
        const trace = typeof scenario.its.trace === 'string'
            ? JSON.parse(scenario.its.trace)
            : scenario.its.trace;
        if (trace && trace.algorithm) {
            const candidateCount = trace.candidates?.length || trace.num_iterations || 'multiple';
            const candidateLabel = trace.candidates ? 'candidate responses' : 'iterations';
            traceHTML = `
                <div class="trace-visualization-container">
                    <div class="trace-intro">
                        <strong>Algorithm Execution Trace</strong>
                        <p>See how ${algName} explored ${candidateCount} ${candidateLabel} to find the best answer.</p>
                    </div>
                    ${renderAlgorithmTrace(trace, true)}
                </div>
            `;
        }
    } catch (e) {
        console.error('Failed to parse trace:', e);
    }

    container.innerHTML = `
        ${whyBanner}
        ${resultContent}
        ${perfBadges}
        ${traceHTML}
    `;
    renderMath(container);
}

function copyWizardSummary() {
    const scenario = currentWizardScenario;
    if (!scenario) return;

    const algName = ALGORITHM_DESCRIPTIONS[scenario.algorithm]?.name || scenario.algorithm;
    const latencyChange = ((scenario.its.latency_ms - scenario.baseline.latency_ms) / scenario.baseline.latency_ms * 100).toFixed(1);
    const costChange = scenario.baseline.cost_usd > 0
        ? ((scenario.its.cost_usd - scenario.baseline.cost_usd) / scenario.baseline.cost_usd * 100).toFixed(1)
        : 'N/A';

    let summary = `ITS Demo Summary\n`;
    summary += `================\n`;
    summary += `Use Case: ${wizardState.useCase === 'match_frontier' ? 'Match Frontier Model' : 'Improve Model Performance'}\n`;
    summary += `Model: ${scenario.model}\n`;
    summary += `Algorithm: ${algName}\n`;
    summary += `Budget: ${scenario.budget} candidates\n\n`;
    summary += `Baseline: ${scenario.baseline.latency_ms}ms, $${scenario.baseline.cost_usd.toFixed(4)}\n`;
    summary += `ITS Enhanced: ${scenario.its.latency_ms}ms, $${scenario.its.cost_usd.toFixed(4)}\n`;
    summary += `Latency change: ${latencyChange > 0 ? '+' : ''}${latencyChange}%\n`;
    summary += `Cost change: ${costChange !== 'N/A' ? (costChange > 0 ? '+' : '') + costChange + '%' : 'N/A'}\n\n`;
    summary += `Prompt: ${scenario.prompt}\n`;
    if (scenario.whyItsHelped) {
        summary += `\nWhy ITS Helped: ${scenario.whyItsHelped}\n`;
    }

    navigator.clipboard.writeText(summary).then(() => {
        const btn = document.getElementById('copySummaryBtn');
        if (btn) {
            const orig = btn.textContent;
            btn.textContent = 'Copied!';
            setTimeout(() => { btn.textContent = orig; }, 1500);
        }
    });
}

function renderPerformanceBadges(data, type) {
    return `
        <div class="performance-badges">
            <span class="perf-badge latency">
                <span class="perf-label">Latency:</span>
                <span class="perf-value">${data.latency_ms || 'N/A'} ms</span>
            </span>
            <span class="perf-badge cost">
                <span class="perf-label">Cost:</span>
                <span class="perf-value">$${(data.cost_usd || 0).toFixed(4)}</span>
            </span>
            <span class="perf-badge tokens">
                <span class="perf-label">Tokens:</span>
                <span class="perf-value">${data.input_tokens || 0} in / ${data.output_tokens || 0} out</span>
            </span>
        </div>
    `;
}

function toggleTraceVisibility() {
    const content = document.getElementById('traceContent');
    const toggle = document.querySelector('.trace-toggle');
    const icon = document.querySelector('.trace-toggle-icon');

    const isHidden = content.classList.contains('hidden');
    setVisible(content, isHidden);
    icon.textContent = isHidden ? '▲' : '▼';
    toggle.classList.toggle('active', isHidden);
}

function renderPerformanceDetails(scenario) {
    // Create performance details container
    let perfContainer = document.getElementById('performanceDetailsContainer');
    if (!perfContainer) {
        perfContainer = document.createElement('div');
        perfContainer.id = 'performanceDetailsContainer';
        perfContainer.className = 'performance-details-section';

        // Insert after the results container
        const resultsContainer = document.getElementById('resultsContainer');
        if (resultsContainer && resultsContainer.parentNode) {
            resultsContainer.parentNode.insertBefore(perfContainer, resultsContainer.nextSibling);
        }
    }
    setVisible(perfContainer, true);

    // Calculate comparisons
    const baseline = scenario.baseline;
    const its = scenario.its;

    const latencyChange = ((its.latency_ms - baseline.latency_ms) / baseline.latency_ms * 100).toFixed(1);
    const costChange = baseline.cost_usd > 0
        ? ((its.cost_usd - baseline.cost_usd) / baseline.cost_usd * 100).toFixed(1)
        : 'N/A';
    const tokenChange = ((its.output_tokens - baseline.output_tokens) / baseline.output_tokens * 100).toFixed(1);

    // Color logic: cost/latency increases are expected ITS tradeoff (neutral gray), decreases are wins (green)
    const latencyClass = parseFloat(latencyChange) > 0 ? 'neutral' : 'positive';
    const costClass = costChange !== 'N/A' ? (parseFloat(costChange) > 0 ? 'neutral' : (parseFloat(costChange) < 0 ? 'positive' : 'neutral')) : 'neutral';
    const algName = ALGORITHM_DESCRIPTIONS[scenario.algorithm]?.name || scenario.algorithm;

    const perfHTML = `
        <div class="perf-details-header">
            <h3>Performance Comparison</h3>
            <span class="perf-details-subtitle">Baseline vs ITS Enhanced</span>
        </div>

        <div class="perf-metrics-grid">
            <!-- Quality -->
            <div class="perf-metric-card quality-highlight">
                <div class="perf-metric-label">Quality</div>
                <div class="perf-metric-values">
                    <span class="metric-its" style="font-size: 20px;">Enhanced</span>
                </div>
                <div class="perf-metric-change positive">
                    Best of ${scenario.budget} candidates
                </div>
            </div>

            <!-- Latency -->
            <div class="perf-metric-card">
                <div class="perf-metric-label">Latency (ITS Investment)</div>
                <div class="perf-metric-values">
                    <span class="metric-baseline">${baseline.latency_ms} ms</span>
                    <span class="metric-arrow">→</span>
                    <span class="metric-its">${its.latency_ms} ms</span>
                </div>
                <div class="perf-metric-change ${latencyClass}">
                    ${latencyChange > 0 ? '+' : ''}${latencyChange}%
                </div>
                <div class="perf-metric-note">Expected tradeoff for higher quality</div>
            </div>

            <!-- Cost -->
            <div class="perf-metric-card">
                <div class="perf-metric-label">Cost (ITS Investment)</div>
                <div class="perf-metric-values">
                    <span class="metric-baseline">$${baseline.cost_usd.toFixed(4)}</span>
                    <span class="metric-arrow">→</span>
                    <span class="metric-its">$${its.cost_usd.toFixed(4)}</span>
                </div>
                <div class="perf-metric-change ${costClass}">
                    ${costChange !== 'N/A' ? (costChange > 0 ? '+' : '') + costChange + '%' : costChange}
                </div>
                <div class="perf-metric-note">Expected tradeoff for higher quality</div>
            </div>

            <!-- Tokens -->
            <div class="perf-metric-card">
                <div class="perf-metric-label">Output Tokens</div>
                <div class="perf-metric-values">
                    <span class="metric-baseline">${baseline.output_tokens}</span>
                    <span class="metric-arrow">→</span>
                    <span class="metric-its">${its.output_tokens}</span>
                </div>
                <div class="perf-metric-change ${tokenChange > 0 ? 'neutral' : 'positive'}">
                    ${tokenChange > 0 ? '+' : ''}${tokenChange}%
                </div>
            </div>
        </div>
    `;

    perfContainer.innerHTML = perfHTML;
}

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

    // Configure experience
    if (experience === 'guided') {
        // Set to guided mode (non-expert)
        isExpertMode = false;
        localStorage.setItem('expertMode', 'false');
        setVisible(document.getElementById('expertModeToggle'), false);

        // Wizard initialization is handled by experience:selected event in guided-demo.js
    } else {
        // Interactive mode - show expert toggle, default to non-expert
        isExpertMode = false;
        localStorage.setItem('expertMode', 'false');
        setVisible(document.getElementById('expertModeToggle'), true);
    }

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

    // Reset wizard state
    wizardState = { useCase: null, selectedModel: null, frontierModel: null, algorithm: null, currentStep: 1 };
    currentWizardScenario = null;

    // Clean up wizard DOM elements
    const perfContainer = document.getElementById('performanceDetailsContainer');
    if (perfContainer) perfContainer.remove();
    const headlineBanner = document.getElementById('wizardResultsHeadline');
    if (headlineBanner) headlineBanner.remove();

    // Hide wizard UI
    const wizard = document.getElementById('guidedWizard');
    setVisible(wizard, false);
    const backBtn = document.getElementById('wizardBackBtn');
    setVisible(backBtn, false);
    const promptDisplay = document.getElementById('wizardPromptDisplay');
    setVisible(promptDisplay, false);

    // Re-enable question textarea
    const questionEl = document.getElementById('question');
    if (questionEl) questionEl.disabled = false;

    // Restore hidden UI elements
    const questionSection = document.getElementById('questionSection');
    if (questionSection) {
        const formGroup = questionSection.querySelector('.form-group');
        setVisible(formGroup, true);
        const actionButtons = questionSection.querySelector('.action-buttons');
        setVisible(actionButtons, true);
        const exampleQuestions = questionSection.querySelector('.example-questions-container');
        setVisible(exampleQuestions, true);
    }

    // Hide results
    const resultsContainer = document.getElementById('resultsContainer');
    setVisible(resultsContainer, false);

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

    updateUIForExpertMode();

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

// Expert Mode management
function toggleExpertMode() {
    // Prevent toggling during active run
    if (isRunning) {
        showError('Cannot change mode during active comparison. Please wait for the current run to complete.');
        return;
    }

    isExpertMode = !isExpertMode;
    localStorage.setItem('expertMode', isExpertMode ? 'true' : 'false');

    // Update toggle button visual state
    const toggleButton = document.getElementById('expertModeToggle');
    if (isExpertMode) {
        toggleButton.classList.add('active');
    } else {
        toggleButton.classList.remove('active');
    }

    // Update UI based on mode
    updateUIForExpertMode();
}

// Load saved expert mode preference
const savedExpertMode = localStorage.getItem('expertMode') === 'true';
isExpertMode = savedExpertMode;
if (isExpertMode) {
    document.getElementById('expertModeToggle').classList.add('active');
}

// Update UI based on expert mode
function updateUIForExpertMode() {
    const useCaseSection = document.querySelector('.section:has(input[name="useCase"])');
    const algorithmGroup = document.getElementById('algorithm').closest('.form-group');
    const budgetGroup = document.getElementById('budget').closest('.form-group');
    const algorithmSelect = document.getElementById('algorithm');
    const budgetSlider = document.getElementById('budget');
    const algorithmInfo = document.getElementById('algorithmInfo');
    const exampleQuestionsContainer = document.querySelector('.example-questions-container');
    const configDescription = document.getElementById('configDescription');
    const frontierModelGroup = document.getElementById('frontierModelGroup');
    const clearButton = document.querySelector('.btn-secondary');
    const configSectionTitle = document.querySelector('.section:has(#modelSelectionGrid) .section-title');
    const questionSectionTitle = document.querySelector('.section:has(#question) .section-title');
    const questionSectionDescription = document.querySelector('.section:has(#question) .section-description');
    const headerSubtitle = document.getElementById('headerSubtitle');
    const guidedQuestions = document.getElementById('guidedQuestions');
    const questionTextarea = document.getElementById('question');
    const scenarioSelectorContainer = document.getElementById('scenarioSelectorContainer');
    const guidedDemoBadge = document.getElementById('guidedDemoBadge');
    const runButton = document.getElementById('runButton');
    const configSection = document.querySelector('.section:has(#modelSelectionGrid)');
    const questionSection = document.querySelector('.section:has(#question)');

    // GUIDED DEMO: Always offline scenarios, no expert mode
    if (selectedExperience === 'guided') {
        // Hide use case selection entirely
        setVisible(useCaseSection, false);

        // Hide budget slider
        setVisible(budgetGroup, false);

        // Hide algorithm info cards
        setVisible(algorithmInfo, false);

        // Hide example questions
        setVisible(exampleQuestionsContainer, false);

        // Hide config description
        setVisible(configDescription, false);

        // Hide clear button
        setVisible(clearButton, false);

        // Hide frontier model group
        setVisible(frontierModelGroup, false);

        // Disable use case radio buttons
        document.querySelectorAll('input[name="useCase"]').forEach(radio => {
            radio.disabled = true;
        });

        // Set default budget
        budgetSlider.value = 16;
        document.getElementById('budgetValue').textContent = '16';

        // Hide guided questions
        if (guidedQuestions) {
            guidedQuestions.classList.remove('visible');
        }

        // Show scenario selector
        if (scenarioSelectorContainer) {
            scenarioSelectorContainer.classList.add('visible');
        }

        // Show guided demo badge
        setVisible(guidedDemoBadge, true);

        // Hide Run button (scenarios are pre-loaded)
        setVisible(runButton, false);

        // Hide configuration section (scenarios are pre-configured)
        setVisible(configSection, false);

        // Make question textarea read-only
        if (questionTextarea) {
            questionTextarea.placeholder = 'Question from selected scenario';
            questionTextarea.disabled = true;
        }

        // Update headers for guided mode
        if (headerSubtitle) {
            headerSubtitle.textContent = 'Explore pre-loaded scenarios showing how ITS can improve model responses';
        }
        if (questionSectionTitle) {
            questionSectionTitle.textContent = 'Scenario Prompt';
        }
        setVisible(questionSectionDescription, false);
        if (configSectionTitle) {
            configSectionTitle.textContent = 'Setup';
        }

    // INTERACTIVE DEMO: Original live experience with expert mode toggle
    } else if (selectedExperience === 'interactive') {
        // Always hide scenario selector and guided badge in interactive mode
        if (scenarioSelectorContainer) {
            scenarioSelectorContainer.classList.remove('visible');
        }
        setVisible(guidedDemoBadge, false);

        // Expert Mode: Show all controls (original behavior)
        if (isExpertMode) {
            setVisible(useCaseSection, true);

            // Show budget slider
            setVisible(budgetGroup, true);

            // Show algorithm info
            setVisible(algorithmInfo, true);

            // Show example questions
            setVisible(exampleQuestionsContainer, true);

            // Show config description
            setVisible(configDescription, true);

            // Show clear button
            setVisible(clearButton, true);

            // Enable algorithm selection
            algorithmSelect.disabled = false;

            // Enable budget slider
            budgetSlider.disabled = false;

            // Enable use case radio buttons
            document.querySelectorAll('input[name="useCase"]').forEach(radio => {
                radio.disabled = false;
            });

            // Restore section titles and descriptions
            if (configSectionTitle) {
                configSectionTitle.textContent = 'Configuration';
            }
            if (questionSectionTitle) {
                questionSectionTitle.textContent = 'Your Question';
            }
            if (questionSectionDescription) {
                setVisible(questionSectionDescription, true);
                questionSectionDescription.textContent = 'Ask anything or try an example';
            }
            if (headerSubtitle) {
                headerSubtitle.textContent = 'Compare baseline inference with ITS algorithms side by side';
            }

            // Hide guided questions
            if (guidedQuestions) {
                guidedQuestions.classList.remove('visible');
            }

            // Show Run button
            setVisible(runButton, true);

            // Show configuration section
            setVisible(configSection, true);

            // Restore placeholder
            if (questionTextarea) {
                questionTextarea.placeholder = 'Enter your question here...';
                questionTextarea.disabled = false;
            }

        // Non-Expert Mode: Simplified but still live (original behavior)
        } else {
            // Simplify section headers
            if (configSectionTitle) {
                configSectionTitle.textContent = 'Setup';
            }
            if (questionSectionTitle) {
                questionSectionTitle.textContent = 'Your Question';
            }
            setVisible(questionSectionDescription, false);
            if (headerSubtitle) {
                headerSubtitle.textContent = 'Compare model responses with different ITS algorithms';
            }

            // Hide use case selection
            setVisible(useCaseSection, false);

            // Hide budget slider
            setVisible(budgetGroup, false);

            // Hide algorithm info cards
            setVisible(algorithmInfo, false);

            // Hide example questions
            setVisible(exampleQuestionsContainer, false);

            // Hide config description
            setVisible(configDescription, false);

            // Hide clear button
            setVisible(clearButton, false);

            // Hide frontier model group if visible
            setVisible(frontierModelGroup, false);

            // Set default budget
            budgetSlider.value = 16;
            document.getElementById('budgetValue').textContent = '16';

            // Hide guided questions
            if (guidedQuestions) {
                guidedQuestions.classList.remove('visible');
            }

            // Show Run button (live API calls)
            setVisible(runButton, true);

            // Show configuration section
            setVisible(configSection, true);

            // Enable question textarea
            if (questionTextarea) {
                questionTextarea.placeholder = 'Enter your question here...';
                questionTextarea.disabled = false;
            }
        }
    }
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

// OFFLINE SCENARIO FUNCTIONS

// Populate scenario dropdown
function populateScenarioDropdown() {
    const dropdown = document.getElementById('scenarioDropdown');
    dropdown.innerHTML = '';

    OFFLINE_SCENARIOS.forEach((scenario, index) => {
        const option = document.createElement('option');
        option.value = index;
        option.textContent = `${index + 1}. ${scenario.title} (${scenario.algorithm})`;
        dropdown.appendChild(option);
    });

    dropdown.value = currentScenarioIndex;
}

// Navigate between scenarios
function navigateScenario(direction) {
    const newIndex = currentScenarioIndex + direction;
    if (newIndex >= 0 && newIndex < OFFLINE_SCENARIOS.length) {
        currentScenarioIndex = newIndex;
        loadScenario(currentScenarioIndex);
    }
}

// Load and render a scenario
function loadScenario(index) {
    currentScenarioIndex = index;
    const scenario = OFFLINE_SCENARIOS[index];

    // Update dropdown
    document.getElementById('scenarioDropdown').value = index;

    // Update navigation buttons
    document.getElementById('prevScenarioBtn').disabled = (index === 0);
    document.getElementById('nextScenarioBtn').disabled = (index === OFFLINE_SCENARIOS.length - 1);

    // Update scenario info display
    const scenarioInfo = document.getElementById('scenarioInfo');
    scenarioInfo.innerHTML = `
        <div>
            <div style="margin-bottom: 8px;">
                ${scenario.tags.map(tag => `<span class="scenario-tag">${tag}</span>`).join('')}
            </div>
            <div class="scenario-meta">
                <div class="scenario-meta-item">
                    <strong>Algorithm:</strong> ${scenario.algorithm}
                </div>
                <div class="scenario-meta-item">
                    <strong>Budget:</strong> ${scenario.budget}
                </div>
                <div class="scenario-meta-item">
                    <strong>Model:</strong> ${scenario.model}
                </div>
            </div>
        </div>
    `;

    // Update question field (read-only in guided mode)
    document.getElementById('question').value = scenario.prompt;

    // Update algorithm dropdown to match scenario
    document.getElementById('algorithm').value = scenario.algorithm;
    selectedAlgorithm = scenario.algorithm;

    // Render the scenario results
    renderScenarioResults(scenario);
}

// Render scenario results in the result panes
function renderScenarioResults(scenario) {
    // Show results container
    const resultsContainer = document.getElementById('resultsContainer');

    // Middle pane: Baseline (ITS off)
    const middlePaneContent = document.getElementById('middlePaneContent');
    middlePaneContent.innerHTML = renderResponse({
        response: scenario.baseline.response,
        latency_ms: scenario.baseline.latency_ms,
        input_tokens: scenario.baseline.input_tokens,
        output_tokens: scenario.baseline.output_tokens,
        cost_usd: scenario.baseline.cost_usd,
        model_size: scenario.baseline.model_size
    });

    // Show latency badge
    const middlePaneLatency = document.getElementById('middlePaneLatency');
    middlePaneLatency.textContent = `${scenario.baseline.latency_ms}ms`;
    setVisible(middlePaneLatency, true);

    // Show cost badge if available
    if (scenario.baseline.cost_usd > 0) {
        const middlePaneCost = document.getElementById('middlePaneCost');
        const costFormatted = scenario.baseline.cost_usd < 0.0001
            ? `$${scenario.baseline.cost_usd.toExponential(2)}`
            : `$${scenario.baseline.cost_usd.toFixed(4)}`;
        middlePaneCost.textContent = costFormatted;
        setVisible(middlePaneCost, true);
    }

    // Show actions
    setVisible(document.getElementById('middlePaneActions'), true);

    // Right pane: ITS result
    const rightPaneContent = document.getElementById('rightPaneContent');
    rightPaneContent.innerHTML = renderResponse({
        response: scenario.its.response,
        latency_ms: scenario.its.latency_ms,
        input_tokens: scenario.its.input_tokens,
        output_tokens: scenario.its.output_tokens,
        cost_usd: scenario.its.cost_usd,
        model_size: scenario.its.model_size,
        trace: scenario.its.trace
    });

    // Add "Why ITS Helped" banner at the top of ITS result
    const whyItsHelpedBanner = `
        <div style="
            background: var(--primary-light);
            border-left: 4px solid var(--primary);
            padding: 12px 16px;
            margin-bottom: 16px;
            border-radius: var(--radius-sm);
        ">
            <div style="font-size: 11px; font-weight: 700; color: var(--primary); text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 6px;">
                💡 Why ITS Helped
            </div>
            <div style="font-size: 13px; color: var(--text-primary); line-height: 1.6;">
                ${scenario.whyItsHelped}
            </div>
        </div>
    `;
    rightPaneContent.innerHTML = whyItsHelpedBanner + rightPaneContent.innerHTML;

    // Show latency badge
    const rightPaneLatency = document.getElementById('rightPaneLatency');
    rightPaneLatency.textContent = `${scenario.its.latency_ms}ms`;
    setVisible(rightPaneLatency, true);

    // Show cost badge if available
    if (scenario.its.cost_usd > 0) {
        const rightPaneCost = document.getElementById('rightPaneCost');
        const costFormatted = scenario.its.cost_usd < 0.0001
            ? `$${scenario.its.cost_usd.toExponential(2)}`
            : `$${scenario.its.cost_usd.toFixed(4)}`;
        rightPaneCost.textContent = costFormatted;
        setVisible(rightPaneCost, true);

        // Add cost comparison badge
        const costMultiple = (scenario.its.cost_usd / scenario.baseline.cost_usd).toFixed(1);
        if (costMultiple > 1) {
            rightPaneCost.classList.add('expensive');
        }
    }

    // Show actions
    setVisible(document.getElementById('rightPaneActions'), true);

    // Hide performance visualization for now
    setVisible(document.getElementById('performance-visualization-container'), false);
}

// Helper function to render response HTML (simplified version)
function renderResponse(data) {
    let html = '';

    // If response contains TODO, show placeholder
    if (data.response.includes('TODO')) {
        html = `
            <div style="padding: 40px; text-align: center; color: var(--text-tertiary);">
                <div style="font-size: 48px; margin-bottom: 16px; opacity: 0.3;">📝</div>
                <div style="font-size: 14px; font-weight: 600; margin-bottom: 8px;">Scenario Not Yet Captured</div>
                <div style="font-size: 13px;">This scenario will be populated with real run data in Phase B.</div>
            </div>
        `;
        return html;
    }

    // Main response
    html += `<div class="chat-response">${formatAsHTML(data.response)}</div>`;

    // Performance details (expandable)
    html += `
        <div class="expandable-section" style="margin-top: 16px;">
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
                        <span class="detail-value">${data.latency_ms || 0}ms</span>
                    </div>
    `;

    if (data.input_tokens > 0) {
        html += `
            <div class="detail-row">
                <span class="detail-label">Input Tokens</span>
                <span class="detail-value">${data.input_tokens.toLocaleString()}</span>
            </div>
        `;
    }

    if (data.output_tokens > 0) {
        html += `
            <div class="detail-row">
                <span class="detail-label">Output Tokens</span>
                <span class="detail-value">${data.output_tokens.toLocaleString()}</span>
            </div>
        `;
    }

    if (data.cost_usd > 0) {
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

    html += `
                </div>
            </div>
        </div>
    `;

    // Add trace section if available
    if (data.trace && !data.trace.includes('TODO')) {
        html += `
            <div class="expandable-section" style="margin-top: 12px;">
                <button class="expand-button" onclick="toggleSection(this)">
                    <svg class="expand-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
                        <path d="M4 6L8 10L12 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                    <span>Algorithm Trace</span>
                </button>
                <div class="expandable-content">
                    <div class="trace-section">
                        <div style="font-size: 13px; line-height: 1.6; color: var(--text-primary);">
                            ${formatAsHTML(data.trace)}
                        </div>
                    </div>
                </div>
            </div>
        `;
    }

    return html;
}

// Populate guided mode questions based on algorithm
function populateGuidedQuestions() {
    const questionsGrid = document.getElementById('guidedQuestionsGrid');
    const questions = GUIDED_MODE_QUESTIONS[selectedAlgorithm] || [];

    questionsGrid.innerHTML = '';

    questions.forEach((question, index) => {
        const card = document.createElement('label');
        card.className = 'guided-question-card';
        card.innerHTML = `
            <input type="radio" name="guidedQuestion" value="${index}">
            <div class="guided-question-text">
                <span class="guided-question-number">${index + 1}</span>
                <span>${question}</span>
            </div>
        `;

        card.addEventListener('click', () => {
            // Remove selected class from all cards
            document.querySelectorAll('.guided-question-card').forEach(c => {
                c.classList.remove('selected');
            });
            // Add selected class to clicked card
            card.classList.add('selected');
            // Update textarea with selected question
            document.getElementById('question').value = question;
        });

        questionsGrid.appendChild(card);
    });
}

// Handle algorithm change
function onAlgorithmChange() {
    selectedAlgorithm = document.getElementById('algorithm').value;
    showAlgorithmInfo(selectedAlgorithm);
    loadExampleQuestions();

    // Update guided questions if in guided mode
    if (!isExpertMode) {
        populateGuidedQuestions();
        // Clear any previous selection
        document.getElementById('question').value = '';
        document.querySelectorAll('.guided-question-card').forEach(c => {
            c.classList.remove('selected');
        });
    }
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
    // In guided mode, prevent budget changes
    if (!isExpertMode) {
        e.target.value = 16;
        document.getElementById('budgetValue').textContent = '16';
        return;
    }

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

// Extract final answer from text
function extractFinalAnswer(text) {
    // Try to find boxed answer (common in math problems)
    // Need to handle nested braces properly for things like \boxed{\frac{14}{3}}
    const boxedIndex = text.indexOf('\\boxed{');
    if (boxedIndex !== -1) {
        let braceCount = 0;
        let startIndex = boxedIndex + 7; // Start after '\boxed{'
        let endIndex = startIndex;

        for (let i = startIndex; i < text.length; i++) {
            if (text[i] === '{') {
                braceCount++;
            } else if (text[i] === '}') {
                if (braceCount === 0) {
                    endIndex = i;
                    break;
                }
                braceCount--;
            }
        }

        if (endIndex > startIndex) {
            return text.substring(startIndex, endIndex);
        }
    }

    // Try to find explicit "Final Answer:" patterns
    const finalAnswerPatterns = [
        /Final Answer:\s*(.+?)(?:\n\n|$)/is,
        /Answer:\s*(.+?)(?:\n\n|$)/is,
        /Therefore,?\s+the\s+(?:answer|value|result)\s+(?:is|equals?)\s+(.+?)(?:\.|$)/is,
        /Therefore,?\s+(.+?)(?:\n\n|$)/is,
        /Thus,?\s+(.+?)(?:\n\n|$)/is,
        /So,?\s+(.+?)(?:\n\n|$)/is,
        /In conclusion,?\s+(.+?)(?:\n\n|$)/is
    ];

    for (const pattern of finalAnswerPatterns) {
        const match = text.match(pattern);
        if (match) {
            let answer = match[1].trim();

            // Extract just the math expression if it's in the format "is $...$"
            const mathMatch = answer.match(/\$([^$]+)\$/);
            if (mathMatch) {
                answer = mathMatch[1];
            }

            // Only use if it's reasonably short (not a whole paragraph)
            if (answer.length < 200) {
                return answer;
            }
        }
    }

    // Look for standalone answer at the end
    const lines = text.trim().split('\n');
    const lastLine = lines[lines.length - 1].trim();

    // If last line is short and looks like an answer
    if (lastLine.length < 150 && (
        /^[A-Z]:|^[-+]?\d+/.test(lastLine) ||
        /\$.*\$/.test(lastLine) ||
        /^x\s*=|^y\s*=/i.test(lastLine) ||
        /=.*\d/.test(lastLine)
    )) {
        return lastLine;
    }

    return null;
}

// Format and render answer text
function formatAnswer(text) {
    if (!text || !text.trim()) return '';

    // Extract final answer
    const finalAnswer = extractFinalAnswer(text);

    // Remove final answer from main text if present
    let mainText = text;
    if (finalAnswer) {
        // Remove boxed answer notation
        mainText = text.replace(/\\boxed\{[^}]+\}/, '').trim();

        // Remove common "Final Answer:" patterns
        mainText = mainText.replace(/(?:Final Answer|Answer|Therefore|Thus|So|In conclusion)[:\s]+[^\n]+/gi, '').trim();

        // If the final answer is the last line, remove it from main text to avoid duplication
        const lines = mainText.trim().split('\n');
        const lastLine = lines[lines.length - 1].trim();
        if (lastLine === finalAnswer || lastLine.includes(finalAnswer)) {
            lines.pop();
            mainText = lines.join('\n').trim();
        }

        // If the entire text is just the final answer (no reasoning), clear mainText
        if (mainText === finalAnswer || mainText.trim() === finalAnswer.trim()) {
            mainText = '';
        }
    }

    let html = '';

    // Add final answer section at the top
    if (finalAnswer) {
        // Check if the answer contains LaTeX
        const hasLatex = finalAnswer.includes('$') || /[\\{}_^]/.test(finalAnswer);
        const displayAnswer = hasLatex
            ? finalAnswer
            : (finalAnswer.includes('=') || /^[-+]?\d/.test(finalAnswer))
                ? '$' + finalAnswer + '$'
                : finalAnswer;

        html += `
            <div class="final-answer-section">
                <div class="final-answer-label">
                    <span>✓</span>
                    <span>Final Answer</span>
                </div>
                <div class="final-answer-content">${displayAnswer}</div>
            </div>
        `;
    }

    // Add reasoning section - show full text if no answer extracted
    const contentToShow = mainText && mainText.trim() ? mainText : text;
    if (contentToShow && contentToShow.trim()) {
        const sectionTitle = finalAnswer ? 'Reasoning Steps' : 'Response';
        const sectionIcon = finalAnswer ? '📝' : '💬';

        // Check if content is very short (just one paragraph)
        const isShort = contentToShow.length < 300 && !contentToShow.includes('\n\n');

        if (isShort && !finalAnswer) {
            // For short responses without final answer, just show directly
            html += `<div>${formatReasoningSteps(contentToShow)}</div>`;
        } else {
            // Show in collapsible section
            html += `
                <div class="reasoning-section">
                    <div class="reasoning-header" onclick="toggleReasoning(event)">
                        <div class="reasoning-title">
                            <span>${sectionIcon}</span>
                            <span>${sectionTitle}</span>
                        </div>
                        <div class="reasoning-toggle">▼</div>
                    </div>
                    <div class="reasoning-content">
                        ${formatReasoningSteps(contentToShow)}
                    </div>
                </div>
            `;
        }
    }

    return html || '<p>No answer provided</p>';
}

// Format the reasoning steps
function formatReasoningSteps(text) {
    // Split into paragraphs
    let paragraphs = text.split(/\n\n+/);

    // If no double line breaks, try single line breaks for numbered lists
    if (paragraphs.length === 1 && /\n\d+\./.test(text)) {
        paragraphs = text.split(/\n(?=\d+\.)/);
    }

    // Check if this looks like step-by-step
    const hasSteps = paragraphs.some(p =>
        /^(\d+\.|Step \d+:?|\*\*Step \d+|#+ Step|\d+\))/i.test(p.trim())
    );

    let html = '';

    if (hasSteps) {
        // Format as steps with visual markers
        let stepNum = 1;
        paragraphs.forEach(para => {
            const trimmed = para.trim();
            if (trimmed) {
                html += `<div class="answer-step">`;

                // Extract step number if present, otherwise use counter
                const stepMatch = trimmed.match(/^(\d+)[\.\)]/);
                const currentStepNum = stepMatch ? stepMatch[1] : stepNum;

                // Add step marker
                html += `<span class="step-marker">${currentStepNum}</span>`;

                // Remove step prefix from text if present
                let stepText = trimmed.replace(/^(\d+[\.\)]|Step \d+:?|\*\*Step \d+\*\*:?|#+ Step \d+:?)\s*/i, '');

                html += `<span>${formatParagraph(stepText)}</span>`;
                html += `</div>`;

                stepNum++;
            }
        });
    } else {
        // Format as regular paragraphs
        paragraphs.forEach(para => {
            const trimmed = para.trim();
            if (trimmed) {
                html += `<p>${formatParagraph(trimmed)}</p>`;
            }
        });
    }

    return html;
}

// Format expected answer (simpler version)
function formatExpectedAnswer(text) {
    if (!text || !text.trim()) return '';

    // Check if it looks like it has LaTeX
    const hasLatex = text.includes('$') || /\\[a-z]+\{/.test(text);

    // If it has clear paragraphs, format them
    if (text.includes('\n\n')) {
        const paragraphs = text.split(/\n\n+/);
        return paragraphs.map(p => `<p>${formatParagraph(p.trim())}</p>`).join('');
    }

    // Otherwise just format as a single paragraph
    return `<p>${formatParagraph(text.trim())}</p>`;
}

// Toggle reasoning section
function toggleReasoning(event) {
    const header = event.currentTarget;
    const content = header.nextElementSibling;
    const toggle = header.querySelector('.reasoning-toggle');

    content.classList.toggle('collapsed');
    toggle.classList.toggle('collapsed');
}

function formatParagraph(text) {
    // Convert **bold** to <strong>
    text = text.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');

    // Convert *italic* to <em> (but not if it's part of math expression)
    text = text.replace(/\*([^*$]+?)\*/g, '<em>$1</em>');

    // Convert ### headers to styled headers (if present)
    text = text.replace(/^###\s+(.+)$/gm, '<strong style="font-size: 1.1em; display: block; margin: 8px 0;">$1</strong>');
    text = text.replace(/^##\s+(.+)$/gm, '<strong style="font-size: 1.2em; display: block; margin: 10px 0;">$1</strong>');

    // Preserve line breaks within paragraph
    text = text.replace(/\n/g, '<br>');

    return text;
}

function renderMath(element) {
    if (typeof renderMathInElement !== 'undefined') {
        renderMathInElement(element, {
            delimiters: [
                {left: '$$', right: '$$', display: true},
                {left: '$', right: '$', display: false},
                {left: '\\[', right: '\\]', display: true},
                {left: '\\(', right: '\\)', display: false}
            ],
            throwOnError: false,
            errorColor: '#cc0000',
            trust: true
        });
    }
}

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
        // Mark as expensive if above threshold
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

// Extract the concluding answer from a model response.
// Returns the last meaningful paragraph(s) that contain the answer,
// stripping away the step-by-step reasoning.
function extractConclusion(text) {
    if (!text || !text.trim()) return text;

    // Split into paragraphs
    const paragraphs = text.trim().split(/\n\n+/).map(p => p.trim()).filter(p => p);

    // If 3 or fewer paragraphs, it's already short — show it all
    if (paragraphs.length <= 3) return text;

    // Look for concluding paragraphs from the end
    // These typically start with conclusion-like phrases or contain "answer is"
    const conclusionPatterns = [
        /^(therefore|thus|so,|hence|in conclusion|the answer|the sum|the value|the result|the remainder|the probability|the largest|the number|there are|this gives|we get|we find|we have|finally)/i,
        /answer is/i,
        /=\s*\$?[\d\\]/,
        /\\boxed/,
    ];

    // Walk backwards to find where the conclusion starts
    let conclusionStart = paragraphs.length - 1;
    for (let i = paragraphs.length - 1; i >= Math.max(0, paragraphs.length - 3); i--) {
        const p = paragraphs[i];
        const isConclusion = conclusionPatterns.some(pat => pat.test(p));
        if (isConclusion) {
            conclusionStart = i;
        }
    }

    // Take from the conclusion start to the end
    const conclusion = paragraphs.slice(conclusionStart).join('\n\n');
    return conclusion;
}

// Format text as HTML paragraphs
function formatAsHTML(text) {
    if (!text || !text.trim()) return '<p>No response</p>';

    let html = text;

    // Escape HTML but preserve $ for KaTeX
    html = html.replace(/&/g, '&amp;')
               .replace(/</g, '&lt;')
               .replace(/>/g, '&gt;');

    // Bold: **text**
    html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');

    // Split into paragraphs on double newlines
    const paragraphs = html.split(/\n\n+/);
    html = paragraphs.map(p => {
        p = p.trim();
        if (!p) return '';
        p = p.replace(/\n/g, '<br>');
        return `<p>${p}</p>`;
    }).filter(p => p).join('');

    return html;
}

// --- Algorithm Trace Rendering ---

function toggleCandidateContent(btn) {
    const content = btn.previousElementSibling;
    content.classList.toggle('expanded-candidate');
    btn.textContent = content.classList.contains('expanded-candidate') ? 'Show less' : 'Show more';
}

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
