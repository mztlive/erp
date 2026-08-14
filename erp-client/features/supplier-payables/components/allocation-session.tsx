"use client"

import { ArrowLeftIcon, SaveIcon, ShieldAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    FormalActionConfirmDialog,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { AllocationFactFormCard } from "@/features/supplier-payables/components/allocation-fact-form-card"
import { AllocationPoolCard } from "@/features/supplier-payables/components/allocation-pool-card"
import { AllocationResultView } from "@/features/supplier-payables/components/allocation-result-view"
import { useAllocationSession } from "@/features/supplier-payables/hooks/use-allocation-session"
import type {
    AllocationTrack,
    FormalSubmitResult,
} from "@/features/supplier-payables/types"
import { workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"

export type AllocationSessionProps = {
    track: AllocationTrack
    supplierId: string
    draftSessionId?: string
    purchaseOrderId?: string
    returnTo?: string
    fromWorkspace?: string
    existingPaymentId?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
    onClose: () => void
    onCompleted?: (result: FormalSubmitResult) => void
    onGoToInvoiceView?: () => void
}

export function AllocationSession({
    track,
    supplierId,
    draftSessionId,
    purchaseOrderId,
    returnTo,
    fromWorkspace,
    existingPaymentId,
    existingInvoiceId,
    preselectPayableAccountId,
    onClose,
    onCompleted,
    onGoToInvoiceView,
}: AllocationSessionProps) {
    const {
        sessionQuery,
        session,
        policy,
        selected,
        amounts,
        confirmOpen,
        setConfirmOpen,
        result,
        draftHint,
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
        },
        { onCompleted },
    )

    if (sessionQuery.isPending) {
        return (
            <div className="space-y-3 p-1">
                <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
                <div className="grid gap-3 md:grid-cols-2">
                    <div className="h-72 animate-pulse rounded-2xl bg-muted" />
                    <div className="h-72 animate-pulse rounded-2xl bg-muted" />
                </div>
            </div>
        )
    }

    if (sessionQuery.isError) {
        return (
            <BusinessFailureState
                title="无法开始本次核销"
                error={sessionQuery.error}
                onRetry={() => void sessionQuery.refetch()}
                action={
                    <Button type="button" variant="outline" onClick={onClose}>
                        返回列表
                    </Button>
                }
            />
        )
    }

    if (!session) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="没有可核销内容"
                description="请返回列表重新选择供应商往来。"
                action={
                    <Button type="button" variant="outline" onClick={onClose}>
                        返回列表
                    </Button>
                }
            />
        )
    }

    return (
        <section className="space-y-4" aria-label="供应商核销工作区">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onClose}
                        >
                            <ArrowLeftIcon className="size-4" />
                            返回列表
                        </Button>
                        <h2 className="text-lg font-semibold tracking-tight">
                            核销 · {session.supplierName}
                        </h2>
                        <span className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                            {track === "payment" ? "付款核销" : "进项票核销"}
                        </span>
                    </div>
                    <p className="text-sm text-muted-foreground">
                        供应商已锁定；核销池仅含该供应商开放目标。采购单与结算单可混合，不同供应商禁止混入。
                        {session.existingDocumentNo
                            ? ` · 继续核销 ${session.existingDocumentNo}`
                            : null}
                    </p>
                    <DataFreshness
                        updatedAt={new Date(session.queriedAt).toLocaleString(
                            "zh-CN",
                        )}
                        dateTime={session.queriedAt}
                        label="更新于"
                        className="text-xs"
                    />
                </div>
                <div className="flex flex-wrap gap-2">
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => void handleSaveDraft()}
                        disabled={isSavingDraft || Boolean(result)}
                    >
                        <SaveIcon className="size-4" />
                        保存草稿
                    </Button>
                </div>
            </div>

            {draftHint ? (
                <p className="text-xs text-muted-foreground">
                    {draftHint}（不形成业务记录）
                </p>
            ) : null}

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
                <Alert>
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

            {result ? (
                <AllocationResultView
                    result={result}
                    returnTo={returnTo}
                    hasSubmitKey={hasSubmitKey}
                    onClose={onClose}
                    onGoToInvoiceView={onGoToInvoiceView}
                    onResolveUnknown={handleResolveUnknown}
                />
            ) : null}

            {!result ? (
                <>
                    <div className="grid gap-4 lg:grid-cols-2">
                        <AllocationPoolCard
                            supplierName={session.supplierName}
                            pool={session.pool}
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
                            onClose={onClose}
                            onSubmitClick={requestSubmit}
                        />
                    </div>
                </>
            ) : null}

            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                actionLabel={
                    track === "payment"
                        ? "登记付款并核销"
                        : "登记进项发票并核销"
                }
                title={
                    track === "payment"
                        ? "确认登记付款并核销"
                        : "确认登记进项发票并核销"
                }
                description="提交后形成不可编辑记录；纠错须追加冲正/红票。提交时系统将校验供应商、余额与混合来源规则。"
                confirmLabel="确认提交"
                fromStatus={{ label: "本次草稿", tone: "neutral" }}
                toStatus={{ label: "已确认", tone: "success" }}
                lockedFields={[
                    `供应商 ${session.supplierName}`,
                    `目标 ${selected.size} 笔`,
                    `拟分配 ${allocatedHint}`,
                ]}
                effects={[
                    track === "payment"
                        ? "形成供应商付款单与有效分配"
                        : "形成进项发票与有效分配",
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
        </section>
    )
}
