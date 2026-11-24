"use client"

import { useState } from "react"
import { ChevronDownIcon, ChevronUpIcon } from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { PurchasePaymentReceipts } from "@/features/fulfillment-operations/components/purchase-payment-receipts"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { FulfillmentPaymentRequest } from "../api/purchase-payment"
import { usePurchasePayment } from "../hooks/use-purchase-payment"
import {
    purchasePaymentStatus,
    purchaseRequestStatus,
    purchaseRequestSummary,
} from "../lib/purchase-payment"

function paymentSectionId(workItemId: string) {
    return `workspace-purchase-payment-${toAutomationIdSegment(workItemId)}`
}

export function WorkspacePurchasePaymentSummary({
    workItemId,
}: {
    workItemId: string
}) {
    const query = usePurchasePayment(workItemId)
    const payment = query.data
    return (
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2 text-xs">
            <span className="text-muted-foreground">采购付款</span>
            {query.isError ? (
                <span role="status" className="text-muted-foreground">
                    付款信息暂不可用
                </span>
            ) : payment ? (
                <>
                    <StatusBadge {...purchasePaymentStatus(payment)} />
                    <span className="text-muted-foreground">付款申请</span>
                    <StatusBadge
                        {...purchaseRequestSummary(payment.requests)}
                    />
                </>
            ) : (
                <span role="status" className="text-muted-foreground">
                    加载中…
                </span>
            )}
            <Button
                id={`${paymentSectionId(workItemId)}-jump`}
                type="button"
                variant="link"
                size="sm"
                className="h-auto px-0 text-xs text-info"
                onClick={() => {
                    const section = document.getElementById(
                        paymentSectionId(workItemId),
                    )
                    section?.scrollIntoView({
                        behavior: "smooth",
                        block: "nearest",
                    })
                    section?.focus({ preventScroll: true })
                }}
            >
                查看付款详情
                <ChevronDownIcon aria-hidden="true" data-icon="inline-end" />
            </Button>
        </div>
    )
}

export function WorkspacePurchasePayment({
    workItemId,
}: {
    workItemId: string
}) {
    const query = usePurchasePayment(workItemId)
    const payment = query.data
    const [expanded, setExpanded] = useState(false)
    const id = paymentSectionId(workItemId)
    const summary = payment ? purchaseRequestSummary(payment.requests) : null
    return (
        <section
            id={id}
            tabIndex={-1}
            aria-label="采购付款"
            className="min-w-0 scroll-m-4 outline-none"
        >
            <h3 className="text-body-compact font-semibold">采购付款</h3>
            {query.isError ? (
                <BusinessFailureState
                    title="付款信息加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !payment ? (
                <p role="status" className="mt-3 text-xs text-muted-foreground">
                    正在加载采购付款信息…
                </p>
            ) : (
                <div className="mt-3 space-y-3">
                    <WorkspaceFactGrid columns={3} ariaLabel="采购付款金额">
                        {[
                            { label: "采购总额", value: payment.gross_amount },
                            { label: "累计已付", value: payment.paid_amount },
                            { label: "剩余应付", value: payment.open_amount },
                        ].map(({ label, value }) => (
                            <WorkspaceFactItem
                                key={label}
                                label={label}
                                numeric
                            >
                                <MoneyValue
                                    value={value}
                                    className="font-semibold"
                                />
                            </WorkspaceFactItem>
                        ))}
                    </WorkspaceFactGrid>
                    <WorkspaceFactGrid columns={3} ariaLabel="付款条件与申请">
                        <WorkspaceFactItem
                            label="付款条件"
                            value={payment.payment_term_label}
                        />
                        <WorkspaceFactItem label="付款申请" span={2}>
                            <div className="flex flex-wrap items-center gap-2">
                                {summary ? (
                                    <StatusBadge
                                        {...summary}
                                        appearance="text"
                                    />
                                ) : null}
                                {payment.requests.length === 1 ? (
                                    <span>
                                        · 本单申请{" "}
                                        <MoneyValue
                                            value={
                                                payment.requests[0]
                                                    .requested_amount
                                            }
                                        />
                                    </span>
                                ) : payment.requests.length > 1 ? (
                                    <span className="num text-xs text-muted-foreground">
                                        共 {payment.requests.length} 份申请
                                    </span>
                                ) : null}
                                {payment.requests.length ? (
                                    <Button
                                        id={`${id}-requests-toggle`}
                                        type="button"
                                        variant="link"
                                        size="sm"
                                        className="h-auto px-0 text-xs text-info"
                                        aria-expanded={expanded}
                                        aria-controls={`${id}-requests`}
                                        onClick={() => setExpanded(!expanded)}
                                    >
                                        {expanded
                                            ? "收起付款申请"
                                            : "查看付款申请"}
                                        {expanded ? (
                                            <ChevronUpIcon
                                                aria-hidden="true"
                                                data-icon="inline-end"
                                            />
                                        ) : (
                                            <ChevronDownIcon
                                                aria-hidden="true"
                                                data-icon="inline-end"
                                            />
                                        )}
                                    </Button>
                                ) : null}
                            </div>
                        </WorkspaceFactItem>
                    </WorkspaceFactGrid>
                    {expanded ? (
                        <div
                            id={`${id}-requests`}
                            className="divide-y divide-border"
                        >
                            {payment.requests.map((request) => (
                                <RequestRow
                                    key={request.request_id}
                                    request={request}
                                />
                            ))}
                        </div>
                    ) : null}
                </div>
            )}
            <div className="mt-3">
                <PurchasePaymentReceipts
                    workItemId={workItemId}
                    embedded
                    title="付款回单"
                    layout="row"
                />
            </div>
        </section>
    )
}

function RequestRow({ request }: { request: FulfillmentPaymentRequest }) {
    return (
        <div className="space-y-2 py-3 text-xs">
            <div className="flex flex-wrap items-center gap-2">
                <span className="num font-medium">{request.request_no}</span>
                <StatusBadge {...purchaseRequestStatus(request)} />
                <span className="num ml-auto text-muted-foreground">
                    希望付款日：{request.desired_payment_date}
                </span>
            </div>
            <div className="flex flex-wrap gap-x-5 gap-y-2 text-muted-foreground">
                <span>
                    本单申请 <MoneyValue value={request.requested_amount} />
                </span>
                <span>
                    本单已付 <MoneyValue value={request.paid_amount} />
                </span>
                <span>
                    未付金额 <MoneyValue value={request.remaining_amount} />
                </span>
                {request.status === "closed" ? (
                    <span>
                        已关闭金额 <MoneyValue value={request.closed_amount} />
                    </span>
                ) : null}
            </div>
        </div>
    )
}
