"use client"

import { BusinessStatusBadge } from "@/components/business"
import {
    DetailRecordSection,
    DetailSummary,
    DetailSummaryItem,
} from "@/components/business/detail-presentation"
import { PurchaseChangeOrderApprovalSection } from "@/features/purchase-orders/components/purchase-change-order-approval-section"
import { PurchaseReturnOrderRelatedSection } from "@/features/purchase-orders/components/purchase-return-order-section"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

/** 当前版本、在途变更和历史分层展示；采购退货只读展示执行状态。 */
export function PurchaseOrderDetailChangesSection({
    order,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onApprovalResult,
}: {
    order: PurchaseOrderCenterView
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onApprovalResult?: (result: PurchaseOrderDetailResult) => void
}) {
    return (
        <div className="space-y-6">
            <DetailSummary label="采购变更摘要">
                <DetailSummaryItem
                    label="当前有效版本"
                    value={
                        order.identity.revisionNo == null
                            ? "尚未生效"
                            : `v${order.identity.revisionNo}`
                    }
                />
                <DetailSummaryItem
                    label="当前变更"
                    value={order.activeChangeOrder?.statusLabel ?? "无在途变更"}
                />
            </DetailSummary>
            <div className="divide-y divide-border/70">
                {order.activeChangeOrder ? (
                    <DetailRecordSection title="当前变更">
                        <p className="text-sm">
                            {order.activeChangeOrder.reason}
                        </p>
                        <PurchaseChangeOrderApprovalSection
                            readonlyApproval
                            purchaseOrderId={order.identity.purchaseOrderId}
                            changeOrder={order.activeChangeOrder}
                            workItemId={workItemId}
                            expectedTaskVersion={expectedTaskVersion}
                            workItemAllowedActions={workItemAllowedActions}
                            onResult={onApprovalResult}
                        />
                    </DetailRecordSection>
                ) : null}
                <DetailRecordSection
                    title="变更记录"
                    count={order.changes.length}
                    compact={order.changes.length === 0}
                >
                    {order.changes.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            暂无采购变更
                        </p>
                    ) : (
                        <ul className="divide-y divide-border/70">
                            {order.changes.map((change) => (
                                <li
                                    key={change.changeId}
                                    className="flex items-start justify-between gap-4 py-3 text-sm first:pt-0 last:pb-0"
                                >
                                    <div className="min-w-0">
                                        <p className="break-words font-medium">
                                            {change.label}
                                        </p>
                                        {change.baseRevisionNo != null ? (
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                基准版本 v
                                                {change.baseRevisionNo}
                                            </p>
                                        ) : null}
                                    </div>
                                    <BusinessStatusBadge
                                        context="list"
                                        label={change.statusLabel}
                                        tone={change.tone}
                                    />
                                </li>
                            ))}
                        </ul>
                    )}
                </DetailRecordSection>
                <PurchaseReturnOrderRelatedSection
                    purchaseOrderId={order.identity.purchaseOrderId}
                />
            </div>
        </div>
    )
}
