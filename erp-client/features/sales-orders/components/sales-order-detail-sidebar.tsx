"use client"

import type { ReactNode } from "react"

import { MoneyValue } from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { LifecycleRail } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import { stageDueDisplay } from "@/features/sales-orders/lib/labels"
import {
    nextStepOwner,
    type FocusTask,
} from "@/features/sales-orders/lib/sales-order-detail-model"
import { remainingReceivableAmount } from "@/features/sales-orders/lib/sales-order-receivable"

/** 已提交销售单的金额、办理进度和责任信息。操作由页面传入，沿用原有权限。 */
export function SalesOrderDetailSidebar({
    order,
    focusTask,
    action,
    hideFinanceSummary = false,
}: {
    order: SalesOrderDetailView
    focusTask?: FocusTask | null
    hideFinanceSummary?: boolean
    action?: ReactNode
}) {
    const due = stageDueDisplay(order)
    const amounts = [
        { label: "已回款", value: order.receivedAmount },
        {
            label: "待回款",
            value: remainingReceivableAmount(
                order.amountGross,
                order.receivedAmount,
            ),
        },
        { label: "已开票", value: order.invoicedAmount },
    ]

    return (
        <aside
            aria-label="销售单摘要"
            className="min-w-0 self-start rounded-xl bg-muted/45 p-5 lg:p-6"
        >
            <section aria-labelledby="sales-order-amount-heading">
                <h2
                    id="sales-order-amount-heading"
                    className="text-lg font-semibold"
                >
                    金额信息
                </h2>
                <p className="mt-5 text-sm text-muted-foreground">
                    成交金额（含税）
                </p>
                <MoneyValue
                    value={order.amountGross}
                    className="mt-1 block wrap-anywhere text-[44px] leading-tight font-semibold tracking-tight"
                />
                {!hideFinanceSummary ? (
                    <>
                        <dl className="mt-5 grid grid-cols-3 gap-3">
                            {amounts.map((item) => (
                                <div key={item.label} className="min-w-0">
                                    <dt className="text-xs text-muted-foreground">
                                        {item.label}
                                    </dt>
                                    <dd className="mt-1.5 wrap-anywhere text-lg font-semibold">
                                        <MoneyValue value={item.value} />
                                    </dd>
                                </div>
                            ))}
                        </dl>
                        <div className="mt-3 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                            <span className="inline-flex items-center gap-1">
                                回款{" "}
                                <StatusBadge
                                    tone={order.collection.tone}
                                    label={order.collection.label}
                                    className="border-0 bg-transparent shadow-none"
                                />
                            </span>
                            <span className="inline-flex items-center gap-1">
                                开票{" "}
                                <StatusBadge
                                    tone={order.invoicing.tone}
                                    label={order.invoicing.label}
                                    className="border-0 bg-transparent shadow-none"
                                />
                            </span>
                        </div>
                    </>
                ) : null}
            </section>
            <section
                aria-labelledby="sales-order-progress-heading"
                className="mt-6 border-t border-border/70 pt-5"
            >
                <h2
                    id="sales-order-progress-heading"
                    className="text-lg font-semibold"
                >
                    {focusTask?.id === "approval" ? "审批进度" : "办理进度"}
                </h2>
                <div className="mt-3 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <StatusBadge
                        tone={order.primaryStatus.tone}
                        label={order.primaryStatus.label}
                        className="border-0 shadow-none"
                    />
                    <span>{nextStepOwner(order)}</span>
                </div>
                {focusTask?.description ? (
                    <p className="mt-2 text-xs leading-5 text-muted-foreground">
                        {focusTask.description}
                    </p>
                ) : null}
                {due ? (
                    <p className="mt-2 text-xs text-muted-foreground">
                        时限 {due.label}
                    </p>
                ) : null}
                <div className="mt-5">
                    <LifecycleRail order={order} />
                </div>
                {action ? (
                    <div className="mt-4 [&>button]:w-full">{action}</div>
                ) : null}
            </section>
            <section
                aria-labelledby="sales-order-owner-heading"
                className="mt-6 border-t border-border/70 pt-5"
            >
                <h2
                    id="sales-order-owner-heading"
                    className="text-lg font-semibold"
                >
                    责任人信息
                </h2>
                <dl className="mt-4 grid grid-cols-3 gap-3 text-sm">
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            负责人
                        </dt>
                        <dd className="mt-2 break-words">{order.ownerName}</dd>
                    </div>
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            创建于
                        </dt>
                        <dd className="mt-2 break-words">
                            {order.originSystem === "erp" ? "ERP" : "商城"}
                        </dd>
                    </div>
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">履约</dt>
                        <dd className="mt-2 break-words">
                            {order.fulfillment.label}
                        </dd>
                    </div>
                </dl>
            </section>
        </aside>
    )
}
