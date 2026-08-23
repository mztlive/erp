"use client"

import * as React from "react"

import { MoneyValue } from "@/components/business"
import { welfareScenarioLabel } from "@/lib/business-options"
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
        <div className="overflow-x-auto">
            <table className="w-full text-sm">
                <thead className="bg-muted/50 text-left">
                    <tr>
                        <th className="px-3 py-1.5 font-medium">项目</th>
                        <th className="px-3 py-1.5 font-medium">数量</th>
                        {isCard ? (
                            <th className="px-3 py-1.5 font-medium">
                                面额 / 形态
                            </th>
                        ) : (
                            <th className="px-3 py-1.5 font-medium">
                                交付方式
                            </th>
                        )}
                        <th className="px-3 py-1.5 font-medium text-right">
                            含税金额
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {order.lineItems.map((line) => (
                        <tr key={line.id} className="border-t border-grid">
                            <td className="px-3 py-1.5">
                                <div>{line.name}</div>
                                {line.sku ? (
                                    <div className="num text-xs text-muted-foreground">
                                        {line.sku}
                                    </div>
                                ) : null}
                            </td>
                            <td className="num px-3 py-1.5">
                                {line.quantity} {line.unit}
                            </td>
                            {isCard ? (
                                <td className="px-3 py-1.5 text-sm">
                                    {line.faceValue ? (
                                        <MoneyValue value={line.faceValue} />
                                    ) : (
                                        "—"
                                    )}
                                    {line.cardForm ? (
                                        <span className="mt-0.5 block text-xs text-muted-foreground">
                                            {line.cardForm}
                                        </span>
                                    ) : null}
                                </td>
                            ) : (
                                <td className="px-3 py-1.5 text-sm text-muted-foreground">
                                    <div>{line.fulfillmentMode ?? "—"}</div>
                                    {line.dueDate ? (
                                        <div className="num mt-0.5 text-xs">
                                            {line.dueDate}
                                        </div>
                                    ) : null}
                                </td>
                            )}
                            <td className="px-3 py-1.5 text-right">
                                <MoneyValue
                                    value={line.amountGross}
                                    taxBasis="gross"
                                />
                            </td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    )
}

export function OverviewPanel({ order }: { order: SalesOrderDetailView }) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="space-y-4">
            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 xl:grid-cols-3">
                <OverviewField
                    label="关联合同"
                    value={order.contractRevisionLabel || "—"}
                />
                <OverviewField
                    label="福利场景"
                    value={welfareScenarioLabel(order.welfareScene)}
                />
                <OverviewField
                    label="付款条件"
                    value={order.paymentTerms || "—"}
                />
                <OverviewField
                    label={isCard ? "履约期限（到期交付）" : "履约期限"}
                    value={order.fulfillmentDeadline || "—"}
                    numeric
                />
                <OverviewField
                    label="客户联系人"
                    value={order.customerContact ?? "—"}
                />
                <OverviewField
                    label="当前版本"
                    value={`v${order.version}`}
                    numeric
                />
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
