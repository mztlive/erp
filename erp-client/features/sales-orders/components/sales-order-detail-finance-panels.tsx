"use client"

import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
import { RevisionHistoryCard } from "@/features/sales-orders/components/revision-history-card"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"

/** 版本页：在途改单在上，已生效版本按时间倒序；当前版本由时间线标记。 */
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
        <div className="space-y-8">
            {order.activeChangeOrder && showActiveChange ? (
                <SalesChangeOrderApprovalSection
                    readonlyApproval
                    salesOrderId={order.id}
                    nature={order.nature}
                    changeOrder={order.activeChangeOrder}
                    onResult={onApprovalResult}
                />
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
