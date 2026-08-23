"use client"

import * as React from "react"

import { DocumentHeader, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { LifecycleRail } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"
import { sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

function remainingReceivable(gross: string, received: string) {
    try {
        return sumFixed([gross, `-${received}`], {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        })
    } catch {
        return gross
    }
}

export function SalesOrderIdentityHeader({
    order,
    primaryAction,
    secondaryActions,
}: {
    order: SalesOrderDetailView
    primaryAction?: React.ReactNode
    secondaryActions?: React.ReactNode
}) {
    return (
        <DocumentHeader
            density="compact"
            title={order.customerName}
            documentNumber={order.documentNumber}
            version={
                order.currentRevisionNo == null
                    ? "尚未生效"
                    : `v${order.currentRevisionNo}`
            }
            primaryStatus={order.primaryStatus}
            statuses={[
                {
                    id: "fulfillment",
                    label: "履约",
                    status: order.fulfillment,
                },
                {
                    id: "collection",
                    label: "回款",
                    status: order.collection,
                },
                {
                    id: "invoicing",
                    label: "开票",
                    status: order.invoicing,
                },
            ]}
            meta={
                <span className="inline-flex flex-wrap items-center gap-x-2 gap-y-1">
                    <Badge variant="secondary" className="font-normal">
                        {NATURE_LABEL[order.nature]}
                    </Badge>
                    <span>
                        负责人{" "}
                        <span className="font-medium text-foreground">
                            {order.ownerName}
                        </span>
                    </span>
                    <span className="text-border" aria-hidden="true">
                        ·
                    </span>
                    <span>{ORIGIN_LABEL[order.originSystem]}</span>
                </span>
            }
            primaryAction={primaryAction}
            secondaryActions={secondaryActions}
        >
            <div className="space-y-3">
                <LifecycleRail order={order} />
                <SalesOrderAmountSummary order={order} />
            </div>
        </DocumentHeader>
    )
}

function AmountCell({
    label,
    value,
    hint,
    className,
}: {
    label: string
    value: React.ReactNode
    hint?: React.ReactNode
    className?: string
}) {
    return (
        <div className={cn("min-w-0", className)}>
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="mt-0.5 text-sm font-semibold tabular-nums">
                {value}
                {hint != null ? (
                    <span className="ml-1.5 text-xs font-normal text-muted-foreground">
                        {hint}
                    </span>
                ) : null}
            </dd>
        </div>
    )
}

function SalesOrderAmountSummary({ order }: { order: SalesOrderDetailView }) {
    const receivableLeft = remainingReceivable(
        order.amountGross,
        order.receivedAmount,
    )

    return (
        <dl
            className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-4 sm:gap-0 sm:divide-x sm:divide-border"
            aria-label="销售单金额摘要"
        >
            <AmountCell
                className="sm:pr-4"
                label="成交金额（含税）"
                value={
                    <MoneyValue value={order.amountGross} taxBasis="gross" />
                }
            />
            <AmountCell
                className="sm:px-4"
                label="已回款"
                value={
                    <MoneyValue value={order.receivedAmount} taxBasis="gross" />
                }
                hint={order.collection.label}
            />
            <AmountCell
                className="sm:px-4"
                label="待回款"
                value={<MoneyValue value={receivableLeft} taxBasis="gross" />}
            />
            <AmountCell
                className="sm:pl-4"
                label="已开票"
                value={
                    <MoneyValue value={order.invoicedAmount} taxBasis="gross" />
                }
            />
        </dl>
    )
}
