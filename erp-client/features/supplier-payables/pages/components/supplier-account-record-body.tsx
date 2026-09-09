"use client"

import { SupplierSourceDocuments } from "../../components/supplier-source-documents"
import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import {
    PreviewAmount,
    PreviewSection,
    PreviewFact,
    PreviewNote,
} from "@/components/business/financial-preview"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    PaymentRow,
    PurchaseInvoiceRow,
    UnallocatedRow,
} from "../../types"

/** 未分配余额、单据依据、核销去向按决策顺序排列。 */
export function SupplierAccountRecordBody({
    payment,
    invoice,
    unallocated,
    isPayment,
    allocationReason,
    onOpenPayable,
    onOpenReversal,
}: {
    payment?: PaymentRow | null
    invoice?: PurchaseInvoiceRow | null
    unallocated?: UnallocatedRow | null
    isPayment: boolean
    allocationReason?: string
    onOpenPayable: (id: string) => void
    onOpenReversal: (id: string) => void
}) {
    const row = payment ?? invoice ?? unallocated
    if (!row) return null
    const allocations = payment
        ? payment.allocations.map((allocation) => ({
              ...allocation,
              amountGross: allocation.amount,
          }))
        : (invoice?.allocations ?? [])
    const total = payment?.amount ?? invoice?.grossAmount ?? unallocated?.amount
    const allocated = payment?.allocatedTotal ?? invoice?.allocatedTotal
    return (
        <div className="space-y-6 text-sm">
            <PreviewAmount
                label={isPayment ? "待核销付款" : "待分配金额（含税）"}
                value={row.unallocatedAmount}
            >
                <span>
                    {isPayment ? "付款金额" : "发票金额"}{" "}
                    <MoneyValue value={total} />
                </span>
                {allocated != null ? (
                    <span>
                        已核销 <MoneyValue value={allocated} />
                    </span>
                ) : null}
            </PreviewAmount>
            <PreviewSection title={isPayment ? "付款信息" : "发票资料"}>
                <dl className="space-y-3">
                    <PreviewFact label={isPayment ? "付款时间" : "开票日期"}>
                        <span className="num">
                            {formatDateTime(
                                payment?.paidAt ??
                                    invoice?.invoiceDate ??
                                    unallocated?.occurredAt,
                                "full",
                                "passthrough",
                            )}
                        </span>
                    </PreviewFact>
                    {payment ? (
                        <>
                            <PreviewFact label="银行引用">
                                <span className="num">
                                    {payment.bankReferenceMasked || "—"}
                                </span>
                            </PreviewFact>
                            <PreviewFact label="银行回单">
                                {payment.bankReceipt?.fileName ?? "未附回单"}
                            </PreviewFact>
                        </>
                    ) : null}
                    {invoice ? (
                        <>
                            <PreviewFact label="发票类型">
                                {invoice.invoiceKindLabel}
                            </PreviewFact>
                            <PreviewFact label="不含税金额">
                                <MoneyValue value={invoice.netAmount} />
                            </PreviewFact>
                            <PreviewFact label="税额">
                                <MoneyValue value={invoice.taxAmount} />
                            </PreviewFact>
                        </>
                    ) : null}
                </dl>
            </PreviewSection>
            {payment?.paymentRecipient ? (
                <PreviewSection title="收款账户">
                    <dl className="space-y-3">
                        <PreviewFact label="户名">
                            {payment.paymentRecipient.accountName}
                        </PreviewFact>
                        <PreviewFact label="开户银行">
                            {payment.paymentRecipient.bankName}
                        </PreviewFact>
                        <PreviewFact label="账号">
                            <span className="num">
                                {payment.paymentRecipient.accountNumberMasked}
                            </span>
                        </PreviewFact>
                    </dl>
                </PreviewSection>
            ) : null}
            <PreviewSection title="核销去向">
                {allocations.length ? (
                    <ul className="divide-y divide-border">
                        {allocations.map((allocation) => (
                            <li
                                key={allocation.allocationId}
                                className="py-3 first:pt-0 last:pb-0"
                            >
                                <div className="flex items-baseline justify-between gap-5">
                                    <Button
                                        id={`supplier-payables-preview-record-allocation-${toAutomationIdSegment(allocation.allocationId)}`}
                                        variant="link"
                                        className="h-auto min-w-0 justify-start whitespace-normal break-all px-0 text-left"
                                        onClick={() =>
                                            onOpenPayable(
                                                allocation.payableAccountId,
                                            )
                                        }
                                    >
                                        {allocation.sourceDocumentNo}
                                    </Button>
                                    <MoneyValue
                                        value={allocation.amountGross}
                                        className="shrink-0"
                                    />
                                </div>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    {allocation.action === "REVERSE"
                                        ? "撤销核销"
                                        : "核销"}{" "}
                                    ·{" "}
                                    <span className="num">
                                        {formatDateTime(
                                            allocation.occurredAt,
                                            "full",
                                            "passthrough",
                                        )}
                                    </span>
                                </p>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <PreviewNote>尚未关联应付单据</PreviewNote>
                )}
            </PreviewSection>
            <SupplierSourceDocuments scope={{ allocations }} />
            {payment?.relatedReversals.length ? (
                <PreviewSection title="关联冲正">
                    <ul className="space-y-3">
                        {payment.relatedReversals.map((reversal) => (
                            <li key={reversal.reversalId}>
                                <div className="flex items-center justify-between gap-3">
                                    <Button
                                        id={`supplier-payables-preview-record-reversal-${toAutomationIdSegment(reversal.reversalId)}`}
                                        variant="link"
                                        className="h-auto px-0"
                                        onClick={() =>
                                            onOpenReversal(reversal.reversalId)
                                        }
                                    >
                                        {reversal.reversalNo}
                                    </Button>
                                    <BusinessStatusBadge
                                        context="preview"
                                        label={reversal.statusLabel}
                                        tone={reversal.statusTone}
                                    />
                                </div>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    冲正金额{" "}
                                    <MoneyValue value={reversal.amount} /> ·{" "}
                                    {reversal.reasonText || "未填写原因"}
                                </p>
                            </li>
                        ))}
                    </ul>
                </PreviewSection>
            ) : null}
            {unallocated && allocationReason ? (
                <PreviewNote>{allocationReason}</PreviewNote>
            ) : (
                <PreviewNote>
                    {isPayment
                        ? "已过账付款不可编辑；纠错请办理冲正或退款。"
                        : "收票分配与付款独立；发票纠错请办理红票。"}
                </PreviewNote>
            )}
        </div>
    )
}
