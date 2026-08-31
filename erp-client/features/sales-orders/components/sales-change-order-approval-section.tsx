"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { SalesChangeOrderApprovalArea } from "@/features/sales-orders/components/sales-change-order-approval-area"
import { SalesChangeOrderSubmitConfirmDialog } from "@/features/sales-orders/components/sales-change-order-submit-confirm-dialog"
import { useSubmitSalesChangeOrderMutation } from "@/features/sales-orders/hooks/queries"
import {
    mergeSalesChangeOrderAllowedActions,
    salesChangeOrderApprovalPhase,
} from "@/features/sales-orders/lib/sales-change-order-approval"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import type {
    SalesChangeOrderSummary,
    SalesOrderNature,
} from "@/features/sales-orders/types"
import { getErrorPresentation } from "@/lib/api/errors"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"

/**
 * 销售变更单在详情页上的审批区入口。
 *
 * 未提交且服务端允许时展示提交确认；决定、升级和撤回只读 `allowed_actions`。
 */
export function SalesChangeOrderApprovalSection({
    salesOrderId,
    nature,
    changeOrder,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onResult,
}: {
    salesOrderId: string
    nature: SalesOrderNature
    changeOrder: SalesChangeOrderSummary | null
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onResult?: (result: SalesOrderDetailActionResult) => void
}) {
    const submitMutation = useSubmitSalesChangeOrderMutation()
    const [submitOpen, setSubmitOpen] = React.useState(false)
    const ledgerRef = React.useRef<FormalCommandKeyLedger | null>(null)
    if (ledgerRef.current == null) {
        ledgerRef.current = new FormalCommandKeyLedger()
    }
    const commandLedger = ledgerRef.current

    if (!changeOrder) {
        return (
            <Alert variant="warning">
                <AlertTitle>改单已不在当前销售单上</AlertTitle>
                <AlertDescription>
                    任务或改单关系已变化，请返回后刷新再处理。
                </AlertDescription>
            </Alert>
        )
    }

    const phase = salesChangeOrderApprovalPhase(
        changeOrder.approval,
        changeOrder.statusCode,
    )
    const allowedActions = mergeSalesChangeOrderAllowedActions(
        changeOrder.approval?.allowedActions,
        workItemAllowedActions,
    )
    const canSubmit =
        phase === "draft" &&
        allowedActions.includes("SUBMIT") &&
        (changeOrder.version ?? 0) > 0

    const submitChange = async () => {
        const slot = `submit-change:${changeOrder.id}`
        let command = commandLedger.peek<{
            salesChangeOrderId: string
            salesOrderId: string
            version: number
            nature: SalesOrderNature
        }>(slot)
        if (!command) {
            command = commandLedger.acquire(
                slot,
                `sales-change:${changeOrder.id}:submit`,
                {
                    salesChangeOrderId: changeOrder.id,
                    salesOrderId,
                    version: changeOrder.version ?? 0,
                    nature,
                },
            )
        }
        if (!command) return
        try {
            const submitted = await submitMutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle(slot, "succeeded")
            setSubmitOpen(false)
            onResult?.({
                status: "succeeded",
                title: "改单已提交审批",
                description: `已进入「${submitted.statusLabel}」。当前销售版本对客户仍然有效。`,
                reference: submitted.id,
                nextResponsible:
                    submitted.approval?.instance?.currentAssigneeName ??
                    submitted.approval?.instance?.currentAssignee,
            })
        } catch (error) {
            const settlement = classifyFormalCommandError(error)
            commandLedger.settle(slot, settlement)
            const failure = getErrorPresentation(
                error,
                "改单未提交，请刷新后重试。",
            )
            onResult?.({
                status: settlement === "unknown" ? "unknown" : "blocked",
                title:
                    settlement === "unknown" ? "处理结果待确认" : failure.title,
                description:
                    settlement === "unknown"
                        ? "请使用本次操作重试；确认前不要重复提交改单。"
                        : failure.description,
                reference: changeOrder.id,
            })
            throw error
        }
    }

    return (
        <div className="space-y-3">
            <SalesChangeOrderApprovalArea
                phase={phase}
                approval={changeOrder.approval}
                documentId={changeOrder.id}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                workItemAllowedActions={workItemAllowedActions}
                onDecisionApplied={(view: ApprovalCommandView) =>
                    onResult?.({
                        status: "succeeded",
                        title: "审批决定已提交",
                        description: view.latestRejectionReason
                            ? `已按当前任务提交决定。${view.latestRejectionReason}`
                            : "已按当前任务提交决定。",
                        reference: changeOrder.id,
                        nextResponsible: view.currentAssigneeName,
                    })
                }
            />
            {canSubmit ? (
                <Button
                    id="sales-orders-change-submit"
                    type="button"
                    onClick={() => setSubmitOpen(true)}
                >
                    提交改单
                </Button>
            ) : null}
            <SalesChangeOrderSubmitConfirmDialog
                open={submitOpen}
                pending={submitMutation.isPending}
                approval={changeOrder.approval}
                onOpenChange={setSubmitOpen}
                onConfirm={() => {
                    void submitChange()
                }}
            />
        </div>
    )
}
