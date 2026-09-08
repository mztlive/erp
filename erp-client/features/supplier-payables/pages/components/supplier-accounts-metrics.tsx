"use client"

import { MetricItem, MetricStrip, MoneyValue } from "@/components/business"
import type { SupplierAccountsListView } from "@/features/supplier-payables/types"

export function SupplierAccountsMetrics({
    metrics,
}: {
    metrics: SupplierAccountsListView["metrics"] | undefined
}) {
    if (!metrics) {
        return (
            <MetricStrip columns={5} aria-label="统计加载中" aria-busy="true">
                {Array.from({ length: 5 }).map((_, i) => (
                    <div
                        key={i}
                        className="h-12 animate-pulse rounded-md bg-muted"
                    />
                ))}
            </MetricStrip>
        )
    }

    return (
        <MetricStrip columns={5} aria-label="供应商往来指标">
            <MetricItem
                id="supplier-payables-metrics-open"
                label="开放应付"
                value={<MoneyValue value={metrics.openPayableTotal} />}
                detail="系统口径"
                detailMode="tooltip"
            />
            <MetricItem
                id="supplier-payables-metrics-overdue"
                label="已到期应付"
                value={<MoneyValue value={metrics.overduePayableTotal} />}
                detail="含逾期开放"
                detailMode="tooltip"
            />
            <MetricItem
                id="supplier-payables-metrics-unallocated-payment"
                label="待分配付款"
                value={<MoneyValue value={metrics.unallocatedPaymentTotal} />}
                detail="付款轨道"
                detailMode="tooltip"
            />
            <MetricItem
                id="supplier-payables-metrics-unallocated-invoice"
                label="待分配进项票"
                value={<MoneyValue value={metrics.unallocatedInvoiceTotal} />}
                detail="与付款独立"
                detailMode="tooltip"
            />
            <MetricItem
                id="supplier-payables-metrics-prepay-gate-blocked"
                label="先款条件待满足"
                value={String(metrics.prepayGateBlockedCount)}
                detail="户/单数"
                detailMode="tooltip"
            />
        </MetricStrip>
    )
}
