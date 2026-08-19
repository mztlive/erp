"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PurchaseOrderApprovalArea } from "@/features/purchase-orders/components/purchase-order-approval-area"
import { EditSurface } from "@/features/purchase-orders/components/purchase-order-surfaces"
import { PurchaseOrderDetailDialogs } from "@/features/purchase-orders/components/purchase-order-detail-dialogs"
import { PurchaseOrderDetailHeader } from "@/features/purchase-orders/components/purchase-order-detail-header"
import { PurchaseOrderDetailSections } from "@/features/purchase-orders/components/purchase-order-detail-sections"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { usePurchaseOrderDetailCommandState } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { usePurchaseOrderDetailEditActions } from "@/features/purchase-orders/hooks/use-purchase-order-detail-edit-actions"
import { usePurchaseOrderDetailEditGuard } from "@/features/purchase-orders/hooks/use-purchase-order-detail-edit-guard"
import { usePurchaseOrderDetailPermissions } from "@/features/purchase-orders/hooks/use-purchase-order-detail-permissions"
import { usePurchaseOrderDetailReviewActions } from "@/features/purchase-orders/hooks/use-purchase-order-detail-review-actions"
import { purchaseOrderApprovalPhase } from "@/features/purchase-orders/lib/purchase-order-approval"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import {
    buildPurchaseOrderDetailNavItems,
    resolvePurchaseOrderDetailMode,
    resolvePurchaseOrderDetailSection,
} from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import { purchaseOrderDraftFormSchema } from "@/features/purchase-orders/lib/purchase-order-validation"

/**
 * 采购单详情。创建结果、编辑与运行中都嵌入通用审批区，不复制状态推导。
 */
export function PurchaseOrderDetailPage({
    purchaseOrderId,
    section,
    mode: modeParam,
    workItemId,
}: {
    purchaseOrderId: string
    section?: string
    mode?: string
    workItemId?: string
}) {
    const router = useRouter()
    const query = usePurchaseOrderCenterQuery(purchaseOrderId)
    const focusedWorkItemQuery = useWorkItemDetailQuery(workItemId ?? "")
    const focusedWorkItem = focusedWorkItemQuery.data
        ? mapWorkItemDto(focusedWorkItemQuery.data)
        : undefined

    const activeSection = resolvePurchaseOrderDetailSection(section)
    const mode = resolvePurchaseOrderDetailMode(modeParam)
    const order = query.data

    const { commandLedger, result, setResult } =
        usePurchaseOrderDetailCommandState(purchaseOrderId)

    const reviewActions = usePurchaseOrderDetailReviewActions({
        purchaseOrderId,
        order,
        refetch: query.refetch,
        commandLedger,
        setResult,
    })

    const draftForm = useAppForm({
        defaultValues: {
            paymentTermCode: order?.header.paymentTermCode ?? "POSTPAY_NET15",
            note: "",
        },
        validators: { onChange: purchaseOrderDraftFormSchema },
        onSubmit: async () => {
            await editActions.handleSave()
        },
    })

    const editActions = usePurchaseOrderDetailEditActions({
        purchaseOrderId,
        mode,
        order,
        refetch: query.refetch,
        commandLedger,
        setResult,
        getPaymentTermCode: () => draftForm.state.values.paymentTermCode,
        setDraftPaymentTermCode: (value) =>
            draftForm.setFieldValue("paymentTermCode", value),
    })

    const permissions = usePurchaseOrderDetailPermissions(order, commandLedger)

    const guard = usePurchaseOrderDetailEditGuard({
        mode,
        order,
        paymentTermCode: draftForm.state.values.paymentTermCode,
        note: draftForm.state.values.note,
        lineEdits: editActions.lineEdits,
        onSave: editActions.handleSave,
    })

    const titleRef = React.useRef<HTMLHeadingElement>(null)

    React.useEffect(() => {
        titleRef.current?.focus()
    }, [purchaseOrderId, mode])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (mode !== "edit") return
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key.toLowerCase() === "s"
            ) {
                event.preventDefault()
                void editActions.handleSave()
            }
            if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
                event.preventDefault()
                editActions.setSubmitConfirmOpen(true)
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [mode, editActions.draftEditToken, editActions.lineEdits, order])

    if (query.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="采购单" description="正在加载详情…" />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-24 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (query.isError) {
        return (
            <PageScaffold>
                <PageHeader title="采购单" description="详情加载失败" />
                <BusinessFailureState
                    title="详情加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                    action={
                        <Button
                            variant="outline"
                            size="sm"
                            render={<Link href="/procurement/orders" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!order) {
        return (
            <PageScaffold>
                <PageHeader
                    title="采购单不存在"
                    description="单据可能已删除或编号有误"
                />
                <BusinessEmptyState
                    kind="no-data"
                    title="未找到该采购单"
                    description="该采购单可能已删除或不在当前数据范围内。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        <Button
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            render={<Link href="/procurement/orders" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const baseHref = `/procurement/orders/${order.identity.purchaseOrderId}`
    const w12PayHref = `/finance/supplier-accounts?view=payable&session=payment&from=W08&purchaseOrderId=${encodeURIComponent(order.identity.purchaseOrderId)}&supplierId=${encodeURIComponent(order.header.supplierId)}&returnTo=${encodeURIComponent(baseHref)}`
    const w27SettleHref = `/supplier-api/settlements?supplierId=${encodeURIComponent(order.header.supplierId)}&returnTo=${encodeURIComponent(baseHref)}`
    const displayNo =
        order.identity.purchaseNo ??
        order.identity.draftLabel ??
        "采购单（未编号）"
    const costMasked = order.currentContent.costMasked
    const gate = order.progress.prepaymentGate

    const navItems = buildPurchaseOrderDetailNavItems(baseHref, mode)

    const modeLabel =
        mode === "edit"
            ? order.identity.reviewStatus === "REJECTED"
                ? "被驳回待修改"
                : "采购草稿编辑"
            : mode === "review"
              ? "审批中（只读）"
              : "详情"

    return (
        <PageScaffold>
            <PurchaseOrderDetailHeader
                order={order}
                mode={mode}
                displayNo={displayNo}
                modeLabel={modeLabel}
                titleRef={titleRef}
                router={router}
                baseHref={baseHref}
                w12PayHref={w12PayHref}
                w27SettleHref={w27SettleHref}
                canPay={permissions.canPay}
                canFulfill={permissions.canFulfill}
                canEdit={permissions.canEdit}
                canOpenReview={permissions.canOpenReview}
                canChange={permissions.canChange}
                requestLeave={guard.requestLeave}
                onRequestChange={() => editActions.setChangeConfirmOpen(true)}
                result={result}
                onDismissResult={() => setResult(null)}
            />

            <PurchaseOrderApprovalArea
                phase={purchaseOrderApprovalPhase(
                    order.approval,
                    order.identity.status,
                )}
                approval={order.approval}
                documentId={order.identity.purchaseOrderId}
                workItemId={focusedWorkItem?.workItemId}
                expectedTaskVersion={focusedWorkItem?.taskVersion}
                workItemAllowedActions={focusedWorkItem?.allowedActions}
                onDecisionApplied={(view: ApprovalCommandView) =>
                    setResult({
                        status: "succeeded",
                        title: "审批决定已提交",
                        description: view.latestRejectionReason
                            ? `已按当前任务提交决定。${view.latestRejectionReason}`
                            : "已按当前任务提交决定。",
                        reference:
                            order.identity.purchaseNo ??
                            order.identity.draftLabel,
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

            {mode === "edit" && permissions.canEdit ? (
                <EditSurface
                    order={order}
                    // @ts-expect-error useAppForm generic variance vs surface prop
                    draftForm={draftForm}
                    lineEdits={editActions.lineEdits}
                    setLineEdits={editActions.setLineEdits}
                    draftEditToken={editActions.draftEditToken}
                    canSubmit={permissions.canSubmit}
                    savePending={editActions.savePending}
                    onSave={() => void editActions.handleSave()}
                    onSubmitOpen={() => editActions.setSubmitConfirmOpen(true)}
                />
            ) : null}

            <PurchaseOrderDetailSections
                order={order}
                activeSection={activeSection}
                mode={mode}
                navItems={navItems}
                costMasked={costMasked}
                gate={gate}
                canPay={permissions.canPay}
                canFulfill={permissions.canFulfill}
                fulfillBlocker={permissions.fulfillBlocker}
                canChange={permissions.canChange}
                changeBlocker={permissions.changeBlocker}
                baseHref={baseHref}
                w12PayHref={w12PayHref}
                onRequestChange={() => editActions.setChangeConfirmOpen(true)}
            />

            <PurchaseOrderDetailDialogs
                order={order}
                submitConfirmOpen={editActions.submitConfirmOpen}
                onSubmitConfirmOpenChange={editActions.setSubmitConfirmOpen}
                approveConfirmOpen={reviewActions.approveConfirmOpen}
                onApproveConfirmOpenChange={reviewActions.setApproveConfirmOpen}
                releaseConfirmOpen={reviewActions.releaseConfirmOpen}
                onReleaseConfirmOpenChange={reviewActions.setReleaseConfirmOpen}
                changeConfirmOpen={editActions.changeConfirmOpen}
                onChangeConfirmOpenChange={editActions.setChangeConfirmOpen}
                leaveGuardOpen={guard.leaveGuardOpen}
                onLeaveGuardOpenChange={guard.setLeaveGuardOpen}
                releaseReason={reviewActions.releaseReason}
                onReleaseReasonChange={reviewActions.setReleaseReason}
                submitPending={editActions.submitPending}
                savePending={editActions.savePending}
                reviewPending={reviewActions.reviewPending}
                responsibilityPending={reviewActions.responsibilityPending}
                changePending={editActions.changePending}
                onConfirmSubmit={() => void editActions.handleSubmit()}
                onConfirmApprove={() => void reviewActions.handleApprove()}
                onConfirmRelease={() =>
                    void reviewActions.handleReleaseToTeam()
                }
                onConfirmChange={() => void editActions.handleStartChange()}
                onSaveAndLeave={() => void guard.saveAndLeave()}
                onDiscardAndLeave={guard.discardAndLeave}
            />
        </PageScaffold>
    )
}
