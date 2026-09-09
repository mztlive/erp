"use client"

import * as React from "react"
import Link from "next/link"
import {
    invoicePreviewHref,
    paymentPreviewHref,
} from "../../lib/related-documents"
import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { SupplierPaymentDetailDialog } from "../../components/supplier-payment-detail-dialog"
import type {
    PaymentRow,
    PurchaseInvoiceRow,
    ReverseTarget,
    SessionState,
    SupplierRefundRequest,
    UnallocatedRow,
} from "../../types"
import { SupplierAccountRecordBody } from "./supplier-account-record-body"

export interface SupplierAccountRecordPreviewProps {
    kind: "payment" | "invoice" | "unallocated"
    payment?: PaymentRow | null
    invoice?: PurchaseInvoiceRow | null
    unallocated?: UnallocatedRow | null
    loading?: boolean
    error?: string
    onRetry?: () => void
    onClose: () => void
    onClosed?: () => void
    onOpenPayable: (id: string) => void
    onOpenReversal: (id: string) => void
    onOpenSession: (session: SessionState) => void
    onReverse: (target: ReverseTarget) => void
    onRedInvoiceNo: (value: string) => void
    onRefund: (request: SupplierRefundRequest) => void
}

/** 付款、进项发票与待核销记录共用的轻预览及操作入口。 */
export function SupplierAccountRecordPreview({
    kind,
    payment,
    invoice,
    unallocated,
    loading,
    error,
    onRetry,
    onClose,
    onClosed,
    onOpenPayable,
    onOpenReversal,
    onOpenSession,
    onReverse,
    onRedInvoiceNo,
    onRefund,
}: SupplierAccountRecordPreviewProps) {
    const [paymentDetailOpen, setPaymentDetailOpen] = React.useState(false)
    const originalHref = invoice?.originalInvoiceId
        ? invoicePreviewHref(invoice.originalInvoiceId)
        : payment?.reverseOfPaymentId
          ? paymentPreviewHref(payment.reverseOfPaymentId)
          : undefined
    const row = payment ?? invoice ?? unallocated
    const documentNo =
        payment?.paymentNo ??
        (invoice
            ? `${invoice.invoiceCode}-${invoice.invoiceNo}`
            : unallocated?.documentNo)
    const isPayment =
        kind === "payment" ||
        Boolean(payment) ||
        unallocated?.track === "payment"
    const canAllocate = Boolean(
        invoice?.allowedActions.includes("CONTINUE_ALLOCATE"),
    )
    const allocationReason = isPayment
        ? "付款必须在付款任务登记时完成核销"
        : !invoice
          ? "未找到原发票，请回到进项发票视图操作"
          : !canAllocate
            ? "当前发票不可继续核销"
            : undefined
    return (
        <>
            <QuickPreviewSheet
                idPrefix="supplier-payables-preview-record"
                open={!paymentDetailOpen}
                onOpenChange={(open) => {
                    if (!open) onClose()
                }}
                onOpenChangeComplete={(open) => {
                    if (!open && !paymentDetailOpen) onClosed?.()
                }}
                contentClassName="data-[side=right]:sm:w-[480px] data-[side=right]:sm:max-w-[480px]"
                identity={
                    documentNo
                        ? `${isPayment ? "付款单" : "发票号码"}：${documentNo}`
                        : undefined
                }
                title={row?.supplierName ?? "往来记录"}
                description={
                    isPayment
                        ? "付款记录"
                        : kind === "unallocated" && !row
                          ? "待核销记录"
                          : "进项发票"
                }
                summary={
                    row ? (
                        <BusinessStatusBadge
                            context="preview"
                            label={row.statusLabel}
                            tone={row.statusTone}
                        />
                    ) : undefined
                }
                footer={
                    <>
                        <Button
                            id="supplier-payables-preview-record-dismiss"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {originalHref ? (
                            <Button
                                id="supplier-payables-preview-record-original"
                                variant="outline"
                                render={<Link href={originalHref} />}
                            >
                                {invoice ? "打开原发票" : "打开原付款"}
                            </Button>
                        ) : null}
                        {payment?.allowedActions.includes("VIEW_DETAIL") ? (
                            <Button
                                id="supplier-payables-preview-record-detail"
                                variant="outline"
                                onClick={() => setPaymentDetailOpen(true)}
                            >
                                查看详情
                            </Button>
                        ) : null}
                        {payment?.allowedActions.includes("REVERSE") ? (
                            <Button
                                id="supplier-payables-preview-record-reverse"
                                variant="outline"
                                onClick={() => {
                                    onClose()
                                    onReverse({
                                        kind: "payment",
                                        id: payment.paymentId,
                                        no: payment.paymentNo,
                                        amount: payment.amount,
                                        supplierName: payment.supplierName,
                                    })
                                }}
                            >
                                冲正
                            </Button>
                        ) : null}
                        {payment?.allowedActions.includes("REFUND") ? (
                            <Button
                                id="supplier-payables-preview-record-refund"
                                variant="outline"
                                onClick={() => {
                                    onClose()
                                    onRefund({
                                        sourcePaymentId: payment.paymentId,
                                        sourcePaymentNo: payment.paymentNo,
                                        supplierId: payment.supplierId,
                                        supplierName: payment.supplierName,
                                        amount: payment.amount,
                                    })
                                }}
                            >
                                退款
                            </Button>
                        ) : null}
                        {invoice?.allowedActions.includes("RED_INVOICE") ? (
                            <Button
                                id="supplier-payables-preview-record-red-invoice"
                                variant="outline"
                                onClick={() => {
                                    onClose()
                                    onRedInvoiceNo(`R${invoice.invoiceNo}`)
                                    onReverse({
                                        kind: "invoice",
                                        id: invoice.invoiceId,
                                        no: `${invoice.invoiceCode}-${invoice.invoiceNo}`,
                                    })
                                }}
                            >
                                红票
                            </Button>
                        ) : null}
                        {canAllocate || unallocated ? (
                            <Button
                                id="supplier-payables-preview-record-allocate"
                                disabled={!canAllocate || isPayment}
                                title={allocationReason}
                                onClick={() => {
                                    if (!invoice || !canAllocate || isPayment)
                                        return
                                    onOpenSession({
                                        track: "purchase_invoice",
                                        supplierId: invoice.supplierId,
                                        existingInvoiceId: invoice.invoiceId,
                                    })
                                }}
                            >
                                继续核销
                            </Button>
                        ) : null}
                    </>
                }
            >
                {row ? (
                    <SupplierAccountRecordBody
                        payment={payment}
                        invoice={invoice}
                        unallocated={unallocated}
                        isPayment={isPayment}
                        allocationReason={allocationReason}
                        onOpenPayable={onOpenPayable}
                        onOpenReversal={onOpenReversal}
                    />
                ) : loading ? (
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                ) : (
                    <div className="space-y-3">
                        <p className="text-muted-foreground">
                            {error ?? "未找到往来记录"}
                        </p>
                        {error && onRetry ? (
                            <Button
                                id="supplier-payables-preview-record-retry"
                                variant="outline"
                                onClick={onRetry}
                            >
                                重试
                            </Button>
                        ) : null}
                    </div>
                )}
            </QuickPreviewSheet>
            {paymentDetailOpen ? (
                <SupplierPaymentDetailDialog
                    open
                    onOpenChange={setPaymentDetailOpen}
                    isPending={false}
                    isError={false}
                    error={null}
                    onRetry={() => {}}
                    row={payment}
                    onOpenPayable={onOpenPayable}
                />
            ) : null}
        </>
    )
}
