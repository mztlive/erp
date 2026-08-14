"use client"

import {
    MetricFilterItem,
    MetricStrip,
    MoneyValue,
} from "@/components/business"
import type {
    AllocationTrack,
    SupplierAccountsListView,
    SupplierAccountsView,
} from "@/features/supplier-payables/types"

export interface SupplierAccountsMetricsProps {
    metrics: SupplierAccountsListView["metrics"]
    view: SupplierAccountsView
    status: string | undefined
    due: "not_due" | "due_today" | "overdue" | "all" | undefined
    trackFilter: AllocationTrack | "all"
    paymentGate: "satisfied" | "unsatisfied" | "all" | undefined
    onFilter: (patch: Record<string, string | null | undefined>) => void
}

export function SupplierAccountsMetrics({
    metrics,
    view,
    status,
    due,
    trackFilter,
    paymentGate,
    onFilter,
}: SupplierAccountsMetricsProps) {
    return (
        // 指标 toggle 取消语义保留（D23）：再次点击已激活指标即取消该筛选
        // （due/paymentGate 置回 all、view 回 payable）；指标/视图/筛选变更均回第 1 页（P6）。
        <MetricStrip>
            <MetricFilterItem
                label="开放应付"
                value={<MoneyValue value={metrics.openPayableTotal} />}
                detail="系统口径"
                active={view === "payable" && !status}
                onClick={() => {
                    onFilter({ view: "payable", status: null, page: null })
                }}
            />
            <MetricFilterItem
                label="已到期应付"
                value={<MoneyValue value={metrics.overduePayableTotal} />}
                detail="含逾期开放"
                active={due === "overdue"}
                onClick={() => {
                    onFilter({
                        view: "payable",
                        due: due === "overdue" ? null : "overdue",
                        page: null,
                    })
                }}
            />
            <MetricFilterItem
                label="待分配付款"
                value={
                    <MoneyValue value={metrics.unallocatedPaymentTotal} />
                }
                detail="付款轨道"
                active={view === "unallocated" && trackFilter === "payment"}
                onClick={() => {
                    onFilter({
                        view: "unallocated",
                        track: "payment",
                        page: null,
                    })
                }}
            />
            <MetricFilterItem
                label="待分配进项票"
                value={<MoneyValue value={metrics.unallocatedInvoiceTotal} />}
                detail="与付款独立"
                active={
                    view === "unallocated" &&
                    trackFilter === "purchase_invoice"
                }
                onClick={() => {
                    onFilter({
                        view: "unallocated",
                        track: "purchase_invoice",
                        page: null,
                    })
                }}
            />
            <MetricFilterItem
                label="先款条件待满足"
                value={String(metrics.prepayGateBlockedCount)}
                detail="户/单数"
                active={paymentGate === "unsatisfied"}
                onClick={() => {
                    onFilter({
                        view: "payable",
                        paymentGate:
                            paymentGate === "unsatisfied"
                                ? null
                                : "unsatisfied",
                        page: null,
                    })
                }}
            />
        </MetricStrip>
    )
}
