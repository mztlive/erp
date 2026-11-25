"use client"

import {
    DataFreshness,
    DecisionKpiBar,
    DecisionKpiCell,
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
            <DecisionKpiBar
                columns={4}
                ariaLabel="结算指标"
                gridClassName="grid-cols-2 gap-y-2"
            >
                <DecisionKpiCell
                    id="supplier-settlements-metrics-pending"
                    label="待处理"
                    value={pendingReconcile}
                />
                <DecisionKpiCell
                    id="supplier-settlements-metrics-has-difference"
                    label="有差异"
                    value={hasDifference}
                />
                <DecisionKpiCell
                    id="supplier-settlements-metrics-pending-review"
                    label="待复核"
                    value={pendingReview}
                />
                <DecisionKpiCell
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
            </DecisionKpiBar>
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
