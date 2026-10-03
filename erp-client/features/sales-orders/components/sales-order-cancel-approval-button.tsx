"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorPresentation } from "@/lib/api/errors"
import { documentIsEditableDraft } from "@/features/approval-workflow/api/document-cancel"
import { classifyFormalCommandError } from "@/lib/formal-command"

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
    const router = useRouter()
    const [open, setOpen] = React.useState(false)
    const [reason, setReason] = React.useState("")
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const profileQuery = useAccountProfileQuery()
    const permissions = useSalesOrderDetailPermissions()
    const cancelMutation = useCancelSalesOrderApprovalMutation()
    const reviseRejected = Boolean(order.approval?.instance?.latestRejection)
    const [uncertain, setUncertain] = React.useState(false)
    const [checking, setChecking] = React.useState(false)
    const commandInFlight = React.useRef(false)
    const busy = cancelMutation.isPending || checking
    const pendingCommand = React.useRef<
        Parameters<typeof cancelMutation.mutateAsync>[0] | null
    >(null)

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
                id="sales-orders-detail-cancel-approval-trigger"
                type="button"
                size="sm"
                variant="outline"
                disabled={!gate.enabled}
                title={gate.reason}
                onClick={() => {
                    pendingCommand.current = null
                    setUncertain(false)
                    setReason(
                        reviseRejected ? "按驳回意见修改原单后重新提交" : "",
                    )
                    setConfirmError(null)
                    setIdempotencyKey(
                        `sales-cancel-approval:${order.id}:${crypto.randomUUID()}`,
                    )
                    setOpen(true)
                }}
            >
                {reviseRejected ? "修改原单" : "撤回审批"}
            </Button>
            <AlertDialog
                open={open}
                onOpenChange={(next) => {
                    if (!pendingCommand.current && !uncertain && !busy)
                        setOpen(next)
                }}
            >
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {reviseRejected ? "修改原单" : "撤回审批"}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            撤回后，销售单将回到草稿。
                            {reviseRejected
                                ? "原销售单编号和审批记录保留，修改后重新提交审批。"
                                : null}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div className="space-y-2">
                        <label
                            htmlFor={`sales-orders-detail-cancel-reason-${toAutomationIdSegment(order.id)}`}
                            className="text-sm font-medium"
                        >
                            撤回原因
                        </label>
                        <Textarea
                            id={`sales-orders-detail-cancel-reason-${toAutomationIdSegment(order.id)}`}
                            value={reason}
                            onChange={(event) => setReason(event.target.value)}
                            placeholder="请输入撤回原因"
                            rows={3}
                            disabled={busy || uncertain}
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
                        <AlertDialogCancel
                            id="sales-orders-detail-cancel-approval-cancel"
                            disabled={busy || uncertain}
                        >
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            loading={busy}
                            id="sales-orders-detail-cancel-approval-confirm"
                            disabled={busy || !reason.trim()}
                            onClick={(event) => {
                                event.preventDefault()
                                if (commandInFlight.current) return
                                commandInFlight.current = true
                                setChecking(true)
                                setConfirmError(null)
                                const command = pendingCommand.current ?? {
                                    salesOrderId: order.id,
                                    expectedVersion:
                                        order.lockVersion || order.version,
                                    reason: reason.trim(),
                                    idempotencyKey,
                                }
                                pendingCommand.current = command
                                let checkingUnknown = false
                                void (async () => {
                                    if (uncertain) {
                                        checkingUnknown = true
                                        if (
                                            await documentIsEditableDraft(
                                                "SalesOrder",
                                                command.salesOrderId,
                                            )
                                        )
                                            return
                                        checkingUnknown = false
                                    }
                                    return cancelMutation.mutateAsync(command)
                                })()
                                    .then(() => {
                                        pendingCommand.current = null
                                        setUncertain(false)
                                        setOpen(false)
                                        if (reviseRejected) {
                                            router.push(
                                                `/sales/orders/${encodeURIComponent(order.id)}`,
                                            )
                                        }
                                        onResult?.({
                                            status: "succeeded",
                                            title: "审批已撤回",
                                            description:
                                                "已撤回当前审批，单据回到可编辑草稿。",
                                            reference: order.documentNumber,
                                        })
                                    })
                                    .catch((error: unknown) => {
                                        const unknown =
                                            uncertain ||
                                            checkingUnknown ||
                                            classifyFormalCommandError(
                                                error,
                                            ) === "unknown"
                                        setUncertain(unknown)
                                        if (!unknown)
                                            pendingCommand.current = null
                                        const failure = getErrorPresentation(
                                            error,
                                            "撤回审批未完成，请刷新后重试。",
                                        )
                                        setConfirmError(failure.description)
                                        onResult?.({
                                            status: unknown
                                                ? "unknown"
                                                : "blocked",
                                            title: unknown
                                                ? "处理结果待确认"
                                                : failure.title,
                                            description: unknown
                                                ? "请使用本次操作重试；确认前不要修改输入或重复撤回。"
                                                : failure.description,
                                            reference: order.documentNumber,
                                        })
                                    })
                                    .finally(() => {
                                        commandInFlight.current = false
                                        setChecking(false)
                                    })
                            }}
                        >
                            {busy
                                ? "撤回中"
                                : uncertain
                                  ? "核对撤回结果"
                                  : reviseRejected
                                    ? "撤回并修改原单"
                                    : "确认撤回"}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    )
}
