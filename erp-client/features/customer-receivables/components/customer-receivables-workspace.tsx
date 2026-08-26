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
    DUE_LABEL,
    RECEIVABLE_STATUS_LABEL,
    REVIEW_STATUS_LABEL,
    VIEW_LABEL,
    type AllocationMode,
} from "@/features/customer-receivables/types"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { isCustomerReceiptWorkItem } from "@/features/customer-receivables/lib/customer-receipt-approval"
import { isCustomerRefundWorkItem } from "@/features/customer-receivables/lib/customer-refund-approval"
import { isReceiptReversalWorkItem } from "@/features/customer-receivables/lib/receipt-reversal-approval"
import { getErrorMessage } from "@/lib/api/errors"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { AllocationSessionScreen } from "@/features/customer-receivables/pages/components/allocation-session-screen"
import { CustomerReceivablesHeader } from "@/features/customer-receivables/pages/components/customer-receivables-header"
import { CustomerReceivablesMetrics } from "@/features/customer-receivables/pages/components/customer-receivables-metrics"
import { CustomerReceivablesTable } from "@/features/customer-receivables/pages/components/customer-receivables-table"
import {
    CustomerReceivablesToolbar,
    type ReceivableAppliedChip,
} from "@/features/customer-receivables/pages/components/customer-receivables-toolbar"
import { SalesOrderReturnAlert } from "@/features/customer-receivables/pages/components/sales-order-return-alert"
import { useAutoAllocationSession } from "@/features/customer-receivables/pages/hooks/use-auto-allocation-session"
import { useCustomerReceivablesPreview } from "@/features/customer-receivables/pages/hooks/use-customer-receivables-preview"
import { useCustomerReceivablesUrlState } from "@/features/customer-receivables/pages/hooks/use-customer-receivables-url-state"
import { useReverseFlow } from "@/features/customer-receivables/pages/hooks/use-reverse-flow"
import { useCustomerReceivablesPermissions } from "@/features/customer-receivables/hooks/use-customer-receivables-permissions"

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
    const permissions = useCustomerReceivablesPermissions()

    const urlState = useCustomerReceivablesUrlState({
        fixedSalesOrderId: salesOrderId,
        stateMode: embedded ? "local" : "url",
    })

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

    const listQuery = useCustomerAccountsListQuery(urlState.query)
    const detailQuery = useCustomerAccountsDetailQuery(previewKind, previewId)
    const sessionQuery = useAllocationSessionQuery(urlState.sessionId ?? null)
    const createSession = useCreateAllocationSessionMutation()
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

    const data = listQuery.data

    /** 客户锁定（customerId）显性化为可移除 chip。 */
    const lockedCustomerName = React.useMemo(
        () =>
            (data?.counterparties ?? []).find(
                (c) => c.customerId === urlState.customerId,
            )?.customerName,
        [data?.counterparties, urlState.customerId],
    )

    /** 已生效条件全部显性化为可单独移除的 chip（含深链来源锁定）。 */
    const appliedChips = React.useMemo<readonly ReceivableAppliedChip[]>(() => {
        const chips: ReceivableAppliedChip[] = []
        const trimmedQ = urlState.qParam.trim()
        if (trimmedQ) {
            chips.push({ key: "q", label: `搜索：${trimmedQ}` })
        }
        if (urlState.counterpartyPartyId) {
            const party = data?.counterparties.find(
                (c) => c.counterpartyPartyId === urlState.counterpartyPartyId,
            )
            const embeddedPartyName =
                embedded && counterpartyPartyId === urlState.counterpartyPartyId
                    ? counterpartyPartyName
                    : undefined
            chips.push({
                key: "counterpartyId",
                label: `往来主体：${party?.counterpartyPartyName ?? embeddedPartyName ?? urlState.counterpartyPartyId}`,
            })
        }
        if (urlState.customerId) {
            chips.push({
                key: "customerId",
                label: `经营客户 ${lockedCustomerName ?? urlState.customerId}`,
            })
        }
        if (urlState.due && urlState.due !== "all") {
            chips.push({
                key: "due",
                label: `到期：${DUE_LABEL[urlState.due]}`,
            })
        }
        if (urlState.status) {
            chips.push({
                key: "status",
                label: `状态：${RECEIVABLE_STATUS_LABEL[urlState.status]}`,
            })
        }
        if (urlState.reviewStatus) {
            chips.push({
                key: "reviewStatus",
                label: `复核状态：${REVIEW_STATUS_LABEL[urlState.reviewStatus]}`,
            })
        }
        if (urlState.salesOrderId && !embedded) {
            const row = data?.receivables.find(
                (r) => r.salesOrderId === urlState.salesOrderId,
            )
            chips.push({
                key: "salesOrderId",
                label: `销售单：${row?.salesOrderNo ?? urlState.salesOrderId}`,
            })
        }
        if (urlState.receivableAccountId) {
            const row = data?.receivables.find(
                (r) => r.accountId === urlState.receivableAccountId,
            )
            chips.push({
                key: "receivableAccountId",
                label:
                    row?.accountSeq != null
                        ? `往来子账：${row.accountSeq}`
                        : `往来子账：${urlState.receivableAccountId}`,
            })
        }
        return chips
    }, [
        counterpartyPartyId,
        counterpartyPartyName,
        data?.counterparties,
        data?.receivables,
        embedded,
        lockedCustomerName,
        urlState.counterpartyPartyId,
        urlState.customerId,
        urlState.due,
        urlState.qParam,
        urlState.receivableAccountId,
        urlState.reviewStatus,
        urlState.salesOrderId,
        urlState.status,
    ])

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
                "当前开票任务不属于所选应收子账，请从工作台重新进入。",
            )
            return
        }
        setActionError(null)
        setLastResult(null)
        try {
            const session = await createSession.mutateAsync({
                mode,
                counterpartyPartyId: partyId,
                counterpartyPartyName,
                customerId,
                customerName,
                existingFactId,
                salesOrderId: target?.salesOrderId ?? urlState.salesOrderId,
                receivableAccountId:
                    mode === "invoice"
                        ? invoiceExecutionTask?.businessObjectId
                        : (target?.receivableAccountId ??
                          urlState.receivableAccountId),
                returnTo: urlState.returnTo,
                from: urlState.from,
            })
            setPartyPickerOpen(false)
            urlState.patchUrl({
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

    const receivableColumns = React.useMemo(
        () =>
            createReceivableColumns({
                onPreview: openPreview,
                onStartSession: startSession,
                canStartSession: canStartAssignedSession,
                permissionReason: permissions.reason,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [
            invoiceExecutionTask?.workItemId,
            permissions.canRegisterInvoice,
            permissions.canRegisterReceipt,
            permissions.reason,
        ],
    )

    const receiptColumns = React.useMemo(
        () =>
            createReceiptColumns({
                onPreview: openPreview,
                onStartSession: startSession,
                canStartSession: canStartAssignedSession,
                permissionReason: permissions.reason,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [
            invoiceExecutionTask?.workItemId,
            permissions.canRegisterInvoice,
            permissions.canRegisterReceipt,
            permissions.reason,
        ],
    )

    const invoiceColumns = React.useMemo(
        () =>
            createInvoiceColumns({
                onPreview: openPreview,
                onStartSession: startSession,
                canStartSession: canStartAssignedSession,
                permissionReason: permissions.reason,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [
            invoiceExecutionTask?.workItemId,
            permissions.canRegisterInvoice,
            permissions.reason,
        ],
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
                    if (!embedded && ret?.returnTo && ret.from === "W05") {
                        router.push(ret.returnTo)
                        return
                    }
                    urlState.patchUrl({ sessionId: null })
                }}
                onPosted={() => {
                    void listQuery.refetch()
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

    const metrics = data?.metrics
    const content = (
        <>
            <CustomerReceivablesHeader
                data={data}
                embedded={embedded}
                salesOrderNo={salesOrderNo}
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
                canRegisterInvoice={
                    permissions.canRegisterInvoice &&
                    Boolean(invoiceExecutionTask)
                }
                canRegisterReceipt={permissions.canRegisterReceipt}
                canExport={permissions.canExport}
                permissionReason={permissions.reason}
                invoiceBlockedReason={invoiceTaskBlockedReason}
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
                    {!listQuery.isError ? (
                        <CustomerReceivablesMetrics
                            view={urlState.view}
                            due={urlState.due}
                            metrics={metrics}
                            queriedAt={data?.queriedAt}
                            patchUrl={urlState.patchUrl}
                        />
                    ) : null}

                    <CustomerReceivablesTable
                        view={urlState.view}
                        data={data}
                        isPending={listQuery.isPending}
                        isError={listQuery.isError}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        metrics={metrics}
                        pagination={urlState.pagination}
                        receivableColumns={receivableColumns}
                        receiptColumns={receiptColumns}
                        invoiceColumns={invoiceColumns}
                        toolbar={
                            <CustomerReceivablesToolbar
                                view={urlState.view}
                                searchDraft={urlState.searchDraft}
                                setSearchDraft={urlState.setSearchDraft}
                                searchInputRef={urlState.searchInputRef}
                                counterpartyPartyIdDraft={
                                    urlState.counterpartyPartyIdDraft
                                }
                                setCounterpartyPartyIdDraft={
                                    urlState.setCounterpartyPartyIdDraft
                                }
                                dueDraft={urlState.dueDraft}
                                setDueDraft={urlState.setDueDraft}
                                statusDraft={urlState.statusDraft}
                                setStatusDraft={urlState.setStatusDraft}
                                reviewStatusDraft={urlState.reviewStatusDraft}
                                setReviewStatusDraft={
                                    urlState.setReviewStatusDraft
                                }
                                panelOpen={urlState.panelOpen}
                                setPanelOpen={urlState.setPanelOpen}
                                hasStructuredFilters={
                                    urlState.hasStructuredFilters
                                }
                                hasActiveFilters={urlState.hasActiveFilters}
                                appliedChips={appliedChips}
                                removeFilter={urlState.removeFilter}
                                applyFilters={urlState.applyFilters}
                                resetMoreFilters={urlState.resetMoreFilters}
                                clearFilters={urlState.clearFilters}
                            />
                        }
                        patchUrl={urlState.patchUrl}
                        onPaginationChange={urlState.handlePaginationChange}
                        clearFilters={urlState.clearFilters}
                    />
                </>
            )}

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
                onStartSession={startSession}
                canStartSession={permissions.canStartSession}
                canRequestReverse={permissions.canReverse}
                canSubmitRefund={permissions.canSubmitRefund}
                canSubmitReversal={permissions.canSubmitReversal}
                permissionReason={permissions.reason}
                workItemId={focusedWorkItem?.workItemId}
                expectedTaskVersion={focusedWorkItem?.taskVersion}
                workItemAllowedActions={focusedWorkItem?.allowedActions}
                onDecisionApplied={(view: ApprovalCommandView) => {
                    void detailQuery.refetch()
                    void listQuery.refetch()
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
                createPending={createSession.isPending}
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
                pending={reverseFlow.refundDraftPending}
                sourceLabel={reverseFlow.reverseConfirm?.label}
                amount={reverseFlow.reverseConfirm?.amount}
                onOpenChange={(open) => {
                    if (!open) {
                        reverseFlow.setReverseConfirm(null)
                        reverseFlow.setReverseReason("")
                    }
                }}
                onSubmit={(reason) => {
                    if (!permissions.canRefund) {
                        setActionError(permissions.reason)
                        return
                    }
                    void reverseFlow.prepareRefundDraft(reason)
                }}
            />

            <CustomerRefundSubmitConfirmDialog
                open={reverseFlow.refundSubmitOpen}
                pending={reverseFlow.refundSubmitPending}
                approval={
                    reverseFlow.refundDraft?.approval ??
                    detailQuery.data?.refund?.approval
                }
                onOpenChange={reverseFlow.setRefundSubmitOpen}
                onConfirm={() => {
                    if (!permissions.canSubmitRefund) {
                        setActionError(permissions.reason)
                        return
                    }
                    void reverseFlow.confirmRefundSubmit()
                }}
            />

            <ReceiptReversalRequestDialog
                open={reverseFlow.reverseConfirm?.kind === "receipt_reverse"}
                pending={reverseFlow.reversalDraftPending}
                sourceLabel={reverseFlow.reverseConfirm?.label}
                amount={reverseFlow.reverseConfirm?.amount}
                onOpenChange={(open) => {
                    if (!open) {
                        reverseFlow.setReverseConfirm(null)
                        reverseFlow.setReverseReason("")
                    }
                }}
                onSubmit={(reason) => {
                    if (!permissions.canReverseReceipt) {
                        setActionError(permissions.reason)
                        return
                    }
                    void reverseFlow.prepareReversalDraft(reason)
                }}
            />

            <ReceiptReversalSubmitConfirmDialog
                open={reverseFlow.reversalSubmitOpen}
                pending={reverseFlow.reversalSubmitPending}
                approval={
                    reverseFlow.reversalDraft?.approval ??
                    detailQuery.data?.reversal?.approval
                }
                onOpenChange={reverseFlow.setReversalSubmitOpen}
                onConfirm={() => {
                    if (!permissions.canSubmitReversal) {
                        setActionError(permissions.reason)
                        return
                    }
                    void reverseFlow.confirmReversalSubmit()
                }}
            />
        </>
    )

    if (embedded) {
        return <div className="flex flex-col gap-4">{content}</div>
    }
    return <PageScaffold density="compact">{content}</PageScaffold>
}
