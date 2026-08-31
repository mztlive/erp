"use client"

import {
    BusinessStatusBadge,
    DocumentSection,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

import { PurchaseChangeOrderApprovalSection } from "@/features/purchase-orders/components/purchase-change-order-approval-section"
import { PurchaseReturnOrderRelatedSection } from "@/features/purchase-orders/components/purchase-return-order-section"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

/**
 * 采购单变更子区。在途改单嵌入通用审批区，动作只读服务端白名单。
 * PurchaseReturnOrder 为 NO_APPROVAL，关联采购退货只展示执行事实，
 * 不接入审批区；PENDING_EXECUTION 渲染为待执行，不是审批复核。
 */
export function PurchaseOrderDetailChangesSection({
    order,
    canChange,
    changeBlocker,
    onRequestChange,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onApprovalResult,
}: {
    order: PurchaseOrderCenterView
    canChange: boolean
    changeBlocker: PurchaseOrderCenterView["actionBlockers"][number] | undefined
    onRequestChange: () => void
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onApprovalResult?: (result: PurchaseOrderDetailResult) => void
}) {
    return (
        <DocumentSection title="变更与异常">
            {order.changes.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    暂无采购变更。生效后变化须走变更，不得在本版本直接覆写付款/发票/履约记录。
                </p>
            ) : (
                <ul className="space-y-2">
                    {order.changes.map((change) => (
                        <li
                            key={change.changeId}
                            className={cn(
                                surfaceInsetClassName,
                                "flex items-center justify-between px-3 py-2 text-sm",
                            )}
                        >
                            <span>
                                {change.label}
                                {change.baseRevisionNo != null
                                    ? ` · 基准 v${change.baseRevisionNo}`
                                    : ""}
                            </span>
                            <BusinessStatusBadge
                                context="list"
                                label={change.statusLabel}
                                tone={change.tone}
                            />
                        </li>
                    ))}
                </ul>
            )}
            {order.activeChangeOrder ? (
                <div className="mt-4">
                    <PurchaseChangeOrderApprovalSection
                        purchaseOrderId={order.identity.purchaseOrderId}
                        changeOrder={order.activeChangeOrder}
                        workItemId={workItemId}
                        expectedTaskVersion={expectedTaskVersion}
                        workItemAllowedActions={workItemAllowedActions}
                        onResult={onApprovalResult}
                    />
                </div>
            ) : null}
            <PurchaseReturnOrderRelatedSection
                purchaseOrderId={order.identity.purchaseOrderId}
            />
            <div className="mt-4 flex flex-wrap gap-2">
                {canChange ? (
                    <Button
                        id={`procurement-orders-detail-changes-create-${order.identity.purchaseOrderId}`}
                        type="button"
                        onClick={onRequestChange}
                    >
                        发起采购变更
                    </Button>
                ) : (
                    <div className="space-y-1">
                        <Button
                            id={`procurement-orders-detail-changes-disabled-${order.identity.purchaseOrderId}`}
                            type="button"
                            disabled
                        >
                            发起采购变更
                        </Button>
                        <p className="text-xs text-muted-foreground">
                            {changeBlocker?.message ??
                                "当前状态下不能发起变更，可先完成前置条件。"}
                        </p>
                    </div>
                )}
            </div>
        </DocumentSection>
    )
}
