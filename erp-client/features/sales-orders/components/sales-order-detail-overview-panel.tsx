"use client"
import { SalesOrderInquirySource } from "./sales-order-inquiry-source"

import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import * as React from "react"

import { SalesOrderSelectionSource } from "./sales-order-selection-source"
import { MoneyValue } from "@/components/business"
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
import {
    DeliveryLineStatus,
    DeliveryProgressNotice,
    type DeliveryProgressQuery,
} from "./sales-order-delivery-overview"

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

export function LineItemsTable({
    order,
    deliveryQuery,
    canReadDelivery = false,
}: {
    order: SalesOrderDetailView
    deliveryQuery?: DeliveryProgressQuery
    canReadDelivery?: boolean
}) {
    const isCard = order.nature === "card_voucher"
    const showDelivery =
        isCard &&
        order.lineItems.some((line) => line.deliveryAllocations?.length)
    const progressByLine = new Map(
        canReadDelivery &&
            order.currentRevisionNo != null &&
            !deliveryQuery?.isError
            ? deliveryQuery?.data?.lines.map((line) => [
                  line.salesOrderLineId,
                  line,
              ])
            : [],
    )
    return (
        <div className="overflow-hidden rounded-lg border-b border-border">
            <Table
                data-density="comfortable"
                className={isCard ? "min-w-[54rem]" : "min-w-[44rem]"}
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>项目</TableHead>
                        {showDelivery ? <TableHead>交付地址</TableHead> : null}
                        <TableHead data-align="end">数量 / 单位</TableHead>
                        <TableHead data-align="end">含税单价</TableHead>
                        {isCard ? (
                            <>
                                <TableHead data-align="end">面值</TableHead>
                                <TableHead data-align="end">配赠</TableHead>
                                <TableHead>卡形态</TableHead>
                            </>
                        ) : (
                            <TableHead>期望交付日</TableHead>
                        )}
                        <TableHead data-align="end">含税小计</TableHead>
                        {!isCard ? <TableHead>交付状态</TableHead> : null}
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
                                {!isCard ? (
                                    <SalesOrderDeliverySummary line={line} />
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
                                {!isCard && line.procurementSupplyPrice ? (
                                    <div className="mt-1 text-xs text-muted-foreground">
                                        采购供给价{" "}
                                        <MoneyValue
                                            value={line.procurementSupplyPrice}
                                        />
                                    </div>
                                ) : null}
                            </TableCell>
                            {isCard ? (
                                <>
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
                                        {line.giftRate
                                            ? `${line.giftRate}%`
                                            : "—"}
                                    </TableCell>
                                    <TableCell>
                                        {line.cardForm || "—"}
                                    </TableCell>
                                </>
                            ) : (
                                <TableCell className="text-muted-foreground">
                                    {line.dueDate || "—"}
                                </TableCell>
                            )}
                            <TableCell data-align="end">
                                <MoneyValue value={line.amountGross} />
                            </TableCell>
                            {!isCard ? (
                                <TableCell>
                                    {order.currentRevisionNo == null ? (
                                        <span className="text-xs text-muted-foreground">
                                            尚未生效
                                        </span>
                                    ) : (
                                        <DeliveryLineStatus
                                            progress={progressByLine.get(
                                                line.id,
                                            )}
                                        />
                                    )}
                                </TableCell>
                            ) : null}
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}

/** 概览以唯一商品表汇总成交与交付状态，交易约定独立展示。 */
export function OverviewPanel({
    order,
    related,
    deliveryQuery,
    canReadDelivery,
    onOpenFulfillment,
}: {
    order: SalesOrderDetailView
    related?: React.ReactNode
    deliveryQuery: DeliveryProgressQuery
    canReadDelivery: boolean
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
                        {isCard ? "卡券明细" : "销售明细"}
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
                                查看采购与交付
                            </Button>
                        ) : null}
                    </div>
                </div>
                {!isCard ? (
                    <div className="mb-4">
                        <DeliveryProgressNotice
                            order={order}
                            query={deliveryQuery}
                            canRead={canReadDelivery}
                        />
                    </div>
                ) : null}
                <LineItemsTable
                    order={order}
                    deliveryQuery={deliveryQuery}
                    canReadDelivery={canReadDelivery}
                />
                <div className="flex items-baseline justify-end gap-5 pt-5 text-sm">
                    <span className="text-muted-foreground">合计（含税）</span>
                    <MoneyValue
                        value={order.amountGross}
                        className="text-xl font-semibold"
                    />
                </div>
            </section>
            {order.inquirySource && (
                <SalesOrderInquirySource source={order.inquirySource} />
            )}
            {order.selectionSource && (
                <SalesOrderSelectionSource source={order.selectionSource} />
            )}
            {related}
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
        </div>
    )
}
