"use client"

import {
    MetricFilterItem,
    MetricStrip,
    MoneyValue,
} from "@/components/business"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"

export function SettlementMetricsStrip({
    pendingReconcile,
    hasDifference,
    pendingReview,
    confirmedAmount,
    urlState,
    patchUrl,
}: {
    pendingReconcile: number
    hasDifference: number
    pendingReview: number
    confirmedAmount: string
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
}) {
    return (
        <MetricStrip columns={4} aria-label="结算快捷筛选">
            {/* 指标与「差异类型」下拉为双入口：指标点击时同步清 differenceType，
                避免 status 指标与差异类型组合出矛盾空结果；下拉单独选择差异类型不重置指标。 */}
            <MetricFilterItem
                id="supplier-settlements-metrics-pending"
                label="待处理"
                value={pendingReconcile}
                active={urlState.view === "pending" && !urlState.status}
                onClick={() =>
                    patchUrl({
                        view: "pending",
                        status: undefined,
                        differenceType: undefined,
                        page: 1,
                    })
                }
            />
            <MetricFilterItem
                id="supplier-settlements-metrics-has-difference"
                label="有差异"
                value={hasDifference}
                active={urlState.status === "HAS_DIFFERENCE"}
                onClick={() =>
                    patchUrl({
                        view: "pending",
                        status: "HAS_DIFFERENCE",
                        differenceType: undefined,
                        page: 1,
                    })
                }
            />
            <MetricFilterItem
                id="supplier-settlements-metrics-pending-review"
                label="待复核"
                value={pendingReview}
                active={urlState.status === "PENDING_REVIEW"}
                onClick={() =>
                    patchUrl({
                        view: "pending",
                        status: "PENDING_REVIEW",
                        differenceType: undefined,
                        page: 1,
                    })
                }
            />
            <MetricFilterItem
                id="supplier-settlements-metrics-confirmed"
                label="已确认金额"
                value={<MoneyValue value={confirmedAmount} taxBasis="gross" />}
                active={urlState.view === "confirmed"}
                onClick={() =>
                    patchUrl({
                        view: "confirmed",
                        status: undefined,
                        differenceType: undefined,
                        page: 1,
                    })
                }
            />
        </MetricStrip>
    )
}
