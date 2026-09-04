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
        <div className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className={cn("mt-0.5 truncate text-sm", numeric && "num")}>
                {value}
            </dd>
        </div>
    )
}

export function LineItemsTable({ order }: { order: SalesOrderDetailView }) {
    const isCard = order.nature === "card_voucher"
    return (
        <div className="overflow-hidden rounded-lg border">
            <Table
                data-density="compact"
                className={isCard ? "min-w-[62rem]" : "min-w-[54rem]"}
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>项目</TableHead>
                        <TableHead>数量 / 单位</TableHead>
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
                            <TableCell className="whitespace-normal">
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

export function OverviewPanel({ order }: { order: SalesOrderDetailView }) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="space-y-4">
            <dl className="grid grid-cols-1 gap-x-4 gap-y-2 sm:grid-cols-2 xl:grid-cols-4">
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
                {isCard ? (
                    <>
                        <OverviewField
                            label="履约期限（到期交付）"
                            value={order.fulfillmentDeadline || "—"}
                            numeric
                        />
                        <OverviewField
                            label="应收到期日"
                            value={order.receivableDueDate || "—"}
                            numeric
                        />
                    </>
                ) : (
                    <OverviewField
                        label="客户承诺期限摘要"
                        value={order.fulfillmentDeadline || "—"}
                        numeric
                    />
                )}
                <OverviewField
                    label="税率"
                    value={
                        order.taxRatePercent ? `${order.taxRatePercent}%` : "—"
                    }
                    numeric
                />
                <OverviewField
                    label="客户联系人"
                    value={order.customerContact ?? "—"}
                />
                <OverviewField
                    label="当前销售版本"
                    value={
                        order.currentRevisionNo == null
                            ? "尚未生效"
                            : `v${order.currentRevisionNo}`
                    }
                    numeric
                />
            </dl>

            <dl>
                <div className="min-w-0">
                    <dt className="text-xs text-muted-foreground">内部说明</dt>
                    <dd className="mt-0.5 whitespace-pre-wrap break-words text-sm">
                        {order.remark?.trim() || "—"}
                    </dd>
                </div>
            </dl>

            <div>
                <div className="mb-2 flex items-baseline justify-between gap-2">
                    <h2 className="text-sm font-medium">
                        {isCard ? "卡券明细" : "销售明细"}
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        {order.lineItems.length} 行
                    </p>
                </div>
                <LineItemsTable order={order} />
            </div>
        </div>
    )
}
