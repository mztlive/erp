"use client"
import { SalesOrderInquirySource } from "./sales-order-inquiry-source"

import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import * as React from "react"

import { SalesOrderSelectionSource } from "./sales-order-selection-source"
import { MoneyValue, QuantityValue } from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { paymentTermLabel, welfareScenarioLabel } from "@/lib/business-options"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { cn } from "@/lib/utils"
import { SalesOrderDeliverySummary } from "./sales-order-delivery-summary"

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
        <div className="grid min-w-0 grid-cols-[7rem_minmax(0,1fr)] items-baseline gap-3">
            <dt className="text-sm text-muted-foreground">{label}</dt>
            <dd
                className={cn(
                    "break-words text-sm leading-6",
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
        <div className="overflow-hidden rounded-lg border-b border-border">
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
                                <div className="flex flex-wrap items-center gap-2">
                                    <span>{line.name}</span>
                                    {line.isGift ? (
                                        <Badge variant="secondary">赠品</Badge>
                                    ) : null}
                                </div>
                                {line.sku && line.sku !== "无规格" ? (
                                    <div className="num text-xs text-muted-foreground">
                                        {line.sku}
                                    </div>
                                ) : null}
                            </TableCell>
                            {showDelivery ? (
                                <TableCell className="min-w-44 whitespace-normal">
                                    <SalesOrderDeliverySummary line={line} />
                                </TableCell>
                            ) : null}
                            <TableCell data-align="end">
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
                            <TableCell data-align="end">
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

/** 概览展示当前生效版本的商品摘要；完整交付明细在交付分区查看。 */
export function OverviewPanel({
    order,
    related,
    onOpenFulfillment,
}: {
    order: SalesOrderDetailView
    related?: React.ReactNode
    onOpenFulfillment?: () => void
}) {
    const isCard = order.nature === "card_voucher"
    return (
        <div className="space-y-5">
            <section
                aria-labelledby="sales-order-lines-heading"
                className="min-w-0 rounded-lg border border-border/70 p-4 md:p-5"
            >
                <div className="mb-4 flex flex-wrap items-baseline justify-between gap-2">
                    <h2
                        id="sales-order-lines-heading"
                        className="text-lg font-semibold"
                    >
                        {isCard ? "卡券明细" : "销售摘要"}
                        {order.currentRevisionNo != null ? (
                            <span className="ml-3 text-xs font-normal text-muted-foreground">
                                当前生效 v{order.currentRevisionNo}
                            </span>
                        ) : null}
                    </h2>
                    <div className="flex flex-wrap items-center gap-3">
                        <p className="text-xs text-muted-foreground">
                            共 {order.lineItems.length} 行
                        </p>
                        {!isCard && onOpenFulfillment ? (
                            <Button
                                id="sales-order-overview-open-fulfillment"
                                variant="outline"
                                size="sm"
                                onClick={onOpenFulfillment}
                            >
                                查看销售明细与交付
                            </Button>
                        ) : null}
                    </div>
                </div>
                {isCard ? (
                    <CardLineItemsTable order={order} />
                ) : (
                    <>
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>商品 / 服务</TableHead>
                                    <TableHead data-align="end">数量</TableHead>
                                    <TableHead>交付期限</TableHead>
                                    <TableHead data-align="end">
                                        含税金额
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {order.lineItems.slice(0, 3).map((line) => (
                                    <TableRow key={line.id}>
                                        <TableCell className="min-w-40 whitespace-normal">
                                            <span>{line.name}</span>
                                            {line.isGift ? (
                                                <Badge
                                                    className="ml-2"
                                                    variant="secondary"
                                                >
                                                    赠品
                                                </Badge>
                                            ) : null}
                                            {line.sku &&
                                            line.sku !== "无规格" ? (
                                                <p className="mt-1 text-xs text-muted-foreground">
                                                    {line.sku}
                                                </p>
                                            ) : null}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <QuantityValue
                                                value={line.quantity}
                                                unit={line.unit}
                                            />
                                        </TableCell>
                                        <TableCell>
                                            {line.dueDate || "未填写"}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={line.amountGross}
                                            />
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                        {order.lineItems.length === 0 ? (
                            <p className="py-4 text-sm text-muted-foreground">
                                暂无销售明细
                            </p>
                        ) : null}
                        {order.lineItems.length > 3 ? (
                            <p className="mt-3 text-xs text-muted-foreground">
                                已展示前 3 项，其余 {order.lineItems.length - 3}{" "}
                                项请查看销售明细与交付。
                            </p>
                        ) : null}
                    </>
                )}
                <div className="flex items-baseline justify-end gap-5 pt-5 text-sm">
                    <span className="text-muted-foreground">合计（含税）</span>
                    <MoneyValue
                        value={order.amountGross}
                        className="text-xl font-semibold"
                    />
                </div>
            </section>
            <section
                aria-labelledby="sales-order-transaction-heading"
                className="rounded-lg border border-border/70 p-4 md:p-5"
            >
                <h2
                    id="sales-order-transaction-heading"
                    className="mb-5 text-lg font-semibold"
                >
                    交易约定
                </h2>
                <dl className="grid gap-x-8 gap-y-4 2xl:grid-cols-2">
                    <OverviewField
                        label="关联合同"
                        value={order.contractRevisionLabel || "无合同"}
                    />
                    <OverviewField
                        label="结算主体"
                        value={order.settlementEntity || "—"}
                    />
                    <OverviewField
                        label="福利场景"
                        value={welfareScenarioLabel(order.welfareScene) || "—"}
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
                        label="客户联系人"
                        value={order.customerContact ?? "—"}
                    />
                    <OverviewField
                        label={
                            isCard ? "履约期限（到期交付）" : "客户承诺期限摘要"
                        }
                        value={order.fulfillmentDeadline || "—"}
                        numeric
                    />
                    <OverviewField
                        label="当前销售版本"
                        value={
                            order.currentRevisionNo == null
                                ? "尚未生效"
                                : `v${order.currentRevisionNo}`
                        }
                    />
                    {isCard ? (
                        <OverviewField
                            label="应收到期日"
                            value={order.receivableDueDate || "—"}
                            numeric
                        />
                    ) : null}
                    {order.remark?.trim() ? (
                        <div className="2xl:col-span-2">
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
                </dl>
            </section>
            {order.inquirySource && (
                <SalesOrderInquirySource source={order.inquirySource} compact />
            )}
            {order.selectionSource && (
                <SalesOrderSelectionSource source={order.selectionSource} />
            )}
            {related}
        </div>
    )
}
