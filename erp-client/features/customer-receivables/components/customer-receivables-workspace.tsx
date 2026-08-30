"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { FormalActionResult, PageScaffold } from "@/components/business"
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
import { CustomerReceivablesListPane } from "@/features/customer-receivables/pages/components/customer-receivables-list-pane"
import { SalesOrderReturnAlert } from "@/features/customer-receivables/pages/components/sales-order-return-alert"
import { useAutoAllocationSession } from "@/features/customer-receivables/pages/hooks/use-auto-allocation-session"
import { useCustomerReceivablesAppliedChips } from "@/features/customer-receivables/pages/hooks/use-customer-receivables-applied-chips"
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

    const appliedChips = useCustomerReceivablesAppliedChips({
        data,
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

            <CustomerReceivablesListPane
                data={data}
                urlState={urlState}
                appliedChips={appliedChips}
                isPending={listQuery.isPending}
                isError={listQuery.isError}
                error={listQuery.error}
                onRetry={() => void listQuery.refetch()}
                receivableColumns={receivableColumns}
                receiptColumns={receiptColumns}
                invoiceColumns={invoiceColumns}
            />

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
