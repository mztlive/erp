"use client"

import {
    MetricFilterItem,
    MetricStrip,
    MoneyValue,
} from "@/components/business"
import type {
    CustomerAccountsListView,
    CustomerAccountsView,
    DueFilter,
} from "@/features/customer-receivables/types"
import { formatDateTime } from "@/lib/datetime"
import type { CustomerReceivablesPatchUrl } from "../hooks/use-customer-receivables-url-state"

type CustomerReceivablesMetricsProps = {
    view: CustomerAccountsView
    due: DueFilter | undefined
    metrics: CustomerAccountsListView["metrics"] | undefined
    queriedAt: string | undefined
    patchUrl: CustomerReceivablesPatchUrl
}

export function CustomerReceivablesMetrics({
    view,
    due,
    metrics,
    queriedAt,
    patchUrl,
}: CustomerReceivablesMetricsProps) {
    if (!metrics) {
        return (
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                {Array.from({ length: 4 }).map((_, i) => (
                    <div
                        key={i}
                        className="h-20 animate-pulse rounded-lg bg-muted"
                    />
                ))}
            </div>
        )
    }
    return (
        <MetricStrip columns={4} aria-label="客户往来指标">
            <MetricFilterItem
                id="customer-receivables-metrics-open"
                label="开放应收"
                value={<MoneyValue value={metrics.openReceivableTotal} />}
                detail={
                    queriedAt
                        ? `更新 ${formatDateTime(queriedAt, "monthDayIntl")}`
                        : undefined
                }
                active={view === "receivable"}
                onClick={() => {
                    // 其余指标点击只设 view（P7），回第 1 页
                    patchUrl(
                        { view: "receivable", page: null },
                        { replace: true },
                    )
                }}
            />
            <MetricFilterItem
                id="customer-receivables-metrics-overdue"
                label="已逾期应收"
                value={<MoneyValue value={metrics.overdueReceivableTotal} />}
                detail="需催收"
                active={view === "receivable" && due === "overdue"}
                onClick={() => {
                    // view+filter 双重语义；与状态/复核维度重叠时一并重置避免矛盾空结果
                    patchUrl(
                        {
                            view: "receivable",
                            due: "overdue",
                            status: null,
                            reviewStatus: null,
                            page: null,
                        },
                        { replace: true },
                    )
                }}
            />
            <MetricFilterItem
                id="customer-receivables-metrics-unallocated-receipt"
                label="待分配回款"
                value={<MoneyValue value={metrics.unallocatedReceiptTotal} />}
                detail="已到账"
                active={view === "unallocated"}
                onClick={() => {
                    patchUrl(
                        {
                            view: "unallocated",
                            due: null,
                            status: null,
                            reviewStatus: null,
                            page: null,
                        },
                        { replace: true },
                    )
                }}
            />
            <MetricFilterItem
                id="customer-receivables-metrics-unallocated-invoice"
                label="待分配销项发票"
                value={<MoneyValue value={metrics.unallocatedInvoiceTotal} />}
                detail={
                    metrics.cardPendingReviewCount > 0
                        ? `卡券待复核 ${metrics.cardPendingReviewCount}`
                        : "独立轨道"
                }
                active={view === "sales_invoice"}
                onClick={() => {
                    patchUrl(
                        {
                            view: "sales_invoice",
                            due: null,
                            status: null,
                            reviewStatus: null,
                            page: null,
                        },
                        { replace: true },
                    )
                }}
            />
        </MetricStrip>
    )
}
