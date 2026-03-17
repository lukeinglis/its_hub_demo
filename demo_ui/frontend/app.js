/**
 * app.js — Core bootstrap, experience selection, shared constants & trace rendering
 *
 * ARCHITECTURE
 * ────────────
 * State ownership:
 *   - app.js owns global state (selectedExperience) and shared constants
 *     (API_BASE_URL, ALGORITHM_DESCRIPTIONS)
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
 *
 * What this file owns:
 *   - API_BASE_URL, ALGORITHM_DESCRIPTIONS constants
 *   - Theme management (toggleTheme)
 *   - Experience selection flow (selectExperience, returnToLanding,
 *     checkSavedExperience, initializeApp)
 *   - Algorithm trace rendering (renderAlgorithmTrace and helpers)
 *   - Shared UI helpers (toggleSection, switchIteration, renderToolCalls,
 *     renderCandidateCard)
 *   - Keyboard accessibility handler
 */

const API_BASE_URL = 'http://localhost:8000';

// Shared utilities (setVisible, escapeHtml, formatLatency, formatCost, etc.)
// are defined in utils.js which is loaded before this file.

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

    // Notify demo modules
    document.dispatchEvent(new CustomEvent('experience:selected', { detail: { experience } }));
}

function returnToLanding() {
    // Notify demo modules to tear down before we reset
    document.dispatchEvent(new CustomEvent('experience:teardown'));
    // Clear selection
    selectedExperience = null;
    localStorage.removeItem('selectedExperience');

    // Hide guided wizard UI
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

// Check if user has a saved experience preference
function checkSavedExperience() {
    const savedExperience = localStorage.getItem('selectedExperience');
    if (savedExperience) {
        // Auto-select the previously chosen experience
        selectExperience(savedExperience);
    }
    // Otherwise, show landing page (default state)
}

// Formatting/rendering utilities (extractFinalAnswer, formatAnswer,
// formatReasoningSteps, extractConclusion, formatAsHTML, renderMath,
// toggleReasoning, toggleCandidateContent, etc.) are in utils.js.

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
        // Clean up Python tuple representations like "('17',)" → "17"
        let displayAnswer = answer;
        const tupleMatch = displayAnswer.match(/^\('?(.+?)'?,?\)$/);
        if (tupleMatch) displayAnswer = tupleMatch[1];
        // Also strip None
        if (displayAnswer === 'None' || displayAnswer === '(None,)') displayAnswer = '(no answer)';
        // Try to extract boxed answer from full response text
        if (displayAnswer.length > 80 && typeof extractFinalAnswer === 'function') {
            const extracted = extractFinalAnswer(displayAnswer);
            if (extracted) displayAnswer = extracted;
        }
        if (displayAnswer.length > 60) displayAnswer = displayAnswer.substring(0, 60) + '...';
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
