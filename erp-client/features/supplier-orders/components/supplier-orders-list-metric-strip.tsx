"use client"

import {
    DataFreshness,
    DecisionKpiBar,
    DecisionKpiCell,
} from "@/components/business"
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

    const columns =
        metrics.length >= 4
            ? 4
            : metrics.length === 3
              ? 3
              : metrics.length === 2
                ? 2
                : 1

    return (
        <div className="flex min-w-0 flex-col gap-2">
            {metrics.length > 0 ? (
                <DecisionKpiBar
                    columns={columns}
                    ariaLabel="供应商订单指标"
                    gridClassName={
                        metrics.length >= 5
                            ? "grid-cols-2 gap-y-2 @[840px]/decision-kpi:grid-cols-5"
                            : "grid-cols-2 gap-y-2"
                    }
                >
                    {metrics.map((m) => (
                        <DecisionKpiCell
                            key={m.key}
                            id={`supplier-orders-list-metric-${toAutomationIdSegment(m.key)}`}
                            label={m.label}
                            value={m.value}
                        />
                    ))}
                </DecisionKpiBar>
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
