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
import { SupplierPaymentDetailBody } from "@/features/supplier-payables/components/supplier-payment-detail-body"
import { SupplierRefundDetailBody } from "@/features/supplier-payables/components/supplier-refund-detail-body"
import { isUnsubmittedSupplierRefundStatus } from "@/features/supplier-payables/lib/supplier-refund-approval"
import type {
    PayableDetailView,
    PaymentRow,
    SessionState,
    SupplierRefundRow,
} from "@/features/supplier-payables/types"

export interface SupplierAccountsPreviewProps {
    previewPayableId: string | null
    previewPaymentId: string | null
    previewRefundId: string | null
    detailQuery: UseQueryResult<PayableDetailView | null, Error>
    paymentQuery: UseQueryResult<PaymentRow | null, Error>
    refundQuery: UseQueryResult<SupplierRefundRow | null, Error>
    onRequestRefundSubmit?: () => void
    returnTo: string | undefined
    fromWorkspace: string | undefined
    onClose: () => void
    onOpenSession: (next: SessionState) => void
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}

/**
 * 供应商往来详情抽屉。付款与供应商退款嵌入通用审批区；应付预览不展示审批。
 */
export function SupplierAccountsPreview({
    previewPayableId,
    previewPaymentId,
    previewRefundId,
    detailQuery,
    paymentQuery,
    refundQuery,
    onRequestRefundSubmit,
    returnTo,
    fromWorkspace,
    onClose,
    onOpenSession,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: SupplierAccountsPreviewProps) {
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
                description="退款事实与审批区只读服务端投影"
                footer={
                    canSubmitDraft ? (
                        <Button
                            type="button"
                            onClick={onRequestRefundSubmit}
                        >
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
            <QuickPreviewSheet
                open
                onOpenChange={(open) => {
                    if (!open) onClose()
                }}
                size="detail"
                title={paymentQuery.data?.paymentNo ?? "付款详情"}
                description="付款事实与审批区只读服务端投影"
            >
                {paymentQuery.isPending ? (
                    <div className="h-40 animate-pulse rounded-xl bg-muted" />
                ) : paymentQuery.data ? (
                    <SupplierPaymentDetailBody
                        row={paymentQuery.data}
                        workItemId={workItemId}
                        expectedTaskVersion={expectedTaskVersion}
                        workItemAllowedActions={workItemAllowedActions}
                        onDecisionApplied={onDecisionApplied}
                    />
                ) : paymentQuery.isError ? (
                    <div className="space-y-3 p-6">
                        <p className="text-sm text-muted-foreground">
                            {getErrorMessage(
                                paymentQuery.error,
                                "付款详情加载失败，请重试。",
                            )}
                        </p>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void paymentQuery.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : (
                    <p className="p-6 text-sm text-muted-foreground">
                        未找到付款详情
                    </p>
                )}
            </QuickPreviewSheet>
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
                        <h4 className="mb-2 text-sm font-medium">
                            应付分录
                        </h4>
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
                        <h4 className="mb-2 text-sm font-medium">
                            付款分配
                        </h4>
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
                                            className="flex justify-between"
                                        >
                                            <span>
                                                {a.action} ·{" "}
                                                {a.sourceDocumentNo}
                                            </span>
                                            <MoneyValue value={a.amount} />
                                        </li>
                                    ),
                                )}
                            </ul>
                        )}
                    </div>
                    <div>
                        <h4 className="mb-2 text-sm font-medium">
                            进项票分配
                        </h4>
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
                                            className="flex justify-between"
                                        >
                                            <span>
                                                {a.action} ·{" "}
                                                {a.sourceDocumentNo}
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
                            onClick={() => {
                                const p = detailQuery.data!.payable
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
                <p className="text-sm text-muted-foreground">
                    未找到应付详情
                </p>
            )}
        </QuickPreviewSheet>
    )
}
