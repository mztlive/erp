"use client"

import { displayInstanceStatus } from "@/features/approval-workflow/display"
import { MoneyValue } from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import { formatDateTime } from "@/lib/datetime"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"

/** 采购详情侧栏展示服务端金额、各办理环节和责任人。 */
export function PurchaseOrderDetailSidebar({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const gate = order.progress.prepaymentGate
    const amounts = [
        {
            label: "已付并核销",
            value: order.payableSummary?.paidAllocatedAmount,
        },
        { label: "应付未结", value: order.payableSummary?.payableOpenAmount },
        {
            label: "已收票并核销",
            value: order.payableSummary?.purchaseInvoiceAllocatedAmount,
        },
    ]
    const tracks = [
        {
            label: "审批",
            value: order.approval?.instance
                ? displayInstanceStatus(order.approval.instance.status)
                : order.identity.reviewLabel,
        },
        { label: "付款", value: order.progress.payment },
        { label: "履约", value: order.progress.fulfillment },
        { label: "进项票", value: order.progress.invoice },
    ]
    const amount = (value?: string) =>
        costMasked ? "•••" : value == null ? "—" : <MoneyValue value={value} />

    return (
        <aside
            aria-label="采购单摘要"
            className="min-w-0 self-start rounded-xl bg-muted/45 p-5 lg:p-6"
        >
            <section aria-labelledby="purchase-order-amount-heading">
                <h2
                    id="purchase-order-amount-heading"
                    className="text-lg font-semibold"
                >
                    金额信息
                </h2>
                <p className="mt-5 text-sm text-muted-foreground">
                    采购金额（含税）
                </p>
                <div className="mt-1 wrap-anywhere text-[44px] leading-tight font-semibold tracking-tight">
                    {amount(order.currentContent.totals.gross)}
                </div>
                <dl className="mt-5 grid grid-cols-3 gap-3">
                    {amounts.map((item) => (
                        <div key={item.label} className="min-w-0">
                            <dt className="text-xs text-muted-foreground">
                                {item.label}
                            </dt>
                            <dd className="mt-1.5 wrap-anywhere text-lg font-semibold">
                                {amount(item.value)}
                            </dd>
                        </div>
                    ))}
                </dl>
                {!order.payableSummary ? (
                    <p className="mt-3 text-xs text-muted-foreground">
                        尚未形成应付（需审批通过）。
                    </p>
                ) : null}
                <dl className="mt-4 space-y-2 border-t border-border/70 pt-4 text-sm">
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">不含税金额</dt>
                        <dd className="min-w-0 wrap-anywhere text-right">
                            {amount(order.currentContent.totals.net)}
                        </dd>
                    </div>
                    <div className="flex justify-between gap-4">
                        <dt className="text-muted-foreground">税额</dt>
                        <dd className="min-w-0 wrap-anywhere text-right">
                            {amount(order.currentContent.totals.tax)}
                        </dd>
                    </div>
                </dl>
            </section>
            <section
                aria-labelledby="purchase-order-progress-heading"
                className="mt-6 border-t border-border/70 pt-5"
            >
                <h2
                    id="purchase-order-progress-heading"
                    className="text-lg font-semibold"
                >
                    办理进度
                </h2>
                <div className="mt-3 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <StatusBadge
                        tone={order.identity.statusTone}
                        label={order.identity.statusLabel}
                        className="border-0 shadow-none"
                    />
                    {order.approval?.instance?.currentAssigneeName ? (
                        <span>
                            当前审批人{" "}
                            {order.approval.instance.currentAssigneeName}
                        </span>
                    ) : null}
                </div>
                <dl className="mt-5 space-y-4">
                    {tracks.map((item) => (
                        <div
                            key={item.label}
                            className="flex items-baseline justify-between gap-4 text-sm"
                        >
                            <dt className="inline-flex items-center gap-3 text-muted-foreground">
                                <span
                                    aria-hidden="true"
                                    className="size-2 rounded-full bg-border"
                                />
                                {item.label}
                            </dt>
                            <dd className="min-w-0 break-words text-right">
                                {item.value}
                            </dd>
                        </div>
                    ))}
                </dl>
                {gate.state !== "NOT_APPLICABLE" ? (
                    <div className="mt-5 rounded-lg border border-border/70 p-3">
                        <p className="text-sm font-medium">
                            先款条件 ·{" "}
                            {gate.state === "SATISFIED" ? "已满足" : "未满足"}
                        </p>
                        <p className="mt-2 text-xs leading-5 text-muted-foreground">
                            {gate.message}
                        </p>
                        <dl className="mt-3 grid grid-cols-3 gap-2 text-xs">
                            {[
                                { label: "需先款", value: gate.required },
                                { label: "已核销", value: gate.allocated },
                                { label: "还差", value: gate.gap },
                            ].map((item) => (
                                <div key={item.label} className="min-w-0">
                                    <dt className="text-muted-foreground">
                                        {item.label}
                                    </dt>
                                    <dd className="mt-1 wrap-anywhere text-sm">
                                        {amount(item.value)}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                        <p className="mt-3 text-xs text-muted-foreground">
                            更新于{" "}
                            <time dateTime={gate.updatedAt} className="num">
                                {formatDateTime(gate.updatedAt, "default")}
                            </time>
                        </p>
                    </div>
                ) : null}
            </section>
            <section
                aria-labelledby="purchase-order-owner-heading"
                className="mt-6 border-t border-border/70 pt-5"
            >
                <h2
                    id="purchase-order-owner-heading"
                    className="text-lg font-semibold"
                >
                    责任人信息
                </h2>
                <dl className="mt-4 grid grid-cols-3 gap-3 text-sm">
                    {[
                        { label: "负责人", value: order.header.ownerName },
                        {
                            label: "提交人",
                            value: order.header.submittedBy ?? "—",
                        },
                        {
                            label: "履约责任",
                            value: FULFILLMENT_RESPONSIBILITY_LABEL[
                                order.header.fulfillmentResponsibility
                            ],
                        },
                    ].map((item) => (
                        <div key={item.label} className="min-w-0">
                            <dt className="text-xs text-muted-foreground">
                                {item.label}
                            </dt>
                            <dd className="mt-2 break-words">{item.value}</dd>
                        </div>
                    ))}
                </dl>
                <p className="mt-4 text-xs text-muted-foreground">
                    最近预计交期{" "}
                    <span className="num">
                        {order.header.expectedDate ?? "—"}
                    </span>
                </p>
                <p className="mt-3 break-all text-xs text-muted-foreground">
                    来源销售单 {order.header.salesOrderNo}
                </p>
            </section>
        </aside>
    )
}
