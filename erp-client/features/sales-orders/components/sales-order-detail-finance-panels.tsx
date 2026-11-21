"use client"

import { DetailWorkspaceSection } from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { RevisionHistoryCard } from "@/features/sales-orders/components/revision-history-card"
import { SalesChangeOrderApprovalSection } from "@/features/sales-orders/components/sales-change-order-approval-section"
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
        <>
            {order.activeChangeOrder && showActiveChange ? (
                <DetailWorkspaceSection width="form">
                    <SalesChangeOrderApprovalSection
                        readonlyApproval
                        salesOrderId={order.id}
                        nature={order.nature}
                        changeOrder={order.activeChangeOrder}
                        onResult={onApprovalResult}
                    />
                </DetailWorkspaceSection>
            ) : null}
            <RevisionHistoryCard
                revisions={order.revisions}
                currentVersion={order.currentRevisionNo}
                contractRevisionLabel={order.contractRevisionLabel}
            />
        </>
    )
}

export function CollaborationPanel({ order }: { order: SalesOrderDetailView }) {
    return (
        <DetailWorkspaceSection width="form" title="卡券履约">
            <dl className="grid gap-x-8 gap-y-4 sm:grid-cols-2">
                <div className="space-y-1">
                    <dt className="text-xs font-medium text-muted-foreground">
                        履约期限
                    </dt>
                    <dd className="num text-body-compact text-foreground">
                        {order.fulfillmentDeadline || "未设置"}
                    </dd>
                </div>
                <div className="space-y-1">
                    <dt className="text-xs font-medium text-muted-foreground">
                        履约状态
                    </dt>
                    <dd className="text-body-compact text-foreground">
                        {order.fulfillment.label}
                    </dd>
                </div>
            </dl>
            <div className="space-y-1 border-t border-border/60 pt-3 text-xs text-muted-foreground">
                <p>
                    卡券按履约期限判定交付完成，无需登记客户验收；回款和开票在票款分区跟进。
                </p>
                {order.originSystem === "mall" ? (
                    <p>本单由商城开立，商业变更由商城处理。</p>
                ) : null}
            </div>
        </DetailWorkspaceSection>
    )
}
