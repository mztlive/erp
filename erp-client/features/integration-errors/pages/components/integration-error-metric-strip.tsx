import { MetricFilterItem, MetricItem, MetricStrip } from "@/components/business"
import type { IntegrationQueueView, IntegrationView } from "../../types"

export function IntegrationErrorMetricStrip({
    metrics,
    activeView,
    focusMode,
    onSelectView,
}: {
    metrics: IntegrationQueueView["metrics"]
    activeView: IntegrationView
    focusMode: boolean
    onSelectView: (view: IntegrationView) => void
}) {
    return (
        <MetricStrip>
            <MetricFilterItem
                label="结果未知"
                value={metrics.resultUnknown}
                active={activeView === "result_unknown"}
                onClick={
                    focusMode ? undefined : () => onSelectView("result_unknown")
                }
            />
            <MetricItem label="待人工" value={metrics.manualRequired} />
            <MetricFilterItem
                label="安全故障"
                value={metrics.securityFaults}
                active={activeView === "security"}
                onClick={focusMode ? undefined : () => onSelectView("security")}
            />
            <MetricFilterItem
                label="未解决差异"
                value={metrics.openDifferences}
                active={activeView === "reconciliation"}
                onClick={
                    focusMode
                        ? undefined
                        : () => onSelectView("reconciliation")
                }
            />
            <MetricItem label="最长滞留" value={metrics.longestAgeLabel} />
        </MetricStrip>
    )
}
