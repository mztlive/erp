"use client"

import { ListWorkspaceViews } from "@/components/business/list-workspace"
import { toAutomationIdSegment } from "@/lib/automation-id"

type ListMetric = {
    key: string
    label: string
    value: number
    detail?: string
}

const LIFECYCLE_KEYS = new Set(["all", "enabled", "disabled"])

export function LifecycleMetricStrip({
    idPrefix,
    metrics,
    metricKey,
    ariaLabel,
    hint,
    allLabel,
    interactive = true,
    onChangeLifecycle,
}: {
    idPrefix?: string
    metrics: readonly ListMetric[]
    metricKey: string
    ariaLabel: string
    hint?: string
    allLabel?: string
    interactive?: boolean
    onChangeLifecycle?: (next: "enabled" | "disabled" | "all") => void
}) {
    const prefix = idPrefix ?? "master-data-list-lifecycle-metric"
    const items = metrics
        .filter((metric) => LIFECYCLE_KEYS.has(metric.key))
        .map((metric) => ({
            id: `${prefix}-preset-${toAutomationIdSegment(metric.key)}`,
            label: metric.key === "all" && allLabel ? allLabel : metric.label,
            count: metric.value,
            active: metricKey === metric.key,
            onClick: () => {
                if (!interactive) return
                onChangeLifecycle?.(
                    metric.key as "enabled" | "disabled" | "all",
                )
            },
        }))
    return (
        <ListWorkspaceViews ariaLabel={ariaLabel} items={items} hint={hint} />
    )
}
