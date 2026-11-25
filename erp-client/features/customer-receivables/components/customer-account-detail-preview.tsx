"use client"

import Link from "next/link"
import { customerDocumentHref, salesOrderHref } from "../lib/source-documents"
import { ArrowUpRightIcon } from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { TaskActionSheet } from "@/components/business/task-action-sheet"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import {
    CustomerRefundDetailBody,
    InvoiceDetailBody,
    ReceiptDetailBody,
    ReceiptReversalDetailBody,
    ReceivableDetailBody,
} from "@/features/customer-receivables/components/detail-bodies"
import {
    customerRefundApprovalPhase,
    isUnsubmittedCustomerRefundStatus,
} from "@/features/customer-receivables/lib/customer-refund-approval"
import {
    isUnsubmittedReceiptReversalStatus,
    receiptReversalApprovalPhase,
} from "@/features/customer-receivables/lib/receipt-reversal-approval"
import type {
    AllocationMode,
    CustomerAccountsDetailView,
} from "@/features/customer-receivables/types"
import { CustomerReceiptApprovalArea } from "./customer-receipt-approval-area"
import { CustomerRefundApprovalArea } from "./customer-refund-approval-area"
import { ReceiptReversalApprovalArea } from "./receipt-reversal-approval-area"
import { customerReceiptApprovalPhase } from "../lib/customer-receipt-approval"
import { getErrorMessage } from "@/lib/api/errors"

export type ReverseRequest = Readonly<{
    kind: "receipt_reverse" | "refund" | "red_invoice"
    sourceFactId: string
    label: string
    amount?: string
}>

type AllocationTarget = Readonly<{
    salesOrderId?: string
    receivableAccountId?: string
}>

type CustomerAccountDetailPreviewProps = Readonly<{
    open: boolean
    data?: CustomerAccountsDetailView | null
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    onClose: () => void
    onClosed?: () => void
    onStartSession: (
        mode: AllocationMode,
        partyId: string,
        existingFactId?: string,
        target?: AllocationTarget,
    ) => void | Promise<void>
    canStartSession?: (mode: AllocationMode) => boolean
    startSessionPending?: boolean
    onRequestReverse: (request: ReverseRequest) => void
    canRequestReverse?: (kind: ReverseRequest["kind"]) => boolean
    onRequestRefundSubmit?: () => void
    onRequestReversalSubmit?: () => void
    canSubmitRefund?: boolean
    canSubmitReversal?: boolean
    permissionReason?: string
    invoiceSessionBlockedReason?: string
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
    /** 销售单对象中心只读本单事实；冲正/退款/红票留在财务工作台。默认展示。 */
    showCorrectionActions?: boolean
}>

/**
 * 客户往来详情抽屉。回款、客户退款与回款冲正嵌入通用审批区；决定与恢复只读服务端白名单。
 * 发票分支只渲染 InvoiceDetailBody，不展示审批流程选择或审批动作。
 */
export function CustomerAccountDetailPreview({
    open,
    data,
    isPending,
    isError,
    error,
    onRetry,
    onClose,
    onClosed,
    onStartSession,
    canStartSession = () => true,
    startSessionPending = false,
    onRequestReverse,
    canRequestReverse = () => true,
    onRequestRefundSubmit,
    onRequestReversalSubmit,
    canSubmitRefund = true,
    canSubmitReversal = true,
    permissionReason,
    invoiceSessionBlockedReason,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
    showCorrectionActions = true,
}: CustomerAccountDetailPreviewProps) {
    const originalReceiptId =
        data?.refund?.originalReceiptId ?? data?.reversal?.originalReceiptId
    const originalHref = data?.receivable?.salesOrderId
        ? salesOrderHref(data.receivable.salesOrderId)
        : originalReceiptId
          ? customerDocumentHref("receipt", originalReceiptId)
          : data?.invoice?.originalInvoiceId
            ? customerDocumentHref("invoice", data.invoice.originalInvoiceId)
            : undefined
    const originalLabel = data?.receivable
        ? "打开销售单"
        : originalReceiptId
          ? "打开原回款"
          : "打开原发票"
    return (
        <TaskActionSheet
            idPrefix="customer-receivables-preview-sheet"
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            onOpenChangeComplete={(nextOpen) => {
                if (!nextOpen) onClosed?.()
            }}
            title={
                data?.receivable?.counterpartyPartyName ??
                data?.receipt?.counterpartyPartyName ??
                data?.invoice?.counterpartyPartyName ??
                (data?.refund
                    ? "客户退款"
                    : data?.reversal
                      ? "回款冲正"
                      : "往来详情")
            }
            subtitle={
                data?.receivable ? (
                    <span className="num">
                        销售单：{data.receivable.salesOrderNo}
                    </span>
                ) : data?.receipt ? (
                    <span className="num">
                        回款单：{data.receipt.receiptNo}
                    </span>
                ) : data?.invoice ? (
                    <span className="num">
                        发票号码：{data.invoice.invoiceNo}
                    </span>
                ) : data?.refund ? (
                    <span className="num">退款单：{data.refund.refundNo}</span>
                ) : data?.reversal ? (
                    <span className="num">
                        冲正单：{data.reversal.reversalNo}
                    </span>
                ) : undefined
            }
            badges={
                data?.receivable ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={data.receivable.statusLabel}
                        tone={data.receivable.statusTone}
                    />
                ) : data?.receipt ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={data.receipt.statusLabel}
                        tone={data.receipt.statusTone}
                    />
                ) : data?.invoice ? (
                    <div className="flex items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={data.invoice.statusLabel}
                            tone={data.invoice.statusTone}
                        />
                        <span className="text-xs text-muted-foreground">
                            {data.invoice.invoiceKindLabel}
                        </span>
                    </div>
                ) : data?.refund ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={data.refund.statusLabel}
                        tone={data.refund.statusTone}
                    />
                ) : data?.reversal ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={data.reversal.statusLabel}
                        tone={data.reversal.statusTone}
                    />
                ) : null
            }
            footerLeading={
                data ? (
                    <>
                        <Button
                            id="customer-receivables-preview-close"
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {originalHref ? (
                            <Button
                                id="customer-receivables-preview-open-original"
                                type="button"
                                variant="outline"
                                render={<Link href={originalHref} />}
                            >
                                {originalLabel}
                                <ArrowUpRightIcon data-icon="inline-end" />
                            </Button>
                        ) : null}
                    </>
                ) : null
            }
            footerActions={
                data ? (
                    <>
                        {data.receivable && canStartSession("receipt") ? (
                            <LoadingButton
                                id="customer-receivables-preview-receivable-register-receipt"
                                type="button"
                                loading={startSessionPending}
                                disabled={
                                    startSessionPending ||
                                    !canStartSession("receipt") ||
                                    !data.receivable!.allowedActions.includes(
                                        "REGISTER_RECEIPT",
                                    )
                                }
                                title={
                                    !canStartSession("receipt")
                                        ? permissionReason
                                        : data.receivable!.allowedActions.includes(
                                                "REGISTER_RECEIPT",
                                            )
                                          ? undefined
                                          : "当前不能登记回款并核销"
                                }
                                onClick={() =>
                                    void onStartSession(
                                        "receipt",
                                        data.receivable!.counterpartyPartyId,
                                        undefined,
                                        {
                                            salesOrderId:
                                                data.receivable!.salesOrderId,
                                            receivableAccountId:
                                                data.receivable!.accountId,
                                        },
                                    )
                                }
                            >
                                {startSessionPending
                                    ? "创建中…"
                                    : "登记回款并核销"}
                            </LoadingButton>
                        ) : null}
                        {data.receipt?.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        ) && canStartSession("receipt") ? (
                            <LoadingButton
                                id="customer-receivables-preview-receipt-continue-allocate"
                                type="button"
                                loading={startSessionPending}
                                disabled={
                                    startSessionPending ||
                                    !canStartSession("receipt")
                                }
                                title={
                                    canStartSession("receipt")
                                        ? undefined
                                        : permissionReason
                                }
                                onClick={() =>
                                    void onStartSession(
                                        "receipt",
                                        data.receipt!.counterpartyPartyId,
                                        data.receipt!.receiptId,
                                    )
                                }
                            >
                                {startSessionPending ? "创建中…" : "继续核销"}
                            </LoadingButton>
                        ) : null}
                        {showCorrectionActions &&
                        data.receipt?.allowedActions.includes(
                            "REVERSE_RECEIPT",
                        ) &&
                        canRequestReverse("receipt_reverse") ? (
                            <Button
                                id="customer-receivables-preview-receipt-reverse"
                                type="button"
                                variant="outline"
                                disabled={!canRequestReverse("receipt_reverse")}
                                title={
                                    canRequestReverse("receipt_reverse")
                                        ? undefined
                                        : permissionReason
                                }
                                onClick={() =>
                                    onRequestReverse({
                                        kind: "receipt_reverse",
                                        sourceFactId: data.receipt!.receiptId,
                                        label: `${data.receipt!.receiptNo} · ${data.receipt!.counterpartyPartyName}`,
                                        amount: data.receipt!.amount,
                                    })
                                }
                            >
                                冲正
                            </Button>
                        ) : null}
                        {showCorrectionActions &&
                        data.receipt?.allowedActions.includes("REFUND") &&
                        canRequestReverse("refund") ? (
                            <Button
                                id="customer-receivables-preview-receipt-refund"
                                type="button"
                                variant="outline"
                                disabled={!canRequestReverse("refund")}
                                title={
                                    canRequestReverse("refund")
                                        ? undefined
                                        : permissionReason
                                }
                                onClick={() =>
                                    onRequestReverse({
                                        kind: "refund",
                                        sourceFactId: data.receipt!.receiptId,
                                        label: `${data.receipt!.receiptNo} · ${data.receipt!.counterpartyPartyName}`,
                                        amount: data.receipt!.amount,
                                    })
                                }
                            >
                                退款
                            </Button>
                        ) : null}
                        {data.invoice?.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        ) && canStartSession("invoice") ? (
                            <LoadingButton
                                id="customer-receivables-preview-invoice-continue-allocate"
                                type="button"
                                loading={startSessionPending}
                                disabled={
                                    startSessionPending ||
                                    !canStartSession("invoice")
                                }
                                title={
                                    canStartSession("invoice")
                                        ? undefined
                                        : (invoiceSessionBlockedReason ??
                                          permissionReason)
                                }
                                onClick={() =>
                                    void onStartSession(
                                        "invoice",
                                        data.invoice!.counterpartyPartyId,
                                        data.invoice!.invoiceId,
                                    )
                                }
                            >
                                {startSessionPending ? "创建中…" : "继续分配"}
                            </LoadingButton>
                        ) : null}
                        {showCorrectionActions &&
                        data.invoice?.allowedActions.includes(
                            "ISSUE_RED_INVOICE",
                        ) &&
                        canRequestReverse("red_invoice") ? (
                            <Button
                                id="customer-receivables-preview-invoice-red"
                                type="button"
                                variant="outline"
                                disabled={!canRequestReverse("red_invoice")}
                                title={
                                    canRequestReverse("red_invoice")
                                        ? undefined
                                        : permissionReason
                                }
                                onClick={() =>
                                    onRequestReverse({
                                        kind: "red_invoice",
                                        sourceFactId: data.invoice!.invoiceId,
                                        label: data.invoice!.invoiceNo,
                                        amount: data.invoice!.allocatedTotal,
                                    })
                                }
                            >
                                红票
                            </Button>
                        ) : null}
                        {data.receipt ? (
                            <CustomerReceiptApprovalArea
                                presentation="actions"
                                phase={customerReceiptApprovalPhase(
                                    data.receipt.approval,
                                    data.receipt.status === "in_approval"
                                        ? "IN_APPROVAL"
                                        : data.receipt.status,
                                )}
                                approval={data.receipt.approval}
                                documentId={data.receipt.receiptId}
                                documentVersion={data.receipt.baselineVersion}
                                workItemId={workItemId}
                                expectedTaskVersion={expectedTaskVersion}
                                workItemAllowedActions={workItemAllowedActions}
                                onDecisionApplied={onDecisionApplied}
                            />
                        ) : null}
                        {data.refund ? (
                            <CustomerRefundApprovalArea
                                presentation="actions"
                                phase={customerRefundApprovalPhase(
                                    data.refund.approval,
                                    data.refund.status === "in_approval"
                                        ? "IN_APPROVAL"
                                        : data.refund.status,
                                )}
                                approval={data.refund.approval}
                                documentId={data.refund.refundId}
                                documentVersion={data.refund.baselineVersion}
                                workItemId={workItemId}
                                expectedTaskVersion={expectedTaskVersion}
                                workItemAllowedActions={workItemAllowedActions}
                                onDecisionApplied={onDecisionApplied}
                            />
                        ) : null}
                        {showCorrectionActions &&
                        data.refund &&
                        isUnsubmittedCustomerRefundStatus(data.refund.status) &&
                        data.refund.approval?.allowedActions.includes(
                            "SUBMIT",
                        ) &&
                        onRequestRefundSubmit &&
                        canSubmitRefund ? (
                            <Button
                                id="customer-receivables-preview-refund-submit"
                                type="button"
                                disabled={!canSubmitRefund}
                                title={
                                    canSubmitRefund
                                        ? undefined
                                        : permissionReason
                                }
                                onClick={onRequestRefundSubmit}
                            >
                                提交审批
                            </Button>
                        ) : null}
                        {data.reversal ? (
                            <ReceiptReversalApprovalArea
                                presentation="actions"
                                phase={receiptReversalApprovalPhase(
                                    data.reversal.approval,
                                    data.reversal.status === "in_approval"
                                        ? "IN_APPROVAL"
                                        : data.reversal.status,
                                )}
                                approval={data.reversal.approval}
                                documentId={data.reversal.reversalId}
                                documentVersion={data.reversal.baselineVersion}
                                workItemId={workItemId}
                                expectedTaskVersion={expectedTaskVersion}
                                workItemAllowedActions={workItemAllowedActions}
                                onDecisionApplied={onDecisionApplied}
                            />
                        ) : null}
                        {showCorrectionActions &&
                        data.reversal &&
                        isUnsubmittedReceiptReversalStatus(
                            data.reversal.status,
                        ) &&
                        data.reversal.approval?.allowedActions.includes(
                            "SUBMIT",
                        ) &&
                        onRequestReversalSubmit &&
                        canSubmitReversal ? (
                            <Button
                                id="customer-receivables-preview-reversal-submit"
                                type="button"
                                disabled={!canSubmitReversal}
                                title={
                                    canSubmitReversal
                                        ? undefined
                                        : permissionReason
                                }
                                onClick={onRequestReversalSubmit}
                            >
                                提交审批
                            </Button>
                        ) : null}
                    </>
                ) : null
            }
        >
            {isPending ? (
                <div className="space-y-4">
                    <div className="h-24 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                </div>
            ) : isError ? (
                <div className="space-y-3">
                    <p className="text-xs text-muted-foreground">
                        {getErrorMessage(error, "详情加载失败，请重试。")}
                    </p>
                    <Button
                        id="customer-receivables-preview-retry"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                </div>
            ) : data?.receivable ? (
                <ReceivableDetailBody
                    row={data.receivable}
                    className="space-y-6 text-body-compact"
                />
            ) : data?.receipt ? (
                <ReceiptDetailBody
                    approvalActionsInFooter
                    row={data.receipt}
                    className="space-y-6 text-body-compact"
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            ) : data?.refund ? (
                <CustomerRefundDetailBody
                    approvalActionsInFooter
                    row={data.refund}
                    className="space-y-6 text-body-compact"
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            ) : data?.reversal ? (
                <ReceiptReversalDetailBody
                    approvalActionsInFooter
                    row={data.reversal}
                    className="space-y-6 text-body-compact"
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            ) : data?.invoice ? (
                <InvoiceDetailBody
                    row={data.invoice}
                    className="space-y-6 text-body-compact"
                />
            ) : (
                <div className="text-body-compact text-muted-foreground">
                    未找到该笔记录，可能已超出当前数据范围。
                </div>
            )}
        </TaskActionSheet>
    )
}
