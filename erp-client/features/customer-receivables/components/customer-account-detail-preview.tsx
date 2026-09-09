"use client"

import Link from "next/link"
import { customerDocumentHref, salesOrderHref } from "../lib/source-documents"
import { ArrowUpRightIcon, LoaderCircleIcon } from "lucide-react"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import {
    CustomerRefundDetailBody,
    InvoiceDetailBody,
    ReceiptDetailBody,
    ReceiptReversalDetailBody,
    ReceivableDetailBody,
} from "@/features/customer-receivables/components/detail-bodies"
import { isUnsubmittedCustomerRefundStatus } from "@/features/customer-receivables/lib/customer-refund-approval"
import { isUnsubmittedReceiptReversalStatus } from "@/features/customer-receivables/lib/receipt-reversal-approval"
import type {
    AllocationMode,
    CustomerAccountsDetailView,
} from "@/features/customer-receivables/types"
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
        <QuickPreviewSheet
            id="customer-receivables-preview-sheet"
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            onOpenChangeComplete={(nextOpen) => {
                if (!nextOpen) onClosed?.()
            }}
            size="detail"
            contentClassName="data-[side=right]:sm:w-[480px] data-[side=right]:sm:max-w-[480px]"
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
            identity={
                data?.receivable
                    ? `销售单：${data.receivable.salesOrderNo}`
                    : data?.receipt
                      ? `回款单：${data.receipt.receiptNo}`
                      : data?.invoice
                        ? `发票号码：${data.invoice.invoiceNo}`
                        : data?.refund
                          ? `退款单：${data.refund.refundNo}`
                          : data?.reversal
                            ? `冲正单：${data.reversal.reversalNo}`
                            : undefined
            }
            summary={
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
                    <div className="flex gap-2">
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
            footer={
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
                        {data.receivable ? (
                            <Button
                                id="customer-receivables-preview-receivable-register-receipt"
                                type="button"
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
                                {startSessionPending ? (
                                    <LoaderCircleIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                        className="animate-spin"
                                    />
                                ) : null}
                                {startSessionPending
                                    ? "创建中…"
                                    : "登记回款并核销"}
                            </Button>
                        ) : null}
                        {data.receipt?.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        ) ? (
                            <Button
                                id="customer-receivables-preview-receipt-continue-allocate"
                                type="button"
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
                                {startSessionPending ? (
                                    <LoaderCircleIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                        className="animate-spin"
                                    />
                                ) : null}
                                {startSessionPending ? "创建中…" : "继续核销"}
                            </Button>
                        ) : null}
                        {showCorrectionActions &&
                        data.receipt?.allowedActions.includes(
                            "REVERSE_RECEIPT",
                        ) ? (
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
                        data.receipt?.allowedActions.includes("REFUND") ? (
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
                        ) ? (
                            <Button
                                id="customer-receivables-preview-invoice-continue-allocate"
                                type="button"
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
                                {startSessionPending ? (
                                    <LoaderCircleIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                        className="animate-spin"
                                    />
                                ) : null}
                                {startSessionPending ? "创建中…" : "继续分配"}
                            </Button>
                        ) : null}
                        {showCorrectionActions &&
                        data.refund &&
                        isUnsubmittedCustomerRefundStatus(data.refund.status) &&
                        data.refund.approval?.allowedActions.includes(
                            "SUBMIT",
                        ) &&
                        onRequestRefundSubmit ? (
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
                        {showCorrectionActions &&
                        data.reversal &&
                        isUnsubmittedReceiptReversalStatus(
                            data.reversal.status,
                        ) &&
                        data.reversal.approval?.allowedActions.includes(
                            "SUBMIT",
                        ) &&
                        onRequestReversalSubmit ? (
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
                        {showCorrectionActions &&
                        data.invoice?.allowedActions.includes(
                            "ISSUE_RED_INVOICE",
                        ) ? (
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
                    </>
                ) : null
            }
        >
            {isPending ? (
                <div className="space-y-3 px-7 py-6">
                    <div className="h-24 animate-pulse rounded-xl bg-muted" />
                    <div className="h-40 animate-pulse rounded-xl bg-muted" />
                </div>
            ) : isError ? (
                <div className="space-y-3 px-7 py-6">
                    <p className="text-sm text-muted-foreground">
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
                <ReceivableDetailBody row={data.receivable} />
            ) : data?.receipt ? (
                <ReceiptDetailBody
                    row={data.receipt}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            ) : data?.refund ? (
                <CustomerRefundDetailBody
                    row={data.refund}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            ) : data?.reversal ? (
                <ReceiptReversalDetailBody
                    row={data.reversal}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            ) : data?.invoice ? (
                <InvoiceDetailBody row={data.invoice} />
            ) : (
                <div className="px-7 py-6 text-sm text-muted-foreground">
                    未找到该笔记录，可能已超出当前数据范围。
                </div>
            )}
        </QuickPreviewSheet>
    )
}
