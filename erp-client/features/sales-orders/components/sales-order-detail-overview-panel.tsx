"use client"

import * as React from "react"

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

export function LineItemsTable({ order }: { order: SalesOrderDetailView }) {
    const isCard = order.nature === "card_voucher"
    return (
        <div className="overflow-hidden rounded-lg border-b border-border">
            <Table
                data-density="compact"
                className={isCard ? "min-w-[54rem]" : "min-w-[40rem]"}
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>项目</TableHead>
                        <TableHead data-align="end">数量 / 单位</TableHead>
                        <TableHead data-align="end">含税单价</TableHead>
                        {isCard ? (
                            <>
                                <TableHead data-align="end">面值</TableHead>
                                <TableHead data-align="end">配赠</TableHead>
                                <TableHead>卡形态</TableHead>
                            </>
                        ) : (
                            <TableHead>承诺交付日</TableHead>
                        )}
                        <TableHead data-align="end">含税小计</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {order.lineItems.map((line) => (
                        <TableRow key={line.id}>
                            <TableCell className="min-w-44 whitespace-normal py-4">
                                <div>{line.name}</div>
                                {line.sku ? (
                                    <div className="num text-xs text-muted-foreground">
                                        {line.sku}
                                    </div>
                                ) : null}
                            </TableCell>
                            <TableCell data-align="end">
                                {line.quantity} {line.unit}
                            </TableCell>
                            <TableCell data-align="end">
                                <MoneyValue value={line.unitPriceGross} />
                            </TableCell>
                            {isCard ? (
                                <>
                                    <TableCell data-align="end">
                                        {line.faceValue ? (
                                            <MoneyValue
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
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}

/** 概览按明细、采购进度、交易约定排序，责任与票款摘要由侧栏承载。 */
export function OverviewPanel({
    order,
    related,
}: {
    order: SalesOrderDetailView
    related?: React.ReactNode
}) {
    const isCard = order.nature === "card_voucher"
    return (
        <div className="space-y-5">
            <section
                aria-labelledby="sales-order-lines-heading"
                className="min-w-0 rounded-lg border border-border/70 p-4 md:p-5"
            >
                <div className="mb-4 flex items-baseline justify-between gap-2">
                    <h2
                        id="sales-order-lines-heading"
                        className="text-lg font-semibold"
                    >
                        {isCard ? "卡券明细" : "销售明细"}
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        共 {order.lineItems.length} 行
                    </p>
                </div>
                <LineItemsTable order={order} />
                <div className="flex items-baseline justify-end gap-5 pt-5 text-sm">
                    <span className="text-muted-foreground">合计（含税）</span>
                    <MoneyValue
                        value={order.amountGross}
                        className="text-xl font-semibold"
                    />
                </div>
            </section>
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
                        value={order.contractRevisionLabel || "—"}
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
