"use client"

import * as React from "react"

import { DocumentHeader, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"
import { remainingReceivableAmount } from "@/features/sales-orders/lib/sales-order-receivable"

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
            titleExtra={
                <Badge variant="secondary" className="font-normal">
                    {NATURE_LABEL[order.nature]}
                </Badge>
            }
            documentNumber={order.documentNumber}
            version={
                order.currentRevisionNo == null
                    ? "尚未生效"
                    : `v${order.currentRevisionNo}`
            }
            primaryStatus={order.primaryStatus}
            meta={
                <span className="inline-flex flex-wrap items-center gap-x-2 gap-y-1">
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
                    <span className="text-border" aria-hidden="true">
                        ·
                    </span>
                    <span className="inline-flex items-center gap-1.5">
                        履约
                        <StatusBadge
                            tone={order.fulfillment.tone}
                            label={order.fulfillment.label}
                        />
                    </span>
                </span>
            }
            primaryAction={primaryAction}
            secondaryActions={secondaryActions}
            summary={<SalesOrderAmountSummary order={order} />}
        />
    )
}

function AmountCell({
    label,
    value,
    status,
    className,
}: {
    label: string
    value: React.ReactNode
    status?: { label: string; tone: StatusTone }
    className?: string
}) {
    return (
        <div className={className}>
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1">
                {value}
                {status != null ? (
                    <StatusBadge tone={status.tone} label={status.label} />
                ) : null}
            </dd>
        </div>
    )
}

function SalesOrderAmountSummary({ order }: { order: SalesOrderDetailView }) {
    const receivableLeft = remainingReceivableAmount(
        order.amountGross,
        order.receivedAmount,
    )

    return (
        <dl
            className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-4 sm:gap-0 sm:divide-x sm:divide-border"
            aria-label="销售单金额摘要"
        >
            <AmountCell
                className="min-w-0 sm:pr-4"
                label="成交金额（含税）"
                value={
                    <MoneyValue
                        value={order.amountGross}
                        className="font-semibold"
                    />
                }
            />
            <AmountCell
                className="min-w-0 sm:px-4"
                label="已回款"
                value={
                    <MoneyValue
                        value={order.receivedAmount}
                        className="font-semibold"
                    />
                }
                status={order.collection}
            />
            <AmountCell
                className="min-w-0 sm:px-4"
                label="待回款"
                value={
                    <MoneyValue
                        value={receivableLeft}
                        className="font-semibold"
                    />
                }
            />
            <AmountCell
                className="min-w-0 sm:pl-4"
                label="已开票"
                value={
                    <MoneyValue
                        value={order.invoicedAmount}
                        className="font-semibold"
                    />
                }
                status={order.invoicing}
            />
        </dl>
    )
}
