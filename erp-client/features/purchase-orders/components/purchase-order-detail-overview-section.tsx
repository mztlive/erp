"use client"

import type { ReactNode } from "react"
import Link from "next/link"

import { MoneyValue } from "@/components/business"
import { LinesTable } from "@/features/purchase-orders/components/purchase-order-surfaces"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

function OverviewField({
    label,
    children,
}: {
    label: string
    children: ReactNode
}) {
    return (
        <div className="grid min-w-0 grid-cols-[7rem_minmax(0,1fr)] items-baseline gap-3">
            <dt className="text-sm text-muted-foreground">{label}</dt>
            <dd className="min-w-0 break-words text-sm leading-6">
                {children}
            </dd>
        </div>
    )
}

/** 概览先展示采购明细，再展示来源与交易约定；金额和责任摘要由侧栏承载。 */
export function PurchaseOrderDetailOverviewSection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const id = toAutomationIdSegment(order.identity.purchaseOrderId)
    return (
        <div className="space-y-5">
            <section
                aria-labelledby="purchase-order-lines-heading"
                className="min-w-0 rounded-lg border border-border/70 p-4 md:p-5"
            >
                <div className="mb-4 flex items-baseline justify-between gap-2">
                    <h2
                        id="purchase-order-lines-heading"
                        className="text-lg font-semibold"
                    >
                        采购明细
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        共 {order.currentContent.lines.length} 行
                    </p>
                </div>
                <LinesTable order={order} costMasked={costMasked} summary />
                <div className="flex items-baseline justify-end gap-5 pt-5 text-sm">
                    <span className="text-muted-foreground">合计（含税）</span>
                    <span className="text-xl font-semibold">
                        {costMasked ? (
                            "•••"
                        ) : (
                            <MoneyValue
                                value={order.currentContent.totals.gross}
                            />
                        )}
                    </span>
                </div>
            </section>
            <section
                aria-labelledby="purchase-order-transaction-heading"
                className="rounded-lg border border-border/70 p-4 md:p-5"
            >
                <h2
                    id="purchase-order-transaction-heading"
                    className="mb-5 text-lg font-semibold"
                >
                    交易约定
                </h2>
                <dl className="grid gap-x-8 gap-y-4 2xl:grid-cols-2">
                    <OverviewField label="来源销售单">
                        <Link
                            id={`procurement-orders-detail-overview-sales-order-${id}`}
                            href={`/sales/orders/${order.header.salesOrderId}`}
                            className="num break-all text-primary underline-offset-2 hover:underline"
                        >
                            {order.header.salesOrderNo}
                        </Link>
                    </OverviewField>
                    <OverviewField label="同销售单采购单">
                        <Link
                            id={`procurement-orders-detail-overview-related-${id}`}
                            href={`/procurement/orders?salesOrderId=${encodeURIComponent(order.header.salesOrderId)}`}
                            className="text-primary underline-offset-2 hover:underline"
                        >
                            查看拆分结果
                        </Link>
                    </OverviewField>
                    <OverviewField label="付款条件">
                        {order.header.paymentTermLabel || "—"}
                    </OverviewField>
                    <OverviewField label="履约责任">
                        {
                            FULFILLMENT_RESPONSIBILITY_LABEL[
                                order.header.fulfillmentResponsibility
                            ]
                        }
                    </OverviewField>
                    <OverviewField label="最近预计交期">
                        <span className="num">
                            {order.header.expectedDate ?? "—"}
                        </span>
                    </OverviewField>
                    <OverviewField label="当前采购版本">
                        {order.identity.revisionNo == null
                            ? "尚未生效"
                            : `v${order.identity.revisionNo}`}
                    </OverviewField>
                    <OverviewField label="内容来源">
                        {order.currentContent.source === "DRAFT"
                            ? "草稿"
                            : order.currentContent.source === "SUBMISSION"
                              ? "已提交内容"
                              : "生效版本"}
                    </OverviewField>
                    {order.header.targetWarehouseId ? (
                        <OverviewField label="目标收货仓">
                            <Link
                                id={`procurement-orders-detail-overview-warehouse-${id}`}
                                href={`/master-data/warehouses/${encodeURIComponent(order.header.targetWarehouseId)}`}
                                className="text-primary underline-offset-2 hover:underline"
                            >
                                查看收货仓资料
                            </Link>
                        </OverviewField>
                    ) : null}
                </dl>
            </section>
        </div>
    )
}
