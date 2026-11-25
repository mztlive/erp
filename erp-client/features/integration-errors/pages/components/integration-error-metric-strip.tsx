"use client"

import { DecisionKpiBar, DecisionKpiCell } from "@/components/business"
import type { IntegrationQueueView } from "../../types"

export function IntegrationErrorMetricStrip({
    metrics,
}: {
    metrics: IntegrationQueueView["metrics"]
}) {
    return (
        <DecisionKpiBar
            ariaLabel="异常队列指标"
            gridClassName="grid-cols-2 @[640px]/decision-kpi:grid-cols-3 @[960px]/decision-kpi:grid-cols-5"
        >
            <DecisionKpiCell
                id="integration-metrics-result-unknown"
                label="结果未知"
                value={metrics.resultUnknown}
            />
            <DecisionKpiCell label="待人工" value={metrics.manualRequired} />
            <DecisionKpiCell
                id="integration-metrics-security-faults"
                label="安全故障"
                value={metrics.securityFaults}
            />
            <DecisionKpiCell
                id="integration-metrics-open-differences"
                label="未解决差异"
                value={metrics.openDifferences}
            />
            <DecisionKpiCell label="最长滞留" value={metrics.longestAgeLabel} />
        </DecisionKpiBar>
    )
}
