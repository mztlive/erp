"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { detailApprovalSummaryClassName } from "@/components/business/detail-presentation"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PurchaseChangeOrderApprovalArea } from "@/features/purchase-orders/components/purchase-change-order-approval-area"
import { PurchaseChangeOrderEditDialog } from "./purchase-change-order-edit-dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import {
    mergePurchaseChangeOrderAllowedActions,
    PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE,
    purchaseChangeOrderApprovalPhase,
} from "@/features/purchase-orders/lib/purchase-change-order-approval"
import type { PurchaseChangeOrderSummary } from "@/features/purchase-orders/types"

/**
 * 采购变更单在详情页上的审批区入口。
 *
 * 详情查阅仅保留采购方提交与撤回；工作台调用方继续按服务端动作办理审批。
 */
export function PurchaseChangeOrderApprovalSection({
    purchaseOrderId,
    changeOrder,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onResult,
    readonlyApproval = false,
}: {
    readonlyApproval?: boolean
    purchaseOrderId: string
    changeOrder: PurchaseChangeOrderSummary | null
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const [editOpen, setEditOpen] = React.useState(false)
    const searchParams = useSearchParams()
    const router = useRouter()
    const pathname = usePathname()
    const consumedEdit = React.useRef<string | null>(null)
    const autoEdit =
        searchParams.get("editChange") === "1" &&
        searchParams.get("changeOrderId") === changeOrder?.id
    const autoCanSubmit = Boolean(
        changeOrder &&
        purchaseChangeOrderApprovalPhase(
            changeOrder.approval,
            changeOrder.statusCode,
        ) === "draft" &&
        mergePurchaseChangeOrderAllowedActions(
            changeOrder.approval?.allowedActions,
            workItemAllowedActions,
        ).includes("SUBMIT"),
    )
    React.useEffect(() => {
        if (!autoEdit) consumedEdit.current = null
        if (
            autoEdit &&
            autoCanSubmit &&
            consumedEdit.current !== changeOrder?.id
        ) {
            consumedEdit.current = changeOrder?.id ?? null
            setEditOpen(true)
        }
    }, [autoEdit, autoCanSubmit, changeOrder?.id])
    const changeEditOpen = (open: boolean) => {
        setEditOpen(open)
        if (!open && autoEdit) {
            const params = new URLSearchParams(searchParams.toString())
            params.delete("editChange")
            router.replace(`${pathname}?${params}`, { scroll: false })
        }
    }

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

    const editDocumentHref = `/procurement/orders/${encodeURIComponent(purchaseOrderId)}?${new URLSearchParams({ section: "changes", changeOrderId: changeOrder.id, editChange: "1" })}`

    return (
        <div className="space-y-3">
            {readonlyApproval ? (
                <ApprovalReadonly
                    className={detailApprovalSummaryClassName}
                    id={`purchase-change-${changeOrder.id}`}
                    approval={changeOrder.approval}
                />
            ) : (
                <PurchaseChangeOrderApprovalArea
                    phase={phase}
                    approval={changeOrder.approval}
                    documentId={changeOrder.id}
                    documentVersion={changeOrder.version}
                    editDocumentHref={editDocumentHref}
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
            )}
            {readonlyApproval ? (
                <ApprovalActionBar
                    id="purchase-orders-change-owner-actions"
                    allowedActions={(
                        changeOrder.approval?.allowedActions ?? []
                    ).filter(
                        (action) =>
                            action === "CANCEL" || action === "CANCEL_APPROVAL",
                    )}
                    instance={changeOrder.approval?.instance}
                    documentType={PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE}
                    documentId={changeOrder.id}
                    documentVersion={changeOrder.version}
                    editDocumentHref={editDocumentHref}
                    onDecisionApplied={() =>
                        onResult?.({
                            status: "succeeded",
                            title: "改单审批已撤回",
                            description: "请查阅更新后的改单状态。",
                            reference: changeOrder.id,
                        })
                    }
                />
            ) : null}
            {canSubmit ? (
                <Button
                    id={`procurement-orders-change-submit-${toAutomationIdSegment(changeOrder.id)}`}
                    type="button"
                    onClick={() => changeEditOpen(true)}
                >
                    修改并提交
                </Button>
            ) : null}
            <PurchaseChangeOrderEditDialog
                open={editOpen}
                purchaseOrderId={purchaseOrderId}
                changeOrderId={changeOrder.id}
                approval={changeOrder.approval}
                onOpenChange={changeEditOpen}
                onResult={onResult}
            />
        </div>
    )
}
