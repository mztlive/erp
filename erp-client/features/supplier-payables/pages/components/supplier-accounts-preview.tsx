"use client"

import Link from "next/link"
import type { UseQueryResult } from "@tanstack/react-query"
import { ExternalLinkIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Separator } from "@/components/ui/separator"
import { getErrorMessage } from "@/lib/api/errors"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { PaymentReversalDetailBody } from "@/features/supplier-payables/components/payment-reversal-detail-body"
import { SupplierPaymentDetailDialog } from "@/features/supplier-payables/components/supplier-payment-detail-dialog"
import { SupplierRefundDetailBody } from "@/features/supplier-payables/components/supplier-refund-detail-body"
import { isUnsubmittedPaymentReversalStatus } from "@/features/supplier-payables/lib/payment-reversal-approval"
import { isUnsubmittedSupplierRefundStatus } from "@/features/supplier-payables/lib/supplier-refund-approval"
import {
    ALLOCATION_ACTION_LABEL,
    type PayableDetailView,
    type PaymentReversalRow,
    type PaymentRow,
    type SessionState,
    type SupplierRefundRow,
} from "@/features/supplier-payables/types"

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

    return (
        <QuickPreviewSheet
            open={Boolean(previewPayableId)}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            title="应付预览"
            description="来源、金额、付款/收票进度与分配关系（系统最新数据）"
        >
            {detailQuery.isPending ? (
                <div className="h-40 animate-pulse rounded-xl bg-muted" />
            ) : detailQuery.data ? (
                <div className="space-y-4">
                    <div>
                        <h3 className="font-medium">
                            {detailQuery.data.payable.supplierName}
                        </h3>
                        <p className="text-sm text-muted-foreground">
                            {detailQuery.data.payable.sourceTypeLabel} ·{" "}
                            <span className="num">
                                {detailQuery.data.payable.sourceDocumentNo}
                            </span>
                        </p>
                    </div>
                    <DescriptionList columns="two">
                        <DescriptionItem>
                            <DescriptionTerm>应付总额</DescriptionTerm>
                            <DescriptionDetails>
                                <MoneyValue
                                    value={detailQuery.data.payable.grossTotal}
                                    taxBasis="gross"
                                />
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>开放应付</DescriptionTerm>
                            <DescriptionDetails>
                                <MoneyValue
                                    value={detailQuery.data.payable.openTotal}
                                />
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>净已付分配</DescriptionTerm>
                            <DescriptionDetails>
                                <MoneyValue
                                    value={
                                        detailQuery.data.payable.settledTotal
                                    }
                                />
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>净已收票</DescriptionTerm>
                            <DescriptionDetails>
                                <MoneyValue
                                    value={
                                        detailQuery.data.payable.invoicedTotal
                                    }
                                />
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>剩余可收票</DescriptionTerm>
                            <DescriptionDetails>
                                <MoneyValue
                                    value={
                                        detailQuery.data.payable
                                            .openInvoiceableTotal
                                    }
                                />
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>状态</DescriptionTerm>
                            <DescriptionDetails>
                                <BusinessStatusBadge
                                    context="preview"
                                    label={detailQuery.data.payable.statusLabel}
                                    tone={detailQuery.data.payable.statusTone}
                                />
                            </DescriptionDetails>
                        </DescriptionItem>
                    </DescriptionList>

                    {detailQuery.data.payable.paymentGateSummary ? (
                        <Alert>
                            <AlertTitle>付款条件（系统校验）</AlertTitle>
                            <AlertDescription>
                                {
                                    detailQuery.data.payable.paymentGateSummary
                                        .message
                                }{" "}
                                · 已核销{" "}
                                {
                                    detailQuery.data.payable.paymentGateSummary
                                        .allocated
                                }{" "}
                                / 门槛{" "}
                                {
                                    detailQuery.data.payable.paymentGateSummary
                                        .required
                                }{" "}
                                · 差额{" "}
                                {
                                    detailQuery.data.payable.paymentGateSummary
                                        .gap
                                }
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    <Separator />
                    <div>
                        <h4 className="mb-2 text-sm font-medium">应付分录</h4>
                        <ul className="space-y-2 text-sm">
                            {detailQuery.data.entries.map((e) => (
                                <li
                                    key={e.entryId}
                                    className="flex justify-between gap-2 rounded-lg border p-2"
                                >
                                    <span>
                                        {e.entryTypeLabel}
                                        <span className="block text-xs text-muted-foreground">
                                            {e.sourceLabel}
                                        </span>
                                    </span>
                                    <MoneyValue value={e.amount} />
                                </li>
                            ))}
                        </ul>
                    </div>
                    <div>
                        <h4 className="mb-2 text-sm font-medium">付款分配</h4>
                        {detailQuery.data.paymentAllocations.length === 0 ? (
                            <p className="text-sm text-muted-foreground">
                                暂无
                            </p>
                        ) : (
                            <ul className="space-y-1 text-sm">
                                {detailQuery.data.paymentAllocations.map(
                                    (a) => (
                                        <li
                                            key={a.allocationId}
                                            className="flex justify-between gap-2"
                                        >
                                            <span>
                                                {
                                                    ALLOCATION_ACTION_LABEL[
                                                        a.action
                                                    ]
                                                }{" "}
                                                · {a.sourceDocumentNo}
                                            </span>
                                            <MoneyValue value={a.amount} />
                                        </li>
                                    ),
                                )}
                            </ul>
                        )}
                    </div>
                    <div>
                        <h4 className="mb-2 text-sm font-medium">进项票分配</h4>
                        {detailQuery.data.invoiceAllocations.length === 0 ? (
                            <p className="text-sm text-muted-foreground">
                                暂无
                            </p>
                        ) : (
                            <ul className="space-y-1 text-sm">
                                {detailQuery.data.invoiceAllocations.map(
                                    (a) => (
                                        <li
                                            key={a.allocationId}
                                            className="flex justify-between gap-2"
                                        >
                                            <span>
                                                {
                                                    ALLOCATION_ACTION_LABEL[
                                                        a.action
                                                    ]
                                                }{" "}
                                                · {a.sourceDocumentNo}
                                            </span>
                                            <MoneyValue value={a.amountGross} />
                                        </li>
                                    ),
                                )}
                            </ul>
                        )}
                    </div>
                    <div className="flex flex-wrap gap-2">
                        {detailQuery.data.payable.sourceHref ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link
                                        href={
                                            detailQuery.data.payable.sourceHref
                                        }
                                    />
                                }
                            >
                                查看来源
                                <ExternalLinkIcon className="size-3.5" />
                            </Button>
                        ) : null}
                        <Button
                            type="button"
                            size="sm"
                            disabled={
                                detailQuery.data.payable.payableAccountId !==
                                paymentTaskPayableAccountId
                            }
                            title={
                                detailQuery.data.payable.payableAccountId ===
                                paymentTaskPayableAccountId
                                    ? undefined
                                    : "付款必须由当前负责人从对应付款任务进入"
                            }
                            onClick={() => {
                                const p = detailQuery.data!.payable
                                if (
                                    p.payableAccountId !==
                                    paymentTaskPayableAccountId
                                ) {
                                    return
                                }
                                onClose()
                                onOpenSession({
                                    track: "payment",
                                    supplierId: p.supplierId,
                                    preselectPayableAccountId:
                                        p.payableAccountId,
                                    purchaseOrderId:
                                        p.sourceType === "PURCHASE_ORDER"
                                            ? p.sourceDocumentId
                                            : undefined,
                                    returnTo,
                                    fromWorkspace,
                                })
                            }}
                        >
                            登记付款
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => {
                                const p = detailQuery.data!.payable
                                onClose()
                                onOpenSession({
                                    track: "purchase_invoice",
                                    supplierId: p.supplierId,
                                    preselectPayableAccountId:
                                        p.payableAccountId,
                                })
                            }}
                        >
                            登记进项发票
                        </Button>
                    </div>
                </div>
            ) : detailQuery.isError ? (
                <div className="space-y-3 p-6">
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
                <p className="text-sm text-muted-foreground">未找到应付详情</p>
            )}
        </QuickPreviewSheet>
    )
}
