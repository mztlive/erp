"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { ChangeOrderVoidButton } from "@/components/business/change-order-void-button"
import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { ChangeOrderPendingCard } from "@/features/approval-workflow/components/change-order-pending-card"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PurchaseChangeOrderApprovalArea } from "@/features/purchase-orders/components/purchase-change-order-approval-area"
import { PurchaseChangeCoverageEffects } from "./purchase-change-coverage-effects"
import { PurchaseChangeOrderEditDialog } from "./purchase-change-order-edit-dialog"
import { PURCHASE_CHANGE_PLACEHOLDER_REASON } from "@/features/purchase-orders/api/purchase-order-commands"
import { useVoidPurchaseChangeMutation } from "@/features/purchase-orders/hooks/queries"
import { getErrorPresentation } from "@/lib/api/errors"
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
 * 详情查阅以「进行中的改单」卡片展示，保留采购方编辑、撤回与作废；
 * 工作台调用方继续按服务端动作办理审批。与销售改单同构。
 */
export function PurchaseChangeOrderApprovalSection({
    purchaseOrderId,
    changeOrder,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onResult,
    readonlyApproval = false,
    baseRevisionNo,
    paidAmount,
}: {
    readonlyApproval?: boolean
    /** 改单基于的采购生效版本号。 */
    baseRevisionNo?: number | null
    /** 已付款金额；改后总额不得低于此金额。 */
    paidAmount?: string
    purchaseOrderId: string
    changeOrder: PurchaseChangeOrderSummary | null
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onResult?: (result: PurchaseOrderDetailResult) => void
}) {
    const [editOpen, setEditOpen] = React.useState(false)
    const voidMutation = useVoidPurchaseChangeMutation()
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

    const voidChange = async () => {
        try {
            await voidMutation.mutateAsync({
                purchaseChangeOrderId: changeOrder.id,
                purchaseOrderId,
                expectedLockVersion: changeOrder.version ?? 0,
            })
            onResult?.({
                status: "succeeded",
                title: "改单已作废",
                description:
                    "采购单仍按当前生效版本执行，需要时可重新发起改单。",
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
                id={`procurement-orders-change-submit-${toAutomationIdSegment(changeOrder.id)}`}
                type="button"
                variant="outline"
                onClick={() => changeEditOpen(true)}
            >
                编辑改单
            </Button>
            <ChangeOrderVoidButton
                id={`procurement-orders-change-void-${toAutomationIdSegment(changeOrder.id)}`}
                sourceLabel="采购单"
                pending={voidMutation.isPending}
                onConfirm={voidChange}
            />
        </>
    ) : null

    const coverage = (
        <PurchaseChangeCoverageEffects
            id={`purchase-change-${toAutomationIdSegment(changeOrder.id)}-coverage`}
            effects={changeOrder.coverageEffects}
        />
    )

    return (
        <div className="space-y-3">
            {coverage}
            {readonlyApproval ? (
                <ChangeOrderPendingCard
                    id={`purchase-change-${changeOrder.id}`}
                    changeOrder={{
                        ...changeOrder,
                        baseRevisionNo: baseRevisionNo ?? 0,
                    }}
                    counterparty="供应商"
                    running={phase === "runtime"}
                    actions={
                        <>
                            <ApprovalActionBar
                                id="purchase-orders-change-owner-actions"
                                allowedActions={(
                                    changeOrder.approval?.allowedActions ?? []
                                ).filter(
                                    (action) =>
                                        action === "CANCEL" ||
                                        action === "CANCEL_APPROVAL",
                                )}
                                instance={changeOrder.approval?.instance}
                                documentType={
                                    PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE
                                }
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
                            {editButton}
                        </>
                    }
                >
                    {isPlaceholderReason(changeOrder.reason) ? null : (
                        <p className="text-sm">
                            <span className="text-muted-foreground">
                                变更原因：
                            </span>
                            {changeOrder.reason}
                        </p>
                    )}
                </ChangeOrderPendingCard>
            ) : (
                <>
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
                    {editButton ? (
                        <div className="flex flex-wrap gap-2">{editButton}</div>
                    ) : null}
                </>
            )}
            {canSubmit && editOpen ? (
                <PurchaseChangeOrderEditDialog
                    open={editOpen}
                    purchaseOrderId={purchaseOrderId}
                    changeOrderId={changeOrder.id}
                    approval={changeOrder.approval}
                    paidAmount={paidAmount}
                    onOpenChange={changeEditOpen}
                    onResult={onResult}
                />
            ) : null}
        </div>
    )
}

/** 发起改单时的占位原因不展示。 */
function isPlaceholderReason(reason: string) {
    return !reason.trim() || reason === PURCHASE_CHANGE_PLACEHOLDER_REASON
}
