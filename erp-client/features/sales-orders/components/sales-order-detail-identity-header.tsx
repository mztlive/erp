"use client"

import * as React from "react"
import Link from "next/link"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    MoneyValue,
    type DetailWorkspaceBackLink,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { StatusBadge } from "@/components/ui/status-badge"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"
import { remainingReceivableAmount } from "@/features/sales-orders/lib/sales-order-receivable"

export function SalesOrderIdentityHeader({
    back,
    navigationMeta,
    order,
    identityOnly = false,
    primaryAction,
    secondaryActions,
    tabs,
}: {
    back?: DetailWorkspaceBackLink
    navigationMeta?: React.ReactNode
    order: SalesOrderDetailView
    identityOnly?: boolean
    primaryAction?: React.ReactNode
    secondaryActions?: React.ReactNode
    tabs?: React.ReactNode
}) {
    return (
        <DetailWorkspaceChrome
            back={back}
            navigationMeta={navigationMeta}
            title={order.customerName}
            primaryStatus={{
                label: order.primaryStatus.label,
                tone: order.primaryStatus.tone,
            }}
            titleExtra={
                <>
                    <Badge variant="secondary" className="font-normal">
                        {NATURE_LABEL[order.nature]}
                    </Badge>
                    {!identityOnly && order.activeChangeOrder ? (
                        <StatusBadge tone="warning" label="改单中" />
                    ) : null}
                </>
            }
            documentNumber={order.documentNumber}
            version={
                !identityOnly && order.currentRevisionNo != null
                    ? `v${order.currentRevisionNo}`
                    : undefined
            }
            meta={
                identityOnly ? undefined : (
                    <DetailWorkspaceMeta
                        items={[
                            order.customerId ? (
                                <span key="customer">
                                    客户{" "}
                                    <Link
                                        id="sales-orders-detail-header-customer-link"
                                        href={`/sales/customers/${encodeURIComponent(order.customerId)}`}
                                        className="font-medium text-foreground underline-offset-4 hover:text-primary hover:underline"
                                    >
                                        {order.customerName}
                                    </Link>
                                </span>
                            ) : null,
                            order.currentRevisionNo == null ? "尚未生效" : null,
                            <span key="owner">
                                销售负责人{" "}
                                <span className="font-medium text-foreground">
                                    {order.ownerName}
                                </span>
                            </span>,
                            ORIGIN_LABEL[order.originSystem],
                        ]}
                    />
                )
            }
            primaryAction={primaryAction}
            secondaryActions={secondaryActions}
            tabs={tabs}
        />
    )
}

/** 置于吸顶头下方的通栏无框金额摘要带：首屏即览，随页面自然滚走不挤占阅读空间。 */
export function SalesOrderAmountSummary({
    order,
}: {
    order: SalesOrderDetailView
}) {
    const receivableLeft = remainingReceivableAmount(
        order.amountGross,
        order.receivedAmount,
    )

    return (
        <DetailWorkspaceMetrics
            columns={4}
            variant="bar"
            ariaLabel="销售单金额摘要"
        >
            <DetailWorkspaceMetricItem
                emphasis="primary"
                label="成交金额（含税）"
                value={<MoneyValue value={order.amountGross} />}
            />
            <DetailWorkspaceMetricItem
                label={
                    <>
                        <span>已回款</span>
                        <StatusBadge
                            tone={order.collection.tone}
                            label={order.collection.label}
                            appearance="text"
                        />
                    </>
                }
                value={<MoneyValue value={order.receivedAmount} />}
            />
            <DetailWorkspaceMetricItem
                label="待回款"
                value={<MoneyValue value={receivableLeft} />}
            />
            <DetailWorkspaceMetricItem
                label={
                    <>
                        <span>已开票</span>
                        <StatusBadge
                            tone={order.invoicing.tone}
                            label={order.invoicing.label}
                            appearance="text"
                        />
                    </>
                }
                value={<MoneyValue value={order.invoicedAmount} />}
            />
        </DetailWorkspaceMetrics>
    )
}
