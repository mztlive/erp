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
    return (
        <section aria-label="卡券履约" className="space-y-4">
            <h2 className="text-lg font-semibold">卡券履约</h2>
            <dl className="grid gap-4 text-sm sm:grid-cols-2">
                <div>
                    <dt className="text-muted-foreground">履约期限</dt>
                    <dd className="mt-1">
                        {order.fulfillmentDeadline || "未设置"}
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">履约状态</dt>
                    <dd className="mt-1">{order.fulfillment.label}</dd>
                </div>
            </dl>
            <p className="text-sm text-muted-foreground">
                卡券按履约期限判定交付完成，无需登记客户验收；回款和开票在票款分区跟进。
            </p>
            {order.originSystem === "mall" ? (
                <p className="text-sm text-muted-foreground">
                    本单由商城开立，商业变更由商城处理。
                </p>
            ) : null}
        </section>
    )
}
