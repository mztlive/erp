"use client"

import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"

import { PurchaseOrderPayments } from "./purchase-order-payments"
import Link from "next/link"
import { useState } from "react"
import { RequestDraftDialog } from "@/features/purchase-payment-requests/components/request-draft-dialog"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    BusinessFailureState,
    BusinessStatusBadge,
    MoneyValue,
} from "@/components/business"
import {
    DetailHint,
    DetailSummary,
    DetailSummaryItem,
} from "@/components/business/detail-presentation"
import { usePaymentArrangement } from "../hooks/use-payment-arrangement"
import {
    purchasePaymentRequestStatusLabel,
    purchasePaymentRequestStatusTone,
} from "../lib/purchase-payment-request-display"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

/** 票款只展示本单已确认的应付、付款核销和收票金额。 */
export function PurchaseOrderDetailPayableSection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const [requestOpen, setRequestOpen] = useState(false)
    const profile = useAccountProfileQuery()
    const canListRequests = hasPermission(
        profile.data?.permissions,
        "purchase_payment_request:list",
    )
    const canOpenRequest = hasPermission(
        profile.data?.permissions,
        "purchase_payment_request:detail",
    )
    const summary = order.payableSummary
    const arrangement = usePaymentArrangement(order.identity.purchaseOrderId)
    const requests = arrangement.data?.requests ?? []
    const purchaseNo = order.identity.purchaseNo ?? order.identity.draftLabel
    const scopeParams = new URLSearchParams({
        purchase_order_id: order.identity.purchaseOrderId,
    })
    if (purchaseNo) scopeParams.set("purchase_order_label", purchaseNo)
    const allRequestsHref = `/procurement/payment-requests?${scopeParams.toString()}`
    const orderPath = `/procurement/orders/${encodeURIComponent(order.identity.purchaseOrderId)}?section=payable`
    if (!summary) {
        return (
            <DetailRecordSection title="本单应付" compact>
                <p className="text-sm text-muted-foreground">尚未形成应付</p>
            </DetailRecordSection>
        )
    }
    const amount = (value: string) =>
        costMasked ? "•••" : <MoneyValue value={value} />
    return (
        <div className="space-y-6">
            <DetailSummary label="采购票款摘要">
                <DetailSummaryItem
                    title="付款"
                    label="待付金额"
                    value={amount(summary.payableOpenAmount)}
                    detail={
                        <>已付并核销 {amount(summary.paidAllocatedAmount)}</>
                    }
                    hint={
                        <DetailHint
                            id="purchase-order-payment-hint"
                            label="付款核销"
                        >
                            仅统计已核销到本单的付款金额，审批中的付款不计入。
                        </DetailHint>
                    }
                />
                <DetailSummaryItem
                    title="收票"
                    label="已收票并核销"
                    value={amount(summary.purchaseInvoiceAllocatedAmount)}
                    detail={order.progress.invoice}
                    hint={
                        <DetailHint
                            id="purchase-order-invoice-hint"
                            label="收票核销"
                        >
                            仅统计已核销到本单的采购发票金额。
                        </DetailHint>
                    }
                />
            </DetailSummary>
            <DetailRecordSection
                title="关联付款申请"
                count={arrangement.isSuccess ? requests.length : undefined}
                description="合并申请仅列出本单申请、已付和未付金额。"
                action={
                    <>
                        {hasPermission(
                            profile.data?.permissions,
                            "purchase_payment_request:create",
                        ) &&
                            hasPermission(
                                profile.data?.permissions,
                                "purchase_payment_request:submit",
                            ) && (
                                <Button
                                    id="purchase-order-payment-request-create"
                                    size="sm"
                                    onClick={() => setRequestOpen(true)}
                                >
                                    申请采购付款
                                </Button>
                            )}

                        {canListRequests && (
                            <Button
                                id="purchase-order-payment-requests-open"
                                variant="outline"
                                size="sm"
                                render={<Link href={allRequestsHref} />}
                            >
                                查看全部付款申请
                            </Button>
                        )}
                    </>
                }
            >
                {arrangement.isPending ? (
                    <p role="status" className="text-sm text-muted-foreground">
                        正在读取付款申请…
                    </p>
                ) : arrangement.isError ? (
                    <BusinessFailureState
                        title="付款申请读取失败"
                        error={arrangement.error}
                        onRetry={() => void arrangement.refetch()}
                    />
                ) : requests.length ? (
                    <ul className="divide-y divide-border/70">
                        {requests.map((request) => (
                            <li
                                key={request.request_id}
                                className="flex flex-wrap items-start justify-between gap-4 py-3 first:pt-0 last:pb-0"
                            >
                                <div className="min-w-0 space-y-1">
                                    <div className="flex flex-wrap items-center gap-2">
                                        {canOpenRequest ? (
                                            <Link
                                                id={`purchase-order-payment-request-${toAutomationIdSegment(request.request_id)}`}
                                                href={`/procurement/payment-requests/${encodeURIComponent(request.request_id)}?returnTo=${encodeURIComponent(orderPath)}`}
                                                className="num text-sm font-medium text-primary underline-offset-2 hover:underline"
                                            >
                                                {request.request_no}
                                            </Link>
                                        ) : (
                                            <span className="num text-sm font-medium">
                                                {request.request_no}
                                            </span>
                                        )}
                                        <BusinessStatusBadge
                                            context="list"
                                            label={
                                                purchasePaymentRequestStatusLabel[
                                                    request.status
                                                ]
                                            }
                                            tone={
                                                purchasePaymentRequestStatusTone[
                                                    request.status
                                                ]
                                            }
                                        />
                                    </div>
                                    <p className="text-xs text-muted-foreground">
                                        希望付款日{" "}
                                        {request.desired_payment_date}
                                    </p>
                                </div>
                                <dl className="grid min-w-0 grid-cols-1 gap-4 rounded-lg bg-muted/35 px-4 py-3 text-sm sm:grid-cols-3">
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            本单申请
                                        </dt>
                                        <dd className="mt-1 wrap-anywhere font-medium">
                                            {amount(request.requested_amount)}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            本单已付
                                        </dt>
                                        <dd className="mt-1 wrap-anywhere font-medium">
                                            {amount(request.paid_amount)}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            本单未付
                                        </dt>
                                        <dd className="mt-1 wrap-anywhere font-medium">
                                            {amount(request.remaining_amount)}
                                        </dd>
                                    </div>
                                </dl>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        暂无关联付款申请。
                    </p>
                )}
            </DetailRecordSection>
            {requestOpen ? (
                <RequestDraftDialog
                    key={`payment-request-${order.identity.purchaseOrderId}`}
                    open
                    onOpenChange={setRequestOpen}
                    fixedPurchaseOrderId={order.identity.purchaseOrderId}
                />
            ) : null}
            <PurchaseOrderPayments
                key={`payment-records-${order.identity.purchaseOrderId}`}
                purchaseOrderId={order.identity.purchaseOrderId}
                costMasked={costMasked}
            />
        </div>
    )
}
