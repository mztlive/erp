"use client"

import { useEffect, useState } from "react"

import { Alert, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"

import { getErrorMessage, getErrorPresentation } from "@/lib/api/errors"
import { FinancialDraftEditDialog } from "@/features/financial-draft-edit/components/financial-draft-edit-dialog"
import {
    BusinessEmptyState,
    BusinessFailureState,
    PageScaffold,
} from "@/components/business"
import {
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { PaymentReversalRequestDialog } from "@/features/supplier-payables/components/payment-reversal-request-dialog"
import { PaymentReversalSubmitConfirmDialog } from "@/features/supplier-payables/components/payment-reversal-submit-confirm-dialog"
import { SupplierRefundRequestDialog } from "@/features/supplier-payables/components/supplier-refund-request-dialog"
import { SupplierRefundSubmitConfirmDialog } from "@/features/supplier-payables/components/supplier-refund-submit-confirm-dialog"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import {
    usePayableDetailQuery,
    usePaymentReversalQuery,
    useReverseInvoiceMutation,
    useSupplierPaymentQuery,
    useSupplierInvoiceQuery,
    useSupplierRefundQuery,
} from "@/features/supplier-payables/hooks/queries"
import { isPaymentReversalWorkItem } from "@/features/supplier-payables/lib/payment-reversal-approval"
import { isSupplierRefundWorkItem } from "@/features/supplier-payables/lib/supplier-refund-approval"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import type { FormalSubmitResult } from "@/features/supplier-payables/types"
import { useSupplierAccountsPage } from "./hooks/use-supplier-accounts-page"
import { SupplierAllocationSessionPage } from "./supplier-allocation-session-page"
import { SupplierAccountsResultBanner } from "./components/supplier-accounts-alerts"
import { SupplierAccountsHeader } from "./components/supplier-accounts-header"
import { SupplierAccountsPreview } from "./components/supplier-accounts-preview"
import { SupplierAccountsTable } from "./components/supplier-accounts-table"
import { SupplierAccountsToolbar } from "./components/supplier-accounts-toolbar"
import { PickSupplierDialog } from "./components/pick-supplier-dialog"
import { ReverseDialog } from "./components/reverse-dialog"
import { usePaymentReversalFlow } from "./hooks/use-payment-reversal-flow"
import { useSupplierRefundFlow } from "./hooks/use-supplier-refund-flow"

export function SupplierAccountsPage() {
    const {
        view,
        supplierId,
        purchaseOrderId,
        fromWorkspace,
        returnTo,
        trackFilter,
        dueFilterUnavailable,
        searchInput,
        setSearchInput,
        searchInputRef,
        panelOpen,
        setPanelOpen,
        appliedChips,
        applyFilters,
        resetMoreFilters,
        cancelMoreFilters,
        hasPendingChanges,
        removeFilter,
        supplierDraft,
        setSupplierDraft,
        sourceTypeDraft,
        setSourceTypeDraft,
        statusDraft,
        setStatusDraft,
        dueDraft,
        setDueDraft,
        paymentGateDraft,
        setPaymentGateDraft,
        trackDraft,
        setTrackDraft,
        pagination,
        handlePaginationChange,
        sorting,
        setSorting,
        previewInvoiceId,
        previewUnallocatedId,
        previewRowId,
        restoreRowFocus,
        openInvoicePreview,
        openUnallocatedPreview,
        previewPayableId,
        previewPaymentId,
        previewRefundId,
        previewReversalId,
        workItemId,
        openPreview,
        openPaymentPreview,
        openRefundPreview,
        openReversalPreview,
        closePreview,
        session,
        openSession,
        closeSession,
        syncSessionId,
        pickSupplierOpen,
        setPickSupplierOpen,
        pickSupplierId,
        setPickSupplierId,
        reverseTarget,
        setReverseTarget,
        reverseReason,
        setReverseReason,
        redInvoiceNo,
        setRedInvoiceNo,
        lastResult,
        setLastResult,
        hasActiveFilters,
        clearFilters,
        switchView,
        listQuery,
        data,
        sortedPayables,
    } = useSupplierAccountsPage()
    const [hasSettledList, setHasSettledList] = useState(false)
    useEffect(() => {
        if (!listQuery.isPending) setHasSettledList(true)
    }, [listQuery.isPending])

    const workItemQuery = useWorkItemDetailQuery(workItemId ?? "")
    const focusedWorkItem = workItemQuery.data
        ? mapWorkItemDto(workItemQuery.data)
        : undefined
    const paymentExecutionTask =
        focusedWorkItem?.workItemType === "SUPPLIER_PAYMENT_EXECUTION" &&
        focusedWorkItem.handlerKey === "supplier_payment_execution" &&
        focusedWorkItem.businessObjectType === "payable_account" &&
        focusedWorkItem.status === "OPEN" &&
        focusedWorkItem.allowedActions.includes("PROCESS")
            ? focusedWorkItem
            : undefined
    const paymentTaskPayableQuery = usePayableDetailQuery(
        paymentExecutionTask?.businessObjectId ?? null,
    )
    const paymentTaskPayable = paymentTaskPayableQuery.data?.payable
    const workItemRefundId = isSupplierRefundWorkItem(focusedWorkItem)
        ? focusedWorkItem?.businessObjectId
        : undefined
    const workItemReversalId = isPaymentReversalWorkItem(focusedWorkItem)
        ? focusedWorkItem?.businessObjectId
        : undefined
    const focusedPaymentId = previewPaymentId ?? null
    const focusedRefundId = previewRefundId ?? workItemRefundId ?? null
    const focusedReversalId = previewReversalId ?? workItemReversalId ?? null
    const detailQuery = usePayableDetailQuery(previewPayableId)
    const invoiceQuery = useSupplierInvoiceQuery(previewInvoiceId)
    const paymentQuery = useSupplierPaymentQuery(focusedPaymentId)
    const refundQuery = useSupplierRefundQuery(focusedRefundId)
    const reversalQuery = usePaymentReversalQuery(focusedReversalId)
    const reverseInvoice = useReverseInvoiceMutation()
    const refundFlow = useSupplierRefundFlow({
        openRefundPreview,
        setLastResult,
        setActionError: (message) => {
            if (message) {
                setLastResult({
                    status: "failed",
                    title: "退款失败",
                    description: message,
                })
            }
        },
    })
    const reversalFlow = usePaymentReversalFlow({
        openReversalPreview,
        setLastResult,
        setActionError: (message) => {
            if (message) {
                setLastResult({
                    status: "failed",
                    title: "冲正失败",
                    description: message,
                })
            }
        },
    })

    function closeAllocationSession() {
        const refreshPaymentContext =
            session?.track === "payment" && lastResult?.status === "succeeded"
        closeSession()
        if (refreshPaymentContext) {
            void Promise.all([listQuery.refetch(), workItemQuery.refetch()])
        }
    }

    if (session) {
        return (
            <SupplierAllocationSessionPage
                {...session}
                paymentWorkItemId={paymentExecutionTask?.workItemId}
                expectedPaymentTaskVersion={paymentExecutionTask?.taskVersion}
                paymentPayableAccountId={paymentExecutionTask?.businessObjectId}
                paymentRecipient={paymentTaskPayable?.paymentRecipient}
                paymentTaskPending={
                    Boolean(workItemId) &&
                    (workItemQuery.isPending ||
                        paymentTaskPayableQuery.isPending)
                }
                onClose={closeAllocationSession}
                onDraftSessionIdChange={syncSessionId}
                onGoToInvoiceView={() => {
                    closeAllocationSession()
                    switchView("purchase_invoice")
                }}
                onCompleted={(result) => {
                    setLastResult(result)
                }}
            />
        )
    }

    const noModule =
        data &&
        (!data.moduleAllowed || data.emptyReason === "PERMISSION_REVOKED")
    const noScope =
        data &&
        (noModule || !data.hasDataScope || data.emptyReason === "NO_DATA_SCOPE")
    const initialLoading = !hasSettledList && listQuery.isPending
    const failed = listQuery.isError || (!listQuery.isPending && !data)
    const accessDenied =
        listQuery.isError &&
        getErrorPresentation(listQuery.error).kind === "permission"
    const showControls = !noScope && !accessDenied
    const canRegisterInvoice = showControls && Boolean(data?.canRegisterInvoice)
    const canRegisterPayment = showControls && Boolean(data?.canRegisterPayment)
    const onRefresh = () => void listQuery.refetch()

    const filterToolbar = showControls ? (
        <SupplierAccountsToolbar
            view={view}
            searchInput={searchInput}
            onSearchInputChange={setSearchInput}
            searchInputRef={searchInputRef}
            panelOpen={panelOpen}
            setPanelOpen={setPanelOpen}
            appliedChips={appliedChips}
            applyFilters={applyFilters}
            resetMoreFilters={resetMoreFilters}
            cancelMoreFilters={cancelMoreFilters}
            clearAllFilters={clearFilters}
            removeFilter={removeFilter}
            supplierDraft={supplierDraft}
            setSupplierDraft={setSupplierDraft}
            sourceTypeDraft={sourceTypeDraft}
            setSourceTypeDraft={setSourceTypeDraft}
            statusDraft={statusDraft}
            setStatusDraft={setStatusDraft}
            dueDraft={dueDraft}
            setDueDraft={setDueDraft}
            paymentGateDraft={paymentGateDraft}
            setPaymentGateDraft={setPaymentGateDraft}
            trackDraft={trackDraft}
            setTrackDraft={setTrackDraft}
            hasPendingChanges={hasPendingChanges}
            loading={listQuery.isFetching}
            failed={failed}
        />
    ) : null

    return (
        <PageScaffold density="compact" className={listWorkspaceStyles.page}>
            <FinancialDraftEditDialog side="supplier" />
            <SupplierAccountsHeader
                view={view}
                showViews={!initialLoading && showControls}
                prioritizeQuery={showControls && panelOpen}
                canRegisterInvoice={canRegisterInvoice}
                queriedAt={
                    !failed && showControls ? data?.queriedAt : undefined
                }
                onViewChange={switchView}
                onRefresh={onRefresh}
                onRegisterInvoice={() => {
                    setPickSupplierId(
                        supplierId ?? data?.suppliers[0]?.supplierId ?? "",
                    )
                    setPickSupplierOpen("purchase_invoice")
                }}
            />

            {!failed && !noScope && dueFilterUnavailable ? (
                <Alert variant="warning">
                    <AlertTitle>
                        当前列表未提供到期日，查询结果未按到期情况筛选。
                    </AlertTitle>
                </Alert>
            ) : null}

            <SupplierAccountsResultBanner
                lastResult={lastResult}
                onDismiss={() => setLastResult(null)}
            />

            {refundFlow.refundSubmitUnknown ? (
                <Button
                    id="supplier-refund-resume-submit"
                    variant="outline"
                    onClick={() => refundFlow.setRefundSubmitOpen(true)}
                >
                    继续核对退款提交结果
                </Button>
            ) : null}
            {reversalFlow.reversalSubmitUnknown ? (
                <Button
                    id="supplier-reversal-resume-submit"
                    variant="outline"
                    onClick={() => reversalFlow.setReversalSubmitOpen(true)}
                >
                    继续核对冲正提交结果
                </Button>
            ) : null}

            <SupplierAccountsPreview
                previewInvoiceId={previewInvoiceId}
                previewUnallocatedId={previewUnallocatedId}
                listData={showControls ? data : undefined}
                listLoading={listQuery.isPending}
                listError={
                    listQuery.isError
                        ? getErrorMessage(
                              listQuery.error,
                              "往来记录加载失败，请重试。",
                          )
                        : undefined
                }
                onRetryList={() => void listQuery.refetch()}
                onOpenReversal={openReversalPreview}
                onReverse={setReverseTarget}
                onRedInvoiceNo={setRedInvoiceNo}
                onRefund={refundFlow.setRefundRequest}
                onClosed={restoreRowFocus}
                canRegisterPayment={canRegisterPayment}
                previewPayableId={previewPayableId}
                previewPaymentId={focusedPaymentId}
                previewRefundId={focusedRefundId}
                previewReversalId={focusedReversalId}
                detailQuery={detailQuery}
                paymentQuery={paymentQuery}
                invoiceQuery={invoiceQuery}
                refundQuery={refundQuery}
                reversalQuery={reversalQuery}
                onRequestRefundSubmit={() => {
                    const refund = refundQuery.data
                    if (refund) refundFlow.beginRefundSubmit(refund)
                }}
                onRequestReversalSubmit={() => {
                    const reversal = reversalQuery.data
                    if (reversal) reversalFlow.beginReversalSubmit(reversal)
                }}
                returnTo={returnTo}
                fromWorkspace={fromWorkspace}
                paymentTaskPayableAccountId={
                    paymentExecutionTask?.businessObjectId
                }
                canRegisterInvoice={canRegisterInvoice}
                onClose={closePreview}
                onOpenPayable={openPreview}
                onOpenSession={openSession}
                workItemId={focusedWorkItem?.workItemId}
                expectedTaskVersion={focusedWorkItem?.taskVersion}
                workItemAllowedActions={focusedWorkItem?.allowedActions}
                onDecisionApplied={(view: ApprovalCommandView) => {
                    void paymentQuery.refetch()
                    void refundQuery.refetch()
                    void reversalQuery.refetch()
                    void listQuery.refetch()
                    setLastResult({
                        status: "succeeded",
                        title: "审批决定已提交",
                        description: view.latestRejectionReason
                            ? `已按当前任务提交决定。${view.latestRejectionReason}`
                            : "已按当前任务提交决定。",
                        reference:
                            reversalQuery.data?.reversalNo ??
                            refundQuery.data?.refundNo ??
                            paymentQuery.data?.paymentNo ??
                            undefined,
                        facts: view.currentAssigneeName
                            ? [
                                  {
                                      label: "当前审批人",
                                      value: view.currentAssigneeName,
                                  },
                              ]
                            : undefined,
                    })
                }}
            />

            <PickSupplierDialog
                track={canRegisterInvoice ? pickSupplierOpen : null}
                supplierId={pickSupplierId}
                onSupplierIdChange={(id) => setPickSupplierId(id)}
                onClose={() => setPickSupplierOpen(null)}
                onConfirm={() => {
                    if (!pickSupplierOpen || !pickSupplierId) return
                    setPickSupplierOpen(null)
                    openSession({
                        track: pickSupplierOpen,
                        supplierId: pickSupplierId,
                        returnTo,
                        fromWorkspace,
                        purchaseOrderId,
                    })
                }}
            />

            <SupplierRefundRequestDialog
                open={Boolean(refundFlow.refundRequest)}
                pending={refundFlow.refundSubmitPending}
                sourceLabel={[
                    refundFlow.refundRequest?.sourcePaymentNo,
                    refundFlow.refundRequest?.supplierName,
                ]
                    .filter(Boolean)
                    .join(" · ")}
                amount={refundFlow.refundRequest?.amount}
                onOpenChange={(open) => {
                    if (!open) refundFlow.setRefundRequest(null)
                }}
                onSubmit={(reason, amount) =>
                    refundFlow.prepareRefundDraft(reason, amount)
                }
            />

            <SupplierRefundSubmitConfirmDialog
                open={refundFlow.refundSubmitOpen}
                unknown={refundFlow.refundSubmitUnknown}
                frozenReason={refundFlow.refundSubmitReason}
                documentId={refundFlow.refundDraft?.refundId}
                expectedVersion={refundFlow.refundDraft?.baselineVersion}
                onEndRecovery={refundFlow.endRefundRecovery}
                pending={refundFlow.refundSubmitPending}
                approval={
                    refundFlow.refundDraft?.approval ??
                    refundQuery.data?.approval
                }
                onOpenChange={refundFlow.setRefundSubmitOpen}
                onConfirm={(reason) => refundFlow.confirmRefundSubmit(reason)}
            />

            <PaymentReversalRequestDialog
                open={
                    Boolean(reversalFlow.reversalRequest) ||
                    reverseTarget?.kind === "payment"
                }
                pending={reversalFlow.reversalSubmitPending}
                sourceLabel={
                    [
                        reversalFlow.reversalRequest?.sourcePaymentNo,
                        reversalFlow.reversalRequest?.supplierName,
                    ]
                        .filter(Boolean)
                        .join(" · ") ||
                    (reverseTarget?.kind === "payment"
                        ? [reverseTarget.no, reverseTarget.supplierName]
                              .filter(Boolean)
                              .join(" · ")
                        : undefined)
                }
                amount={
                    reversalFlow.reversalRequest?.amount ??
                    (reverseTarget?.kind === "payment"
                        ? reverseTarget.amount
                        : undefined)
                }
                onOpenChange={(open) => {
                    if (!open) {
                        reversalFlow.setReversalRequest(null)
                        if (reverseTarget?.kind === "payment") {
                            setReverseTarget(null)
                        }
                    }
                }}
                onSubmit={async (reason) => {
                    const request =
                        reverseTarget?.kind === "payment"
                            ? {
                                  sourcePaymentId: reverseTarget.id,
                                  sourcePaymentNo: reverseTarget.no,
                                  amount: reverseTarget.amount,
                                  supplierName: reverseTarget.supplierName,
                              }
                            : reversalFlow.reversalRequest
                    await reversalFlow.prepareReversalDraft(reason, request)
                    if (reverseTarget?.kind === "payment")
                        setReverseTarget(null)
                }}
            />

            <PaymentReversalSubmitConfirmDialog
                open={reversalFlow.reversalSubmitOpen}
                unknown={reversalFlow.reversalSubmitUnknown}
                frozenReason={reversalFlow.reversalSubmitReason}
                documentId={reversalFlow.reversalDraft?.reversalId}
                expectedVersion={reversalFlow.reversalDraft?.baselineVersion}
                onEndRecovery={reversalFlow.endReversalRecovery}
                pending={reversalFlow.reversalSubmitPending}
                approval={
                    reversalFlow.reversalDraft?.approval ??
                    reversalQuery.data?.approval
                }
                onOpenChange={reversalFlow.setReversalSubmitOpen}
                onConfirm={(reason) =>
                    reversalFlow.confirmReversalSubmit(reason)
                }
            />

            {reverseTarget && reverseTarget.kind === "invoice" ? (
                <ReverseDialog
                    target={reverseTarget}
                    reason={reverseReason}
                    onReasonChange={setReverseReason}
                    redInvoiceNo={redInvoiceNo}
                    onRedInvoiceNoChange={setRedInvoiceNo}
                    submitting={reverseInvoice.isPending}
                    onCancel={() => setReverseTarget(null)}
                    onSubmit={() => {
                        void (async () => {
                            const key = `w12_rev_${reverseTarget.kind}_${reverseTarget.id}_${Date.now()}`
                            const res: FormalSubmitResult =
                                await reverseInvoice.mutateAsync({
                                    invoiceId: reverseTarget.id,
                                    reason: reverseReason,
                                    redInvoiceNo: redInvoiceNo.trim(),
                                    idempotencyKey: key,
                                })
                            setLastResult(res)
                            setReverseTarget(null)
                            setReverseReason("")
                            setRedInvoiceNo("")
                        })()
                    }}
                />
            ) : null}

            {initialLoading ? (
                <div
                    className="flex flex-col gap-2"
                    role="status"
                    aria-busy="true"
                >
                    <span className="sr-only">正在加载供应商往来…</span>
                    <Skeleton className="h-10" />
                    <Skeleton className="h-96" />
                </div>
            ) : accessDenied ? (
                <BusinessFailureState
                    error={listQuery.error}
                    className={listWorkspaceEmptyStateClassName}
                />
            ) : !failed && noScope ? (
                <BusinessEmptyState
                    kind="no-scope"
                    className={listWorkspaceEmptyStateClassName}
                    title={
                        noModule
                            ? "无供应商往来权限"
                            : "当前角色未配置供应商往来范围"
                    }
                    description={
                        noModule
                            ? "权限已收回或未授权。敏感字段与导出结果已清除，不能提交。"
                            : "不能显示为 0 元应付。请联系管理员配置组织/供应商范围后再查询。"
                    }
                />
            ) : (
                <SupplierAccountsTable
                    view={view}
                    data={data}
                    sortedPayables={sortedPayables}
                    track={trackFilter}
                    loading={listQuery.isFetching}
                    isError={failed}
                    error={listQuery.error}
                    onRetry={onRefresh}
                    pagination={pagination}
                    onPaginationChange={handlePaginationChange}
                    sorting={sorting}
                    onSortingChange={setSorting}
                    hasActiveFilters={hasActiveFilters}
                    onClearFilters={clearFilters}
                    previewRowId={
                        view === "payment" && previewReversalId
                            ? (reversalQuery.data?.originalPaymentId ?? null)
                            : previewRowId
                    }
                    openPreview={openPreview}
                    openPaymentPreview={openPaymentPreview}
                    openInvoicePreview={openInvoicePreview}
                    openUnallocatedPreview={openUnallocatedPreview}
                    openReversalPreview={openReversalPreview}
                    toolbar={filterToolbar}
                />
            )}
        </PageScaffold>
    )
}
