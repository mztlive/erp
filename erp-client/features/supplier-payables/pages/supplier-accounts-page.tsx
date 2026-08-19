"use client"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { AllocationSession } from "@/features/supplier-payables/components/allocation-session"
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
import {
    VIEW_LABEL,
    type FormalSubmitResult,
} from "@/features/supplier-payables/types"
import { useSupplierAccountsPage } from "./hooks/use-supplier-accounts-page"
import {
    SupplierAccountsAlerts,
    SupplierAccountsResultBanner,
} from "./components/supplier-accounts-alerts"
import { SupplierAccountsHeader } from "./components/supplier-accounts-header"
import { SupplierAccountsMetrics } from "./components/supplier-accounts-metrics"
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
        sourceType,
        status,
        due,
        paymentGate,
        purchaseOrderId,
        fromWorkspace,
        returnTo,
        trackFilter,
        searchInput,
        setSearchInput,
        searchInputRef,
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
            <PageScaffold>
                <AllocationSession
                    {...session}
                    onClose={closeSession}
                    onGoToInvoiceView={() => {
                        closeSession()
                        patchUrl({ view: "purchase_invoice" })
                    }}
                    onCompleted={(result) => {
                        setLastResult(result)
                    }}
                />
            </PageScaffold>
        )
    }

    if (listQuery.isPending && !data) {
        return (
            <PageScaffold density="compact">
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
                    {Array.from({ length: 5 }).map((_, i) => (
                        <div
                            key={i}
                            className="h-20 animate-pulse rounded-lg bg-muted"
                        />
                    ))}
                </div>
                <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (listQuery.isError && !data) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="供应商往来加载失败"
                    error={listQuery.error}
                    action={
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => void listQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!data) return null

    if (!data.moduleAllowed) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-scope"
                    title="无供应商往来权限"
                    description="权限已收回或未授权。敏感字段与导出结果已清除，不能提交。"
                />
            </PageScaffold>
        )
    }

    if (!data.hasDataScope) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色未配置供应商往来范围"
                    description="不能显示为 0 元应付。请联系管理员配置组织/供应商范围后再查询。"
                />
            </PageScaffold>
        )
    }

    const rows =
        view === "payable"
            ? sortedPayables
            : view === "payment"
              ? data.payments
              : view === "purchase_invoice"
                ? data.invoices
                : trackFilter !== "all"
                  ? data.unallocated.filter((u) => u.track === trackFilter)
                  : data.unallocated

    const pageRows = rows.slice(
        pagination.pageIndex * pagination.pageSize,
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )

    return (
        <PageScaffold density="compact">
            <SupplierAccountsHeader
                data={data}
                onRefresh={() => void listQuery.refetch()}
                onRegisterInvoice={() => {
                    setPickSupplierId(
                        supplierId ?? data.suppliers[0]?.supplierId ?? "",
                    )
                    setPickSupplierOpen("purchase_invoice")
                }}
                onRegisterPayment={() => {
                    setPickSupplierId(
                        supplierId ?? data.suppliers[0]?.supplierId ?? "",
                    )
                    setPickSupplierOpen("payment")
                }}
                onSettle={openSettlements}
            />

            <SupplierAccountsAlerts
                fromWorkspace={fromWorkspace}
                purchaseOrderId={purchaseOrderId}
                returnTo={returnTo}
                policy={data.payablePriorityPolicy}
            />

            <SupplierAccountsResultBanner
                lastResult={lastResult}
                onDismiss={() => setLastResult(null)}
            />

            <SupplierAccountsMetrics
                metrics={data.metrics}
                view={view}
                status={status}
                due={due}
                trackFilter={trackFilter}
                paymentGate={paymentGate}
                onFilter={patchUrl}
            />

            <Tabs
                value={view}
                onValueChange={(v) => {
                    patchUrl({ view: v, page: null })
                }}
            >
                <TabsList>
                    {(
                        [
                            "payable",
                            "payment",
                            "purchase_invoice",
                            "unallocated",
                        ] as const
                    ).map((v) => (
                        <TabsTrigger key={v} value={v}>
                            {VIEW_LABEL[v]}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            <SupplierAccountsTable
                view={view}
                data={data}
                pageRows={pageRows}
                unallocatedRowCount={rows.length}
                pagination={pagination}
                onPaginationChange={handlePaginationChange}
                sorting={sorting}
                onSortingChange={setSorting}
                onClearFilters={clearFilters}
                returnTo={returnTo}
                fromWorkspace={fromWorkspace}
                openPreview={openPreview}
                openPaymentPreview={openPaymentPreview}
                openSession={openSession}
                setReverseTarget={setReverseTarget}
                setRedInvoiceNo={setRedInvoiceNo}
                setRefundRequest={refundFlow.setRefundRequest}
                toolbar={
                    <SupplierAccountsToolbar
                        view={view}
                        trackFilter={trackFilter}
                        supplierId={supplierId}
                        sourceType={sourceType}
                        status={status}
                        searchInput={searchInput}
                        onSearchInputChange={setSearchInput}
                        searchInputRef={searchInputRef}
                        hasActiveFilters={hasActiveFilters}
                        onClearFilters={clearFilters}
                        onFilter={patchUrl}
                    />
                }
            />

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
                onSubmit={(reason) => void refundFlow.prepareRefundDraft(reason)}
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
