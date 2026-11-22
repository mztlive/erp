"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSkeleton,
    FormalActionResult,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { PurchaseChangeOrderApprovalSection } from "@/features/purchase-orders/components/purchase-change-order-approval-section"
import { PurchaseChangeOrderEditDialog } from "@/features/purchase-orders/components/purchase-change-order-edit-dialog"
import { PurchaseOrderDetailDialogs } from "@/features/purchase-orders/components/purchase-order-detail-dialogs"
import {
    PurchaseOrderAmountSummary,
    PurchaseOrderDetailHeader,
} from "@/features/purchase-orders/components/purchase-order-detail-header"
import {
    PurchaseOrderDetailSectionContent,
    PurchaseOrderDetailSectionTabs,
} from "@/features/purchase-orders/components/purchase-order-detail-sections"
import { PurchaseOrderDetailSidebar } from "@/features/purchase-orders/components/purchase-order-detail-sidebar"
import { PurchaseOrderFreightNotice } from "@/features/purchase-orders/components/purchase-order-freight-notice"
import { PurchaseOrderSourceUpdateNotice } from "@/features/purchase-orders/components/purchase-order-source-update-notice"
import { EditSurface } from "@/features/purchase-orders/components/purchase-order-surfaces"
import {
    purchaseOrderKeys,
    usePurchaseOrderCenterQuery,
} from "@/features/purchase-orders/hooks/queries"
import {
    type PurchaseOrderDetailResult,
    usePurchaseOrderDetailCommandState,
} from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { usePurchaseOrderDetailEditActions } from "@/features/purchase-orders/hooks/use-purchase-order-detail-edit-actions"
import { usePurchaseOrderDetailEditGuard } from "@/features/purchase-orders/hooks/use-purchase-order-detail-edit-guard"
import { usePurchaseOrderDetailPermissions } from "@/features/purchase-orders/hooks/use-purchase-order-detail-permissions"
import {
    isPurchaseChangeOrderWorkItem,
    purchaseChangeOrderApprovalPhase,
} from "@/features/purchase-orders/lib/purchase-change-order-approval"
import {
    type PurchaseOrderDetailSectionId,
    purchaseOrderDetailHref,
    purchaseOrderSectionHref,
    resolvePurchaseOrderBackContext,
    resolvePurchaseOrderDetailMode,
    resolvePurchaseOrderDetailSection,
} from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useRouter, useSearchParams } from "@/lib/navigation"

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
    const searchParams = useSearchParams()
    const backContext = resolvePurchaseOrderBackContext(searchParams)
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
    const permissions = usePurchaseOrderDetailPermissions(order)
    const isEditing = mode === "edit" && permissions.canEdit

    const { commandLedger, result, setResult } =
        usePurchaseOrderDetailCommandState(purchaseOrderId)

    // 审批决定/命令成功后单据状态会变（如审批中→已生效），统一 toast 反馈并
    // 失效采购单相关缓存，setResult 使用函数式更新避免陈旧闭包。
    const queryClient = useQueryClient()
    const handleResult = React.useCallback(
        (next: React.SetStateAction<PurchaseOrderDetailResult | null>) => {
            setResult((prev) => {
                const resolved = typeof next === "function" ? next(prev) : next
                if (resolved?.status === "succeeded") {
                    toast.add({
                        title: resolved.title,
                        description: resolved.description,
                        type: "success",
                    })
                    void queryClient.invalidateQueries({
                        queryKey: purchaseOrderKeys.all,
                    })
                    return null
                }
                if (resolved?.status === "unknown") {
                    void queryClient.invalidateQueries({
                        queryKey: purchaseOrderKeys.all,
                    })
                }
                return resolved
            })
        },
        [queryClient, setResult],
    )

    const selectSection = React.useCallback(
        (next: PurchaseOrderDetailSectionId) => {
            router.replace(
                purchaseOrderSectionHref(purchaseOrderId, next, searchParams),
                { scroll: false },
            )
        },
        [purchaseOrderId, router, searchParams],
    )

    const editActions = usePurchaseOrderDetailEditActions({
        purchaseOrderId,
        mode,
        isEditing,
        order,
        refetch: query.refetch,
        commandLedger,
        setResult: handleResult,
    })

    const guard = usePurchaseOrderDetailEditGuard({
        mode,
        isEditing,
        order,
        lineEdits: editActions.lineEdits,
        busy: editActions.savePending || editActions.submitPending,
        onSave: editActions.handleSave,
    })

    const titleRef = React.useRef<HTMLHeadingElement>(null)
    const [editingChangeId, setEditingChangeId] = React.useState<string | null>(
        null,
    )

    React.useEffect(() => {
        titleRef.current?.focus()
    }, [purchaseOrderId, isEditing])

    const handleSaveRef = React.useRef(editActions.handleSave)
    handleSaveRef.current = editActions.handleSave

    const { draftEditToken, savePending, submitPending, setSubmitConfirmOpen } =
        editActions

    React.useEffect(() => {
        if (!isEditing) return
        const onKey = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.repeat ||
                event.altKey ||
                event.shiftKey ||
                !(event.metaKey || event.ctrlKey) ||
                (event.target instanceof Element &&
                    event.target.closest(
                        '[role="dialog"], [role="alertdialog"]',
                    ))
            ) {
                return
            }
            if (event.key.toLowerCase() === "s") {
                event.preventDefault()
                if (draftEditToken && !savePending) {
                    void handleSaveRef.current()
                }
                return
            }
            if (event.key === "Enter") {
                if (
                    isEditing &&
                    permissions.canSubmit &&
                    draftEditToken &&
                    !savePending &&
                    !submitPending
                ) {
                    event.preventDefault()
                    setSubmitConfirmOpen(true)
                }
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [
        draftEditToken,
        isEditing,
        permissions.canSubmit,
        savePending,
        setSubmitConfirmOpen,
        submitPending,
    ])

    if (query.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title="采购单"
                description="正在加载详情…"
                summary
                sidebar
            />
        )
    }

    if (query.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="采购单"
                    meta={<span>详情加载失败</span>}
                    navigationMeta={backContext.sourceHint}
                    back={{
                        id: "procurement-orders-detail-error-back",
                        label: backContext.label,
                        href: backContext.href,
                        visibility: backContext.hasReturnTo
                            ? "always"
                            : "standalone-only",
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="max-w-3xl py-6">
                        <BusinessFailureState
                            title="详情加载失败"
                            error={query.error}
                            action={
                                <Button
                                    id="procurement-orders-detail-retry"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    onClick={() => void query.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!order) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="采购单不存在"
                    meta={<span>单据可能已删除或编号有误</span>}
                    navigationMeta={backContext.sourceHint}
                    back={{
                        id: "procurement-orders-detail-missing-back-top",
                        label: backContext.label,
                        href: backContext.href,
                        visibility: backContext.hasReturnTo
                            ? "always"
                            : "standalone-only",
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="max-w-3xl py-6">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到该采购单"
                            description="该采购单可能已删除或不在当前数据范围内。"
                            className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const displayNo =
        order.identity.purchaseNo ??
        order.identity.draftLabel ??
        "采购单（未编号）"
    const costMasked = order.currentContent.costMasked

    const modeLabel = isEditing
        ? order.identity.status === "REJECTED" ||
          order.identity.reviewStatus === "REJECTED"
            ? "被驳回待修改"
            : "采购草稿编辑"
        : "详情"

    const activeChange = order.activeChangeOrder
    const continueChange = Boolean(
        permissions.canContinueChange &&
        activeChange &&
        purchaseChangeOrderApprovalPhase(
            activeChange.approval,
            activeChange.statusCode,
        ) === "draft" &&
        activeChange.approval?.allowedActions.includes("SUBMIT"),
    )
    const changeIsPrimary =
        !isEditing && continueChange && !permissions.canSubmit
    const hasHeaderPrimaryAction = permissions.canSubmit || changeIsPrimary

    /** 发起或继续改单都直接打开改单弹窗（与销售改单一致）。 */
    const openChange = () => {
        if (continueChange && activeChange) {
            setEditingChangeId(activeChange.id)
            return
        }
        void editActions.handleStartChange().then((changeId) => {
            if (changeId) setEditingChangeId(changeId)
        })
    }

    const viewHref = purchaseOrderDetailHref(
        order.identity.purchaseOrderId,
        searchParams,
        { mode: null },
    )
    const editHref = purchaseOrderDetailHref(
        order.identity.purchaseOrderId,
        searchParams,
        { mode: "edit" },
    )
    const changesHref = purchaseOrderDetailHref(
        order.identity.purchaseOrderId,
        searchParams,
        { mode: null, section: "changes" },
    )

    return (
        <DetailWorkspacePage>
            <PurchaseOrderDetailHeader
                order={order}
                mode={isEditing ? "edit" : "view"}
                displayNo={displayNo}
                modeLabel={modeLabel}
                titleRef={titleRef}
                router={router}
                canEdit={permissions.canEdit}
                canSubmit={permissions.canSubmit}
                canVoid={permissions.canVoid}
                canChange={permissions.canChange}
                continueChange={continueChange}
                changePending={editActions.changePending}
                editDirty={guard.editDirty}
                draftEditToken={editActions.draftEditToken}
                savePending={editActions.savePending}
                requestLeave={guard.requestLeave}
                onExitEdit={() =>
                    guard.requestLeave(() => router.replace(viewHref))
                }
                onSaveDraft={() => void editActions.handleSave()}
                onRequestVoid={() => editActions.setVoidConfirmOpen(true)}
                onRequestChange={openChange}
                onRequestSubmit={() => editActions.setSubmitConfirmOpen(true)}
                onCancelResult={handleResult}
                tabs={
                    isEditing ? undefined : (
                        <PurchaseOrderDetailSectionTabs
                            order={order}
                            activeSection={activeSection}
                            onSelectSection={selectSection}
                        />
                    )
                }
            />

            <DetailWorkspaceBody
                summary={
                    isEditing ? undefined : (
                        <PurchaseOrderAmountSummary
                            order={order}
                            costMasked={costMasked}
                        />
                    )
                }
                alerts={
                    <>
                        {result ? (
                            <FormalActionResult
                                status={result.status}
                                title={result.title}
                                description={result.description}
                                reference={result.reference}
                                facts={result.facts}
                                actions={
                                    <Button
                                        id="procurement-orders-detail-result-close"
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        onClick={() => handleResult(null)}
                                    >
                                        关闭
                                    </Button>
                                }
                            />
                        ) : null}

                        <PurchaseOrderFreightNotice order={order} />

                        <PurchaseOrderSourceUpdateNotice
                            order={order}
                            showActions={!isEditing}
                            canEdit={permissions.canEdit}
                            canVoid={permissions.canVoid}
                            canChange={permissions.canChange || continueChange}
                            continueChange={continueChange}
                            onEdit={() => router.push(editHref)}
                            onRequestVoid={() =>
                                editActions.setVoidConfirmOpen(true)
                            }
                            onRequestChange={openChange}
                        />

                        {!isEditing &&
                        isChangeOrderTask &&
                        activeSection !== "changes" ? (
                            <PurchaseChangeOrderApprovalSection
                                readonlyApproval
                                purchaseOrderId={order.identity.purchaseOrderId}
                                changeOrder={order.activeChangeOrder ?? null}
                                baseRevisionNo={order.identity.revisionNo}
                                paidAmount={
                                    order.payableSummary?.paidAllocatedAmount
                                }
                                onResult={handleResult}
                            />
                        ) : null}
                    </>
                }
                sidebar={
                    isEditing ? undefined : (
                        <PurchaseOrderDetailSidebar
                            order={order}
                            costMasked={costMasked}
                            onSelectSection={selectSection}
                        />
                    )
                }
                sidebarLabel={isEditing ? undefined : "采购单摘要"}
            >
                <PurchaseOrderDetailSectionContent
                    order={order}
                    activeSection={activeSection}
                    editor={
                        isEditing ? (
                            <EditSurface
                                order={order}
                                lineEdits={editActions.lineEdits}
                                setLineEdits={editActions.setLineEdits}
                                draftEditToken={editActions.draftEditToken}
                                editLockPending={editActions.editLockPending}
                                editLockError={editActions.editLockError}
                                onRetryEditLock={
                                    editActions.retryAcquireEditToken
                                }
                                savePending={editActions.savePending}
                            />
                        ) : undefined
                    }
                    costMasked={costMasked}
                    hasHeaderPrimaryAction={hasHeaderPrimaryAction}
                    onChangeApprovalResult={handleResult}
                />
            </DetailWorkspaceBody>

            <PurchaseOrderDetailDialogs
                order={order}
                submitConfirmOpen={editActions.submitConfirmOpen}
                onSubmitConfirmOpenChange={editActions.setSubmitConfirmOpen}
                voidConfirmOpen={editActions.voidConfirmOpen}
                onVoidConfirmOpenChange={editActions.setVoidConfirmOpen}
                leaveGuardOpen={guard.leaveGuardOpen}
                onLeaveGuardOpenChange={guard.setLeaveGuardOpen}
                submitPending={editActions.submitPending}
                savePending={editActions.savePending}
                voidPending={editActions.voidPending}
                onConfirmSubmit={(reason) =>
                    void editActions.handleSubmit(reason)
                }
                onConfirmVoid={() => void editActions.handleVoid()}
                onSaveAndLeave={() => void guard.saveAndLeave()}
                onDiscardAndLeave={guard.discardAndLeave}
            />

            {editingChangeId ? (
                <PurchaseChangeOrderEditDialog
                    open
                    purchaseOrderId={order.identity.purchaseOrderId}
                    changeOrderId={editingChangeId}
                    approval={
                        activeChange?.id === editingChangeId
                            ? activeChange.approval
                            : undefined
                    }
                    paidAmount={order.payableSummary?.paidAllocatedAmount}
                    onOpenChange={(open) => {
                        if (!open) setEditingChangeId(null)
                    }}
                    onResult={(next) => {
                        handleResult(next)
                        if (next.status === "succeeded")
                            router.replace(changesHref)
                    }}
                />
            ) : null}
        </DetailWorkspacePage>
    )
}
