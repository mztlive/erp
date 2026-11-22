"use client"

import {
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    MoneyValue,
} from "@/components/business"
import type { CustomerAccountsListAccess } from "@/features/customer-receivables/api/list-view"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"
import { formatDateTime } from "@/lib/datetime"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

type CustomerReceivablesMetricsProps = {
    metrics: CustomerAccountsListView["metrics"]
    queriedAt: string
    embedded: boolean
    access: CustomerAccountsListAccess
}

function hasPositiveAmount(value: string | null | undefined): boolean {
    if (!value) return false
    try {
        return compareDecimal(value, "0", 2) > 0
    } catch {
        return false
    }
}

export function CustomerReceivablesMetrics({
    metrics,
    queriedAt,
    embedded,
    access,
}: CustomerReceivablesMetricsProps) {
    const items = [
        {
            id: "open",
            visible: access.receivable,
            label: "待收金额",
            value: metrics.openReceivableTotal,
            emphasis: "primary" as const,
            tone: "default" as const,
            detail: !embedded ? (
                <time dateTime={queriedAt}>
                    更新于 {formatDateTime(queriedAt, "monthDayIntl")}
                </time>
            ) : undefined,
        },
        {
            id: "overdue",
            visible: access.receivable,
            label: "已逾期应收",
            value: metrics.overdueReceivableTotal,
            emphasis: "default" as const,
            tone: hasPositiveAmount(metrics.overdueReceivableTotal)
                ? ("destructive" as const)
                : ("default" as const),
            detail: hasPositiveAmount(metrics.overdueReceivableTotal)
                ? "需优先催收"
                : "当前无逾期",
        },
        {
            id: "unallocated-receipt",
            visible: access.receipt,
            label: "待分配回款",
            value: metrics.unallocatedReceiptTotal,
            emphasis: "default" as const,
            tone: "default" as const,
            detail: "已到账待核销",
        },
        {
            id: "unallocated-invoice",
            visible: access.sales_invoice,
            label: "待分配销项发票",
            value: metrics.unallocatedInvoiceTotal,
            emphasis: "default" as const,
            tone: "default" as const,
            detail: "已登记待分配",
        },
    ].filter((item) => item.visible)

    if (items.length === 0) return null

    const columns = items.length >= 4 ? 4 : items.length === 3 ? 3 : 2

    return (
        <section
            data-slot={embedded ? undefined : "list-workspace-metrics"}
            aria-label="客户往来指标"
            className={cn(
                embedded
                    ? "rounded-lg border border-border/70 bg-card px-4"
                    : "-mx-(--scaffold-px) border-b border-border/60 bg-card px-(--scaffold-px)",
            )}
        >
            <DetailWorkspaceMetrics
                columns={columns}
                variant="bar"
                ariaLabel="客户往来核心指标"
            >
                {items.map((item) => (
                    <DetailWorkspaceMetricItem
                        key={item.id}
                        emphasis={item.emphasis}
                        label={item.label}
                        value={
                            <span
                                id={`customer-receivables-metrics-${item.id}`}
                            >
                                <MoneyValue
                                    display="compact"
                                    showHint={false}
                                    value={item.value}
                                    className={
                                        item.tone === "destructive"
                                            ? "text-destructive"
                                            : undefined
                                    }
                                    unavailableReason={
                                        item.value === null
                                            ? "未取得完整统计，请刷新或检查数据范围"
                                            : undefined
                                    }
                                />
                            </span>
                        }
                        detail={
                            item.value === null ? "统计暂不可用" : item.detail
                        }
                    />
                ))}
            </DetailWorkspaceMetrics>
        </section>
    )
}
