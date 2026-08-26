"use client"

import { ArrowLeftIcon, SaveIcon, ShieldAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    FormalActionConfirmDialog,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { AllocationFactFormCard } from "@/features/supplier-payables/components/allocation-fact-form-card"
import { AllocationPoolCard } from "@/features/supplier-payables/components/allocation-pool-card"
import { AllocationResultView } from "@/features/supplier-payables/components/allocation-result-view"
import { SupplierPaymentApprovalArea } from "@/features/supplier-payables/components/supplier-payment-approval-area"
import { SupplierPaymentSubmitConfirmDialog } from "@/features/supplier-payables/components/supplier-payment-submit-confirm-dialog"
import { useAllocationSession } from "@/features/supplier-payables/hooks/use-allocation-session"
import { supplierPaymentApprovalPhase } from "@/features/supplier-payables/lib/supplier-payment-approval"
import type {
    AllocationTrack,
    FormalSubmitResult,
} from "@/features/supplier-payables/types"
import { workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"

export type SupplierAllocationSessionPageProps = {
    track: AllocationTrack
    supplierId: string
    draftSessionId?: string
    purchaseOrderId?: string
    returnTo?: string
    fromWorkspace?: string
    existingPaymentId?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
    paymentWorkItemId?: string
    expectedPaymentTaskVersion?: string
    paymentPayableAccountId?: string
    paymentTaskPending?: boolean
    onClose: () => void
    onCompleted?: (result: FormalSubmitResult) => void
    onGoToInvoiceView?: () => void
    onDraftSessionIdChange?: (draftSessionId: string) => void
}

/**
 * 供应商付款/进项票核销作业页。与往来列表共用路由，但是独立页面骨架：
 * 页头、浮起工作面、提交结果均按作业页渲染，而不是列表内嵌组件。
 */
export function SupplierAllocationSessionPage({
    track,
    supplierId,
    draftSessionId,
    purchaseOrderId,
    returnTo,
    fromWorkspace,
    existingPaymentId,
    existingInvoiceId,
    preselectPayableAccountId,
    paymentWorkItemId,
    expectedPaymentTaskVersion,
    paymentPayableAccountId,
    paymentTaskPending = false,
    onClose,
    onCompleted,
    onGoToInvoiceView,
    onDraftSessionIdChange,
}: SupplierAllocationSessionPageProps) {
    const {
        sessionQuery,
        session,
        policy,
        pool,
        selected,
        amounts,
        confirmOpen,
        setConfirmOpen,
        result,
        draftHint,
        paymentApproval,
        paymentForm,
        invoiceForm,
        factAmount,
        allocatedHint,
        unallocatedHint,
        mixedSources,
        policyBlocksAuto,
        issues,
        canSubmit,
        isSubmitting,
        isSavingDraft,
        hasSubmitKey,
        toggleItem,
        setAmountFor,
        toggleSelectAll,
        fillAllSelected,
        handleSaveDraft,
        requestSubmit,
        doSubmit,
        handleResolveUnknown,
    } = useAllocationSession(
        {
            track,
            supplierId,
            draftSessionId,
            purchaseOrderId,
            returnTo,
            fromWorkspace,
            existingPaymentId,
            existingInvoiceId,
            preselectPayableAccountId,
            paymentWorkItemId,
            expectedPaymentTaskVersion,
            paymentPayableAccountId,
        },
        { onCompleted, onDraftSessionIdChange },
    )

    const title = track === "payment" ? "登记付款" : "登记进项发票"
    const trackLabel = track === "payment" ? "付款核销" : "进项票核销"

    const header = (
        <PageHeader
            title={title}
            status={{ label: trackLabel, tone: "info" }}
            description={
                session
                    ? [
                          `供应商 ${session.supplierName} 已锁定；核销池仅含该供应商开放目标。采购单与结算单可混合，不同供应商禁止混入。`,
                          session.existingDocumentNo
                              ? `继续核销 ${session.existingDocumentNo}。`
                              : null,
                          draftHint ? `${draftHint}（不形成业务记录）` : null,
                      ]
                          .filter(Boolean)
                          .join(" ")
                    : sessionQuery.isError
                      ? "本次核销未能加载。"
                      : "正在加载本次核销…"
            }
            metadata={
                session ? (
                    <DataFreshness
                        updatedAt={
                            sessionQuery.isError
                                ? "查询失败"
                                : session.queriedAt.slice(11, 16)
                        }
                        dateTime={session.queriedAt}
                        state={
                            sessionQuery.isError
                                ? "failed"
                                : sessionQuery.isFetching
                                  ? "syncing"
                                  : "fresh"
                        }
                    />
                ) : null
            }
            actions={
                <PageActions
                    actions={[
                        {
                            actionKey: "back",
                            label: "返回列表",
                            icon: ArrowLeftIcon,
                            variant: "outline",
                            onClick: onClose,
                        },
                        {
                            actionKey: "save-draft",
                            label: "保存草稿",
                            icon: SaveIcon,
                            variant: "outline",
                            disabled:
                                !session || isSavingDraft || Boolean(result),
                            onClick: () => void handleSaveDraft(),
                        },
                    ]}
                />
            }
        />
    )

    if (sessionQuery.isPending) {
        return (
            <PageScaffold density="compact">
                {header}
                <div className="grid gap-4 lg:grid-cols-2">
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (sessionQuery.isError) {
        return (
            <PageScaffold density="compact">
                {header}
                <BusinessFailureState
                    title="无法开始本次核销"
                    error={sessionQuery.error}
                    onRetry={() => void sessionQuery.refetch()}
                    action={
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!session) {
        return (
            <PageScaffold density="compact">
                {header}
                <BusinessEmptyState
                    kind="no-data"
                    title="没有可核销内容"
                    description="请返回列表重新选择供应商往来。"
                    action={
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (track === "payment" && paymentTaskPending) {
        return (
            <PageScaffold density="compact">
                {header}
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (
        track === "payment" &&
        (!paymentWorkItemId ||
            !expectedPaymentTaskVersion ||
            !paymentPayableAccountId)
    ) {
        return (
            <PageScaffold density="compact">
                {header}
                <BusinessEmptyState
                    kind="no-scope"
                    title="请从付款任务进入"
                    description="付款执行已按负责人分派。请回到工作台打开分配给你的供应商付款任务；普通列表入口只能查看，不能提交付款。"
                    action={
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact">
            {header}

            {policy && policy.state !== "AVAILABLE" ? (
                <Alert>
                    <ShieldAlertIcon />
                    <AlertTitle>应付优先级策略不可用</AlertTitle>
                    <AlertDescription>
                        {policy.blockerMessage}
                        混合自动分配已禁用；请显式勾选目标并填写金额。
                    </AlertDescription>
                </Alert>
            ) : null}

            {fromWorkspace || purchaseOrderId ? (
                <Alert variant="info">
                    <AlertTitle>来源上下文</AlertTitle>
                    <AlertDescription>
                        {fromWorkspace
                            ? `来自 ${workspaceLabel(fromWorkspace as WorkspaceId)}`
                            : null}
                        {purchaseOrderId
                            ? ` · 采购单 ${purchaseOrderId}`
                            : null}
                        。完成后请返回来源页，将重新校验先款条件；未核销付款不满足先款要求。
                    </AlertDescription>
                </Alert>
            ) : null}

            {track === "payment" && paymentApproval ? (
                <SupplierPaymentApprovalArea
                    phase={supplierPaymentApprovalPhase(
                        paymentApproval,
                        result?.status === "succeeded"
                            ? "IN_APPROVAL"
                            : "DRAFT",
                    )}
                    approval={paymentApproval}
                    documentId={session.existingPaymentId}
                />
            ) : null}

            {result ? (
                <AllocationResultView
                    result={result}
                    returnTo={returnTo}
                    hasSubmitKey={hasSubmitKey}
                    onClose={onClose}
                    onGoToInvoiceView={onGoToInvoiceView}
                    onResolveUnknown={handleResolveUnknown}
                />
            ) : (
                <div className="grid items-start gap-4 lg:grid-cols-2">
                    <AllocationPoolCard
                        supplierName={session.supplierName}
                        pool={pool}
                        track={track}
                        selected={selected}
                        amounts={amounts}
                        disabled={Boolean(result)}
                        onToggleItem={toggleItem}
                        onAmountChange={setAmountFor}
                        onToggleSelectAll={toggleSelectAll}
                        onFillAllSelected={fillAllSelected}
                    />
                    <AllocationFactFormCard
                        track={track}
                        existingPaymentId={session.existingPaymentId}
                        existingInvoiceId={session.existingInvoiceId}
                        existingDocumentNo={session.existingDocumentNo}
                        existingUnallocated={session.existingUnallocated}
                        paymentForm={paymentForm}
                        invoiceForm={invoiceForm}
                        factAmount={factAmount}
                        allocatedHint={allocatedHint}
                        unallocatedHint={unallocatedHint}
                        mixedSources={mixedSources}
                        policyBlocksAuto={policyBlocksAuto}
                        issues={issues}
                        canSubmit={canSubmit}
                        isSubmitting={isSubmitting}
                        onSubmitClick={requestSubmit}
                    />
                </div>
            )}

            {track === "payment" ? (
                <SupplierPaymentSubmitConfirmDialog
                    open={confirmOpen}
                    pending={isSubmitting}
                    approval={paymentApproval}
                    onOpenChange={setConfirmOpen}
                    onConfirm={() => void doSubmit()}
                />
            ) : (
                <FormalActionConfirmDialog
                    open={confirmOpen}
                    onOpenChange={setConfirmOpen}
                    actionLabel="登记进项发票并核销"
                    title="确认登记进项发票并核销"
                    description="提交后形成不可编辑记录；纠错须追加红票。提交时系统将校验供应商、余额与混合来源规则。"
                    confirmLabel="确认提交"
                    fromStatus={{ label: "本次草稿", tone: "neutral" }}
                    toStatus={{ label: "已确认", tone: "success" }}
                    lockedFields={[
                        `供应商 ${session.supplierName}`,
                        `目标 ${selected.size} 笔`,
                        `拟分配 ${allocatedHint}`,
                    ]}
                    effects={[
                        "形成进项发票与有效分配",
                        "同步更新应付开放余额",
                        "未分配余额保留在待核销视图",
                        "来源页须重新校验先款条件，未核销付款不满足",
                    ]}
                    irreversibleEffects={[
                        "已确认记录不可编辑删除，纠错追加反向记录",
                    ]}
                    pending={isSubmitting}
                    onConfirm={() => void doSubmit()}
                />
            )}
        </PageScaffold>
    )
}
