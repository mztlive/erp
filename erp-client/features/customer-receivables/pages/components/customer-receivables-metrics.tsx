"use client"

import { MetricItem, MetricStrip, MoneyValue } from "@/components/business"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"

type CustomerReceivablesMetricsProps = {
    metrics: CustomerAccountsListView["metrics"] | undefined
}

export function CustomerReceivablesMetrics({
    metrics,
}: CustomerReceivablesMetricsProps) {
    if (!metrics) {
        return (
            <div className="mb-6 grid grid-cols-2 gap-x-6 gap-y-4 md:grid-cols-4">
                {Array.from({ length: 4 }).map((_, i) => (
                    <div
                        key={i}
                        className="h-16 animate-pulse rounded-lg bg-muted"
                    />
                ))}
            </div>
        )
    }

    const items = [
        {
            id: "open",
            label: "开放应收",
            value: metrics.openReceivableTotal,
        },
        {
            id: "overdue",
            label: "已逾期应收",
            value: metrics.overdueReceivableTotal,
            detail: "需催收",
        },
        {
            id: "unallocated-receipt",
            label: "待分配回款",
            value: metrics.unallocatedReceiptTotal,
            detail: "已到账",
        },
        {
            id: "unallocated-invoice",
            label: "待分配销项发票",
            value: metrics.unallocatedInvoiceTotal,
            detail:
                metrics.cardPendingReviewCount > 0
                    ? `卡券待复核 ${metrics.cardPendingReviewCount}`
                    : undefined,
        },
    ]

    return (
        <MetricStrip columns={4} aria-label="客户往来指标" className="mb-6">
            {items.map((item) => (
                <MetricItem
                    key={item.id}
                    id={`customer-receivables-metrics-${item.id}`}
                    label={item.label}
                    value={<MoneyValue value={item.value} />}
                    detail={item.detail}
                />
            ))}
        </MetricStrip>
    )
}
