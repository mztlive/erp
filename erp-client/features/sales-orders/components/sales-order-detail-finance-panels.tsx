"use client"

import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
import { RevisionHistoryCard } from "@/features/sales-orders/components/revision-history-card"
import {
    DetailSummary,
    DetailSummaryItem,
} from "./sales-order-detail-presentation"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"

export { ReceivablePanel } from "@/features/sales-orders/components/sales-order-detail-receivable-panel"

export function VersionsPanel({
    order,
    onApprovalResult,
    showActiveChange = true,
}: {
    order: SalesOrderDetailView
    showActiveChange?: boolean
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
}) {
    return (
        <div className="space-y-6">
            <DetailSummary label="销售版本摘要">
                <DetailSummaryItem
                    label="当前有效版本"
                    value={
                        order.currentRevisionNo == null
                            ? "尚未生效"
                            : `v${order.currentRevisionNo}`
                    }
                    detail={
                        order.contractRevisionLabel ? (
                            <>关联合同 {order.contractRevisionLabel}</>
                        ) : undefined
                    }
                />
                <DetailSummaryItem
                    label="已生效版本"
                    value={
                        <>
                            {order.revisions.length}{" "}
                            <span className="text-sm font-normal text-muted-foreground">
                                个
                            </span>
                        </>
                    }
                    detail="历史版本保留当时的合同、金额与明细"
                />
            </DetailSummary>
            {order.activeChangeOrder && showActiveChange ? (
                <section className="space-y-3 border-b border-border/70 pb-5">
                    <div>
                        <h2 className="text-sm font-semibold">进行中的改单</h2>
                        <p className="mt-1 text-xs text-muted-foreground">
                            生效前仍按当前版本执行
                        </p>
                    </div>
                    <SalesChangeOrderApprovalSection
                        readonlyApproval
                        salesOrderId={order.id}
                        nature={order.nature}
                        changeOrder={order.activeChangeOrder}
                        onResult={onApprovalResult}
                    />
                </section>
            ) : null}
            <RevisionHistoryCard
                revisions={order.revisions}
                currentVersion={order.currentRevisionNo}
                contractRevisionLabel={order.contractRevisionLabel}
            />
        </div>
    )
}

export function CollaborationPanel({ order }: { order: SalesOrderDetailView }) {
    void order
    // TODO(商城重做): 执行投影恢复后替换此占位面板。
    return (
        <p className="text-sm text-muted-foreground">
            商城对接已移除，执行投影待未来重做时恢复。
        </p>
    )
}
