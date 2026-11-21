"use client"

import {
    DataFreshness,
    MetricItem,
    MetricStrip,
    MoneyValue,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"

export function SettlementMetricsStrip({
    pendingReconcile,
    hasDifference,
    pendingReview,
    confirmedAmount,
    sourceAsOf,
}: {
    pendingReconcile: number
    hasDifference: number
    pendingReview: number
    confirmedAmount: string
    sourceAsOf?: string
}) {
    return (
        <div className="flex flex-col gap-2 border-b border-border/60 pb-3">
            <MetricStrip
                variant="plain"
                className="grid-cols-2 gap-y-2"
                columns={4}
                aria-label="结算指标"
            >
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
                    value={
                        <MoneyValue
                            display="compact"
                            value={confirmedAmount}
                            taxBasis="gross"
                        />
                    }
                />
            </MetricStrip>
            {sourceAsOf ? (
                <DataFreshness
                    updatedAt={formatDateTime(sourceAsOf, "default")}
                    dateTime={sourceAsOf}
                    showStatus={false}
                />
            ) : null}
        </div>
    )
}
