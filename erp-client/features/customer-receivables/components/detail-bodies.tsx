"use client"

import * as React from "react"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { CustomerReceiptApprovalArea } from "@/features/customer-receivables/components/customer-receipt-approval-area"
import { customerReceiptApprovalPhase } from "@/features/customer-receivables/lib/customer-receipt-approval"
import type {
    ReceiptRow,
    ReceivableAccountRow,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { formatDateTime } from "@/lib/datetime"

export function ReceivableDetailBody({ row }: { row: ReceivableAccountRow }) {
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

/**
 * 客户回款详情。草稿展示绑定卡，运行中/终态嵌入通用审批区。
 */
export function ReceiptDetailBody({
    row,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    row: ReceiptRow
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const posted = row.status === "posted" || row.status === "reversed"
    return (
        <div className="space-y-5 overflow-auto p-6">
            {posted ? (
                <Alert variant="info">
                    <AlertTitle>已过账记录只读</AlertTitle>
                    <AlertDescription>
                        已过账记录不可编辑、不可删除；纠错仅能追加退款/冲正。
                    </AlertDescription>
                </Alert>
            ) : null}
            <CustomerReceiptApprovalArea
                phase={customerReceiptApprovalPhase(
                    row.approval,
                    row.status === "in_approval" ? "IN_APPROVAL" : row.status,
                )}
                approval={row.approval}
                documentId={row.receiptId}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                workItemAllowedActions={workItemAllowedActions}
                onDecisionApplied={onDecisionApplied}
            />
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

export function InvoiceDetailBody({ row }: { row: SalesInvoiceRow }) {
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
