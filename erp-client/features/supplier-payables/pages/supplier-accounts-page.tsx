"use client"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { AllocationSession } from "@/features/supplier-payables/components/allocation-session"
import {
    usePayableDetailQuery,
    useReverseInvoiceMutation,
    useReversePaymentMutation,
} from "@/features/supplier-payables/hooks/queries"
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
        openPreview,
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

    const detailQuery = usePayableDetailQuery(previewPayableId)
    const reversePayment = useReversePaymentMutation()
    const reverseInvoice = useReverseInvoiceMutation()

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
                openSession={openSession}
                setReverseTarget={setReverseTarget}
                setRedInvoiceNo={setRedInvoiceNo}
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
                detailQuery={detailQuery}
                returnTo={returnTo}
                fromWorkspace={fromWorkspace}
                onClose={closePreview}
                onOpenSession={openSession}
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

            {reverseTarget ? (
                <ReverseDialog
                    target={reverseTarget}
                    reason={reverseReason}
                    onReasonChange={setReverseReason}
                    redInvoiceNo={redInvoiceNo}
                    onRedInvoiceNoChange={setRedInvoiceNo}
                    submitting={
                        reversePayment.isPending || reverseInvoice.isPending
                    }
                    onCancel={() => setReverseTarget(null)}
                    onSubmit={() => {
                        void (async () => {
                            const key = `w12_rev_${reverseTarget.kind}_${reverseTarget.id}_${Date.now()}`
                            let res: FormalSubmitResult
                            if (reverseTarget.kind === "payment") {
                                res = await reversePayment.mutateAsync({
                                    paymentId: reverseTarget.id,
                                    reason: reverseReason,
                                    idempotencyKey: key,
                                })
                            } else {
                                res = await reverseInvoice.mutateAsync({
                                    invoiceId: reverseTarget.id,
                                    reason: reverseReason,
                                    redInvoiceNo: redInvoiceNo.trim(),
                                    idempotencyKey: key,
                                })
                            }
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
