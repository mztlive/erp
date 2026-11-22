"use client"

import * as React from "react"

import { SupplierInvoiceFiles } from "../../components/supplier-invoice-files"
import { SupplierSourceDocuments } from "../../components/supplier-source-documents"
import { useSupplierPaymentBankReceiptQuery } from "../../hooks/queries"
import {
    BusinessStatusBadge,
    DetailWorkspaceSubTabs,
    MoneyValue,
} from "@/components/business"
import {
    PreviewAmount,
    PreviewSection,
    PreviewFact,
    PreviewNote,
} from "@/components/business/financial-preview"
import { Button } from "@/components/ui/button"
import { FileUpload } from "@/components/ui/file-upload"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    PaymentRow,
    PurchaseInvoiceRow,
    UnallocatedRow,
} from "../../types"

type PaymentPreviewTab = "overview" | "bank"

/** 未分配余额、单据依据、核销去向按决策顺序排列；付款记录内置分栏直接查看收款账户与银行回单。 */
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
    const [paymentTab, setPaymentTab] =
        React.useState<PaymentPreviewTab>("overview")
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
        <div className="space-y-6 text-body-compact">
            <PreviewAmount
                label={isPayment ? "待核销付款" : "待分配金额（含税）"}
                value={row.unallocatedAmount}
            >
                <span>
                    {isPayment ? "付款金额" : "发票金额"}{" "}
                    <MoneyValue value={total} />
                </span>
                {payment?.effectiveAmount != null ? (
                    <span>
                        有效金额 <MoneyValue value={payment.effectiveAmount} />
                    </span>
                ) : null}
                {allocated != null ? (
                    <span>
                        已核销 <MoneyValue value={allocated} />
                    </span>
                ) : null}
            </PreviewAmount>

            {payment ? (
                <>
                    <DetailWorkspaceSubTabs
                        idPrefix="supplier-payables-preview-record-tabs"
                        ariaLabel="付款详情分区"
                        value={paymentTab}
                        onValueChange={(next) =>
                            setPaymentTab(next as PaymentPreviewTab)
                        }
                        items={[
                            {
                                id: "overview",
                                label:
                                    allocations.length > 0
                                        ? `概览与核销 (${allocations.length})`
                                        : "概览与核销",
                            },
                            {
                                id: "bank",
                                label: payment.bankReceipt
                                    ? "收款账户与回单 (1)"
                                    : "收款账户与回单",
                            },
                        ]}
                    />

                    {paymentTab === "overview" ? (
                        <div className="space-y-6">
                            <PreviewSection title="付款信息">
                                <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
                                    <PreviewFact label="付款时间">
                                        <span className="num">
                                            {formatDateTime(
                                                payment.paidAt,
                                                "full",
                                                "passthrough",
                                            )}
                                        </span>
                                    </PreviewFact>
                                    <PreviewFact label="银行引用">
                                        <span className="num">
                                            {payment.bankReferenceMasked || "—"}
                                        </span>
                                    </PreviewFact>
                                    <PreviewFact label="银行回单">
                                        {payment.bankReceipt?.fileName ??
                                            "未附回单"}
                                    </PreviewFact>
                                    {payment.reversedAmount != null ? (
                                        <PreviewFact label="已冲正金额">
                                            <MoneyValue
                                                value={payment.reversedAmount}
                                            />
                                        </PreviewFact>
                                    ) : null}
                                </dl>
                            </PreviewSection>

                            <AllocationListSection
                                allocations={allocations}
                                onOpenPayable={onOpenPayable}
                            />

                            <SupplierSourceDocuments scope={{ allocations }} />

                            {payment.relatedReversals?.length ? (
                                <PreviewSection title="关联冲正">
                                    <ul className="space-y-3">
                                        {payment.relatedReversals.map(
                                            (reversal) => (
                                                <li key={reversal.reversalId}>
                                                    <div className="flex items-center justify-between gap-3">
                                                        <Button
                                                            id={`supplier-payables-preview-record-reversal-${toAutomationIdSegment(reversal.reversalId)}`}
                                                            variant="link"
                                                            className="h-auto px-0"
                                                            onClick={() =>
                                                                onOpenReversal(
                                                                    reversal.reversalId,
                                                                )
                                                            }
                                                        >
                                                            {
                                                                reversal.reversalNo
                                                            }
                                                        </Button>
                                                        <BusinessStatusBadge
                                                            context="preview"
                                                            label={
                                                                reversal.statusLabel
                                                            }
                                                            tone={
                                                                reversal.statusTone
                                                            }
                                                        />
                                                    </div>
                                                    <p className="mt-1 text-xs text-muted-foreground">
                                                        冲正金额{" "}
                                                        <MoneyValue
                                                            value={
                                                                reversal.amount
                                                            }
                                                        />{" "}
                                                        ·{" "}
                                                        {reversal.reasonText ||
                                                            "未填写原因"}
                                                    </p>
                                                </li>
                                            ),
                                        )}
                                    </ul>
                                </PreviewSection>
                            ) : null}

                            <PreviewNote>
                                {payment.permissionLimited
                                    ? "仅显示权限范围内的核销明细。"
                                    : "已过账付款不可编辑；纠错请办理冲正或退款。"}
                            </PreviewNote>
                        </div>
                    ) : (
                        <div className="space-y-6">
                            <PreviewSection title="收款账户">
                                {payment.paymentRecipient ? (
                                    <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
                                        <PreviewFact label="户名">
                                            {
                                                payment.paymentRecipient
                                                    .accountName
                                            }
                                        </PreviewFact>
                                        <PreviewFact label="开户银行">
                                            {[
                                                payment.paymentRecipient
                                                    .bankName,
                                                payment.paymentRecipient
                                                    .bankBranchName,
                                            ]
                                                .filter(Boolean)
                                                .join(" · ")}
                                        </PreviewFact>
                                        <PreviewFact label="账号">
                                            <span className="num">
                                                {
                                                    payment.paymentRecipient
                                                        .accountNumberMasked
                                                }
                                            </span>
                                        </PreviewFact>
                                    </dl>
                                ) : (
                                    <PreviewNote>未记录收款账户</PreviewNote>
                                )}
                            </PreviewSection>
                            <PreviewSection title="银行回单">
                                <InlineBankReceiptPreview payment={payment} />
                            </PreviewSection>
                        </div>
                    )}
                </>
            ) : (
                <>
                    <PreviewSection title={isPayment ? "付款信息" : "发票资料"}>
                        <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
                            <PreviewFact
                                label={isPayment ? "付款时间" : "开票日期"}
                            >
                                <span className="num">
                                    {formatDateTime(
                                        invoice?.invoiceDate ??
                                            unallocated?.occurredAt,
                                        "full",
                                        "passthrough",
                                    )}
                                </span>
                            </PreviewFact>
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
                    {invoice ? (
                        <SupplierInvoiceFiles invoiceId={invoice.invoiceId} />
                    ) : null}
                    <AllocationListSection
                        allocations={allocations}
                        onOpenPayable={onOpenPayable}
                    />
                    <SupplierSourceDocuments scope={{ allocations }} />
                    {unallocated && allocationReason ? (
                        <PreviewNote>{allocationReason}</PreviewNote>
                    ) : (
                        <PreviewNote>
                            {isPayment
                                ? "已过账付款不可编辑；纠错请办理冲正或退款。"
                                : "收票分配与付款独立；发票纠错请办理红票。"}
                        </PreviewNote>
                    )}
                </>
            )}
        </div>
    )
}

function AllocationListSection({
    allocations,
    onOpenPayable,
}: {
    allocations: readonly {
        allocationId: string
        payableAccountId: string
        sourceDocumentNo: string
        amountGross: string
        action: "APPLY" | "REVERSE"
        occurredAt: string
    }[]
    onOpenPayable: (id: string) => void
}) {
    return (
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
    )
}

function InlineBankReceiptPreview({ payment }: { payment: PaymentRow }) {
    const query = useSupplierPaymentBankReceiptQuery(
        payment.paymentId,
        Boolean(payment.bankReceipt),
    )
    const [previewUrl, setPreviewUrl] = React.useState<string>()
    React.useEffect(() => {
        if (!query.data) {
            setPreviewUrl(undefined)
            return
        }
        const url = URL.createObjectURL(query.data)
        setPreviewUrl(url)
        return () => URL.revokeObjectURL(url)
    }, [query.data])

    if (!payment.bankReceipt) {
        return <PreviewNote>历史付款未留存银行回单图片</PreviewNote>
    }

    return (
        <div className="max-w-md">
            <FileUpload
                idPrefix="supplier-payables-preview-record-receipt-file"
                onFilesSelected={() => undefined}
                readOnly
                multiple={false}
                density="compact"
                preview={{
                    src: previewUrl,
                    name: payment.bankReceipt.fileName,
                    status: "uploaded",
                }}
            />
            {query.isError ? (
                <p className="mt-2 text-xs text-destructive">
                    回单预览加载失败，请稍后重试。
                </p>
            ) : null}
        </div>
    )
}
