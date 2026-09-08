"use client"

import { MetricItem, MetricStrip, MoneyValue } from "@/components/business"

export function SettlementMetricsStrip({
    pendingReconcile,
    hasDifference,
    pendingReview,
    confirmedAmount,
}: {
    pendingReconcile: number
    hasDifference: number
    pendingReview: number
    confirmedAmount: string
}) {
    return (
        <MetricStrip className="mb-4" columns={4} aria-label="结算指标">
            <MetricItem
                id="supplier-settlements-metrics-pending"
                label="待处理"
                value={pendingReconcile}
            />
            <MetricItem
                id="supplier-settlements-metrics-has-difference"
                label="有差异"
                value={hasDifference}
            />
            <MetricItem
                id="supplier-settlements-metrics-pending-review"
                label="待复核"
                value={pendingReview}
            />
            <MetricItem
                id="supplier-settlements-metrics-confirmed"
                label="已确认金额"
                value={<MoneyValue value={confirmedAmount} taxBasis="gross" />}
            />
        </MetricStrip>
    )
}
