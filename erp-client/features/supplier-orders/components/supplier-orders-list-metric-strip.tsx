"use client"

import { DataFreshness, MetricItem, MetricStrip } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SupplierOrderMetric } from "@/features/supplier-orders/types"

export function SupplierOrdersListMetricStrip({
    metrics,
    queriedAt,
}: {
    metrics: SupplierOrderMetric[]
    queriedAt?: string
}) {
    if (!metrics.length && !queriedAt) return null

    return (
        <div className="flex min-w-0 flex-col gap-2">
            {metrics.length > 0 ? (
                <MetricStrip
                    columns={5}
                    density="compact"
                    variant="plain"
                    className="grid-cols-2 gap-y-2 lg:grid-cols-5"
                >
                    {metrics.map((m) => (
                        <MetricItem
                            key={m.key}
                            id={`supplier-orders-list-metric-${toAutomationIdSegment(m.key)}`}
                            label={m.label}
                            value={m.value}
                        />
                    ))}
                </MetricStrip>
            ) : null}
            {queriedAt ? (
                <DataFreshness
                    updatedAt={queriedAt.slice(11, 16)}
                    dateTime={queriedAt}
                    showStatus={false}
                    role="status"
                />
            ) : null}
        </div>
    )
}
