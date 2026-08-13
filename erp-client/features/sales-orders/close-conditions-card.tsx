"use client"

import * as React from "react"

import { CheckIcon, CircleDashedIcon, XIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

type CloseConditionsCardProps = {
    order: SalesOrderListItem
}

/**
 * 结案条件摘要：交付完成 + 回款收齐；开票不挡结案。
 */
export function CloseConditionsCard({ order }: CloseConditionsCardProps) {
    const { closeEligibility: close, nature } = order
    const isCard = nature === "card_voucher"
    const closed = order.primaryStatus.label === "已关闭"

    return (
        <div className="space-y-3">
            <div className="flex items-center justify-between gap-2">
                <h3 className="text-sm font-medium">结案条件</h3>
                <Badge
                    variant={
                        close.eligibleToClose || closed
                            ? "success"
                            : "secondary"
                    }
                >
                    {closed
                        ? "已结案"
                        : close.eligibleToClose
                          ? "系统将自动结案"
                          : "还差条件"}
                </Badge>
            </div>
            <ul className="space-y-2 text-sm">
                <ConditionRow
                    ok={close.fulfillmentComplete}
                    label="交付完成"
                    detail={
                        isCard
                            ? `期限 ${order.fulfillmentDeadline || "—"} · ${order.fulfillment.label}`
                            : order.fulfillment.label
                    }
                />
                <ConditionRow
                    ok={close.receivableSettled}
                    label="回款收齐"
                    detail={
                        <>
                            <MoneyValue
                                value={order.receivedAmount}
                                taxBasis="gross"
                            />
                            {" / "}
                            <MoneyValue
                                value={order.amountGross}
                                taxBasis="gross"
                            />
                        </>
                    }
                />
                <ConditionRow
                    ok={close.invoiceComplete}
                    label="开票"
                    optional
                    detail={order.invoicing.label}
                />
            </ul>
            {close.blockers.length > 0 ? (
                <p className="text-xs text-muted-foreground">
                    还差：{close.blockers.join("；")}
                </p>
            ) : null}
        </div>
    )
}

function ConditionRow({
    ok,
    label,
    detail,
    optional,
}: {
    ok: boolean
    label: string
    detail: React.ReactNode
    optional?: boolean
}) {
    const Icon = optional ? CircleDashedIcon : ok ? CheckIcon : XIcon
    return (
        <li className="flex items-start gap-2">
            <Icon
                className={
                    optional
                        ? "mt-0.5 size-3.5 shrink-0 text-muted-foreground"
                        : ok
                          ? "mt-0.5 size-3.5 shrink-0 text-success"
                          : "mt-0.5 size-3.5 shrink-0 text-warning"
                }
                aria-hidden="true"
            />
            <div className="min-w-0 flex-1">
                <div className="text-sm">
                    {label}
                    {optional ? (
                        <span className="ml-1 text-xs text-muted-foreground">
                            （不挡结案）
                        </span>
                    ) : null}
                </div>
                <div className="mt-0.5 text-xs text-muted-foreground">
                    {detail}
                </div>
            </div>
        </li>
    )
}
