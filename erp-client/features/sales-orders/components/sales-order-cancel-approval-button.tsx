"use client"

import * as React from "react"

import { FormalActionConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { useCancelSalesOrderApprovalMutation } from "@/features/sales-orders/hooks/queries"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import {
    gateCancelSalesOrderApproval,
    salesOrderAllowsWithdrawApproval,
} from "@/features/sales-orders/lib/sales-order-detail-permissions"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { getErrorPresentation } from "@/lib/api/errors"

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
    const [open, setOpen] = React.useState(false)
    const [reason, setReason] = React.useState("")
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const profileQuery = useAccountProfileQuery()
    const permissions = useSalesOrderDetailPermissions()
    const cancelMutation = useCancelSalesOrderApprovalMutation()

    if (!salesOrderAllowsWithdrawApproval(order)) return null

    const gate = permissions.accountQuery.isPending
        ? ({ enabled: false, reason: "正在核对权限，请稍候。" } as const)
        : permissions.accountQuery.isError
          ? ({
                enabled: false,
                reason: "暂时无法核对权限，请刷新后重试。",
            } as const)
          : gateCancelSalesOrderApproval({
                order,
                currentUserId: profileQuery.data?.userid,
                granted: permissions.granted,
            })

    return (
        <>
            <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={!gate.enabled}
                title={gate.reason}
                onClick={() => {
                    setReason("")
                    setIdempotencyKey(
                        `sales-cancel-approval:${order.id}:${crypto.randomUUID()}`,
                    )
                    setOpen(true)
                }}
            >
                撤回审批
            </Button>
            <FormalActionConfirmDialog
                open={open}
                onOpenChange={setOpen}
                title="撤回审批"
                actionLabel="撤回审批"
                confirmLabel="确认撤回"
                fromStatus={{ label: order.primaryStatus.label, tone: "warning" }}
                toStatus={{ label: "草稿", tone: "neutral" }}
                description={
                    <Textarea
                        value={reason}
                        onChange={(event) => setReason(event.target.value)}
                        placeholder="请填写撤回原因"
                        rows={3}
                    />
                }
                lockedFields={["销售单号", "负责销售"]}
                effects={["审批实例作废", "销售单回到可编辑草稿", "可修改后再次提交"]}
                pending={cancelMutation.isPending}
                confirmDisabled={!reason.trim()}
                onConfirm={async () => {
                    try {
                        await cancelMutation.mutateAsync({
                            salesOrderId: order.id,
                            expectedVersion: order.lockVersion || order.version,
                            reason: reason.trim(),
                            idempotencyKey,
                        })
                        setOpen(false)
                        onResult?.({
                            status: "succeeded",
                            title: "审批已撤回",
                            description:
                                "已撤回当前审批，单据回到可编辑草稿。",
                            reference: order.documentNumber,
                        })
                    } catch (error) {
                        const failure = getErrorPresentation(
                            error,
                            "撤回审批未完成，请刷新后重试。",
                        )
                        onResult?.({
                            status: "blocked",
                            title: failure.title,
                            description: failure.description,
                            reference: order.documentNumber,
                        })
                        throw error
                    }
                }}
            />
        </>
    )
}
