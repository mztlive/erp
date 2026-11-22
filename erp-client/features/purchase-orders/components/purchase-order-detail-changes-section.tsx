"use client"

import {
    BusinessStatusBadge,
    DetailWorkspaceSection,
} from "@/components/business"
import { PurchaseChangeOrderApprovalSection } from "@/features/purchase-orders/components/purchase-change-order-approval-section"
import { PurchaseReturnOrderRelatedSection } from "@/features/purchase-orders/components/purchase-return-order-section"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"

/** 在途变更置顶无双标题套娃，历史变更去除在途重复并补充发起时间；采购退货只读展示执行状态。 */
export function PurchaseOrderDetailChangesSection({
    order,
    onApprovalResult,
}: {
    order: PurchaseOrderCenterView
    onApprovalResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const activeId = order.activeChangeOrder?.id
    const historicalChanges = activeId
        ? order.changes.filter((change) => change.changeId !== activeId)
        : order.changes

    return (
        <>
            {order.activeChangeOrder ? (
                <DetailWorkspaceSection width="form">
                    <PurchaseChangeOrderApprovalSection
                        readonlyApproval
                        purchaseOrderId={order.identity.purchaseOrderId}
                        changeOrder={order.activeChangeOrder}
                        baseRevisionNo={order.identity.revisionNo}
                        paidAmount={order.payableSummary?.paidAllocatedAmount}
                        onResult={onApprovalResult}
                    />
                </DetailWorkspaceSection>
            ) : null}

            {!order.activeChangeOrder || historicalChanges.length > 0 ? (
                <DetailRecordSection
                    width="form"
                    title="变更记录"
                    count={historicalChanges.length}
                >
                    {historicalChanges.length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                            暂无采购变更
                        </p>
                    ) : (
                        <ul className="divide-y divide-border/60 border-y border-border/60">
                            {historicalChanges.map((change) => (
                                <li
                                    key={change.changeId}
                                    className="flex items-start justify-between gap-4 py-3 text-body-compact"
                                >
                                    <div className="min-w-0 space-y-0.5">
                                        <p className="break-words font-medium text-foreground">
                                            {change.label}
                                        </p>
                                        {change.createdAt ||
                                        change.baseRevisionNo != null ? (
                                            <p className="num text-xs text-muted-foreground">
                                                {[
                                                    change.baseRevisionNo !=
                                                    null
                                                        ? `基准版本 v${change.baseRevisionNo}`
                                                        : null,
                                                    change.createdAt
                                                        ? `发起于 ${formatDateTime(change.createdAt, "default")}`
                                                        : null,
                                                ]
                                                    .filter(Boolean)
                                                    .join(" · ")}
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
            ) : null}

            <PurchaseReturnOrderRelatedSection
                purchaseOrderId={order.identity.purchaseOrderId}
            />
        </>
    )
}
