"use client"

import {
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    MoneyValue,
} from "@/components/business"
import type { SupplierAccountsListView } from "@/features/supplier-payables/types"
import { formatDateTime } from "@/lib/datetime"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

interface SupplierAccountsMetricsProps {
    metrics: SupplierAccountsListView["metrics"]
    queriedAt: string
}

function hasPositiveAmount(value: string | null | undefined): boolean {
    if (!value) return false
    try {
        return compareDecimal(value, "0", 2) > 0
    } catch {
        return false
    }
}

export function SupplierAccountsMetrics({
    metrics,
    queriedAt,
}: SupplierAccountsMetricsProps) {
    const overduePositive = hasPositiveAmount(metrics.overduePayableTotal)
    const gateBlocked = metrics.prepayGateBlockedCount > 0

    return (
        <section
            aria-label="供应商往来指标"
            className="-mx-(--scaffold-px) border-b border-border/60 bg-card px-(--scaffold-px)"
        >
            <DetailWorkspaceMetrics
                columns={4}
                variant="bar"
                ariaLabel="供应商往来核心指标"
            >
                <DetailWorkspaceMetricItem
                    emphasis="primary"
                    label="剩余应付"
                    value={
                        <span id="supplier-payables-metrics-open">
                            <MoneyValue
                                display="compact"
                                showHint={false}
                                value={metrics.openPayableTotal}
                            />
                        </span>
                    }
                    detail={
                        <time dateTime={queriedAt}>
                            更新于 {formatDateTime(queriedAt, "monthDayIntl")}
                        </time>
                    }
                />
                <DetailWorkspaceMetricItem
                    label="已逾期应付"
                    value={
                        <span id="supplier-payables-metrics-overdue">
                            <MoneyValue
                                display="compact"
                                showHint={false}
                                value={metrics.overduePayableTotal}
                                className={
                                    overduePositive
                                        ? "text-destructive"
                                        : undefined
                                }
                            />
                        </span>
                    }
                    detail={overduePositive ? "需优先安排付款" : "当前无逾期"}
                />
                <DetailWorkspaceMetricItem
                    label="先款待付阻断"
                    value={
                        <span
                            id="supplier-payables-metrics-gate-blocked"
                            className={cn(
                                "num",
                                gateBlocked && "text-destructive",
                            )}
                        >
                            {metrics.prepayGateBlockedCount} 笔
                        </span>
                    }
                    detail={gateBlocked ? "需先付预付款解锁" : "当前无阻断"}
                />
                <DetailWorkspaceMetricItem
                    label="待核销付款"
                    value={
                        <span id="supplier-payables-metrics-unallocated">
                            <MoneyValue
                                display="compact"
                                showHint={false}
                                value={metrics.unallocatedPaymentTotal}
                            />
                        </span>
                    }
                    detail={
                        <span className="inline-flex items-baseline gap-1">
                            <span>待核销进项票</span>
                            <MoneyValue
                                display="compact"
                                showHint={false}
                                value={metrics.unallocatedInvoiceTotal}
                                className="text-xs text-muted-foreground"
                            />
                        </span>
                    }
                />
            </DetailWorkspaceMetrics>
        </section>
    )
}
