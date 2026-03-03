/**
 * Enhanced Performance Visualization v2
 * Clear, visual charts showing performance differences
 */

class PerformanceVizV2 {
    constructor(containerId) {
        this.container = document.getElementById(containerId);
        if (!this.container) {
            throw new Error(`Container '${containerId}' not found`);
        }
    }

    render(data) {
        const isThreeColumn = !!data.small_baseline;

        let html = `
            <div class="perf-v2-container">
                <h2 class="perf-v2-title">📊 Performance Comparison</h2>
        `;

        // Big savings callout for 3-column
        if (isThreeColumn) {
            html += this.renderSavingsCallout(data.small_baseline, data.its, data.baseline);
        }

        // Metrics comparison section
        html += '<div class="perf-v2-metrics">';

        if (isThreeColumn) {
            html += this.renderMetricComparison(
                'Cost',
                '$',
                [
                    { label: 'Small Baseline', value: data.small_baseline.cost_usd, color: '#64748b' },
                    { label: 'Small + ITS', value: data.its.cost_usd, color: '#0ea5e9' },
                    { label: 'Frontier', value: data.baseline.cost_usd, color: '#8b5cf6' }
                ],
                'lower'
            );

            html += this.renderMetricComparison(
                'Latency',
                'ms',
                [
                    { label: 'Small Baseline', value: data.small_baseline.latency_ms, color: '#64748b' },
                    { label: 'Small + ITS', value: data.its.latency_ms, color: '#0ea5e9' },
                    { label: 'Frontier', value: data.baseline.latency_ms, color: '#8b5cf6' }
                ],
                'lower'
            );

            html += this.renderMetricComparison(
                'Total Tokens',
                '',
                [
                    { label: 'Small Baseline', value: (data.small_baseline.input_tokens || 0) + (data.small_baseline.output_tokens || 0), color: '#64748b' },
                    { label: 'Small + ITS', value: (data.its.input_tokens || 0) + (data.its.output_tokens || 0), color: '#0ea5e9' },
                    { label: 'Frontier', value: (data.baseline.input_tokens || 0) + (data.baseline.output_tokens || 0), color: '#8b5cf6' }
                ],
                'lower'
            );

            // Quality metric — only when correctness data exists
            if (data.small_baseline.is_correct != null || data.its.is_correct != null || data.baseline.is_correct != null) {
                html += this.renderMetricComparison(
                    'Quality',
                    '%',
                    [
                        { label: 'Small Baseline', value: data.small_baseline.is_correct === true ? 100 : data.small_baseline.is_correct === false ? 0 : null, color: '#64748b' },
                        { label: 'Small + ITS', value: data.its.is_correct === true ? 100 : data.its.is_correct === false ? 0 : null, color: '#0ea5e9' },
                        { label: 'Frontier', value: data.baseline.is_correct === true ? 100 : data.baseline.is_correct === false ? 0 : null, color: '#8b5cf6' }
                    ],
                    'higher'
                );
            }
        } else {
            html += this.renderMetricComparison(
                'Cost',
                '$',
                [
                    { label: 'Baseline', value: data.baseline.cost_usd, color: '#64748b' },
                    { label: 'ITS', value: data.its.cost_usd, color: '#0ea5e9' }
                ],
                'lower'
            );

            html += this.renderMetricComparison(
                'Latency',
                'ms',
                [
                    { label: 'Baseline', value: data.baseline.latency_ms, color: '#64748b' },
                    { label: 'ITS', value: data.its.latency_ms, color: '#0ea5e9' }
                ],
                'lower'
            );

            html += this.renderMetricComparison(
                'Total Tokens',
                '',
                [
                    { label: 'Baseline', value: (data.baseline.input_tokens || 0) + (data.baseline.output_tokens || 0), color: '#64748b' },
                    { label: 'ITS', value: (data.its.input_tokens || 0) + (data.its.output_tokens || 0), color: '#0ea5e9' }
                ],
                'lower'
            );

            // Quality metric — only when correctness data exists
            if (data.baseline.is_correct != null || data.its.is_correct != null) {
                html += this.renderMetricComparison(
                    'Quality',
                    '%',
                    [
                        { label: 'Baseline', value: data.baseline.is_correct === true ? 100 : data.baseline.is_correct === false ? 0 : null, color: '#64748b' },
                        { label: 'ITS', value: data.its.is_correct === true ? 100 : data.its.is_correct === false ? 0 : null, color: '#0ea5e9' }
                    ],
                    'higher'
                );
            }
        }

        html += '</div>'; // Close metrics

        // Detailed breakdown table
        html += this.renderDetailTable(data, isThreeColumn);

        html += '</div>'; // Close container

        this.container.innerHTML = html;
    }

    renderSavingsCallout(smallBaseline, its, frontier) {
        if (frontier.cost_usd == null || its.cost_usd == null || frontier.cost_usd === 0) return '';
        const savings = ((frontier.cost_usd - its.cost_usd) / frontier.cost_usd * 100).toFixed(0);
        return `
            <div class="perf-v2-savings">
                <div class="perf-v2-savings-badge">💰 COST SAVINGS</div>
                <div class="perf-v2-savings-amount">${savings}%</div>
                <div class="perf-v2-savings-text">
                    Small + ITS costs <strong>${this.formatCost(its.cost_usd)}</strong> vs
                    Frontier at <strong>${this.formatCost(frontier.cost_usd)}</strong>
                </div>
            </div>
        `;
    }

    renderMetricComparison(title, unit, items, betterWhen = 'lower') {
        const maxValue = Math.max(...items.map(i => i.value));
        const minValue = Math.min(...items.map(i => i.value));

        let html = `
            <div class="perf-v2-metric">
                <h3 class="perf-v2-metric-title">${title}</h3>
                <div class="perf-v2-bars">
        `;

        items.forEach(item => {
            const val = item.value != null ? item.value : 0;
            const percentage = maxValue > 0 ? (val / maxValue * 100) : 0;
            const isBest = betterWhen === 'lower' ? val === minValue : val === maxValue;
            const displayValue = item.value == null ? 'N/A' :
                                unit === '$' ? this.formatCost(item.value) :
                                unit === 'ms' ? Math.round(item.value).toLocaleString() :
                                item.value.toLocaleString();

            html += `
                <div class="perf-v2-bar-row">
                    <div class="perf-v2-bar-label">${item.label}</div>
                    <div class="perf-v2-bar-track">
                        <div class="perf-v2-bar-fill ${isBest ? 'best' : ''}"
                             style="width: ${percentage}%; background-color: ${item.color};">
                        </div>
                    </div>
                    <div class="perf-v2-bar-value ${isBest ? 'best' : ''}">
                        ${isBest ? '✓ ' : ''}${displayValue}${unit === '$' ? '' : ' ' + unit}
                    </div>
                </div>
            `;
        });

        html += `
                </div>
            </div>
        `;

        return html;
    }

    renderDetailTable(data, isThreeColumn) {
        const models = isThreeColumn ?
            [
                { name: 'Small Baseline', data: data.small_baseline },
                { name: 'Small + ITS', data: data.its },
                { name: 'Frontier', data: data.baseline }
            ] :
            [
                { name: 'Baseline', data: data.baseline },
                { name: 'ITS', data: data.its }
            ];

        let html = `
            <div class="perf-v2-table-container">
                <h3 class="perf-v2-table-title">Detailed Breakdown</h3>
                <table class="perf-v2-table">
                    <thead>
                        <tr>
                            <th>Metric</th>
        `;

        models.forEach(model => {
            html += `<th>${model.name}</th>`;
        });

        html += `
                        </tr>
                    </thead>
                    <tbody>
        `;

        // Cost row
        html += '<tr><td class="perf-v2-table-label">Cost</td>';
        models.forEach(model => {
            html += `<td>${this.formatCost(model.data.cost_usd)}</td>`;
        });
        html += '</tr>';

        // Latency row
        html += '<tr><td class="perf-v2-table-label">Latency</td>';
        models.forEach(model => {
            html += `<td>${model.data.latency_ms != null ? Math.round(model.data.latency_ms).toLocaleString() + 'ms' : 'N/A'}</td>`;
        });
        html += '</tr>';

        // Input tokens
        html += '<tr><td class="perf-v2-table-label">Input Tokens</td>';
        models.forEach(model => {
            html += `<td>${(model.data.input_tokens || 0).toLocaleString()}</td>`;
        });
        html += '</tr>';

        // Output tokens
        html += '<tr><td class="perf-v2-table-label">Output Tokens</td>';
        models.forEach(model => {
            html += `<td>${(model.data.output_tokens || 0).toLocaleString()}</td>`;
        });
        html += '</tr>';

        // Total tokens
        html += '<tr><td class="perf-v2-table-label">Total Tokens</td>';
        models.forEach(model => {
            const total = (model.data.input_tokens || 0) + (model.data.output_tokens || 0);
            html += `<td><strong>${total.toLocaleString()}</strong></td>`;
        });
        html += '</tr>';

        // Tokens per second
        html += '<tr><td class="perf-v2-table-label">Tokens/Second</td>';
        models.forEach(model => {
            const tokensPerSec = model.data.latency_ms > 0 ?
                ((model.data.output_tokens || 0) / (model.data.latency_ms / 1000)).toFixed(1) : '0';
            html += `<td>${tokensPerSec}</td>`;
        });
        html += '</tr>';

        // Cost per token
        html += '<tr><td class="perf-v2-table-label">Cost per Token</td>';
        models.forEach(model => {
            const total = (model.data.input_tokens || 0) + (model.data.output_tokens || 0);
            const costPerToken = (total > 0 && model.data.cost_usd != null) ? '$' + (model.data.cost_usd / total).toFixed(6) : 'N/A';
            html += `<td>${costPerToken}</td>`;
        });
        html += '</tr>';

        // Correctness row — only if any model has correctness data
        const hasCorrectness = models.some(m => m.data.is_correct != null);
        if (hasCorrectness) {
            html += '<tr><td class="perf-v2-table-label">Correctness</td>';
            models.forEach(model => {
                if (model.data.is_correct === true) {
                    html += '<td style="color: var(--success, #22c55e); font-weight: 600;">Correct</td>';
                } else if (model.data.is_correct === false) {
                    html += '<td style="color: var(--danger, #ef4444); font-weight: 600;">Incorrect</td>';
                } else {
                    html += '<td>N/A</td>';
                }
            });
            html += '</tr>';
        }

        html += `
                    </tbody>
                </table>
            </div>
        `;

        return html;
    }

    formatCost(cost) {
        return formatCost(cost);
    }
}

// Make globally available
if (typeof window !== 'undefined') {
    window.PerformanceVizV2 = PerformanceVizV2;
}
