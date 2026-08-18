"use client"

import * as React from "react"

import { DocumentHeader, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"
import { sumFixed } from "@/lib/fixed-decimal"

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
            version={order.version}
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
                <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                    <Badge variant="secondary" className="font-normal">
                        {NATURE_LABEL[order.nature]}
                    </Badge>
                    <span aria-hidden="true">·</span>
                    <span>
                        负责人{" "}
                        <span className="font-medium text-foreground">
                            {order.ownerName}
                        </span>
                    </span>
                    <span aria-hidden="true">·</span>
                    <span>{ORIGIN_LABEL[order.originSystem]}</span>
                </span>
            }
            primaryAction={primaryAction}
            secondaryActions={secondaryActions}
        >
            <SalesOrderAmountSummary order={order} />
        </DocumentHeader>
    )
}

function SalesOrderAmountSummary({ order }: { order: SalesOrderDetailView }) {
    const receivableLeft = remainingReceivable(
        order.amountGross,
        order.receivedAmount,
    )

    return (
        <dl
            className="grid grid-cols-2 gap-x-4 gap-y-2 lg:grid-cols-4"
            aria-label="销售单金额摘要"
        >
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">
                    成交金额（含税）
                </dt>
                <dd className="mt-0.5 text-sm font-semibold">
                    <MoneyValue value={order.amountGross} taxBasis="gross" />
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">已回款</dt>
                <dd className="mt-0.5 text-sm font-semibold">
                    <MoneyValue value={order.receivedAmount} taxBasis="gross" />
                    <span className="ml-1.5 text-xs font-normal text-muted-foreground">
                        {order.collection.label}
                    </span>
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">待回款</dt>
                <dd className="mt-0.5 text-sm font-semibold">
                    <MoneyValue value={receivableLeft} taxBasis="gross" />
                    {order.closeEligibility.receivableSettled ? (
                        <span className="ml-1.5 text-xs font-normal text-muted-foreground">
                            已收齐
                        </span>
                    ) : null}
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">已开票</dt>
                <dd className="mt-0.5 text-sm font-semibold">
                    <MoneyValue value={order.invoicedAmount} taxBasis="gross" />
                </dd>
            </div>
        </dl>
    )
}
