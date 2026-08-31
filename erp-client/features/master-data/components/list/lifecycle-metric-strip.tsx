"use client"

import {
    MetricFilterItem,
    MetricItem,
    MetricStrip,
} from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"

type ListMetric = {
    key: string
    label: string
    value: number
    detail?: string
}

export function LifecycleMetricStrip({
    idPrefix,
    metrics,
    metricKey,
    ariaLabel,
    interactive = true,
    onChangeLifecycle,
}: {
    idPrefix?: string
    metrics: readonly ListMetric[]
    metricKey: string
    ariaLabel: string
    interactive?: boolean
    onChangeLifecycle?: (next: "enabled" | "disabled" | "all") => void
}) {
    const prefix = idPrefix ?? "master-data-list-lifecycle-metric"
    if (metrics.length === 0) return null
    return (
        <MetricStrip columns={4} aria-label={ariaLabel}>
            {metrics.map((metric) => {
                const isLifecycleMetric =
                    metric.key === "all" ||
                    metric.key === "enabled" ||
                    metric.key === "disabled"
                if (!interactive || !isLifecycleMetric) {
                    return (
                        <MetricItem
                            key={metric.key}
                            label={metric.label}
                            value={metric.value}
                            detail={metric.detail}
                        />
                    )
                }
                return (
                    <MetricFilterItem
                        key={metric.key}
                        id={`${prefix}-metric-${toAutomationIdSegment(metric.key)}`}
                        label={metric.label}
                        value={metric.value}
                        detail={metric.detail}
                        active={metricKey === metric.key}
                        onClick={() =>
                            onChangeLifecycle?.(
                                metric.key as "enabled" | "disabled" | "all",
                            )
                        }
                    />
                )
            })}
        </MetricStrip>
    )
}
