"use client"

import { BusinessEmptyState, PageScaffold } from "@/components/business"
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
    useSupplierRefundQuery,
} from "@/features/supplier-payables/hooks/queries"
import { isPaymentReversalWorkItem } from "@/features/supplier-payables/lib/payment-reversal-approval"
import { isSupplierPaymentWorkItem } from "@/features/supplier-payables/lib/supplier-payment-approval"
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
        searchInput,
        setSearchInput,
        searchInputRef,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters,
        appliedChips,
        applyFilters,
        resetMoreFilters,
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
        patchUrl,
        listQuery,
        data,
        sortedPayables,
        openSettlements,
    } = useSupplierAccountsPage()

    const filterToolbar = (
        <SupplierAccountsToolbar
            view={view}
            onViewChange={(nextView) => {
                patchUrl({ view: nextView, page: null })
            }}
            searchInput={searchInput}
            onSearchInputChange={setSearchInput}
            searchInputRef={searchInputRef}
            hasActiveFilters={hasActiveFilters}
            hasStructuredFilters={hasStructuredFilters}
            panelOpen={panelOpen}
            setPanelOpen={setPanelOpen}
            appliedChips={appliedChips}
            applyFilters={applyFilters}
            resetMoreFilters={resetMoreFilters}
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
        />
    )

    const workItemQuery = useWorkItemDetailQuery(workItemId ?? "")
    const focusedWorkItem = workItemQuery.data
        ? mapWorkItemDto(workItemQuery.data)
        : undefined
    const workItemPaymentId = isSupplierPaymentWorkItem(focusedWorkItem)
        ? focusedWorkItem?.businessObjectId
        : undefined
    const workItemRefundId = isSupplierRefundWorkItem(focusedWorkItem)
        ? focusedWorkItem?.businessObjectId
        : undefined
    const workItemReversalId = isPaymentReversalWorkItem(focusedWorkItem)
        ? focusedWorkItem?.businessObjectId
        : undefined
    const focusedPaymentId = previewPaymentId ?? workItemPaymentId ?? null
    const focusedRefundId = previewRefundId ?? workItemRefundId ?? null
    const focusedReversalId = previewReversalId ?? workItemReversalId ?? null
    const detailQuery = usePayableDetailQuery(previewPayableId)
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

    if (session) {
        return (
            <SupplierAllocationSessionPage
                {...session}
                onClose={closeSession}
                onDraftSessionIdChange={syncSessionId}
                onGoToInvoiceView={() => {
                    closeSession()
                    patchUrl({ view: "purchase_invoice" })
                }}
                onCompleted={(result) => {
                    setLastResult(result)
                }}
            />
        )
    }

    const rows = data
        ? view === "payable"
            ? sortedPayables
            : view === "payment"
              ? data.payments
              : view === "purchase_invoice"
                ? data.invoices
                : trackFilter !== "all"
                  ? data.unallocated.filter((u) => u.track === trackFilter)
                  : data.unallocated
        : []

    const pageRows = rows.slice(
        pagination.pageIndex * pagination.pageSize,
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )

    const filterDescription =
        listQuery.isError && !data
            ? "列表加载失败"
            : !data
              ? "正在查询"
              : hasActiveFilters
                ? `当前筛选：${appliedChips.map((chip) => chip.label).join(" · ")}`
                : "搜索供应商、采购单、结算单、付款单或发票号；筛选条件会保存在网址中，便于刷新、返回与分享。"

    return (
        <PageScaffold density="compact">
            <SupplierAccountsHeader
                data={data}
                isError={listQuery.isError}
                isFetching={listQuery.isFetching}
                onRefresh={() => void listQuery.refetch()}
                onRegisterInvoice={() => {
                    setPickSupplierId(
                        supplierId ?? data?.suppliers[0]?.supplierId ?? "",
                    )
                    setPickSupplierOpen("purchase_invoice")
                }}
                onRegisterPayment={() => {
                    setPickSupplierId(
                        supplierId ?? data?.suppliers[0]?.supplierId ?? "",
                    )
                    setPickSupplierOpen("payment")
                }}
                onSettle={openSettlements}
            />

            <SupplierAccountsResultBanner
                lastResult={lastResult}
                onDismiss={() => setLastResult(null)}
            />

            {data && !data.moduleAllowed ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无供应商往来权限"
                    description="权限已收回或未授权。敏感字段与导出结果已清除，不能提交。"
                />
            ) : data && !data.hasDataScope ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色未配置供应商往来范围"
                    description="不能显示为 0 元应付。请联系管理员配置组织/供应商范围后再查询。"
                />
            ) : (
                <>
                    <SupplierAccountsTable
                        view={view}
                        data={data}
                        pageRows={pageRows}
                        rowCount={rows.length}
                        loading={listQuery.isFetching}
                        isError={listQuery.isError}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        pagination={pagination}
                        onPaginationChange={handlePaginationChange}
                        sorting={sorting}
                        onSortingChange={setSorting}
                        filterDescription={filterDescription}
                        onClearFilters={clearFilters}
                        returnTo={returnTo}
                        fromWorkspace={fromWorkspace}
                        openPreview={openPreview}
                        openPaymentPreview={openPaymentPreview}
                        openSession={openSession}
                        setReverseTarget={setReverseTarget}
                        setRedInvoiceNo={setRedInvoiceNo}
                        setRefundRequest={refundFlow.setRefundRequest}
                        toolbar={filterToolbar}
                    />
                </>
            )}

            <SupplierAccountsPreview
                previewPayableId={previewPayableId}
                previewPaymentId={focusedPaymentId}
                previewRefundId={focusedRefundId}
                previewReversalId={focusedReversalId}
                detailQuery={detailQuery}
                paymentQuery={paymentQuery}
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
                onClose={closePreview}
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
                            focusedReversalId ??
                            focusedRefundId ??
                            focusedPaymentId ??
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
                track={pickSupplierOpen}
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
                pending={refundFlow.refundDraftPending}
                sourceLabel={refundFlow.refundRequest?.sourcePaymentNo}
                amount={refundFlow.refundRequest?.amount}
                onOpenChange={(open) => {
                    if (!open) refundFlow.setRefundRequest(null)
                }}
                onSubmit={(reason) =>
                    void refundFlow.prepareRefundDraft(reason)
                }
            />

            <SupplierRefundSubmitConfirmDialog
                open={refundFlow.refundSubmitOpen}
                pending={refundFlow.refundSubmitPending}
                approval={
                    refundFlow.refundDraft?.approval ??
                    refundQuery.data?.approval
                }
                onOpenChange={refundFlow.setRefundSubmitOpen}
                onConfirm={() => void refundFlow.confirmRefundSubmit()}
            />

            <PaymentReversalRequestDialog
                open={
                    Boolean(reversalFlow.reversalRequest) ||
                    reverseTarget?.kind === "payment"
                }
                pending={reversalFlow.reversalDraftPending}
                sourceLabel={
                    reversalFlow.reversalRequest?.sourcePaymentNo ??
                    (reverseTarget?.kind === "payment"
                        ? reverseTarget.no
                        : undefined)
                }
                amount={reversalFlow.reversalRequest?.amount}
                onOpenChange={(open) => {
                    if (!open) {
                        reversalFlow.setReversalRequest(null)
                        if (reverseTarget?.kind === "payment") {
                            setReverseTarget(null)
                        }
                    }
                }}
                onSubmit={(reason) => {
                    const request =
                        reverseTarget?.kind === "payment"
                            ? {
                                  sourcePaymentId: reverseTarget.id,
                                  sourcePaymentNo: reverseTarget.no,
                              }
                            : reversalFlow.reversalRequest
                    if (reverseTarget?.kind === "payment") {
                        setReverseTarget(null)
                    }
                    void reversalFlow.prepareReversalDraft(reason, request)
                }}
            />

            <PaymentReversalSubmitConfirmDialog
                open={reversalFlow.reversalSubmitOpen}
                pending={reversalFlow.reversalSubmitPending}
                approval={
                    reversalFlow.reversalDraft?.approval ??
                    reversalQuery.data?.approval
                }
                onOpenChange={reversalFlow.setReversalSubmitOpen}
                onConfirm={() => void reversalFlow.confirmReversalSubmit()}
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
        </PageScaffold>
    )
}
