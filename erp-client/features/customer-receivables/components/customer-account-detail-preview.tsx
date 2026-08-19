"use client"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
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
    onStartSession: (
        mode: AllocationMode,
        partyId: string,
        existingFactId?: string,
        target?: AllocationTarget,
    ) => void | Promise<void>
    onRequestReverse: (request: ReverseRequest) => void
    onRequestRefundSubmit?: () => void
    onRequestReversalSubmit?: () => void
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
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
    onStartSession,
    onRequestReverse,
    onRequestRefundSubmit,
    onRequestReversalSubmit,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: CustomerAccountDetailPreviewProps) {
    return (
        <QuickPreviewSheet
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            size="detail"
            title={
                data?.receivable
                    ? data.receivable.salesOrderNo
                    : data?.receipt
                      ? data.receipt.receiptNo
                      : data?.invoice
                        ? data.invoice.invoiceNo
                        : data?.refund
                          ? data.refund.refundNo
                          : data?.reversal
                            ? data.reversal.reversalNo
                            : "往来详情"
            }
            identity={
                data?.receivable ? (
                    <span>{data.receivable.counterpartyPartyName}</span>
                ) : data?.receipt ? (
                    <span>{data.receipt.counterpartyPartyName}</span>
                ) : data?.invoice ? (
                    <span>{data.invoice.counterpartyPartyName}</span>
                ) : data?.refund ? (
                    <span>{data.refund.refundNo}</span>
                ) : data?.reversal ? (
                    <span>{data.reversal.reversalNo}</span>
                ) : null
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
                        <Badge>{data.invoice.invoiceKindLabel}</Badge>
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
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {data.receivable ? (
                            <Button
                                type="button"
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
                                登记回款并核销
                            </Button>
                        ) : null}
                        {data.receipt?.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        ) ? (
                            <Button
                                type="button"
                                onClick={() =>
                                    void onStartSession(
                                        "receipt",
                                        data.receipt!.counterpartyPartyId,
                                        data.receipt!.receiptId,
                                    )
                                }
                            >
                                继续核销
                            </Button>
                        ) : null}
                        {data.receipt?.allowedActions.includes(
                            "REVERSE_RECEIPT",
                        ) ? (
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() =>
                                    onRequestReverse({
                                        kind: "receipt_reverse",
                                        sourceFactId: data.receipt!.receiptId,
                                        label: data.receipt!.receiptNo,
                                        amount: data.receipt!.amount,
                                    })
                                }
                            >
                                冲正
                            </Button>
                        ) : null}
                        {data.receipt?.allowedActions.includes("REFUND") ? (
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() =>
                                    onRequestReverse({
                                        kind: "refund",
                                        sourceFactId: data.receipt!.receiptId,
                                        label: data.receipt!.receiptNo,
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
                                type="button"
                                onClick={() =>
                                    void onStartSession(
                                        "invoice",
                                        data.invoice!.counterpartyPartyId,
                                        data.invoice!.invoiceId,
                                    )
                                }
                            >
                                继续分配
                            </Button>
                        ) : null}
                        {data.refund &&
                        isUnsubmittedCustomerRefundStatus(data.refund.status) &&
                        data.refund.approval?.allowedActions.includes(
                            "SUBMIT",
                        ) &&
                        onRequestRefundSubmit ? (
                            <Button
                                type="button"
                                onClick={onRequestRefundSubmit}
                            >
                                提交审批
                            </Button>
                        ) : null}
                        {data.reversal &&
                        isUnsubmittedReceiptReversalStatus(
                            data.reversal.status,
                        ) &&
                        data.reversal.approval?.allowedActions.includes(
                            "SUBMIT",
                        ) &&
                        onRequestReversalSubmit ? (
                            <Button
                                type="button"
                                onClick={onRequestReversalSubmit}
                            >
                                提交审批
                            </Button>
                        ) : null}
                        {data.invoice?.allowedActions.includes(
                            "ISSUE_RED_INVOICE",
                        ) ? (
                            <Button
                                type="button"
                                variant="outline"
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
                <div className="space-y-3 p-6">
                    <div className="h-24 animate-pulse rounded-xl bg-muted" />
                    <div className="h-40 animate-pulse rounded-xl bg-muted" />
                </div>
            ) : isError ? (
                <div className="space-y-3 p-6">
                    <p className="text-sm text-muted-foreground">
                        {getErrorMessage(error, "详情加载失败，请重试。")}
                    </p>
                    <Button
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
                <div className="p-6 text-sm text-muted-foreground">
                    未找到该笔记录，可能已超出当前数据范围。
                </div>
            )}
        </QuickPreviewSheet>
    )
}
