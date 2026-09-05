"use client"

import { MetricItem, MetricStrip } from "@/components/business"
import type { IntegrationQueueView } from "../../types"

export function IntegrationErrorMetricStrip({
    metrics,
}: {
    metrics: IntegrationQueueView["metrics"]
}) {
    return (
        <MetricStrip columns={5}>
            <MetricItem
                id="integration-metrics-result-unknown"
                label="结果未知"
                value={metrics.resultUnknown}
            />
            <MetricItem label="待人工" value={metrics.manualRequired} />
            <MetricItem
                id="integration-metrics-security-faults"
                label="安全故障"
                value={metrics.securityFaults}
            />
            <MetricItem
                id="integration-metrics-open-differences"
                label="未解决差异"
                value={metrics.openDifferences}
            />
            <MetricItem label="最长滞留" value={metrics.longestAgeLabel} />
        </MetricStrip>
    )
}
