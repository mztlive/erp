"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import { detailApprovalSummaryClassName } from "@/components/business/detail-presentation"
import { PurchaseChangeOrderApprovalSection } from "@/features/purchase-orders/components/purchase-change-order-approval-section"
import { EditSurface } from "@/features/purchase-orders/components/purchase-order-surfaces"
import { PurchaseOrderDetailDialogs } from "@/features/purchase-orders/components/purchase-order-detail-dialogs"
import { PurchaseOrderDetailHeader } from "@/features/purchase-orders/components/purchase-order-detail-header"
import { PurchaseOrderDetailSidebar } from "@/features/purchase-orders/components/purchase-order-detail-sidebar"
import { PurchaseOrderDetailSections } from "@/features/purchase-orders/components/purchase-order-detail-sections"
import {
    usePurchaseOrderCenterQuery,
    purchaseOrderKeys,
} from "@/features/purchase-orders/hooks/queries"
import {
    type PurchaseOrderDetailResult,
    usePurchaseOrderDetailCommandState,
} from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { usePurchaseOrderDetailEditActions } from "@/features/purchase-orders/hooks/use-purchase-order-detail-edit-actions"
import { usePurchaseOrderDetailEditGuard } from "@/features/purchase-orders/hooks/use-purchase-order-detail-edit-guard"
import { usePurchaseOrderDetailPermissions } from "@/features/purchase-orders/hooks/use-purchase-order-detail-permissions"
import { isPurchaseChangeOrderWorkItem } from "@/features/purchase-orders/lib/purchase-change-order-approval"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import {
    resolvePurchaseOrderDetailMode,
    resolvePurchaseOrderDetailSection,
} from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import { purchaseOrderDraftFormSchema } from "@/features/purchase-orders/lib/purchase-order-validation"

/**
 * 采购单详情保留采购方操作，审批与跨岗位进度只读展示；
 * 采购变更单走独立 DocumentType，不复制状态推导。
 */
export function PurchaseOrderDetailPage({
    purchaseOrderId,
    section,
    mode: modeParam,
    workItemId,
    changeOrderId,
}: {
    purchaseOrderId: string
    section?: string
    mode?: string
    workItemId?: string
    changeOrderId?: string
}) {
    const router = useRouter()
    const focusedWorkItemQuery = useWorkItemDetailQuery(workItemId ?? "")
    const focusedWorkItem = focusedWorkItemQuery.data
        ? mapWorkItemDto(focusedWorkItemQuery.data)
        : undefined
    const isChangeOrderTask = isPurchaseChangeOrderWorkItem(focusedWorkItem)
    const focusedChangeOrderId =
        changeOrderId ??
        (isChangeOrderTask ? focusedWorkItem?.businessObjectId : undefined)
    const query = usePurchaseOrderCenterQuery(purchaseOrderId, {
        changeOrderId: focusedChangeOrderId,
    })

    const activeSection = resolvePurchaseOrderDetailSection(
        section ?? (isChangeOrderTask ? "changes" : undefined),
    )
    const mode = resolvePurchaseOrderDetailMode(modeParam)
    const order = query.data

    const { commandLedger, result, setResult } =
        usePurchaseOrderDetailCommandState(purchaseOrderId)

    // 审批决定/命令成功后单据状态会变（如审批中→已生效），统一在结果落定时
    // 刷新采购单查询，避免页面停留在旧状态。
    const queryClient = useQueryClient()
    const handleResult = React.useCallback(
        (next: React.SetStateAction<PurchaseOrderDetailResult | null>) => {
            const resolved = typeof next === "function" ? next(result) : next
            if (
                resolved &&
                (resolved.status === "succeeded" ||
                    resolved.status === "unknown")
            ) {
                void queryClient.invalidateQueries({
                    queryKey: purchaseOrderKeys.detail(purchaseOrderId),
                })
            }
            setResult(resolved)
        },
        [queryClient, purchaseOrderId, result, setResult],
    )

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
        setResult: handleResult,
        getPaymentTermCode: () => draftForm.state.values.paymentTermCode,
        setDraftPaymentTermCode: (value) =>
            draftForm.setFieldValue("paymentTermCode", value),
    })

    const permissions = usePurchaseOrderDetailPermissions(order)

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
                    action={
                        <>
                            <Button
                                id="procurement-orders-detail-retry"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void query.refetch()}
                            >
                                重试
                            </Button>
                            <Button
                                variant="outline"
                                size="sm"
                                render={
                                    <Link
                                        id="procurement-orders-detail-back-from-error"
                                        href="/procurement/orders"
                                    />
                                }
                            >
                                返回列表
                            </Button>
                        </>
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
                            render={
                                <Link
                                    id="procurement-orders-detail-missing-back"
                                    href="/procurement/orders"
                                />
                            }
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const baseHref = `/procurement/orders/${order.identity.purchaseOrderId}`
    const displayNo =
        order.identity.purchaseNo ??
        order.identity.draftLabel ??
        "采购单（未编号）"
    const costMasked = order.currentContent.costMasked
    const gate = order.progress.prepaymentGate

    const modeLabel =
        mode === "edit"
            ? order.identity.reviewStatus === "REJECTED"
                ? "被驳回待修改"
                : "采购草稿编辑"
            : "详情"

    return (
        <PageScaffold density="compact">
            <PurchaseOrderDetailHeader
                order={order}
                mode={mode}
                displayNo={displayNo}
                modeLabel={modeLabel}
                titleRef={titleRef}
                router={router}
                baseHref={baseHref}
                canEdit={permissions.canEdit}
                canSubmit={permissions.canSubmit}
                canVoid={permissions.canVoid}
                canChange={permissions.canChange}
                requestLeave={guard.requestLeave}
                onRequestVoid={() => editActions.setVoidConfirmOpen(true)}
                onRequestChange={() => editActions.setChangeConfirmOpen(true)}
                onRequestSubmit={() => editActions.setSubmitConfirmOpen(true)}
                onCancelResult={handleResult}
                result={result}
                onDismissResult={() => handleResult(null)}
            />

            {isChangeOrderTask && activeSection !== "changes" ? (
                <PurchaseChangeOrderApprovalSection
                    readonlyApproval
                    purchaseOrderId={order.identity.purchaseOrderId}
                    changeOrder={order.activeChangeOrder ?? null}
                    workItemId={focusedWorkItem?.workItemId}
                    expectedTaskVersion={focusedWorkItem?.taskVersion}
                    workItemAllowedActions={focusedWorkItem?.allowedActions}
                    onResult={handleResult}
                />
            ) : null}

            <PurchaseOrderDetailSections
                order={order}
                activeSection={activeSection}
                editor={
                    mode === "edit" && permissions.canEdit ? (
                        <EditSurface
                            order={order}
                            lineEdits={editActions.lineEdits}
                            setLineEdits={editActions.setLineEdits}
                            draftEditToken={editActions.draftEditToken}
                            canSubmit={permissions.canSubmit}
                            savePending={editActions.savePending}
                            onSave={() => void editActions.handleSave()}
                            onSubmitOpen={() =>
                                editActions.setSubmitConfirmOpen(true)
                            }
                        />
                    ) : undefined
                }

                costMasked={costMasked}
                gate={gate}
                changeWorkItemId={
                    isChangeOrderTask ? focusedWorkItem?.workItemId : undefined
                }
                changeExpectedTaskVersion={
                    isChangeOrderTask ? focusedWorkItem?.taskVersion : undefined
                }
                changeWorkItemAllowedActions={
                    isChangeOrderTask
                        ? focusedWorkItem?.allowedActions
                        : undefined
                }
                onChangeApprovalResult={handleResult}
                approvalPanel={
                    <ApprovalReadonly
                        id="purchase-order-approval"
                        approval={order.approval}
                        className={detailApprovalSummaryClassName}
                    />
                }
                sidebar={
                    <PurchaseOrderDetailSidebar
                        order={order}
                        costMasked={costMasked}
                    />
                }
            />

            <PurchaseOrderDetailDialogs
                order={order}
                submitConfirmOpen={editActions.submitConfirmOpen}
                onSubmitConfirmOpenChange={editActions.setSubmitConfirmOpen}
                voidConfirmOpen={editActions.voidConfirmOpen}
                onVoidConfirmOpenChange={editActions.setVoidConfirmOpen}
                changeConfirmOpen={editActions.changeConfirmOpen}
                onChangeConfirmOpenChange={editActions.setChangeConfirmOpen}
                leaveGuardOpen={guard.leaveGuardOpen}
                onLeaveGuardOpenChange={guard.setLeaveGuardOpen}
                submitPending={editActions.submitPending}
                savePending={editActions.savePending}
                voidPending={editActions.voidPending}
                changePending={editActions.changePending}
                onConfirmSubmit={() => void editActions.handleSubmit()}
                onConfirmVoid={() => void editActions.handleVoid()}
                onConfirmChange={() => void editActions.handleStartChange()}
                onSaveAndLeave={() => void guard.saveAndLeave()}
                onDiscardAndLeave={guard.discardAndLeave}
            />
        </PageScaffold>
    )
}
