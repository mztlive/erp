"use client"

import * as React from "react"
import { useSearchParams } from "next/navigation"
import { SalesChangeOrderEditDialog } from "./sales-change-order-edit-dialog"
import { SalesChangeProcurementProgressCard } from "./sales-change-procurement-progress"
import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { ChangeOrderPendingCard } from "@/features/approval-workflow/components/change-order-pending-card"

import { ChangeOrderVoidButton } from "@/components/business/change-order-void-button"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { useVoidSalesChangeOrderMutation } from "@/features/sales-orders/hooks/queries"
import { getErrorPresentation } from "@/lib/api/errors"
import { SalesChangeOrderApprovalArea } from "@/features/sales-orders/components/sales-change-order-approval-area"
import {
    mergeSalesChangeOrderAllowedActions,
    SALES_CHANGE_ORDER_DOCUMENT_TYPE,
    salesChangeOrderApprovalPhase,
} from "@/features/sales-orders/lib/sales-change-order-approval"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import type {
    SalesChangeOrderSummary,
    SalesOrderNature,
} from "@/features/sales-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

/**
 * 销售变更单在详情页上的审批区入口。
 *
 * 详情查阅只保留销售方提交与撤回；工作台调用方继续按服务端动作办理审批。
 */
export function SalesChangeOrderApprovalSection({
    salesOrderId,
    nature,
    changeOrder,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onResult,
    readonlyApproval = false,
}: {
    readonlyApproval?: boolean
    salesOrderId: string
    nature: SalesOrderNature
    changeOrder: SalesChangeOrderSummary | null
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onResult?: (result: SalesOrderDetailActionResult) => void
}) {
    const [editOpen, setEditOpen] = React.useState(false)
    const voidMutation = useVoidSalesChangeOrderMutation()
    const searchParams = useSearchParams()
    const consumedEditRequest = React.useRef<string | null>(null)
    const editRequested = searchParams.get("editChange") === changeOrder?.id
    React.useEffect(() => {
        if (
            editRequested &&
            consumedEditRequest.current !== changeOrder?.id &&
            changeOrder &&
            salesChangeOrderApprovalPhase(
                changeOrder.approval,
                changeOrder.statusCode,
            ) === "draft"
        ) {
            consumedEditRequest.current = changeOrder.id
            setEditOpen(true)
        }
    }, [editRequested, changeOrder])

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

    const editDocumentHref = `/sales/orders/${encodeURIComponent(salesOrderId)}?section=change-review&changeOrderId=${encodeURIComponent(changeOrder.id)}&editChange=${encodeURIComponent(changeOrder.id)}`

    const voidChange = async () => {
        try {
            await voidMutation.mutateAsync({
                salesChangeOrderId: changeOrder.id,
                salesOrderId,
                version: changeOrder.version ?? 0,
                nature,
            })
            onResult?.({
                status: "succeeded",
                title: "改单已作废",
                description:
                    "销售单仍按当前生效版本执行，需要时可重新发起改单。",
            })
        } catch (error) {
            const failure = getErrorPresentation(
                error,
                "改单未作废，请刷新后重试。",
            )
            onResult?.({
                status: "blocked",
                title: failure.title,
                description: failure.description,
            })
        }
    }

    const editButton = canSubmit ? (
        <>
            <Button
                id="sales-orders-change-edit-original"
                type="button"
                variant="outline"
                onClick={() => setEditOpen(true)}
            >
                编辑改单
            </Button>
            <ChangeOrderVoidButton
                id="sales-orders-change-void"
                sourceLabel="销售单"
                pending={voidMutation.isPending}
                onConfirm={voidChange}
            />
        </>
    ) : null

    return (
        <div className="space-y-3">
            {changeOrder.procurement ? (
                <SalesChangeProcurementProgressCard
                    id={`sales-change-${toAutomationIdSegment(changeOrder.id)}-procurement`}
                    procurement={changeOrder.procurement}
                />
            ) : null}
            {readonlyApproval ? (
                <ChangeOrderPendingCard
                    id={`sales-change-${changeOrder.id}`}
                    changeOrder={changeOrder}
                    counterparty="客户"
                    running={phase === "runtime"}
                    actions={
                        <>
                            <ApprovalActionBar
                                id="sales-orders-change-owner-actions"
                                allowedActions={(
                                    changeOrder.approval?.allowedActions ?? []
                                ).filter(
                                    (action) =>
                                        action === "CANCEL" ||
                                        action === "CANCEL_APPROVAL",
                                )}
                                instance={changeOrder.approval?.instance}
                                documentType={SALES_CHANGE_ORDER_DOCUMENT_TYPE}
                                documentId={changeOrder.id}
                                documentVersion={changeOrder.version}
                                editDocumentHref={editDocumentHref}
                                onDecisionApplied={() =>
                                    onResult?.({
                                        status: "succeeded",
                                        title: "改单审批已撤回",
                                        description: "请查阅更新后的改单状态。",
                                    })
                                }
                            />
                            {editButton}
                        </>
                    }
                />
            ) : (
                <>
                    <SalesChangeOrderApprovalArea
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
                                nextResponsible: view.currentAssigneeName,
                            })
                        }
                    />
                    {editButton}
                </>
            )}
            {canSubmit && editOpen ? (
                <SalesChangeOrderEditDialog
                    open={editOpen}
                    onOpenChange={setEditOpen}
                    salesOrderId={salesOrderId}
                    changeOrderId={changeOrder.id}
                    nature={nature}
                    approval={changeOrder.approval}
                    onApplied={() =>
                        onResult?.({
                            status: "succeeded",
                            title: "改单已提交审批",
                            description:
                                "修改内容已保存，并以原变更单重新发起审批。",
                        })
                    }
                />
            ) : null}
        </div>
    )
}
