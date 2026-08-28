"use client"

import Link from "next/link"
import type { UseQueryResult } from "@tanstack/react-query"
import { ExternalLinkIcon } from "lucide-react"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PaymentReversalDetailBody } from "@/features/supplier-payables/components/payment-reversal-detail-body"
import { SupplierPaymentDetailDialog } from "@/features/supplier-payables/components/supplier-payment-detail-dialog"
import { SupplierRefundDetailBody } from "@/features/supplier-payables/components/supplier-refund-detail-body"
import { isUnsubmittedPaymentReversalStatus } from "@/features/supplier-payables/lib/payment-reversal-approval"
import { isUnsubmittedSupplierRefundStatus } from "@/features/supplier-payables/lib/supplier-refund-approval"
import { buildPayableActivity } from "@/features/supplier-payables/lib/payable-preview-activity"
import type {
    PayableDetailView,
    PayableRow,
    PaymentReversalRow,
    PaymentRow,
    SessionState,
    SupplierRefundRow,
} from "@/features/supplier-payables/types"
import {
    PayablePreviewBody,
    PayablePreviewSkeleton,
} from "./payable-preview-body"

export interface SupplierAccountsPreviewProps {
    previewPayableId: string | null
    previewPaymentId: string | null
    previewRefundId: string | null
    previewReversalId: string | null
    detailQuery: UseQueryResult<PayableDetailView | null, Error>
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
 * 供应商往来详情。付款走分区 Dialog；退款与付款冲正仍用详情抽屉并嵌入通用审批区。
 * 应付预览只能为当前付款任务打开付款作业。
 */
export function SupplierAccountsPreview({
    previewPayableId,
    previewPaymentId,
    previewRefundId,
    previewReversalId,
    detailQuery,
    paymentQuery,
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
                open
                onOpenChange={(open) => {
                    if (!open) onClose()
                }}
                size="detail"
                title={reversalQuery.data?.reversalNo ?? "冲正详情"}
                description="付款冲正记录与审批信息"
                footer={
                    canSubmitDraft ? (
                        <Button type="button" onClick={onRequestReversalSubmit}>
                            提交审批
                        </Button>
                    ) : null
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
                    <div className="space-y-3 p-6">
                        <p className="text-sm text-muted-foreground">
                            {getErrorMessage(
                                reversalQuery.error,
                                "冲正详情加载失败，请重试。",
                            )}
                        </p>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void reversalQuery.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : (
                    <p className="p-6 text-sm text-muted-foreground">
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
                open
                onOpenChange={(open) => {
                    if (!open) onClose()
                }}
                size="detail"
                title={refundQuery.data?.refundNo ?? "退款详情"}
                description="供应商退款记录与审批信息"
                footer={
                    canSubmitDraft ? (
                        <Button type="button" onClick={onRequestRefundSubmit}>
                            提交审批
                        </Button>
                    ) : null
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
                    <div className="space-y-3 p-6">
                        <p className="text-sm text-muted-foreground">
                            {getErrorMessage(
                                refundQuery.error,
                                "退款详情加载失败，请重试。",
                            )}
                        </p>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void refundQuery.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : (
                    <p className="p-6 text-sm text-muted-foreground">
                        未找到退款详情
                    </p>
                )}
            </QuickPreviewSheet>
        )
    }

    if (previewPaymentId) {
        return (
            <SupplierPaymentDetailDialog
                open
                onOpenChange={(open) => {
                    if (!open) onClose()
                }}
                isPending={paymentQuery.isPending}
                isError={paymentQuery.isError}
                error={paymentQuery.error}
                onRetry={() => void paymentQuery.refetch()}
                row={paymentQuery.data}
                onOpenPayable={onOpenPayable}
            />
        )
    }

    const payable = detailQuery.data?.payable
    const canRegisterPayment =
        payable != null &&
        payable.payableAccountId === paymentTaskPayableAccountId
    const showRegisterInvoice =
        payable != null &&
        canRegisterInvoice &&
        payable.allowedActions.includes("REGISTER_INVOICE")

    return (
        <QuickPreviewSheet
            open={Boolean(previewPayableId)}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            size="detail"
            title={payable?.sourceDocumentNo ?? "应付详情"}
            identity={
                payable ? (
                    <span>
                        {payable.supplierName} · {payable.sourceTypeLabel}
                    </span>
                ) : null
            }
            summary={
                payable ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={payable.statusLabel}
                            tone={payable.statusTone}
                        />
                        <Badge variant={dueBadgeVariant(payable.dueState)}>
                            {payable.dueStateLabel}
                        </Badge>
                        <span className="num text-sm text-muted-foreground">
                            {payable.dueDate}
                        </span>
                    </div>
                ) : null
            }
            footer={
                payable ? (
                    <>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        {payable.sourceHref ? (
                            <Button
                                type="button"
                                variant="outline"
                                render={<Link href={payable.sourceHref} />}
                            >
                                查看来源
                                <ExternalLinkIcon data-icon="inline-end" />
                            </Button>
                        ) : null}
                        {showRegisterInvoice ? (
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => {
                                    onClose()
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
                                type="button"
                                onClick={() => {
                                    onClose()
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
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => void detailQuery.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : (
                <p className="p-6 text-sm text-muted-foreground">
                    未找到应付详情
                </p>
            )}
        </QuickPreviewSheet>
    )
}

function dueBadgeVariant(
    dueState: PayableRow["dueState"],
): "destructive" | "warning" | "neutral" {
    if (dueState === "overdue") return "destructive"
    if (dueState === "due_today") return "warning"
    return "neutral"
}
