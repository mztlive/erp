"use client"

import * as React from "react"

import { MoneyValue } from "@/components/business"
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
        <div className="overflow-x-auto">
            <table
                className={cn(
                    "w-full text-sm",
                    isCard ? "min-w-[62rem]" : "min-w-[54rem]",
                )}
            >
                <thead className="bg-muted/50 text-left">
                    <tr>
                        <th className="whitespace-nowrap px-3 py-1.5 font-medium">
                            项目
                        </th>
                        <th className="whitespace-nowrap px-3 py-1.5 font-medium">
                            数量 / 单位
                        </th>
                        <th className="whitespace-nowrap px-3 py-1.5 text-right font-medium">
                            含税单价
                        </th>
                        {isCard ? (
                            <>
                                <th className="whitespace-nowrap px-3 py-1.5 text-right font-medium">
                                    面值
                                </th>
                                <th className="whitespace-nowrap px-3 py-1.5 text-right font-medium">
                                    配赠
                                </th>
                                <th className="whitespace-nowrap px-3 py-1.5 font-medium">
                                    卡形态
                                </th>
                            </>
                        ) : (
                            <>
                                <th className="whitespace-nowrap px-3 py-1.5 font-medium">
                                    履约方式
                                </th>
                                <th className="whitespace-nowrap px-3 py-1.5 font-medium">
                                    交付日期
                                </th>
                            </>
                        )}
                        <th className="whitespace-nowrap px-3 py-1.5 text-right font-medium">
                            含税小计
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
                            <td className="px-3 py-1.5 text-right">
                                <MoneyValue value={line.unitPriceGross} />
                            </td>
                            {isCard ? (
                                <>
                                    <td className="px-3 py-1.5 text-right text-sm">
                                        {line.faceValue ? (
                                            <MoneyValue
                                                value={line.faceValue}
                                            />
                                        ) : (
                                            "—"
                                        )}
                                    </td>
                                    <td className="num px-3 py-1.5 text-right text-sm">
                                        {line.giftRate
                                            ? `${line.giftRate}%`
                                            : "—"}
                                    </td>
                                    <td className="px-3 py-1.5 text-sm">
                                        {line.cardForm || "—"}
                                    </td>
                                </>
                            ) : (
                                <>
                                    <td className="px-3 py-1.5 text-sm text-muted-foreground">
                                        {line.fulfillmentMode || "—"}
                                    </td>
                                    <td className="num px-3 py-1.5 text-sm text-muted-foreground">
                                        {line.dueDate || "—"}
                                    </td>
                                </>
                            )}
                            <td className="px-3 py-1.5 text-right">
                                <MoneyValue value={line.amountGross} />
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
                            label="目标商城"
                            value={order.targetMallName || "—"}
                        />
                        <OverviewField
                            label="应收到期日"
                            value={order.receivableDueDate || "—"}
                            numeric
                        />
                    </>
                ) : (
                    <OverviewField
                        label="履约期限摘要"
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
