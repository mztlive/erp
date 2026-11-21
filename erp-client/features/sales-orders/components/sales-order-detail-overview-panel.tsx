"use client"

import * as React from "react"

import {
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { ArrangementSummary } from "@/features/delivery-arrangements/components/arrangement-summary"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { paymentTermLabel, welfareScenarioLabel } from "@/lib/business-options"
import { cn } from "@/lib/utils"
import { SalesOrderDeliverySummary } from "./sales-order-delivery-summary"
import { SalesOrderInquirySource } from "./sales-order-inquiry-source"
import { SalesOrderSelectionSource } from "./sales-order-selection-source"

function OverviewField({
    label,
    value,
    numeric,
}: {
    label: string
    value: React.ReactNode
    numeric?: boolean
}) {
    return (
        <div className="min-w-0 space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
                {label}
            </dt>
            <dd
                className={cn(
                    "break-words text-body-compact text-foreground",
                    numeric && "num",
                )}
            >
                {value}
            </dd>
        </div>
    )
}

/** 卡券明细保留面值、配赠、卡形态及交付地址。 */
function CardLineItemsTable({ order }: { order: SalesOrderDetailView }) {
    const showDelivery = order.lineItems.some(
        (line) => line.deliveryAllocations?.length,
    )
    return (
        <div
            data-slot="detail-inline-table"
            className={detailWorkspaceStyles.dataTableWrapper}
        >
            <Table data-density="comfortable" className="min-w-[54rem]">
                <TableHeader>
                    <TableRow>
                        <TableHead>项目</TableHead>
                        {showDelivery ? <TableHead>交付地址</TableHead> : null}
                        <TableHead data-align="end">数量 / 单位</TableHead>
                        <TableHead data-align="end">含税单价</TableHead>
                        <TableHead data-align="end">面值</TableHead>
                        <TableHead data-align="end">配赠</TableHead>
                        <TableHead>卡形态</TableHead>
                        <TableHead data-align="end">含税小计</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {order.lineItems.map((line) => (
                        <TableRow key={line.id}>
                            <TableCell className="min-w-44 whitespace-normal">
                                <div className="flex flex-wrap items-center gap-2 font-medium">
                                    <span>{line.name}</span>
                                    {line.isGift ? (
                                        <Badge variant="secondary">赠品</Badge>
                                    ) : null}
                                </div>
                                {line.sku && line.sku !== "无规格" ? (
                                    <div className="num mt-0.5 text-xs text-muted-foreground">
                                        {line.sku}
                                    </div>
                                ) : null}
                            </TableCell>
                            {showDelivery ? (
                                <TableCell className="min-w-44 whitespace-normal">
                                    <SalesOrderDeliverySummary line={line} />
                                </TableCell>
                            ) : null}
                            <TableCell data-align="end" className="num">
                                {line.quantity} {line.unit}
                            </TableCell>
                            <TableCell data-align="end">
                                <MoneyValue
                                    display="exact"
                                    value={line.unitPriceGross}
                                />
                            </TableCell>
                            <TableCell data-align="end">
                                {line.faceValue ? (
                                    <MoneyValue
                                        display="exact"
                                        value={line.faceValue}
                                    />
                                ) : (
                                    "—"
                                )}
                            </TableCell>
                            <TableCell data-align="end" className="num">
                                {line.giftRate ? `${line.giftRate}%` : "—"}
                            </TableCell>
                            <TableCell>{line.cardForm || "—"}</TableCell>
                            <TableCell data-align="end">
                                <MoneyValue value={line.amountGross} />
                            </TableCell>
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}

/** 概览保留交易约定；实物服务明细统一在交付分区查看，卡券保留专用明细。 */
export function OverviewPanel({
    order,
    related,
}: {
    order: SalesOrderDetailView
    related?: React.ReactNode
}) {
    const isCard = order.nature === "card_voucher"
    return (
        <>
            {isCard ? (
                <DetailWorkspaceSection
                    width="full"
                    title="卡券明细"
                    headingId="sales-order-lines-heading"
                    subtitle={`共 ${order.lineItems.length} 行`}
                >
                    <CardLineItemsTable order={order} />
                    <div className="flex items-baseline justify-end gap-4 pt-1 text-body-compact">
                        <span className="text-xs text-muted-foreground">
                            合计（含税）
                        </span>
                        <MoneyValue
                            value={order.amountGross}
                            className="text-value-compact font-semibold tracking-tight"
                        />
                    </div>
                </DetailWorkspaceSection>
            ) : null}

            <DetailWorkspaceSection
                width="form"
                title="交易约定"
                headingId="sales-order-transaction-heading"
            >
                <dl className="@container/terms grid gap-x-8 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
                    <OverviewField
                        label="结算主体"
                        value={order.settlementEntity || "—"}
                    />
                    <OverviewField
                        label="关联合同"
                        value={
                            order.contractRevisionLabel ||
                            (order.contractId ? "合同资料暂不可查看" : "无合同")
                        }
                    />
                    <OverviewField
                        label="付款条件"
                        value={paymentTermLabel(order.paymentTerms) || "—"}
                    />
                    <OverviewField
                        label="税率"
                        value={
                            order.taxRatePercent
                                ? `${order.taxRatePercent}%`
                                : "—"
                        }
                        numeric
                    />
                    <OverviewField
                        label="福利场景"
                        value={welfareScenarioLabel(order.welfareScene) || "—"}
                    />
                    <OverviewField
                        label="客户联系人"
                        value={
                            order.customerContact ??
                            (order.loadWarnings?.some((warning) =>
                                ["客户资料", "客户联系人"].includes(
                                    warning.source,
                                ),
                            )
                                ? "暂不可查看"
                                : "—")
                        }
                    />
                    <OverviewField
                        label={isCard ? "履约期限" : "承诺交期"}
                        value={order.fulfillmentDeadline || "—"}
                        numeric
                    />
                    {isCard ? (
                        <OverviewField
                            label="应收到期日"
                            value={order.receivableDueDate || "—"}
                            numeric
                        />
                    ) : null}
                    {order.remark?.trim() ? (
                        <div className="col-span-full border-t border-border/60 pt-4">
                            <OverviewField
                                label="内部说明"
                                value={
                                    <span className="whitespace-pre-wrap">
                                        {order.remark.trim()}
                                    </span>
                                }
                            />
                        </div>
                    ) : null}
                    {order.inquirySource || order.selectionSource ? (
                        <div className="col-span-full flex flex-wrap items-baseline gap-x-4 gap-y-2 border-t border-border/60 pt-4">
                            <dt className="text-xs font-medium text-muted-foreground">
                                业务来源
                            </dt>
                            <dd className="min-w-0 space-y-2">
                                {order.inquirySource ? (
                                    <SalesOrderInquirySource
                                        source={order.inquirySource}
                                        compact
                                    />
                                ) : null}
                                {order.selectionSource ? (
                                    <SalesOrderSelectionSource
                                        source={order.selectionSource}
                                        compact
                                    />
                                ) : null}
                            </dd>
                        </div>
                    ) : null}
                </dl>
            </DetailWorkspaceSection>

            {order.currentRevisionNo &&
            !isCard &&
            !["已关闭", "已作废"].includes(order.primaryStatus.label) ? (
                <ArrangementSummary salesOrderId={order.id} />
            ) : null}

            {related}
        </>
    )
}
