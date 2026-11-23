import type { useWorkspaceOverview } from "../hooks/use-workspace-overview"
import type { OverviewData, OverviewMetric } from "../lib/overview"

export type WorkspaceOverview = ReturnType<typeof useWorkspaceOverview>

export function overviewValue(metric: OverviewMetric, data?: OverviewData) {
    if (!data || data.noScope) return undefined
    return data[metric.field]
}
