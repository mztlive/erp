"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    PageScaffold,
} from "@/components/business"
import { type ResultState } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    createInvoiceColumns,
    createReceivableColumns,
    createReceiptColumns,
} from "@/features/customer-receivables/components/customer-account-columns"
import {
    CustomerAccountDetailPreview,
    type ReverseRequest,
} from "@/features/customer-receivables/components/customer-account-detail-preview"
import { ReceivableActionDialogs } from "@/features/customer-receivables/components/receivable-action-dialogs"
import {
    useAllocationSessionQuery,
    useCreateAllocationSessionMutation,
    useCustomerAccountsDetailQuery,
    useCustomerAccountsListQuery,
} from "@/features/customer-receivables/hooks/queries"
import { buildAccountsCsv, downloadCsv } from "@/features/customer-receivables/lib/export-csv"
import {
    VIEW_LABEL,
    type AllocationMode,
} from "@/features/customer-receivables/types"
import { getErrorMessage } from "@/lib/api/errors"
import { AllocationSessionScreen } from "./components/allocation-session-screen"
import { CustomerReceivablesHeader } from "./components/customer-receivables-header"
import { CustomerReceivablesMetrics } from "./components/customer-receivables-metrics"
import { CustomerReceivablesTable } from "./components/customer-receivables-table"
import { CustomerReceivablesToolbar } from "./components/customer-receivables-toolbar"
import { SalesOrderReturnAlert } from "./components/sales-order-return-alert"
import { useAutoAllocationSession } from "./hooks/use-auto-allocation-session"
import { useCustomerReceivablesPreview } from "./hooks/use-customer-receivables-preview"
import { useCustomerReceivablesUrlState } from "./hooks/use-customer-receivables-url-state"
import { useReverseFlow } from "./hooks/use-reverse-flow"

export function CustomerReceivablesPage() {
    const router = useRouter()

    const urlState = useCustomerReceivablesUrlState()

    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [partyPickerOpen, setPartyPickerOpen] = React.useState(false)
    const [partyPickerMode, setPartyPickerMode] =
        React.useState<AllocationMode>("receipt")
    const [selectedPartyId, setSelectedPartyId] = React.useState("")

    const { preview, openPreview, closePreview } =
        useCustomerReceivablesPreview({
            view: urlState.view,
            previewKind: urlState.previewKind,
            previewId: urlState.previewId,
            focusId: urlState.focusId,
            patchUrl: urlState.patchUrl,
        })

    const reverseFlow = useReverseFlow({
        closePreview,
        setLastResult,
        setActionError,
    })

    const listQuery = useCustomerAccountsListQuery(urlState.query)
    const detailQuery = useCustomerAccountsDetailQuery(
        preview?.kind ?? null,
        preview?.id ?? null,
    )
    const sessionQuery = useAllocationSessionQuery(urlState.sessionId ?? null)
    const createSession = useCreateAllocationSessionMutation()

    const data = listQuery.data

    /** 客户锁定（customerId）显性化为可移除 chip。 */
    const lockedCustomerName = React.useMemo(
        () =>
            (data?.counterparties ?? []).find(
                (c) => c.customerId === urlState.customerId,
            )?.customerName,
        [data?.counterparties, urlState.customerId],
    )

    useAutoAllocationSession({
        data,
        from: urlState.from,
        returnTo: urlState.returnTo,
        sessionId: urlState.sessionId,
        counterpartyPartyId: urlState.counterpartyPartyId,
        customerId: urlState.customerId,
        salesOrderId: urlState.salesOrderId,
        receivableAccountId: urlState.receivableAccountId,
        createSession,
        patchUrl: urlState.patchUrl,
        setActionError,
    })

    async function startSession(
        mode: AllocationMode,
        partyId: string,
        existingFactId?: string,
        target?: { salesOrderId?: string; receivableAccountId?: string },
    ) {
        setActionError(null)
        setLastResult(null)
        try {
            const session = await createSession.mutateAsync({
                mode,
                counterpartyPartyId: partyId,
                existingFactId,
                salesOrderId: target?.salesOrderId ?? urlState.salesOrderId,
                receivableAccountId:
                    target?.receivableAccountId ??
                    urlState.receivableAccountId,
                returnTo: urlState.returnTo,
                from: urlState.from,
            })
            setPartyPickerOpen(false)
            urlState.patchUrl({
                sessionId: session.draftSessionId,
                counterpartyId: partyId,
            })
        } catch (err) {
            setActionError(getErrorMessage(err, "创建本次核销失败"))
        }
    }

    function openRegister(mode: AllocationMode) {
        setPartyPickerMode(mode)
        setSelectedPartyId(urlState.counterpartyPartyId ?? "")
        setPartyPickerOpen(true)
    }

    const receivableColumns = React.useMemo(
        () =>
            createReceivableColumns({
                onPreview: openPreview,
                onStartSession: startSession,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [data?.canRegister],
    )

    const receiptColumns = React.useMemo(
        () =>
            createReceiptColumns({
                onPreview: openPreview,
                onStartSession: startSession,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [],
    )

    const invoiceColumns = React.useMemo(
        () =>
            createInvoiceColumns({
                onPreview: openPreview,
                onStartSession: startSession,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [],
    )

    // 核销会话全屏
    if (urlState.sessionId) {
        return (
            <AllocationSessionScreen
                isPending={sessionQuery.isPending}
                session={sessionQuery.data}
                onBackToList={() => urlState.patchUrl({ sessionId: null })}
                onClose={() => {
                    const ret = sessionQuery.data?.returnContext
                    if (ret?.returnTo && ret.from === "W05") {
                        router.push(ret.returnTo)
                        return
                    }
                    urlState.patchUrl({ sessionId: null })
                }}
                onPosted={() => {
                    void listQuery.refetch()
                }}
            />
        )
    }

    if (listQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="客户往来加载失败"
                    error={listQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void listQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const metrics = data?.metrics
    return (
        <PageScaffold density="compact">
            <CustomerReceivablesHeader
                data={data}
                onExport={() => {
                    if (!data) return
                    const fileName = `客户往来-${VIEW_LABEL[data.view]}-${new Date().toISOString().slice(0, 10)}.csv`
                    downloadCsv(fileName, buildAccountsCsv(data))
                    setLastResult({
                        status: "succeeded",
                        title: "导出已完成",
                        description: `已按当前筛选生成 CSV 文件 ${fileName}，并开始下载。`,
                    })
                }}
                onRegisterInvoice={() => openRegister("invoice")}
                onRegisterReceipt={() => openRegister("receipt")}
            />

            {urlState.from === "W05" && urlState.returnTo ? (
                <SalesOrderReturnAlert
                    salesOrderId={urlState.salesOrderId}
                    salesOrderNo={
                        data?.receivables.find(
                            (r) => r.salesOrderId === urlState.salesOrderId,
                        )?.salesOrderNo ?? ""
                    }
                    returnTo={urlState.returnTo}
                />
            ) : null}

            {lastResult ? (
                <FormalActionResult
                    status={
                        lastResult.status === "failed"
                            ? "blocked"
                            : lastResult.status
                    }
                    title={lastResult.title}
                    description={lastResult.description}
                    reference={lastResult.reference}
                    facts={lastResult.facts}
                />
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作未成功</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            {data && !data.moduleAllowed ? (
                <BusinessFailureState
                    kind="permission"
                    description="无客户往来模块权限或权限已收回。"
                />
            ) : data && !data.hasDataScope ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色未配置客户往来范围"
                    description="不得用 0 元假装无应收。请申请财务数据范围。"
                />
            ) : (
                <>
                    <CustomerReceivablesMetrics
                        view={urlState.view}
                        due={urlState.due}
                        metrics={metrics}
                        queriedAt={data?.queriedAt}
                        patchUrl={urlState.patchUrl}
                    />

                    <CustomerReceivablesTable
                        view={urlState.view}
                        data={data}
                        isPending={listQuery.isPending}
                        metrics={metrics}
                        pagination={urlState.pagination}
                        receivableColumns={receivableColumns}
                        receiptColumns={receiptColumns}
                        invoiceColumns={invoiceColumns}
                        toolbar={
                            <CustomerReceivablesToolbar
                                view={urlState.view}
                                due={urlState.due}
                                status={urlState.status}
                                reviewStatus={urlState.reviewStatus}
                                counterpartyPartyId={
                                    urlState.counterpartyPartyId
                                }
                                customerId={urlState.customerId}
                                lockedCustomerName={lockedCustomerName}
                                hasActiveFilters={urlState.hasActiveFilters}
                                total={data?.total ?? 0}
                                searchInput={urlState.searchInput}
                                searchInputRef={urlState.searchInputRef}
                                setSearchInput={urlState.setSearchInput}
                                patchUrl={urlState.patchUrl}
                                clearFilters={urlState.clearFilters}
                                onRefresh={() => void listQuery.refetch()}
                            />
                        }
                        patchUrl={urlState.patchUrl}
                        onPaginationChange={urlState.handlePaginationChange}
                        clearFilters={urlState.clearFilters}
                    />
                </>
            )}

            <CustomerAccountDetailPreview
                open={preview != null}
                data={detailQuery.data}
                isPending={detailQuery.isPending}
                isError={detailQuery.isError}
                error={detailQuery.error}
                onRetry={() => void detailQuery.refetch()}
                onClose={closePreview}
                onStartSession={startSession}
                onRequestReverse={(request: ReverseRequest) => {
                    if (request.kind === "red_invoice") {
                        reverseFlow.setReverseAmount(request.amount ?? "")
                    }
                    reverseFlow.setReverseConfirm(request)
                }}
            />

            <ReceivableActionDialogs
                partyPickerOpen={partyPickerOpen}
                partyPickerMode={partyPickerMode}
                selectedPartyId={selectedPartyId}
                createPending={createSession.isPending}
                onPartyPickerOpenChange={setPartyPickerOpen}
                onSelectedPartyIdChange={setSelectedPartyId}
                onStartSession={(mode, partyId) =>
                    void startSession(mode, partyId)
                }
                reverseRequest={reverseFlow.reverseConfirm}
                reverseReason={reverseFlow.reverseReason}
                reverseAmount={reverseFlow.reverseAmount}
                reversePending={reverseFlow.reverseMutation.isPending}
                onReverseOpenChange={(open) => {
                    if (!open) {
                        reverseFlow.setReverseConfirm(null)
                        reverseFlow.setReverseReason("")
                    }
                }}
                onReverseReasonChange={reverseFlow.setReverseReason}
                onReverseAmountChange={reverseFlow.setReverseAmount}
                onCancelReverse={() => {
                    reverseFlow.setReverseConfirm(null)
                    reverseFlow.setReverseReason("")
                    reverseFlow.setReverseAmount("")
                }}
                onConfirmReverse={() => void reverseFlow.confirmReverse()}
            />
        </PageScaffold>
    )
}
