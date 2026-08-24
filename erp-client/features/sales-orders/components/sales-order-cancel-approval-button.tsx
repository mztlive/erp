"use client"

import * as React from "react"
import { LoaderCircleIcon } from "lucide-react"

import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
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
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const profileQuery = useAccountProfileQuery()
    const permissions = useSalesOrderDetailPermissions()
    const cancelMutation = useCancelSalesOrderApprovalMutation()

    if (!salesOrderAllowsWithdrawApproval(order)) return null

    const permissionFailure = permissions.accountQuery.isError
        ? getErrorPresentation(
              permissions.accountQuery.error,
              "暂时无法核对权限，请刷新后重试。",
          )
        : null
    const gate = permissions.accountQuery.isPending
        ? ({ enabled: false, reason: "正在核对权限，请稍候。" } as const)
        : permissions.accountQuery.isError
          ? ({
                enabled: false,
                reason:
                    permissionFailure?.description ??
                    "暂时无法核对权限，请刷新后重试。",
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
                    setConfirmError(null)
                    setIdempotencyKey(
                        `sales-cancel-approval:${order.id}:${crypto.randomUUID()}`,
                    )
                    setOpen(true)
                }}
            >
                撤回审批
            </Button>
            <AlertDialog open={open} onOpenChange={setOpen}>
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>撤回审批</AlertDialogTitle>
                        <AlertDialogDescription>
                            撤回后，销售单将回到草稿。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div className="space-y-2">
                        <label
                            htmlFor={`sales-order-cancel-reason-${order.id}`}
                            className="text-sm font-medium"
                        >
                            撤回原因
                        </label>
                        <Textarea
                            id={`sales-order-cancel-reason-${order.id}`}
                            value={reason}
                            onChange={(event) => setReason(event.target.value)}
                            placeholder="请输入撤回原因"
                            rows={3}
                            disabled={cancelMutation.isPending}
                        />
                        {confirmError ? (
                            <p
                                className="text-sm text-destructive"
                                role="alert"
                            >
                                {confirmError}
                            </p>
                        ) : null}
                    </div>
                    <AlertDialogFooter>
                        <AlertDialogCancel disabled={cancelMutation.isPending}>
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            disabled={
                                cancelMutation.isPending || !reason.trim()
                            }
                            onClick={() => {
                                setConfirmError(null)
                                void cancelMutation
                                    .mutateAsync({
                                        salesOrderId: order.id,
                                        expectedVersion:
                                            order.lockVersion || order.version,
                                        reason: reason.trim(),
                                        idempotencyKey,
                                    })
                                    .then(() => {
                                        setOpen(false)
                                        onResult?.({
                                            status: "succeeded",
                                            title: "审批已撤回",
                                            description:
                                                "已撤回当前审批，单据回到可编辑草稿。",
                                            reference: order.documentNumber,
                                        })
                                    })
                                    .catch((error: unknown) => {
                                        const failure = getErrorPresentation(
                                            error,
                                            "撤回审批未完成，请刷新后重试。",
                                        )
                                        setConfirmError(failure.description)
                                        onResult?.({
                                            status: "blocked",
                                            title: failure.title,
                                            description: failure.description,
                                            reference: order.documentNumber,
                                        })
                                    })
                            }}
                        >
                            {cancelMutation.isPending ? (
                                <LoaderCircleIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                    className="animate-spin"
                                />
                            ) : null}
                            {cancelMutation.isPending ? "撤回中" : "确认撤回"}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    )
}
