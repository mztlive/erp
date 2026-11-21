"use client"

import { Button } from "@/components/ui/button"

import * as React from "react"
import {
    receiptRegistrationHref,
    receiptReturnHref,
} from "../lib/receipt-registration-route"
import { useRouter, usePathname, useSearchParams } from "@/lib/navigation"

import { FormalActionResult, PageScaffold } from "@/components/business"
import {
    ListWorkspaceViews,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { type ResultState } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    createInvoiceColumns,
    createReceivableColumns,
    createReceiptColumns,
} from "@/features/customer-receivables/components/customer-account-columns"
import {
    CustomerAccountDetailPreview,
    type ReverseRequest,
} from "@/features/customer-receivables/components/customer-account-detail-preview"
import { CustomerRefundRequestDialog } from "@/features/customer-receivables/components/customer-refund-request-dialog"
import { CustomerRefundSubmitConfirmDialog } from "@/features/customer-receivables/components/customer-refund-submit-confirm-dialog"
import { ReceiptReversalRequestDialog } from "@/features/customer-receivables/components/receipt-reversal-request-dialog"
import { ReceiptReversalSubmitConfirmDialog } from "@/features/customer-receivables/components/receipt-reversal-submit-confirm-dialog"
import { ReceivableActionDialogs } from "@/features/customer-receivables/components/receivable-action-dialogs"
import {
    useAllocationSessionQuery,
    useCreateAllocationSessionMutation,
    useCustomerAccountsDetailQuery,
    useCustomerAccountsListQuery,
} from "@/features/customer-receivables/hooks/queries"
import {
    buildAccountsCsv,
    downloadCsv,
} from "@/features/customer-receivables/lib/export-csv"
import {
    VIEW_LABEL,
    type AllocationMode,
    type CustomerAccountsView,
} from "@/features/customer-receivables/types"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { isCustomerReceiptWorkItem } from "@/features/customer-receivables/lib/customer-receipt-approval"
import { isCustomerRefundWorkItem } from "@/features/customer-receivables/lib/customer-refund-approval"
import { isReceiptReversalWorkItem } from "@/features/customer-receivables/lib/receipt-reversal-approval"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { AllocationSessionScreen } from "@/features/customer-receivables/pages/components/allocation-session-screen"
import { CustomerReceivablesHeader } from "@/features/customer-receivables/pages/components/customer-receivables-header"
import { CustomerReceivablesListPane } from "@/features/customer-receivables/pages/components/customer-receivables-list-pane"
import { CustomerReceivablesMetrics } from "@/features/customer-receivables/pages/components/customer-receivables-metrics"
import { SalesOrderReturnAlert } from "@/features/customer-receivables/pages/components/sales-order-return-alert"
import { useAutoAllocationSession } from "@/features/customer-receivables/pages/hooks/use-auto-allocation-session"
import { useCustomerReceivablesAppliedChips } from "@/features/customer-receivables/pages/hooks/use-customer-receivables-applied-chips"
import { useCustomerReceivablesPreview } from "@/features/customer-receivables/pages/hooks/use-customer-receivables-preview"
import { useCustomerReceivablesUrlState } from "@/features/customer-receivables/pages/hooks/use-customer-receivables-url-state"
import { useReverseFlow } from "@/features/customer-receivables/pages/hooks/use-reverse-flow"
import { useCustomerReceivablesPermissions } from "@/features/customer-receivables/hooks/use-customer-receivables-permissions"

const VIEWS = ["receivable", "receipt", "sales_invoice", "unallocated"] as const
const LEDGER_PERMISSIONS = {
    receivable: "receivable_account:list",
    receipt: "customer_receipt:list",
    sales_invoice: "invoice:list",
} as const
const PREVIEW_PERMISSIONS = {
    receivable: "receivable_account:detail",
    receipt: "customer_receipt:detail",
    invoice: "invoice:detail",
} as const
const INVOICE_REQUESTS_HREF = "/finance/customer-accounts/invoice-requests"

/**
 * 客户往来工作面。客户回款、客户退款与回款冲正嵌入通用审批区；
 * Invoice 为 NO_APPROVAL，详情/预览/登记路径不展示审批区。
 */
export type CustomerReceivablesWorkspaceProps = {
    embedded?: boolean
    salesOrderId?: string
    salesOrderNo?: string
    counterpartyPartyId?: string
    counterpartyPartyName?: string
    customerId?: string
    customerName?: string
    onSalesOrderChanged?: () => void
}

export function CustomerReceivablesWorkspace({
    embedded = false,
    salesOrderId,
    salesOrderNo,
    counterpartyPartyId,
    counterpartyPartyName,
    customerId,
    customerName,
    onSalesOrderChanged,
}: CustomerReceivablesWorkspaceProps = {}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const registrationReturnTo = receiptReturnHref(
        `${pathname}?${searchParams}`,
    )
    const permissions = useCustomerReceivablesPermissions()
    const profile = useAccountProfileQuery()
    const granted = profile.isError ? undefined : profile.data?.permissions
    const listAccess = {
        receivable: hasPermission(granted, LEDGER_PERMISSIONS.receivable),
        receipt: hasPermission(granted, LEDGER_PERMISSIONS.receipt),
        sales_invoice: hasPermission(granted, LEDGER_PERMISSIONS.sales_invoice),
    }
    const canReadView = (view: CustomerAccountsView) =>
        view === "unallocated"
            ? listAccess.receipt && listAccess.sales_invoice
            : listAccess[view]
    const readableViews = VIEWS.filter(canReadView)
    const canPreview = (kind: keyof typeof PREVIEW_PERMISSIONS) =>
        hasPermission(granted, PREVIEW_PERMISSIONS[kind])
    const canReadInvoiceRequests = hasPermission(
        granted,
        "sales_invoice_request:list",
    )

    const urlState = useCustomerReceivablesUrlState({
        fixedSalesOrderId: salesOrderId,
        defaultView: readableViews[0],
        stateMode: embedded ? "local" : "url",
    })

    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [partyPickerOpen, setPartyPickerOpen] = React.useState(false)
    const [partyPickerMode, setPartyPickerMode] =
        React.useState<AllocationMode>("receipt")
    const [selectedPartyId, setSelectedPartyId] = React.useState("")

    const { preview, openPreview, closePreview, restoreRowFocus } =
        useCustomerReceivablesPreview({
            view: urlState.view,
            previewKind: urlState.previewKind,
            previewId: urlState.previewId,
            focusId: urlState.focusId,
            patchUrl: urlState.patchUrl,
        })

    const reverseFlow = useReverseFlow({
        closePreview,
        openRefundPreview: (refundId) => {
            openPreview({ kind: "refund", id: refundId })
        },
        openReversalPreview: (reversalId) => {
            openPreview({ kind: "reversal", id: reversalId })
        },
        setLastResult,
        setActionError,
        onChanged: onSalesOrderChanged,
    })

    const workItemQuery = useWorkItemDetailQuery(urlState.workItemId ?? "")
    const focusedWorkItem = workItemQuery.data
        ? mapWorkItemDto(workItemQuery.data)
        : undefined
    const invoiceExecutionTask =
        focusedWorkItem?.workItemType === "SALES_INVOICE_EXECUTION" &&
        focusedWorkItem.handlerKey === "sales_invoice_execution" &&
        focusedWorkItem.businessObjectType === "receivable_account" &&
        focusedWorkItem.status === "OPEN" &&
        focusedWorkItem.allowedActions.includes("PROCESS")
            ? focusedWorkItem
            : undefined
    const invoiceTaskBlockedReason =
        "销项开票必须由当前负责人从工作台的开票任务进入"
    const canStartAssignedSession = (mode: AllocationMode) =>
        permissions.canStartSession(mode) &&
        (mode !== "invoice" || Boolean(invoiceExecutionTask))
    const workItemReceiptId = isCustomerReceiptWorkItem(focusedWorkItem)
        ? focusedWorkItem?.businessObjectId
        : undefined
    const workItemRefundId = isCustomerRefundWorkItem(focusedWorkItem)
        ? focusedWorkItem?.businessObjectId
        : undefined
    const workItemReversalId = isReceiptReversalWorkItem(focusedWorkItem)
        ? focusedWorkItem?.businessObjectId
        : undefined
    const previewKind =
        preview?.kind ??
        (workItemReceiptId
            ? "receipt"
            : workItemRefundId
              ? "refund"
              : workItemReversalId
                ? "reversal"
                : null)
    const previewId =
        preview?.id ??
        workItemReceiptId ??
        workItemRefundId ??
        workItemReversalId ??
        null

    const canReadCurrentView = canReadView(urlState.view)
    const isRegistrationLink =
        !urlState.sessionId &&
        Boolean(
            urlState.registerMode &&
            canStartAssignedSession(urlState.registerMode) &&
            ((urlState.from === "W05" && urlState.returnTo) ||
                (urlState.from === "W01" &&
                    urlState.registerMode === "invoice" &&
                    urlState.receivableAccountId)),
        )
    const listEnabled = canReadCurrentView || isRegistrationLink
    const listQuery = useCustomerAccountsListQuery(
        urlState.query,
        listAccess,
        listEnabled,
    )
    const detailQuery = useCustomerAccountsDetailQuery(previewKind, previewId)
    const sessionQuery = useAllocationSessionQuery(urlState.sessionId ?? null)
    // 旧会话链接只负责转入独立登记页，不再替换客户往来列表。
    React.useEffect(() => {
        if (sessionQuery.data?.mode !== "receipt") return
        router.replace(
            receiptRegistrationHref(
                sessionQuery.data.draftSessionId,
                sessionQuery.data.returnContext?.returnTo ??
                    registrationReturnTo,
            ),
        )
    }, [sessionQuery.data, registrationReturnTo, router])

    const createSession = useCreateAllocationSessionMutation()
    const startSessionPending = createSession.isPending
    const invoiceSessionMatchesTask =
        sessionQuery.data?.mode !== "invoice" ||
        Boolean(
            invoiceExecutionTask &&
            sessionQuery.data?.pool.some(
                (target) =>
                    target.targetId === invoiceExecutionTask.businessObjectId,
            ),
        )
    const sessionCanOperate = sessionQuery.data
        ? permissions.canStartSession(sessionQuery.data.mode) &&
          invoiceSessionMatchesTask
        : false
    const sessionPermissionReason =
        sessionQuery.data?.mode === "invoice" && !invoiceSessionMatchesTask
            ? invoiceTaskBlockedReason
            : permissions.reason

    const result =
        listEnabled && !listQuery.isError ? listQuery.data : undefined
    const data = listQuery.isPlaceholderData ? undefined : result
    const listData = canReadCurrentView ? result : undefined
    const exportData = canReadCurrentView ? data : undefined
    const listFailed =
        canReadCurrentView &&
        (listQuery.isError || (!listQuery.isPending && !listData))
    const refreshList = () => {
        if (listEnabled) void listQuery.refetch()
    }

    const appliedChips = useCustomerReceivablesAppliedChips({
        data: exportData,
        urlState,
        embedded,
        embeddedCounterpartyId: counterpartyPartyId,
        embeddedCounterpartyName: counterpartyPartyName,
    })

    useAutoAllocationSession({
        data,
        from: urlState.from,
        returnTo: urlState.returnTo,
        sessionId: urlState.sessionId,
        counterpartyPartyId: urlState.counterpartyPartyId,
        customerId: urlState.customerId,
        salesOrderId: urlState.salesOrderId,
        registerMode: urlState.registerMode,
        receivableAccountId: urlState.receivableAccountId,
        canRegister: canStartAssignedSession(
            urlState.registerMode ?? "receipt",
        ),
        createSession,
        patchUrl: urlState.patchUrl,
        onReceiptCreated: (id) =>
            router.replace(
                receiptRegistrationHref(
                    id,
                    urlState.returnTo ?? registrationReturnTo,
                ),
            ),
        setActionError,
    })

    async function startSession(
        mode: AllocationMode,
        partyId: string,
        existingFactId?: string,
        target?: { salesOrderId?: string; receivableAccountId?: string },
    ) {
        if (!canStartAssignedSession(mode)) {
            setActionError(
                mode === "invoice"
                    ? invoiceTaskBlockedReason
                    : permissions.reason,
            )
            return
        }
        if (
            mode === "invoice" &&
            target?.receivableAccountId &&
            target.receivableAccountId !==
                invoiceExecutionTask?.businessObjectId
        ) {
            setActionError(
                "当前开票任务不属于所选应收款项，请从工作台重新进入。",
            )
            return
        }
        setActionError(null)
        setLastResult(null)
        try {
            const party = data?.counterparties.find(
                (item) => item.counterpartyPartyId === partyId,
            )
            const receivable = data?.receivables.find(
                (row) =>
                    row.counterpartyPartyId === partyId &&
                    (!target?.receivableAccountId ||
                        row.accountId === target.receivableAccountId) &&
                    (!(target?.salesOrderId ?? urlState.salesOrderId) ||
                        row.salesOrderId ===
                            (target?.salesOrderId ?? urlState.salesOrderId)),
            )
            const session = await createSession.mutateAsync({
                mode,
                counterpartyPartyId: partyId,
                counterpartyPartyName:
                    party?.counterpartyPartyName ??
                    receivable?.counterpartyPartyName ??
                    (partyId === counterpartyPartyId
                        ? counterpartyPartyName
                        : undefined),
                customerId:
                    party?.customerId ?? receivable?.customerId ?? customerId,
                customerName:
                    party?.customerName ??
                    receivable?.customerName ??
                    customerName,
                existingFactId,
                salesOrderId: target?.salesOrderId ?? urlState.salesOrderId,
                receivableAccountId:
                    mode === "invoice"
                        ? invoiceExecutionTask?.businessObjectId
                        : (target?.receivableAccountId ??
                          urlState.receivableAccountId),
                returnTo:
                    mode === "receipt"
                        ? (urlState.returnTo ?? registrationReturnTo)
                        : urlState.returnTo,
                from:
                    mode === "receipt" &&
                    registrationReturnTo.startsWith("/sales/orders/")
                        ? "W05"
                        : urlState.from,
            })
            setPartyPickerOpen(false)
            openPreview(null)
            if (mode === "receipt") {
                router.push(
                    receiptRegistrationHref(
                        session.draftSessionId,
                        urlState.returnTo ?? registrationReturnTo,
                    ),
                )
                return
            }
            urlState.patchUrl({
                previewKind: null,
                previewId: null,
                focusId: null,
                sessionId: session.draftSessionId,
                counterpartyId: embedded ? null : partyId,
            })
        } catch (err) {
            setActionError(getErrorMessage(err, "创建本次核销失败"))
        }
    }

    function openRegister(mode: AllocationMode) {
        const uniqueParty =
            counterpartyPartyId ??
            urlState.counterpartyPartyId ??
            (mode === "invoice"
                ? data?.receivables.find(
                      (row) =>
                          row.accountId ===
                          invoiceExecutionTask?.businessObjectId,
                  )?.counterpartyPartyId
                : undefined) ??
            data?.receivables.find(
                (row) => row.salesOrderId === urlState.salesOrderId,
            )?.counterpartyPartyId ??
            (data?.counterparties.length === 1
                ? data.counterparties[0]?.counterpartyPartyId
                : undefined)
        if (uniqueParty) {
            void startSession(mode, uniqueParty)
            return
        }
        if (embedded) {
            setActionError(
                "当前销售单缺少结算主体，无法在本页登记票款。请先补齐销售单结算信息。",
            )
            return
        }
        setPartyPickerMode(mode)
        setSelectedPartyId("")
        setPartyPickerOpen(true)
    }

    const receivableColumns = React.useMemo(createReceivableColumns, [])
    const receiptColumns = React.useMemo(createReceiptColumns, [])
    const invoiceColumns = React.useMemo(createInvoiceColumns, [])

    // 核销会话全屏
    if (urlState.sessionId && sessionQuery.data?.mode === "receipt") {
        return (
            <PageScaffold>
                <p role="status" className="text-xs text-muted-foreground">
                    正在打开回款登记…
                </p>
            </PageScaffold>
        )
    }
    if (urlState.sessionId) {
        return (
            <AllocationSessionScreen
                isPending={sessionQuery.isPending}
                error={sessionQuery.error}
                onRetry={() => {
                    void sessionQuery.refetch()
                }}
                session={sessionQuery.data}
                onBackToList={() =>
                    urlState.patchUrl({ sessionId: null, register: null })
                }
                onClose={() => {
                    const ret = sessionQuery.data?.returnContext
                    if (!embedded && ret?.returnTo && ret.from === "W05") {
                        router.push(ret.returnTo)
                        return
                    }
                    urlState.patchUrl({ sessionId: null })
                }}
                onPosted={() => {
                    refreshList()
                    onSalesOrderChanged?.()
                }}
                canOperate={sessionCanOperate}
                permissionReason={sessionPermissionReason}
                workItemId={invoiceExecutionTask?.workItemId}
                expectedTaskVersion={invoiceExecutionTask?.taskVersion}
                taskReceivableAccountId={invoiceExecutionTask?.businessObjectId}
                embedded={embedded}
            />
        )
    }

    const views = readableViews.map((view) => ({
        id: `customer-receivables-view-${view}`,
        view,
        label: VIEW_LABEL[view],
        count:
            view === urlState.view
                ? exportData?.total.toLocaleString("zh-CN")
                : undefined,
        active: view === urlState.view,
        onClick: () => urlState.changeView(view),
    }))
    const viewSwitcher =
        views.length === 1 && !canReadCurrentView ? (
            <Button
                id={views[0].id}
                type="button"
                variant="ghost"
                className={listWorkspaceStyles.quietButton}
                aria-pressed={views[0].active}
                onClick={views[0].onClick}
            >
                {views[0].label}
            </Button>
        ) : views.length >= 2 ? (
            embedded ? (
                <Tabs
                    value={urlState.view}
                    onValueChange={(value) =>
                        views.find((item) => item.view === value)?.onClick()
                    }
                >
                    <TabsList
                        variant="line"
                        aria-label="本单客户往来工作视图"
                        className="w-full min-w-0 justify-start overflow-x-auto border-b border-border/60"
                    >
                        {views.map((item) => (
                            <TabsTrigger
                                key={item.id}
                                id={item.id}
                                value={item.view}
                            >
                                {item.label}
                            </TabsTrigger>
                        ))}
                    </TabsList>
                </Tabs>
            ) : (
                <ListWorkspaceViews
                    ariaLabel="客户往来工作视图"
                    items={views}
                    variant="inline"
                />
            )
        ) : null

    const content = (
        <>
            <CustomerReceivablesHeader
                data={exportData}
                exportPending={
                    listQuery.isPlaceholderData ||
                    (listQuery.isPending && listEnabled)
                }
                views={embedded ? undefined : viewSwitcher}
                onOpenInvoiceRequests={
                    !embedded && canReadInvoiceRequests
                        ? () => router.push(INVOICE_REQUESTS_HREF)
                        : undefined
                }
                embedded={embedded}
                salesOrderNo={salesOrderNo}
                onExport={() => {
                    if (!exportData) return
                    const fileName = `客户往来-${VIEW_LABEL[exportData.view]}-${new Date().toISOString().slice(0, 10)}.csv`
                    downloadCsv(fileName, buildAccountsCsv(exportData))
                    setLastResult({
                        status: "succeeded",
                        title: "导出已完成",
                        description: `已按当前筛选生成 CSV 文件 ${fileName}，并开始下载。`,
                    })
                }}
                onRegisterInvoice={() =>
                    router.push(`${INVOICE_REQUESTS_HREF}?create=1`)
                }
                onRegisterReceipt={() => openRegister("receipt")}
                startSessionPending={startSessionPending}
                canRegisterInvoice={permissions.canSubmitInvoiceRequest}
                canRegisterReceipt={permissions.canRegisterReceipt}
                canExport={
                    canReadCurrentView &&
                    permissions.canExport &&
                    listData?.moduleAllowed !== false
                }
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

            {reverseFlow.refundSubmitUnknown ? (
                <Button
                    id="customer-refund-resume-submit"
                    variant="outline"
                    onClick={() => reverseFlow.setRefundSubmitOpen(true)}
                >
                    继续核对退款提交结果
                </Button>
            ) : null}
            {reverseFlow.reversalSubmitUnknown ? (
                <Button
                    id="customer-reversal-resume-submit"
                    variant="outline"
                    onClick={() => reverseFlow.setReversalSubmitOpen(true)}
                >
                    继续核对冲正提交结果
                </Button>
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作未成功</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            <CustomerAccountDetailPreview
                open={
                    preview != null ||
                    Boolean(workItemReceiptId) ||
                    Boolean(workItemRefundId) ||
                    Boolean(workItemReversalId)
                }
                data={detailQuery.data}
                isPending={detailQuery.isPending}
                isError={detailQuery.isError}
                error={detailQuery.error}
                onRetry={() => void detailQuery.refetch()}
                onClose={closePreview}
                onClosed={restoreRowFocus}
                onStartSession={startSession}
                canStartSession={canStartAssignedSession}
                invoiceSessionBlockedReason={invoiceTaskBlockedReason}
                startSessionPending={startSessionPending}
                canRequestReverse={permissions.canReverse}
                canSubmitRefund={permissions.canSubmitRefund}
                canSubmitReversal={permissions.canSubmitReversal}
                permissionReason={permissions.reason}
                workItemId={focusedWorkItem?.workItemId}
                expectedTaskVersion={focusedWorkItem?.taskVersion}
                workItemAllowedActions={focusedWorkItem?.allowedActions}
                onDecisionApplied={(view: ApprovalCommandView) => {
                    void detailQuery.refetch()
                    refreshList()
                    onSalesOrderChanged?.()
                    setLastResult({
                        status: "succeeded",
                        title: "审批决定已提交",
                        description: view.latestRejectionReason
                            ? `已按当前任务提交决定。${view.latestRejectionReason}`
                            : "已按当前任务提交决定。",
                        reference:
                            workItemReceiptId ??
                            workItemRefundId ??
                            workItemReversalId,
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
                onRequestReverse={(request: ReverseRequest) => {
                    if (!permissions.canReverse(request.kind)) {
                        setActionError(permissions.reason)
                        return
                    }
                    if (request.kind === "red_invoice") {
                        reverseFlow.setReverseAmount(request.amount ?? "")
                    }
                    reverseFlow.setReverseConfirm(request)
                }}
                onRequestRefundSubmit={() => {
                    if (!permissions.canSubmitRefund) {
                        setActionError(permissions.reason)
                        return
                    }
                    const refund = detailQuery.data?.refund
                    if (refund) reverseFlow.beginRefundSubmit(refund)
                }}
                onRequestReversalSubmit={() => {
                    if (!permissions.canSubmitReversal) {
                        setActionError(permissions.reason)
                        return
                    }
                    const reversal = detailQuery.data?.reversal
                    if (reversal) reverseFlow.beginReversalSubmit(reversal)
                }}
            />

            <ReceivableActionDialogs
                partyPickerOpen={partyPickerOpen}
                partyPickerMode={partyPickerMode}
                selectedPartyId={selectedPartyId}
                createPending={startSessionPending}
                onPartyPickerOpenChange={setPartyPickerOpen}
                onSelectedPartyIdChange={setSelectedPartyId}
                onStartSession={(mode, partyId) =>
                    void startSession(mode, partyId)
                }
                reverseRequest={
                    reverseFlow.reverseConfirm?.kind === "refund" ||
                    reverseFlow.reverseConfirm?.kind === "receipt_reverse"
                        ? null
                        : reverseFlow.reverseConfirm
                }
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
                onConfirmReverse={() => {
                    const kind = reverseFlow.reverseConfirm?.kind
                    if (kind && !permissions.canReverse(kind)) {
                        setActionError(permissions.reason)
                        return
                    }
                    void reverseFlow.confirmReverse()
                }}
            />

            <CustomerRefundRequestDialog
                open={reverseFlow.reverseConfirm?.kind === "refund"}
                pending={reverseFlow.refundSubmitPending}
                sourceLabel={reverseFlow.reverseConfirm?.label}
                amount={reverseFlow.reverseConfirm?.amount}
                onOpenChange={(open) => {
                    if (!open) {
                        reverseFlow.setReverseConfirm(null)
                        reverseFlow.setReverseReason("")
                    }
                }}
                onSubmit={(reason) => {
                    if (
                        !permissions.canRefund ||
                        !permissions.canSubmitRefund
                    ) {
                        setActionError(permissions.reason)
                        throw new Error(permissions.reason)
                    }
                    return reverseFlow.prepareRefundDraft(reason)
                }}
            />

            <CustomerRefundSubmitConfirmDialog
                open={reverseFlow.refundSubmitOpen}
                unknown={reverseFlow.refundSubmitUnknown}
                frozenReason={reverseFlow.refundSubmitReason}
                documentId={reverseFlow.refundDraft?.refundId}
                expectedVersion={reverseFlow.refundDraft?.baselineVersion}
                onEndRecovery={reverseFlow.endRefundRecovery}
                pending={reverseFlow.refundSubmitPending}
                approval={
                    reverseFlow.refundDraft?.approval ??
                    detailQuery.data?.refund?.approval
                }
                onOpenChange={reverseFlow.setRefundSubmitOpen}
                onConfirm={(resubmissionReason) => {
                    if (!permissions.canSubmitRefund) {
                        setActionError(permissions.reason)
                        return
                    }
                    return reverseFlow.confirmRefundSubmit(resubmissionReason)
                }}
            />

            <ReceiptReversalRequestDialog
                open={reverseFlow.reverseConfirm?.kind === "receipt_reverse"}
                pending={reverseFlow.reversalSubmitPending}
                sourceLabel={reverseFlow.reverseConfirm?.label}
                amount={reverseFlow.reverseConfirm?.amount}
                onOpenChange={(open) => {
                    if (!open) {
                        reverseFlow.setReverseConfirm(null)
                        reverseFlow.setReverseReason("")
                    }
                }}
                onSubmit={(reason) => {
                    if (
                        !permissions.canReverseReceipt ||
                        !permissions.canSubmitReversal
                    ) {
                        setActionError(permissions.reason)
                        throw new Error(permissions.reason)
                    }
                    return reverseFlow.prepareReversalDraft(reason)
                }}
            />

            <ReceiptReversalSubmitConfirmDialog
                open={reverseFlow.reversalSubmitOpen}
                unknown={reverseFlow.reversalSubmitUnknown}
                frozenReason={reverseFlow.reversalSubmitReason}
                documentId={reverseFlow.reversalDraft?.reversalId}
                expectedVersion={reverseFlow.reversalDraft?.baselineVersion}
                onEndRecovery={reverseFlow.endReversalRecovery}
                pending={reverseFlow.reversalSubmitPending}
                approval={
                    reverseFlow.reversalDraft?.approval ??
                    detailQuery.data?.reversal?.approval
                }
                onOpenChange={reverseFlow.setReversalSubmitOpen}
                onConfirm={(resubmissionReason) => {
                    if (!permissions.canSubmitReversal) {
                        setActionError(permissions.reason)
                        return
                    }
                    return reverseFlow.confirmReversalSubmit(resubmissionReason)
                }}
            />
        </>
    )

    const ledger = (
        <>
            {embedded ? viewSwitcher : null}
            {listData?.moduleAllowed && listData.hasDataScope ? (
                <CustomerReceivablesMetrics
                    metrics={listData.metrics}
                    access={listAccess}
                    queriedAt={listData.queriedAt}
                    embedded={embedded}
                />
            ) : null}
            <CustomerReceivablesListPane
                embedded={embedded}
                data={listData}
                urlState={urlState}
                appliedChips={appliedChips}
                profilePending={profile.isPending}
                isPending={canReadCurrentView && listQuery.isPending}
                isFetching={listQuery.isFetching}
                isPlaceholder={listQuery.isPlaceholderData}
                isError={listFailed}
                error={listQuery.error}
                canReadView={canReadCurrentView}
                profileError={profile.error}
                onRetryProfile={() => void profile.refetch()}
                canPreview={canPreview}
                onRetry={refreshList}
                receivableColumns={receivableColumns}
                receiptColumns={receiptColumns}
                invoiceColumns={invoiceColumns}
                preview={preview}
                onPreview={openPreview}
            />
        </>
    )
    if (embedded) {
        return (
            <div className="flex flex-col gap-4">
                {content}
                {ledger}
            </div>
        )
    }
    return (
        <PageScaffold
            density="compact"
            className={`${listWorkspaceStyles.page} min-h-0`}
        >
            {content}
            {ledger}
        </PageScaffold>
    )
}
