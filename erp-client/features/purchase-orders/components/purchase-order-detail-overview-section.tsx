"use client"

import type { ReactNode } from "react"
import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { DetailInlineTable, MoneyValue } from "@/components/business"
import { PurchaseOrderExpenseSection } from "@/features/purchase-orders/components/purchase-order-expense-section"
import { PurchaseOrderSourceSales } from "@/features/purchase-orders/components/purchase-order-source-sales"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    SUPPLY_PRICE_BASIS_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { sumFixed } from "@/lib/fixed-decimal"
import { PurchaseOrderDetailLines } from "./purchase-order-detail-lines"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"

function OverviewField({
    label,
    children,
}: {
    label: string
    children: ReactNode
}) {
    return (
        <div className="min-w-0 space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
                {label}
            </dt>
            <dd className="min-w-0 break-words text-body-compact text-foreground">
                {children}
            </dd>
        </div>
    )
}

/**
 * 概览聚焦业务核心：先处理费用确认（如有），再展示商品/服务明细，
 * 最后在一个紧凑分区内收口来源单据与履约付款约定；已置于吸顶头的身份信息不再重复。
 */
export function PurchaseOrderDetailOverviewSection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const expenseLines = order.currentContent.lines.filter(
        (line) => line.expense,
    )
    const productLines = order.currentContent.lines.filter(
        (line) => !line.expense,
    )
    const hasExpenses = expenseLines.length > 0
    const showProductLines = productLines.length > 0 || !hasExpenses
    const notEffective = order.identity.revisionNo == null
    const productSubtotal =
        productLines.length > 0
            ? hasExpenses
                ? sumFixed(
                      productLines.map((line) => line.grossAmount),
                      { maxScale: 2, outputScale: 2 },
                  )
                : order.currentContent.totals.gross
            : null

    return (
        <>
            <PurchaseOrderExpenseSection
                order={order}
                costMasked={costMasked}
            />

            {showProductLines ? (
                <DetailRecordSection
                    width="full"
                    title="采购明细"
                    count={productLines.length}
                >
                    <DetailInlineTable frame="scroll">
                        <PurchaseOrderDetailLines
                            order={order}
                            lines={productLines}
                            costMasked={costMasked}
                        />
                    </DetailInlineTable>
                    {productSubtotal != null ? (
                        <div className="flex flex-wrap items-baseline justify-end gap-x-4 gap-y-1 pt-1 text-xs text-muted-foreground">
                            <span className="num">
                                共 {productLines.length} 项
                            </span>
                            <span>
                                {hasExpenses ? "商品含税小计" : "含税合计"}{" "}
                                <span className="num text-body-compact font-semibold text-foreground">
                                    {costMasked ? (
                                        "•••"
                                    ) : (
                                        <MoneyValue value={productSubtotal} />
                                    )}
                                </span>
                            </span>
                        </div>
                    ) : null}
                </DetailRecordSection>
            ) : null}

            <DetailRecordSection width="form" title="关联单据与基本信息">
                <dl className="grid gap-x-8 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
                    <OverviewField label="来源销售单">
                        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5">
                            <Link
                                id={`procurement-orders-source-sales-${toAutomationIdSegment(order.identity.purchaseOrderId)}-detail`}
                                href={`/sales/orders/${encodeURIComponent(order.header.salesOrderId)}`}
                                className="inline-flex max-w-full items-center gap-1 rounded-sm font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
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

                    {order.header.targetWarehouseName ? (
                        <OverviewField label="目标收货仓">
                            {order.header.targetWarehouseName}
                        </OverviewField>
                    ) : null}

                    {order.delivery ? (
                        <OverviewField label="价格依据">
                            {
                                SUPPLY_PRICE_BASIS_LABEL[
                                    order.delivery.priceBasis
                                ]
                            }
                        </OverviewField>
                    ) : null}

                    {productLines.length > 0 && order.header.expectedDate ? (
                        <OverviewField label="最早预计交期">
                            <span className="num">
                                {order.header.expectedDate}
                            </span>
                        </OverviewField>
                    ) : null}

                    {order.header.createdAt ? (
                        <OverviewField label="创建时间">
                            <span className="num">
                                {formatDateTime(
                                    order.header.createdAt,
                                    "default",
                                )}
                            </span>
                        </OverviewField>
                    ) : null}

                    {notEffective ? (
                        <OverviewField label="生效状态">
                            <span className="inline-flex flex-wrap items-center gap-1.5">
                                <span>尚未生效</span>
                                <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                                    {order.currentContent.source === "DRAFT"
                                        ? "草稿"
                                        : "已提交审批"}
                                </span>
                            </span>
                        </OverviewField>
                    ) : null}
                </dl>
            </DetailRecordSection>
        </>
    )
}
