"use client"

import { ShieldAlertIcon } from "lucide-react"

import { FormalActionConfirmDialog } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { AllocationFactFormCard } from "@/features/supplier-payables/components/allocation-fact-form-card"
import { AllocationPoolCard } from "@/features/supplier-payables/components/allocation-pool-card"
import { AllocationResultView } from "@/features/supplier-payables/components/allocation-result-view"
import { SupplierPaymentApprovalArea } from "@/features/supplier-payables/components/supplier-payment-approval-area"
import { SupplierPaymentSubmitConfirmDialog } from "@/features/supplier-payables/components/supplier-payment-submit-confirm-dialog"
import type { AllocationSessionState } from "@/features/supplier-payables/hooks/use-allocation-session"
import { supplierPaymentApprovalPhase } from "@/features/supplier-payables/lib/supplier-payment-approval"
import type { AllocationTrack } from "@/features/supplier-payables/types"
import { workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"
import { cn } from "@/lib/utils"

export type SupplierAllocationWorkspaceProps = {
    state: AllocationSessionState
    track: AllocationTrack
    fromWorkspace?: string
    purchaseOrderId?: string
    returnTo?: string
    embedded?: boolean
    onClose: () => void
    onGoToInvoiceView?: () => void
}

/**
 * 付款/进项票核销工作面。W12 作业页与 W01 工作台共用同一套池、记录表和提交确认。
 */
export function SupplierAllocationWorkspace({
    state,
    track,
    fromWorkspace,
    purchaseOrderId,
    returnTo,
    embedded = false,
    onClose,
    onGoToInvoiceView,
}: SupplierAllocationWorkspaceProps) {
    const {
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
    } = state

    if (!session) return null

    return (
        <div className={cn("flex min-w-0 flex-col gap-4", embedded && "pb-4")}>
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

            {!embedded && (fromWorkspace || purchaseOrderId) ? (
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

            {embedded && !result ? (
                <div className="flex flex-wrap items-center justify-end gap-2">
                    {draftHint ? (
                        <p className="text-xs text-muted-foreground">
                            {draftHint}（不形成业务记录）
                        </p>
                    ) : null}
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={isSavingDraft}
                        onClick={() => void handleSaveDraft()}
                    >
                        {isSavingDraft ? "保存中…" : "保存草稿"}
                    </Button>
                </div>
            ) : null}

            {result ? (
                <AllocationResultView
                    result={result}
                    returnTo={embedded ? undefined : returnTo}
                    hasSubmitKey={hasSubmitKey}
                    closeLabel={embedded ? "继续处理" : "回到列表"}
                    onClose={onClose}
                    onGoToInvoiceView={embedded ? undefined : onGoToInvoiceView}
                    onResolveUnknown={handleResolveUnknown}
                />
            ) : (
                <div
                    className={cn(
                        "grid items-start gap-4",
                        embedded ? "grid-cols-1" : "lg:grid-cols-2",
                    )}
                >
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
        </div>
    )
}
