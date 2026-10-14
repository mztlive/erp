"use client"

import type { ReactNode } from "react"
import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { LinesTable } from "@/features/purchase-orders/components/purchase-order-surfaces"
import { PurchaseOrderExpenseSection } from "@/features/purchase-orders/components/purchase-order-expense-section"
import { PurchaseOrderSourceSales } from "@/features/purchase-orders/components/purchase-order-source-sales"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { DetailRecordSection } from "@/components/business/detail-presentation"
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
    return (
        <div className="divide-y divide-border/70">
            <DetailRecordSection
                title="采购明细"
                count={order.currentContent.lines.length}
            >
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
            </DetailRecordSection>
            <PurchaseOrderExpenseSection
                order={order}
                costMasked={costMasked}
            />
            <DetailRecordSection title="交易约定">
                <dl className="grid gap-x-8 gap-y-4 2xl:grid-cols-2">
                    <OverviewField label="来源销售单">
                        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                            <Link
                                id={`procurement-orders-source-sales-${toAutomationIdSegment(order.identity.purchaseOrderId)}-detail`}
                                href={`/sales/orders/${encodeURIComponent(order.header.salesOrderId)}`}
                                className="inline-flex max-w-full items-center gap-1 rounded-sm text-primary underline-offset-4 hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
                                aria-label={`查看销售单 ${order.header.salesOrderNo}`}
                            >
                                <span className="num break-all">
                                    {order.header.salesOrderNo}
                                </span>
                                <ArrowUpRightIcon
                                    className="size-3.5 shrink-0"
                                    aria-hidden="true"
                                />
                            </Link>
                            <PurchaseOrderSourceSales order={order} />
                        </div>
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
                    <OverviewField label="采购类型">
                        {PURCHASE_TYPE_LABEL[order.header.purchaseType]}
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
                </dl>
            </DetailRecordSection>
        </div>
    )
}
