/**
 * utils.js — Shared formatting and rendering utilities
 *
 * Pure utility functions used across app.js, guided-demo.js, and
 * interactive-demo.js. This file must be loaded before any of those.
 *
 * Contains:
 *   - Visibility & safety: setVisible(), escapeHtml()
 *   - Formatting: formatLatency(), formatCost(), formatParagraph(), formatAsHTML()
 *   - Answer extraction: extractFinalAnswer(), extractConclusion()
 *   - Rendering: formatAnswer(), formatReasoningSteps(), formatExpectedAnswer()
 *   - UI helpers: renderMath(), toggleReasoning(), toggleCandidateContent()
 */

// ============================================================
// VISIBILITY & SAFETY
// ============================================================

// Unified visibility utility — replaces all style.display assignments.
// Uses the .hidden CSS class for consistent show/hide behavior.
function setVisible(el, show) {
    if (!el) return;
    if (show) el.classList.remove('hidden');
    else el.classList.add('hidden');
}

// Global HTML escaping utility — safe against XSS.
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = String(text);
    return div.innerHTML;
}

// ============================================================
// FORMATTING
// ============================================================

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

// ============================================================
// ANSWER EXTRACTION
// ============================================================

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

// ============================================================
// RENDERING
// ============================================================

function formatAnswer(text) {
    if (!text || !text.trim()) return '';

    // Extract final answer
    const finalAnswer = extractFinalAnswer(text);

    // Remove final answer from main text if present
    let mainText = text;
    if (finalAnswer) {
        mainText = text.replace(/\\boxed\{[^}]+\}/, '').trim();
        mainText = mainText.replace(/(?:Final Answer|Answer|Therefore|Thus|So|In conclusion)[:\s]+[^\n]+/gi, '').trim();

        const lines = mainText.trim().split('\n');
        const lastLine = lines[lines.length - 1].trim();
        if (lastLine === finalAnswer || lastLine.includes(finalAnswer)) {
            lines.pop();
            mainText = lines.join('\n').trim();
        }

        if (mainText === finalAnswer || mainText.trim() === finalAnswer.trim()) {
            mainText = '';
        }
    }

    let html = '';

    if (finalAnswer) {
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

    const contentToShow = mainText && mainText.trim() ? mainText : text;
    if (contentToShow && contentToShow.trim()) {
        const sectionTitle = finalAnswer ? 'Reasoning Steps' : 'Response';
        const sectionIcon = finalAnswer ? '📝' : '💬';
        const isShort = contentToShow.length < 300 && !contentToShow.includes('\n\n');

        if (isShort && !finalAnswer) {
            html += `<div>${formatReasoningSteps(contentToShow)}</div>`;
        } else {
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

function formatReasoningSteps(text) {
    let paragraphs = text.split(/\n\n+/);

    if (paragraphs.length === 1 && /\n\d+\./.test(text)) {
        paragraphs = text.split(/\n(?=\d+\.)/);
    }

    const hasSteps = paragraphs.some(p =>
        /^(\d+\.|Step \d+:?|\*\*Step \d+|#+ Step|\d+\))/i.test(p.trim())
    );

    let html = '';

    if (hasSteps) {
        let stepNum = 1;
        paragraphs.forEach(para => {
            const trimmed = para.trim();
            if (trimmed) {
                html += `<div class="answer-step">`;
                const stepMatch = trimmed.match(/^(\d+)[\.\)]/);
                const currentStepNum = stepMatch ? stepMatch[1] : stepNum;
                html += `<span class="step-marker">${currentStepNum}</span>`;
                let stepText = trimmed.replace(/^(\d+[\.\)]|Step \d+:?|\*\*Step \d+\*\*:?|#+ Step \d+:?)\s*/i, '');
                html += `<span>${formatParagraph(stepText)}</span>`;
                html += `</div>`;
                stepNum++;
            }
        });
    } else {
        paragraphs.forEach(para => {
            const trimmed = para.trim();
            if (trimmed) {
                html += `<p>${formatParagraph(trimmed)}</p>`;
            }
        });
    }

    return html;
}

function formatExpectedAnswer(text) {
    if (!text || !text.trim()) return '';

    if (text.includes('\n\n')) {
        const paragraphs = text.split(/\n\n+/);
        return paragraphs.map(p => `<p>${formatParagraph(p.trim())}</p>`).join('');
    }

    return `<p>${formatParagraph(text.trim())}</p>`;
}

// ============================================================
// UI HELPERS
// ============================================================

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

function toggleReasoning(event) {
    const header = event.currentTarget;
    const content = header.nextElementSibling;
    const toggle = header.querySelector('.reasoning-toggle');
    content.classList.toggle('collapsed');
    toggle.classList.toggle('collapsed');
}

function toggleCandidateContent(btn) {
    const content = btn.previousElementSibling;
    content.classList.toggle('expanded-candidate');
    btn.textContent = content.classList.contains('expanded-candidate') ? 'Show less' : 'Show more';
}

// ============================================================
// SAVINGS SUMMARY CARD
// ============================================================

/**
 * Build a savings summary card comparing ITS cost/infra vs baseline/frontier.
 *
 * @param {object} opts
 * @param {number} opts.itsCost - ITS result cost in USD
 * @param {number} opts.baselineCost - Baseline/frontier cost in USD
 * @param {boolean|null} opts.itsCorrect - Whether ITS got the correct answer
 * @param {boolean|null} opts.baselineCorrect - Whether baseline got the correct answer
 * @param {object|null} opts.infrastructure - Infrastructure metadata from meta.infrastructure
 * @param {string} opts.useCase - Use case ('improve_model', 'match_frontier', 'tool_consensus')
 * @returns {string} HTML string for the savings card
 */
function buildSavingsCard(opts) {
    const { itsCost, baselineCost, itsCorrect, baselineCorrect, infrastructure, useCase } = opts;

    const cards = [];

    // Cost savings (most relevant for match_frontier)
    if (itsCost != null && baselineCost != null && baselineCost > 0 && itsCost < baselineCost) {
        const pct = Math.round((1 - itsCost / baselineCost) * 100);
        cards.push({
            icon: '💰',
            label: 'Cost Savings',
            value: `${pct}%`,
            detail: `${formatCost(itsCost)} vs ${formatCost(baselineCost)} per request`,
        });
    } else if (itsCost != null && baselineCost != null && baselineCost > 0) {
        const ratio = (itsCost / baselineCost).toFixed(1);
        cards.push({
            icon: '💰',
            label: 'Cost Multiplier',
            value: `${ratio}x`,
            detail: `${formatCost(itsCost)} vs ${formatCost(baselineCost)} — small increase for better accuracy`,
        });
    }

    // Quality comparison
    if (itsCorrect != null || baselineCorrect != null) {
        if (itsCorrect && !baselineCorrect) {
            cards.push({
                icon: '✅',
                label: 'Quality',
                value: 'Corrected',
                detail: 'ITS fixed the baseline\'s incorrect answer',
            });
        } else if (itsCorrect && baselineCorrect) {
            cards.push({
                icon: '✅',
                label: 'Quality',
                value: 'Matched',
                detail: 'Both produced correct answers',
            });
        }
    }

    // Infrastructure / deployment story
    const infra = infrastructure || {};
    const modelInfra = infra.model || {};
    const frontierInfra = infra.frontier || {};

    if (modelInfra.self_hostable && modelInfra.min_gpu) {
        let detail = `Self-host on ${modelInfra.min_gpu}`;
        if (modelInfra.gpu_cloud_cost_hr) {
            detail += ` (~$${modelInfra.gpu_cloud_cost_hr.toFixed(2)}/hr)`;
        }
        if (useCase === 'match_frontier' && !frontierInfra.self_hostable) {
            cards.push({
                icon: '🖥️',
                label: 'Hardware',
                value: 'Single GPU',
                detail: `${detail} — frontier requires API subscription`,
            });
        } else if (modelInfra.self_hostable) {
            cards.push({
                icon: '🖥️',
                label: 'Deployable',
                value: modelInfra.min_gpu.split('(')[0].trim(),
                detail: detail,
            });
        }
    }

    // Deployment flexibility for self-hostable models
    if (modelInfra.self_hostable && useCase === 'match_frontier') {
        cards.push({
            icon: '🔒',
            label: 'Flexibility',
            value: 'Self-hosted',
            detail: 'Run on-premise, at the edge, or in air-gapped environments',
        });
    }

    if (cards.length === 0) return '';

    let html = '<div class="savings-card">';
    html += '<div class="savings-card-header">Savings Summary</div>';
    html += '<div class="savings-card-grid">';
    cards.forEach(card => {
        html += `
            <div class="savings-card-item">
                <div class="savings-card-icon">${card.icon}</div>
                <div class="savings-card-body">
                    <div class="savings-card-label">${card.label}</div>
                    <div class="savings-card-value">${card.value}</div>
                    <div class="savings-card-detail">${card.detail}</div>
                </div>
            </div>
        `;
    });
    html += '</div></div>';
    return html;
}
