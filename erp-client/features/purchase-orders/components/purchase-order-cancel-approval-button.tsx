"use client"

import type { ComponentProps } from "react"
import type { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { DocumentWithdrawApprovalButton } from "@/features/approval-workflow/components/document-withdraw-approval-button"
import { useCancelPurchaseOrderApprovalMutation } from "@/features/purchase-orders/hooks/queries"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { isPurchaseOrderApprovalInProgress } from "@/features/purchase-orders/lib/purchase-order-approval"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"

/**
 * 采购单详情页头「撤回审批」。
 * 可点条件：单据仍在审批中，且当前账号有 `purchase_order:cancel_approval`。
 * 走采购单专用撤回接口，不依赖详情里可能为空的 instance 投影。
 */
export function PurchaseOrderCancelApprovalButton({
    order,
    onResult,
    onTriggerClick,
    triggerVariant,
    triggerClassName,
}: {
    order: PurchaseOrderCenterView
    onResult?: (result: PurchaseOrderDetailResult) => void
    onTriggerClick?: () => void
    triggerVariant?: ComponentProps<typeof Button>["variant"]
    triggerClassName?: string
}) {
    const profileQuery = useAccountProfileQuery()
    const cancelMutation = useCancelPurchaseOrderApprovalMutation()
    const reviseRejected = Boolean(order.approval?.instance?.latestRejection)
    const visible = isPurchaseOrderApprovalInProgress(order)
    const orderId = order.identity.purchaseOrderId
    const permissionFailure = profileQuery.isError
        ? getErrorPresentation(
              profileQuery.error,
              "暂时无法核对权限，请刷新后重试。",
          )
        : null
    const gate = !visible
        ? { enabled: false }
        : profileQuery.isPending
          ? { enabled: false, reason: "正在核对权限，请稍候。" }
          : profileQuery.isError
            ? {
                  enabled: false,
                  reason:
                      permissionFailure?.description ??
                      "暂时无法核对权限，请刷新后重试。",
              }
            : hasPermission(
                    profileQuery.data?.permissions,
                    "purchase_order:cancel_approval",
                )
              ? { enabled: true }
              : {
                    enabled: false,
                    reason: "当前账号没有撤回采购单审批权限",
                }

    return (
        <DocumentWithdrawApprovalButton
            visible={visible}
            permitted={hasPermission(
                profileQuery.data?.permissions,
                "purchase_order:cancel_approval",
            )}
            pending={cancelMutation.isPending}
            gate={gate}
            reviseRejected={reviseRejected}
            documentNoun="采购单"
            ids={{
                trigger: `procurement-orders-detail-cancel-approval-trigger-${orderId}`,
                reason: `procurement-orders-detail-cancel-reason-${orderId}`,
                cancel: `procurement-orders-detail-cancel-approval-cancel-${orderId}`,
                confirm: `procurement-orders-detail-cancel-approval-confirm-${orderId}`,
            }}
            createIdempotencyKey={() =>
                `purchase-cancel-approval:${orderId}:${crypto.randomUUID()}`
            }
            buildCommand={({ reason, idempotencyKey }) => ({
                purchaseOrderId: orderId,
                expectedLockVersion: order.identity.lockVersion,
                reason,
                idempotencyKey,
            })}
            documentType="PurchaseOrder"
            documentIdOf={(command) => command.purchaseOrderId}
            submit={(command) => cancelMutation.mutateAsync(command)}
            onTriggerClick={onTriggerClick}
            successHref={
                reviseRejected
                    ? `/procurement/orders/${encodeURIComponent(orderId)}?mode=edit`
                    : undefined
            }
            reference={order.identity.purchaseNo ?? order.identity.draftLabel}
            onResult={onResult}
            triggerVariant={triggerVariant}
            triggerClassName={triggerClassName}
        />
    )
}
