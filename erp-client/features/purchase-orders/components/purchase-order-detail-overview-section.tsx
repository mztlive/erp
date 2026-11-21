"use client"

import type { ReactNode } from "react"
import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { MoneyValue, detailWorkspaceStyles } from "@/components/business"
import { PurchaseOrderExpenseSection } from "@/features/purchase-orders/components/purchase-order-expense-section"
import { PurchaseOrderSourceSales } from "@/features/purchase-orders/components/purchase-order-source-sales"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
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

/** 概览先处理费用确认，再展示采购明细、来源与交易约定；金额和进度摘要由顶部通栏与侧栏承载。 */
export function PurchaseOrderDetailOverviewSection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const expenseOnly =
        order.currentContent.lines.length > 0 &&
        order.currentContent.lines.every((line) => line.expense)
    return (
        <>
            <PurchaseOrderExpenseSection
                order={order}
                costMasked={costMasked}
            />

            {!expenseOnly ? (
                <DetailRecordSection
                    width="full"
                    title="采购明细"
                    count={order.currentContent.lines.length}
                >
                    <div
                        data-slot="detail-inline-table"
                        className={detailWorkspaceStyles.dataTableWrapper}
                    >
                        <PurchaseOrderDetailLines
                            order={order}
                            costMasked={costMasked}
                        />
                    </div>
                    <div className="flex items-baseline justify-end gap-4 pt-1 text-body-compact">
                        <span className="text-xs text-muted-foreground">
                            合计（含税）
                        </span>
                        <span className="num text-value-compact font-semibold tracking-tight text-foreground">
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
            ) : null}

            <DetailRecordSection width="form" title="关联单据与基本信息">
                <dl className="grid gap-x-8 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
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
                    <OverviewField label="采购类型">
                        {PURCHASE_TYPE_LABEL[order.header.purchaseType]}
                    </OverviewField>
                    <OverviewField label="修订版次">
                        <span className="num">
                            {order.identity.revisionNo == null
                                ? "尚未生效"
                                : `v${order.identity.revisionNo}`}
                        </span>
                    </OverviewField>
                    <OverviewField label="当前内容">
                        {order.currentContent.source === "DRAFT"
                            ? "草稿"
                            : order.currentContent.source === "SUBMISSION"
                              ? "已提交审批"
                              : "已生效"}
                    </OverviewField>
                </dl>
            </DetailRecordSection>

            <DetailRecordSection width="form" title="付款与履约约定">
                <dl className="grid gap-x-8 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
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
                </dl>
            </DetailRecordSection>
        </>
    )
}
