import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InvoiceDetailBody,
    ReceiptDetailBody,
    ReceivableDetailBody,
} from "@/features/customer-receivables/components/detail-bodies"
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
}>

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
                        : "往来详情"
            }
            identity={
                data?.receivable ? (
                    <span>{data.receivable.counterpartyPartyName}</span>
                ) : data?.receipt ? (
                    <span>{data.receipt.counterpartyPartyName}</span>
                ) : data?.invoice ? (
                    <span>{data.invoice.counterpartyPartyName}</span>
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
                <ReceiptDetailBody row={data.receipt} />
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
