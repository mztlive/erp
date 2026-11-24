"use client"

import { useState } from "react"
import Link from "next/link"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
    detailWorkspaceStyles,
} from "@/components/business"
import { DetailHint } from "@/components/business/detail-presentation"
import { Button } from "@/components/ui/button"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { RequestDraftDialog } from "@/features/purchase-payment-requests/components/request-draft-dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { usePaymentArrangement } from "../hooks/use-payment-arrangement"
import {
    purchasePaymentRequestStatusLabel,
    purchasePaymentRequestStatusTone,
} from "../lib/purchase-payment-request-display"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"
import { PurchaseOrderPayments } from "./purchase-order-payments"

function invoiceProgressBadge(raw: string): {
    label: string
    tone: StatusTone
} {
    if (raw === "完成") return { label: "已收齐", tone: "success" }
    if (raw === "部分") return { label: "部分收票", tone: "info" }
    return { label: "未收票", tone: "neutral" }
}

/** 票款聚焦关联付款申请与财务付款核销记录；顶部金额摘要已展示 3 项 KPI，此处仅保留收票进度与口径说明。 */
export function PurchaseOrderDetailPayableSection({
    order,
    costMasked,
    hasHeaderPrimaryAction = false,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
    hasHeaderPrimaryAction?: boolean
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
            <DetailRecordSection width="form" title="本单应付">
                <p className="text-xs text-muted-foreground">
                    采购单审批通过后自动生成应付，届时可在此发起付款申请并查阅财务付款核销记录。
                </p>
            </DetailRecordSection>
        )
    }

    const amount = (value: string) =>
        costMasked ? "•••" : <MoneyValue value={value} />
    const invoiceStatus = invoiceProgressBadge(order.progress.invoice)

    return (
        <>
            <section aria-label="采购票款口径与收票进度" className="pt-5 pb-1">
                <WorkspaceFactGrid
                    columns={2}
                    ariaLabel="采购票款口径与收票进度"
                >
                    <WorkspaceFactItem
                        label={
                            <span className="inline-flex items-center gap-1">
                                <span>进项票进度</span>
                                <DetailHint
                                    id="purchase-order-invoice-hint"
                                    label="收票核销"
                                >
                                    仅统计已核销到本单的采购发票金额。
                                </DetailHint>
                            </span>
                        }
                    >
                        <StatusBadge
                            tone={invoiceStatus.tone}
                            label={invoiceStatus.label}
                            appearance="text"
                        />
                    </WorkspaceFactItem>
                    <WorkspaceFactItem
                        label={
                            <span className="inline-flex items-center gap-1">
                                <span>付款核销口径</span>
                                <DetailHint
                                    id="purchase-order-payment-hint"
                                    label="付款核销"
                                >
                                    仅统计已核销到本单的付款金额，审批中的付款不计入。
                                </DetailHint>
                            </span>
                        }
                        value="仅计已核销付款（审批中不计入）"
                        tone="muted"
                    />
                </WorkspaceFactGrid>
            </section>

            <DetailRecordSection
                width="full"
                title="关联付款申请"
                count={arrangement.isSuccess ? requests.length : undefined}
                description="合并申请仅列出本单申请、已付和未付金额。"
                action={
                    <>
                        {canListRequests ? (
                            <Button
                                id="purchase-order-payment-requests-open"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.subtleButton}
                                render={<Link href={allRequestsHref} />}
                            >
                                查看全部付款申请
                            </Button>
                        ) : null}
                        {hasPermission(
                            profile.data?.permissions,
                            "purchase_payment_request:create",
                        ) &&
                        hasPermission(
                            profile.data?.permissions,
                            "purchase_payment_request:submit",
                        ) ? (
                            <Button
                                id="purchase-order-payment-request-create"
                                variant={
                                    hasHeaderPrimaryAction ? "ghost" : "default"
                                }
                                size="sm"
                                className={
                                    hasHeaderPrimaryAction
                                        ? detailWorkspaceStyles.subtleButton
                                        : "px-3.5"
                                }
                                onClick={() => setRequestOpen(true)}
                            >
                                申请采购付款
                            </Button>
                        ) : null}
                    </>
                }
            >
                {arrangement.isPending ? (
                    <p role="status" className="text-xs text-muted-foreground">
                        正在读取付款申请…
                    </p>
                ) : arrangement.isError ? (
                    <BusinessFailureState
                        title="付款申请读取失败"
                        error={arrangement.error}
                        onRetry={() => void arrangement.refetch()}
                    />
                ) : requests.length ? (
                    <ul className="divide-y divide-border/60 border-y border-border/60">
                        {requests.map((request) => (
                            <li
                                key={request.request_id}
                                className="flex flex-wrap items-center justify-between gap-4 py-3.5"
                            >
                                <div className="min-w-0 space-y-1">
                                    <div className="flex flex-wrap items-center gap-2">
                                        {canOpenRequest ? (
                                            <Link
                                                id={`purchase-order-payment-request-${toAutomationIdSegment(request.request_id)}`}
                                                href={`/procurement/payment-requests/${encodeURIComponent(request.request_id)}?returnTo=${encodeURIComponent(orderPath)}`}
                                                className="num text-body-compact font-medium text-primary underline-offset-2 hover:underline"
                                            >
                                                {request.request_no}
                                            </Link>
                                        ) : (
                                            <span className="num text-body-compact font-medium">
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
                                    <p className="num text-xs text-muted-foreground">
                                        希望付款日{" "}
                                        {request.desired_payment_date}
                                    </p>
                                </div>
                                <dl className="grid min-w-0 grid-cols-3 gap-x-6 gap-y-1 text-body-compact sm:text-right">
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            本单申请
                                        </dt>
                                        <dd className="num mt-0.5 wrap-anywhere font-medium text-foreground">
                                            {amount(request.requested_amount)}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            本单已付
                                        </dt>
                                        <dd className="num mt-0.5 wrap-anywhere font-medium text-foreground">
                                            {amount(request.paid_amount)}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            本单未付
                                        </dt>
                                        <dd className="num mt-0.5 wrap-anywhere font-medium text-foreground">
                                            {amount(request.remaining_amount)}
                                        </dd>
                                    </div>
                                </dl>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="text-xs text-muted-foreground">
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
        </>
    )
}
