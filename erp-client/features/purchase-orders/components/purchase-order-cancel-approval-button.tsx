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
import { useCancelPurchaseOrderApprovalMutation } from "@/features/purchase-orders/hooks/queries"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { isPurchaseOrderApprovalInProgress } from "@/features/purchase-orders/lib/purchase-order-approval"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { getErrorPresentation } from "@/lib/api/errors"
import { documentIsEditableDraft } from "@/features/approval-workflow/api/document-cancel"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"

/**
 * 采购单详情页头「撤回审批」。
 * 可点条件：单据仍在审批中，且当前账号有 `purchase_order:cancel_approval`。
 * 走采购单专用撤回接口，不依赖详情里可能为空的 instance 投影。
 */
export function PurchaseOrderCancelApprovalButton({
    order,
    onResult,
}: {
    order: PurchaseOrderCenterView
    onResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const router = useRouter()
    const [open, setOpen] = React.useState(false)
    const [reason, setReason] = React.useState("")
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const profileQuery = useAccountProfileQuery()
    const cancelMutation = useCancelPurchaseOrderApprovalMutation()
    const reviseRejected = Boolean(order.approval?.instance?.latestRejection)
    const [uncertain, setUncertain] = React.useState(false)
    const [checking, setChecking] = React.useState(false)
    const commandInFlight = React.useRef(false)
    const busy = cancelMutation.isPending || checking
    const pendingCommand = React.useRef<
        Parameters<typeof cancelMutation.mutateAsync>[0] | null
    >(null)
    const documentReference =
        order.identity.purchaseNo ?? order.identity.draftLabel

    if (!isPurchaseOrderApprovalInProgress(order)) return null

    const permissionFailure = profileQuery.isError
        ? getErrorPresentation(
              profileQuery.error,
              "暂时无法核对权限，请刷新后重试。",
          )
        : null
    const gate = profileQuery.isPending
        ? ({ enabled: false, reason: "正在核对权限，请稍候。" } as const)
        : profileQuery.isError
          ? ({
                enabled: false,
                reason:
                    permissionFailure?.description ??
                    "暂时无法核对权限，请刷新后重试。",
            } as const)
          : hasPermission(
                  profileQuery.data?.permissions,
                  "purchase_order:cancel_approval",
              )
            ? ({ enabled: true, reason: undefined } as const)
            : ({
                  enabled: false,
                  reason: "当前账号没有撤回采购单审批权限",
              } as const)

    return (
        <>
            <Button
                id={`procurement-orders-detail-cancel-approval-trigger-${order.identity.purchaseOrderId}`}
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
                        `purchase-cancel-approval:${order.identity.purchaseOrderId}:${crypto.randomUUID()}`,
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
                            撤回后，采购单将回到草稿。
                            {reviseRejected
                                ? "原采购单编号和审批记录保留，修改后重新提交审批。"
                                : null}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div className="space-y-2">
                        <label
                            htmlFor={`procurement-orders-detail-cancel-reason-${order.identity.purchaseOrderId}`}
                            className="text-sm font-medium"
                        >
                            撤回原因
                        </label>
                        <Textarea
                            id={`procurement-orders-detail-cancel-reason-${order.identity.purchaseOrderId}`}
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
                            id={`procurement-orders-detail-cancel-approval-cancel-${order.identity.purchaseOrderId}`}
                            disabled={busy || uncertain}
                        >
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            loading={busy}
                            id={`procurement-orders-detail-cancel-approval-confirm-${order.identity.purchaseOrderId}`}
                            disabled={busy || !reason.trim()}
                            onClick={(event) => {
                                event.preventDefault()
                                if (commandInFlight.current) return
                                commandInFlight.current = true
                                setChecking(true)
                                setConfirmError(null)
                                const command = pendingCommand.current ?? {
                                    purchaseOrderId:
                                        order.identity.purchaseOrderId,
                                    expectedLockVersion:
                                        order.identity.lockVersion,
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
                                                "PurchaseOrder",
                                                command.purchaseOrderId,
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
                                                `/procurement/orders/${encodeURIComponent(order.identity.purchaseOrderId)}?mode=edit`,
                                            )
                                        }
                                        onResult?.({
                                            status: "succeeded",
                                            title: "审批已撤回",
                                            description:
                                                "已撤回当前审批，单据回到可编辑草稿。",
                                            reference: documentReference,
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
                                            reference: documentReference,
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
