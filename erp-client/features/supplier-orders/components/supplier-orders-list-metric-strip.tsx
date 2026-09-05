"use client"

import { MetricItem, MetricStrip } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SupplierOrderMetric } from "@/features/supplier-orders/types"

export function SupplierOrdersListMetricStrip({
    metrics,
}: {
    metrics: SupplierOrderMetric[]
}) {
    return (
        <MetricStrip className="mb-6" columns={5}>
            {metrics.map((m) => (
                <MetricItem
                    key={m.key}
                    id={`supplier-orders-list-metric-${toAutomationIdSegment(m.key)}`}
                    label={m.label}
                    value={m.value}
                />
            ))}
        </MetricStrip>
    )
}
