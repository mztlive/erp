"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { DocumentWithdrawApprovalButton } from "@/features/approval-workflow/components/document-withdraw-approval-button"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { useCancelSalesOrderApprovalMutation } from "@/features/sales-orders/hooks/queries"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import {
    gateCancelSalesOrderApproval,
    salesOrderAllowsWithdrawApproval,
} from "@/features/sales-orders/lib/sales-order-detail-permissions"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { getErrorPresentation } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

/**
 * 销售单详情页头「撤回审批」。
 * 可点条件：单据未审结 + 当前用户是负责销售 + 有 `sales_order:cancel_approval`。
 * 走销售单专用撤回接口，不依赖详情里可能为空的 instance 投影。
 */
export function SalesOrderCancelApprovalButton({
    order,
    onResult,
}: {
    order: SalesOrderDetailView
    onResult?: (result: SalesOrderDetailActionResult) => void
}) {
    const profileQuery = useAccountProfileQuery()
    const permissions = useSalesOrderDetailPermissions()
    const cancelMutation = useCancelSalesOrderApprovalMutation()
    const reviseRejected = Boolean(order.approval?.instance?.latestRejection)
    const visible = salesOrderAllowsWithdrawApproval(order)
    const permissionFailure = permissions.accountQuery.isError
        ? getErrorPresentation(
              permissions.accountQuery.error,
              "暂时无法核对权限，请刷新后重试。",
          )
        : null
    const gate = !visible
        ? { enabled: false }
        : permissions.accountQuery.isPending
          ? { enabled: false, reason: "正在核对权限，请稍候。" }
          : permissions.accountQuery.isError
            ? {
                  enabled: false,
                  reason:
                      permissionFailure?.description ??
                      "暂时无法核对权限，请刷新后重试。",
              }
            : gateCancelSalesOrderApproval({
                  order,
                  currentUserId: profileQuery.data?.userid,
                  granted: permissions.granted,
              })

    return (
        <DocumentWithdrawApprovalButton
            visible={visible}
            permitted={hasPermission(
                permissions.granted,
                "sales_order:cancel_approval",
            )}
            pending={cancelMutation.isPending}
            gate={gate}
            reviseRejected={reviseRejected}
            documentNoun="销售单"
            ids={{
                trigger: "sales-orders-detail-cancel-approval-trigger",
                reason: `sales-orders-detail-cancel-reason-${toAutomationIdSegment(order.id)}`,
                cancel: "sales-orders-detail-cancel-approval-cancel",
                confirm: "sales-orders-detail-cancel-approval-confirm",
            }}
            createIdempotencyKey={() =>
                `sales-cancel-approval:${order.id}:${crypto.randomUUID()}`
            }
            buildCommand={({ reason, idempotencyKey }) => ({
                salesOrderId: order.id,
                expectedVersion: order.lockVersion || order.version,
                reason,
                idempotencyKey,
            })}
            documentType="SalesOrder"
            documentIdOf={(command) => command.salesOrderId}
            submit={(command) => cancelMutation.mutateAsync(command)}
            successHref={
                reviseRejected
                    ? `/sales/orders/${encodeURIComponent(order.id)}`
                    : undefined
            }
            reference={order.documentNumber}
            onResult={onResult}
        />
    )
}
