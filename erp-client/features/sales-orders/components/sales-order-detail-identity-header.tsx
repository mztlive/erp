"use client"

import * as React from "react"

import {
    DocumentHeader,
    MetricItem,
    MetricStrip,
    MoneyValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { StatusBadge } from "@/components/ui/status-badge"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"
import { remainingReceivableAmount } from "@/features/sales-orders/lib/sales-order-receivable"

export function SalesOrderIdentityHeader({
    order,
    identityOnly = false,
    primaryAction,
    secondaryActions,
}: {
    order: SalesOrderDetailView
    identityOnly?: boolean
    primaryAction?: React.ReactNode
    secondaryActions?: React.ReactNode
}) {
    return (
        <DocumentHeader
            density="compact"
            className="border-0 pb-0 [&>div:first-child]:flex-col sm:[&>div:first-child]:flex-row [&_h1]:wrap-anywhere [&_h1]:text-3xl [&_[data-slot=document-header-summary]]:mt-4 [&_[data-slot=document-header-summary]]:border-0 [&_[data-slot=document-header-summary]]:pt-0"
            title={order.customerName}
            titleExtra={
                <Badge variant="secondary" className="font-normal">
                    {NATURE_LABEL[order.nature]}
                </Badge>
            }
            documentNumber={order.documentNumber}
            version={
                identityOnly
                    ? undefined
                    : order.currentRevisionNo == null
                      ? "尚未生效"
                      : `v${order.currentRevisionNo}`
            }
            primaryStatus={order.primaryStatus}
            meta={
                identityOnly ? undefined : (
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
                )
            }
            primaryAction={primaryAction}
            secondaryActions={secondaryActions}
            summary={
                identityOnly ? undefined : (
                    <SalesOrderAmountSummary order={order} />
                )
            }
        />
    )
}

function SalesOrderAmountSummary({ order }: { order: SalesOrderDetailView }) {
    const receivableLeft = remainingReceivableAmount(
        order.amountGross,
        order.receivedAmount,
    )

    return (
        <MetricStrip
            columns={4}
            className="items-end py-4"
            aria-label="销售单金额摘要"
        >
            <MetricItem
                label="成交金额（含税）"
                value={
                    <MoneyValue
                        value={order.amountGross}
                        className="text-2xl font-semibold sm:text-3xl"
                    />
                }
            />
            <MetricItem
                label={
                    <span className="flex flex-wrap items-center gap-2">
                        已回款
                        <StatusBadge
                            tone={order.collection.tone}
                            label={order.collection.label}
                            className="border-0 bg-transparent px-0 shadow-none"
                        />
                    </span>
                }
                value={
                    <MoneyValue
                        value={order.receivedAmount}
                        className="font-semibold"
                    />
                }
            />
            <MetricItem
                label="待回款"
                value={
                    <MoneyValue
                        value={receivableLeft}
                        className="font-semibold"
                    />
                }
            />
            <MetricItem
                label={
                    <span className="flex flex-wrap items-center gap-2">
                        已开票
                        <StatusBadge
                            tone={order.invoicing.tone}
                            label={order.invoicing.label}
                            className="border-0 bg-transparent px-0 shadow-none"
                        />
                    </span>
                }
                value={
                    <MoneyValue
                        value={order.invoicedAmount}
                        className="font-semibold"
                    />
                }
            />
        </MetricStrip>
    )
}
