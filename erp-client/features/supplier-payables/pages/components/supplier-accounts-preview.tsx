"use client"

import Link from "next/link"
import {
    sourceDocumentHref,
    sourceDocumentOpenLabel,
} from "../../lib/related-documents"
import type { UseQueryResult } from "@tanstack/react-query"
import { ExternalLinkIcon } from "lucide-react"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PaymentReversalDetailBody } from "@/features/supplier-payables/components/payment-reversal-detail-body"
import { SupplierAccountRecordPreview } from "./supplier-account-record-preview"
import { SupplierRefundDetailBody } from "@/features/supplier-payables/components/supplier-refund-detail-body"
import { isUnsubmittedPaymentReversalStatus } from "@/features/supplier-payables/lib/payment-reversal-approval"
import { isUnsubmittedSupplierRefundStatus } from "@/features/supplier-payables/lib/supplier-refund-approval"
import { buildPayableActivity } from "@/features/supplier-payables/lib/payable-preview-activity"
import type {
    PayableDetailView,
    PaymentReversalRow,
    PaymentRow,
    PurchaseInvoiceRow,
    SessionState,
    SupplierAccountsListView,
    ReverseTarget,
    SupplierRefundRequest,
    SupplierRefundRow,
} from "@/features/supplier-payables/types"
import {
    PayablePreviewBody,
    PayablePreviewSkeleton,
} from "./payable-preview-body"

export interface SupplierAccountsPreviewProps {
    previewInvoiceId: string | null
    previewUnallocatedId: string | null
    listData: SupplierAccountsListView | undefined
    listLoading: boolean
    listError?: string
    onRetryList: () => void
    onOpenReversal: (id: string) => void
    onReverse: (target: ReverseTarget) => void
    onRedInvoiceNo: (value: string) => void
    onRefund: (request: SupplierRefundRequest) => void
    onClosed?: () => void
    canRegisterPayment?: boolean
    previewPayableId: string | null
    previewPaymentId: string | null
    previewRefundId: string | null
    previewReversalId: string | null
    detailQuery: UseQueryResult<PayableDetailView | null, Error>
    invoiceQuery?: UseQueryResult<PurchaseInvoiceRow, Error>
    paymentQuery: UseQueryResult<PaymentRow | null, Error>
    refundQuery: UseQueryResult<SupplierRefundRow | null, Error>
    reversalQuery: UseQueryResult<PaymentReversalRow | null, Error>
    onRequestRefundSubmit?: () => void
    onRequestReversalSubmit?: () => void
    returnTo: string | undefined
    fromWorkspace: string | undefined
    paymentTaskPayableAccountId?: string
    canRegisterInvoice?: boolean
    onClose: () => void
    /** 在当前页打开应付预览，保持付款工作视图，不跳到台账列表。 */
    onOpenPayable: (payableAccountId: string) => void
    onOpenSession: (next: SessionState) => void
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}

/**
 * 供应商往来预览与操作入口；付款完整明细由预览进入分区 Dialog。
 * 应付预览只能为当前付款任务打开付款作业。
 */
export function SupplierAccountsPreview({
    previewInvoiceId,
    previewUnallocatedId,
    listData,
    listLoading,
    listError,
    onRetryList,
    onOpenReversal,
    onReverse,
    onRedInvoiceNo,
    onRefund,
    onClosed,
    canRegisterPayment: paymentAllowed = false,
    previewPayableId,
    previewPaymentId,
    previewRefundId,
    previewReversalId,
    detailQuery,
    paymentQuery,
    invoiceQuery,
    refundQuery,
    reversalQuery,
    onRequestRefundSubmit,
    onRequestReversalSubmit,
    returnTo,
    fromWorkspace,
    paymentTaskPayableAccountId,
    canRegisterInvoice = false,
    onClose,
    onOpenPayable,
    onOpenSession,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: SupplierAccountsPreviewProps) {
    if (previewReversalId) {
        const canSubmitDraft =
            Boolean(reversalQuery.data) &&
            isUnsubmittedPaymentReversalStatus(reversalQuery.data?.status) &&
            Boolean(
                reversalQuery.data?.approval?.allowedActions.includes("SUBMIT"),
            ) &&
            Boolean(onRequestReversalSubmit)
        return (
            <QuickPreviewSheet
                idPrefix="supplier-payables-preview-reversal"
                open
                onOpenChange={(open) => {
                    if (!open) onClose()
                }}
                onOpenChangeComplete={(open) => {
                    if (!open) onClosed?.()
                }}
                size="detail"
                contentClassName="data-[side=right]:sm:w-[480px] data-[side=right]:sm:max-w-[480px]"
                title="付款冲正"
                identity={
                    reversalQuery.data
                        ? `冲正单：${reversalQuery.data.reversalNo}`
                        : undefined
                }
                summary={
                    reversalQuery.data ? (
                        <BusinessStatusBadge
                            context="preview"
                            label={reversalQuery.data.statusLabel}
                            tone={reversalQuery.data.statusTone}
                        />
                    ) : undefined
                }
                footer={
                    <>
                        <Button
                            id="supplier-payables-preview-reversal-dismiss"
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {canSubmitDraft ? (
                            <Button
                                id="supplier-payables-preview-reversal-submit"
                                type="button"
                                onClick={onRequestReversalSubmit}
                            >
                                提交审批
                            </Button>
                        ) : null}
                    </>
                }
            >
                {reversalQuery.isPending ? (
                    <div className="h-40 animate-pulse rounded-xl bg-muted" />
                ) : reversalQuery.data ? (
                    <PaymentReversalDetailBody
                        row={reversalQuery.data}
                        workItemId={workItemId}
                        expectedTaskVersion={expectedTaskVersion}
                        workItemAllowedActions={workItemAllowedActions}
                        onDecisionApplied={onDecisionApplied}
                    />
                ) : reversalQuery.isError ? (
                    <div className="space-y-3 px-7 py-6">
                        <p className="text-sm text-muted-foreground">
                            {getErrorMessage(
                                reversalQuery.error,
                                "冲正详情加载失败，请重试。",
                            )}
                        </p>
                        <Button
                            id="supplier-payables-preview-reversal-retry"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void reversalQuery.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : (
                    <p className="px-7 py-6 text-sm text-muted-foreground">
                        未找到冲正详情
                    </p>
                )}
            </QuickPreviewSheet>
        )
    }

    if (previewRefundId) {
        const canSubmitDraft =
            Boolean(refundQuery.data) &&
            isUnsubmittedSupplierRefundStatus(refundQuery.data?.status) &&
            Boolean(
                refundQuery.data?.approval?.allowedActions.includes("SUBMIT"),
            ) &&
            Boolean(onRequestRefundSubmit)
        return (
            <QuickPreviewSheet
                idPrefix="supplier-payables-preview-refund"
                open
                onOpenChange={(open) => {
                    if (!open) onClose()
                }}
                onOpenChangeComplete={(open) => {
                    if (!open) onClosed?.()
                }}
                size="detail"
                contentClassName="data-[side=right]:sm:w-[480px] data-[side=right]:sm:max-w-[480px]"
                title="供应商退款"
                identity={
                    refundQuery.data
                        ? `退款单：${refundQuery.data.refundNo}`
                        : undefined
                }
                summary={
                    refundQuery.data ? (
                        <BusinessStatusBadge
                            context="preview"
                            label={refundQuery.data.statusLabel}
                            tone={refundQuery.data.statusTone}
                        />
                    ) : undefined
                }
                footer={
                    <>
                        <Button
                            id="supplier-payables-preview-refund-dismiss"
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {canSubmitDraft ? (
                            <Button
                                id="supplier-payables-preview-refund-submit"
                                type="button"
                                onClick={onRequestRefundSubmit}
                            >
                                提交审批
                            </Button>
                        ) : null}
                    </>
                }
            >
                {refundQuery.isPending ? (
                    <div className="h-40 animate-pulse rounded-xl bg-muted" />
                ) : refundQuery.data ? (
                    <SupplierRefundDetailBody
                        row={refundQuery.data}
                        workItemId={workItemId}
                        expectedTaskVersion={expectedTaskVersion}
                        workItemAllowedActions={workItemAllowedActions}
                        onDecisionApplied={onDecisionApplied}
                    />
                ) : refundQuery.isError ? (
                    <div className="space-y-3 px-7 py-6">
                        <p className="text-sm text-muted-foreground">
                            {getErrorMessage(
                                refundQuery.error,
                                "退款详情加载失败，请重试。",
                            )}
                        </p>
                        <Button
                            id="supplier-payables-preview-refund-retry"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void refundQuery.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : (
                    <p className="px-7 py-6 text-sm text-muted-foreground">
                        未找到退款详情
                    </p>
                )}
            </QuickPreviewSheet>
        )
    }

    if (previewPaymentId || previewInvoiceId || previewUnallocatedId) {
        const unallocated = listData?.unallocated.find(
            (row) => row.id === previewUnallocatedId,
        )
        const invoiceId =
            previewInvoiceId ??
            (unallocated?.track === "purchase_invoice" ? unallocated.id : null)
        const payment = previewPaymentId
            ? paymentQuery.data
            : unallocated?.track === "payment"
              ? listData?.payments.find(
                    (row) => row.paymentId === unallocated.id,
                )
              : undefined
        return (
            <SupplierAccountRecordPreview
                key={
                    previewPaymentId ?? previewInvoiceId ?? previewUnallocatedId
                }
                kind={
                    previewPaymentId
                        ? "payment"
                        : previewInvoiceId
                          ? "invoice"
                          : "unallocated"
                }
                payment={payment}
                invoice={
                    invoiceQuery?.data ??
                    listData?.invoices.find(
                        (row) => row.invoiceId === invoiceId,
                    )
                }
                unallocated={unallocated}
                loading={
                    previewPaymentId
                        ? paymentQuery.isPending
                        : previewInvoiceId && invoiceQuery
                          ? invoiceQuery.isPending
                          : listLoading
                }
                error={
                    previewPaymentId
                        ? paymentQuery.isError
                            ? getErrorMessage(
                                  paymentQuery.error,
                                  "付款详情加载失败，请重试。",
                              )
                            : undefined
                        : previewInvoiceId && invoiceQuery?.isError
                          ? getErrorMessage(
                                invoiceQuery.error,
                                "原发票读取失败，请重试。",
                            )
                          : listError
                }
                onRetry={
                    previewPaymentId
                        ? () => void paymentQuery.refetch()
                        : previewInvoiceId && invoiceQuery
                          ? () => void invoiceQuery.refetch()
                          : onRetryList
                }
                onClose={onClose}
                onClosed={onClosed}
                onOpenPayable={onOpenPayable}
                onOpenReversal={onOpenReversal}
                onOpenSession={onOpenSession}
                onReverse={onReverse}
                onRedInvoiceNo={onRedInvoiceNo}
                onRefund={onRefund}
            />
        )
    }

    const payable = detailQuery.data?.payable
    const canRegisterPayment =
        paymentAllowed &&
        payable != null &&
        payable.payableAccountId === paymentTaskPayableAccountId
    const sourceHref = payable
        ? (payable.sourceHref ??
          sourceDocumentHref(payable.sourceType, payable.sourceDocumentId))
        : undefined
    const showRegisterInvoice =
        payable != null &&
        canRegisterInvoice &&
        payable.allowedActions.includes("REGISTER_INVOICE")

    return (
        <QuickPreviewSheet
            idPrefix="supplier-payables-preview-payable"
            open={Boolean(previewPayableId)}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            onOpenChangeComplete={(open) => {
                if (!open) onClosed?.()
            }}
            size="detail"
            contentClassName="data-[side=right]:sm:w-[480px] data-[side=right]:sm:max-w-[480px]"
            title={payable?.supplierName ?? "应付详情"}
            identity={
                payable
                    ? `${payable.sourceTypeLabel}：${payable.sourceDocumentNo}`
                    : undefined
            }
            summary={
                payable ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={payable.statusLabel}
                        tone={payable.statusTone}
                    />
                ) : undefined
            }
            footer={
                payable ? (
                    <>
                        <Button
                            id="supplier-payables-preview-close"
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {sourceHref ? (
                            <Button
                                id="supplier-payables-preview-open-source"
                                type="button"
                                variant="outline"
                                render={<Link href={sourceHref} />}
                            >
                                {sourceDocumentOpenLabel(payable.sourceType)}
                                <ExternalLinkIcon data-icon="inline-end" />
                            </Button>
                        ) : null}
                        {showRegisterInvoice ? (
                            <Button
                                id="supplier-payables-preview-register-invoice"
                                type="button"
                                variant="outline"
                                onClick={() => {
                                    onOpenSession({
                                        track: "purchase_invoice",
                                        supplierId: payable.supplierId,
                                        preselectPayableAccountId:
                                            payable.payableAccountId,
                                    })
                                }}
                            >
                                登记进项发票
                            </Button>
                        ) : null}
                        {canRegisterPayment ? (
                            <Button
                                id="supplier-payables-preview-register-payment"
                                type="button"
                                onClick={() => {
                                    onOpenSession({
                                        track: "payment",
                                        supplierId: payable.supplierId,
                                        preselectPayableAccountId:
                                            payable.payableAccountId,
                                        purchaseOrderId:
                                            payable.sourceType ===
                                            "PURCHASE_ORDER"
                                                ? payable.sourceDocumentId
                                                : undefined,
                                        returnTo,
                                        fromWorkspace,
                                    })
                                }}
                            >
                                登记付款
                            </Button>
                        ) : null}
                    </>
                ) : null
            }
        >
            {detailQuery.isPending ? (
                <PayablePreviewSkeleton />
            ) : detailQuery.data ? (
                <PayablePreviewBody
                    payable={detailQuery.data.payable}
                    entries={detailQuery.data.entries}
                    activity={buildPayableActivity(detailQuery.data)}
                    paymentBlockedReason={
                        canRegisterPayment
                            ? undefined
                            : "付款需从工作台的供应商付款任务进入。"
                    }
                />
            ) : detailQuery.isError ? (
                <div className="flex flex-col gap-3 p-6">
                    <p className="text-sm text-muted-foreground">
                        {getErrorMessage(
                            detailQuery.error,
                            "应付详情加载失败，请重试。",
                        )}
                    </p>
                    <Button
                        id="supplier-payables-preview-retry"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => void detailQuery.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : (
                <p className="px-7 py-6 text-sm text-muted-foreground">
                    未找到应付详情
                </p>
            )}
        </QuickPreviewSheet>
    )
}
