import * as React from "react"

import {
    BusinessStatusBadge,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type {
    AllocationMode,
    CustomerAccountsDetailView,
    ReceiptRow,
    ReceivableAccountRow,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { getErrorMessage } from "@/lib/api/errors"
import { formatDateTime } from "@/lib/datetime"

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

function ReceivableDetailBody({ row }: { row: ReceivableAccountRow }) {
    return (
        <div className="space-y-5 overflow-auto p-6">
            <div className="grid grid-cols-2 gap-3">
                <Fact label="往来主体" value={row.counterpartyPartyName} />
                <Fact label="经营归属客户" value={row.customerName} />
                <Fact label="销售单" value={row.salesOrderNo} mono />
                <Fact label="业务性质" value={row.businessTypeLabel} />
                <Fact
                    label="应收总额"
                    value={
                        <MoneyValue value={row.grossTotal} taxBasis="gross" />
                    }
                />
                <Fact
                    label="开放应收"
                    value={
                        <MoneyValue value={row.openTotal} taxBasis="gross" />
                    }
                />
                <Fact
                    label="已核销回款"
                    value={
                        <MoneyValue value={row.settledTotal} taxBasis="gross" />
                    }
                />
                <Fact
                    label="净已开票"
                    value={
                        <MoneyValue
                            value={row.invoicedTotal}
                            taxBasis="gross"
                        />
                    }
                />
                <Fact label="到期日" value={row.dueDate} mono />
                <Fact label="复核" value={row.reviewStatusLabel} />
            </div>
            <p className="text-xs text-muted-foreground">
                回款进度与开票进度独立；不可用开票状态推断结清。
            </p>
            <section>
                <h4 className="mb-2 text-sm font-semibold">不可变分录</h4>
                <ul className="space-y-2">
                    {row.entries.map((entry) => (
                        <li
                            key={entry.entryId}
                            className="rounded-xl border px-3 py-2 text-sm"
                        >
                            <div className="flex justify-between gap-2">
                                <span>
                                    {entry.entryType} ·{" "}
                                    {entry.direction === "increase"
                                        ? "增加"
                                        : "减少"}
                                </span>
                                <MoneyValue
                                    value={entry.amountGross}
                                    taxBasis="gross"
                                />
                            </div>
                            <div className="text-xs text-muted-foreground">
                                {entry.sourceLabel} · 到期 {entry.dueDate}
                            </div>
                        </li>
                    ))}
                </ul>
            </section>
        </div>
    )
}

function ReceiptDetailBody({ row }: { row: ReceiptRow }) {
    return (
        <div className="space-y-5 overflow-auto p-6">
            <Alert variant="info">
                <AlertTitle>已确认记录只读</AlertTitle>
                <AlertDescription>
                    已确认记录不可编辑、不可删除；纠错仅能追加退款/冲正。
                </AlertDescription>
            </Alert>
            <div className="grid grid-cols-2 gap-3">
                <Fact label="回款单号" value={row.receiptNo} mono />
                <Fact label="往来主体" value={row.counterpartyPartyName} />
                <Fact
                    label="到账时间"
                    value={formatDateTime(
                        row.receivedAt,
                        "full",
                        "passthrough",
                    )}
                    mono
                />
                <Fact
                    label="到账金额"
                    value={<MoneyValue value={row.amount} taxBasis="gross" />}
                />
                <Fact label="银行引用" value={row.bankReferenceMasked} mono />
                <Fact
                    label="净已分配"
                    value={
                        <MoneyValue
                            value={row.allocatedTotal}
                            taxBasis="gross"
                        />
                    }
                />
                <Fact
                    label="未分配"
                    value={
                        <MoneyValue
                            value={row.unallocatedAmount}
                            taxBasis="gross"
                        />
                    }
                />
            </div>
            <section>
                <h4 className="mb-2 text-sm font-semibold">
                    分配明细（新增不覆盖原金额）
                </h4>
                {row.allocations.length === 0 ? (
                    <p className="text-sm text-muted-foreground">尚无分配行</p>
                ) : (
                    <ul className="space-y-2">
                        {row.allocations.map((allocation) => (
                            <li
                                key={allocation.allocationId}
                                className="rounded-xl border px-3 py-2 text-sm"
                            >
                                <div className="flex justify-between gap-2">
                                    <span>
                                        <Badge
                                            variant={
                                                allocation.action === "REVERSE"
                                                    ? "warning"
                                                    : "secondary"
                                            }
                                        >
                                            {allocation.action}
                                        </Badge>{" "}
                                        {allocation.targetLabel}
                                    </span>
                                    <MoneyValue
                                        value={allocation.amountGross}
                                    />
                                </div>
                                <div className="text-xs text-muted-foreground">
                                    {formatDateTime(
                                        allocation.occurredAt,
                                        "full",
                                        "passthrough",
                                    )}
                                    {allocation.reverseOfAllocationId
                                        ? " · 冲减原分配"
                                        : null}
                                </div>
                            </li>
                        ))}
                    </ul>
                )}
            </section>
        </div>
    )
}

function InvoiceDetailBody({ row }: { row: SalesInvoiceRow }) {
    return (
        <div className="space-y-5 overflow-auto p-6">
            <Alert variant="info">
                <AlertTitle>已登记发票只读</AlertTitle>
                <AlertDescription>
                    已登记发票不可编辑、不可删除；红票为独立记录加反向分配。
                </AlertDescription>
            </Alert>
            <div className="grid grid-cols-2 gap-3">
                <Fact label="发票号码" value={row.invoiceNo} mono />
                <Fact label="种类" value={row.invoiceKindLabel} />
                <Fact label="代码" value={row.invoiceCode ?? "—"} mono />
                <Fact label="开票日期" value={row.invoiceDate} mono />
                <Fact
                    label="含税"
                    value={
                        <MoneyValue value={row.grossAmount} taxBasis="gross" />
                    }
                />
                <Fact
                    label="不含税 / 税额"
                    value={
                        <span>
                            <MoneyValue value={row.netAmount} /> /{" "}
                            <MoneyValue value={row.taxAmount} />
                        </span>
                    }
                />
                <Fact
                    label="净已分配"
                    value={
                        <MoneyValue
                            value={row.allocatedTotal}
                            taxBasis="gross"
                        />
                    }
                />
                <Fact
                    label="未分配"
                    value={
                        <MoneyValue
                            value={row.unallocatedAmount}
                            taxBasis="gross"
                        />
                    }
                />
            </div>
            <section>
                <h4 className="mb-2 text-sm font-semibold">
                    分配明细（独立于回款）
                </h4>
                {row.allocations.length === 0 ? (
                    <p className="text-sm text-muted-foreground">尚无分配行</p>
                ) : (
                    <ul className="space-y-2">
                        {row.allocations.map((allocation) => (
                            <li
                                key={allocation.allocationId}
                                className="rounded-xl border px-3 py-2 text-sm"
                            >
                                <div className="flex justify-between gap-2">
                                    <span>
                                        <Badge
                                            variant={
                                                allocation.action === "REVERSE"
                                                    ? "warning"
                                                    : "secondary"
                                            }
                                        >
                                            {allocation.action === "REVERSE"
                                                ? "反向记录"
                                                : "分配"}
                                        </Badge>{" "}
                                        {allocation.targetLabel}
                                    </span>
                                    <MoneyValue
                                        value={allocation.amountGross}
                                    />
                                </div>
                            </li>
                        ))}
                    </ul>
                )}
            </section>
        </div>
    )
}

function Fact({
    label,
    value,
    mono,
}: {
    label: string
    value: React.ReactNode
    mono?: boolean
}) {
    return (
        <div>
            <div className="text-xs text-muted-foreground">{label}</div>
            <div
                className={
                    mono ? "num text-sm font-medium" : "text-sm font-medium"
                }
            >
                {value}
            </div>
        </div>
    )
}
