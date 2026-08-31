"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PurchaseChangeOrderApprovalArea } from "@/features/purchase-orders/components/purchase-change-order-approval-area"
import { PurchaseChangeOrderSubmitConfirmDialog } from "@/features/purchase-orders/components/purchase-change-order-submit-confirm-dialog"
import { useSubmitPurchaseChangeMutation } from "@/features/purchase-orders/hooks/queries"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import {
    mergePurchaseChangeOrderAllowedActions,
    purchaseChangeOrderApprovalPhase,
} from "@/features/purchase-orders/lib/purchase-change-order-approval"
import type { PurchaseChangeOrderSummary } from "@/features/purchase-orders/types"
import { FormalCommandKeyLedger } from "@/lib/formal-command"

/**
 * 采购变更单在详情页上的审批区入口。
 *
 * 未提交且服务端允许时展示提交确认；决定、升级和撤回只读 `allowed_actions`。
 */
export function PurchaseChangeOrderApprovalSection({
    purchaseOrderId,
    changeOrder,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onResult,
}: {
    purchaseOrderId: string
    changeOrder: PurchaseChangeOrderSummary | null
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const submitMutation = useSubmitPurchaseChangeMutation()
    const [submitOpen, setSubmitOpen] = React.useState(false)
    const ledgerRef = React.useRef<FormalCommandKeyLedger | null>(null)
    if (ledgerRef.current == null) {
        ledgerRef.current = new FormalCommandKeyLedger()
    }
    const commandLedger = ledgerRef.current

    if (!changeOrder) {
        return (
            <Alert variant="warning">
                <AlertTitle>改单已不在当前采购单上</AlertTitle>
                <AlertDescription>
                    任务或改单关系已变化，请返回后刷新再处理。
                </AlertDescription>
            </Alert>
        )
    }

    const phase = purchaseChangeOrderApprovalPhase(
        changeOrder.approval,
        changeOrder.statusCode,
    )
    const allowedActions = mergePurchaseChangeOrderAllowedActions(
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
            purchaseChangeOrderId: string
            purchaseOrderId: string
            expectedLockVersion: number
        }>(slot)
        if (!command) {
            command = commandLedger.acquire(
                slot,
                `purchase-change:${changeOrder.id}:submit`,
                {
                    purchaseChangeOrderId: changeOrder.id,
                    purchaseOrderId,
                    expectedLockVersion: changeOrder.version ?? 0,
                },
            )
        }
        if (!command) return
        const response = await submitMutation.mutateAsync({
            ...command.payload,
            idempotencyKey: command.idempotencyKey,
        })
        commandLedger.settle(slot, response.status)
        if (response.status === "succeeded") {
            setSubmitOpen(false)
            onResult?.({
                status: "succeeded",
                title: "改单已提交审批",
                description: `已进入「${response.data.statusLabel}」。当前采购版本对供应商仍然有效。`,
                reference: response.reference,
                facts: response.data.approval?.instance?.currentAssigneeName
                    ? [
                          {
                              label: "当前审批人",
                              value: response.data.approval.instance
                                  .currentAssigneeName,
                          },
                      ]
                    : undefined,
            })
            return
        }
        if (response.status === "unknown") {
            onResult?.({
                status: "unknown",
                title: "处理结果待确认",
                description: "请使用本次操作重试；确认前不要重复提交改单。",
                reference: changeOrder.id,
            })
            return
        }
        onResult?.({
            status: "blocked",
            title: "改单未提交",
            description: response.message,
            reference: changeOrder.id,
        })
    }

    return (
        <div className="space-y-3">
            <PurchaseChangeOrderApprovalArea
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
                        facts: view.currentAssigneeName
                            ? [
                                  {
                                      label: "当前审批人",
                                      value: view.currentAssigneeName,
                                  },
                              ]
                            : undefined,
                    })
                }
            />
            {canSubmit ? (
                <Button
                    id={`procurement-orders-change-submit-${changeOrder.id}`}
                    type="button"
                    onClick={() => setSubmitOpen(true)}
                >
                    提交改单
                </Button>
            ) : null}
            <PurchaseChangeOrderSubmitConfirmDialog
                idPrefix="procurement-orders-change-submit-confirm"
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
