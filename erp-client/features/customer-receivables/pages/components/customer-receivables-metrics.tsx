"use client"

import { MetricItem, MetricStrip, MoneyValue } from "@/components/business"
import type { CustomerAccountsListAccess } from "@/features/customer-receivables/api/list-view"
import { formatDateTime } from "@/lib/datetime"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"

type CustomerReceivablesMetricsProps = {
    metrics: CustomerAccountsListView["metrics"]
    queriedAt: string
    embedded: boolean
    access: CustomerAccountsListAccess
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
        },
        {
            id: "overdue",
            visible: access.receivable,
            label: "已逾期应收",
            value: metrics.overdueReceivableTotal,
            detail: "需催收",
        },
        {
            id: "unallocated-receipt",
            visible: access.receipt,
            label: "待分配回款",
            value: metrics.unallocatedReceiptTotal,
            detail: "已到账",
        },
        {
            id: "unallocated-invoice",
            visible: access.sales_invoice,
            label: "待分配销项发票",
            value: metrics.unallocatedInvoiceTotal,
        },
    ].filter((item) => item.visible)

    return (
        <section aria-label="客户往来指标">
            <MetricStrip
                columns={
                    items.length === 4
                        ? 4
                        : items.length === 3
                          ? 3
                          : items.length === 2
                            ? 2
                            : 1
                }
                variant={embedded ? "summary" : "plain"}
                className={items.length > 1 ? "grid-cols-2 gap-y-2" : undefined}
            >
                {items.map((item) => (
                    <MetricItem
                        key={item.id}
                        id={`customer-receivables-metrics-${item.id}`}
                        label={item.label}
                        value={
                            <MoneyValue
                                display="compact"
                                showHint={false}
                                value={item.value}
                                unavailableReason={
                                    item.value === null
                                        ? "未取得完整统计，请刷新或检查数据范围"
                                        : undefined
                                }
                            />
                        }
                        detail={
                            item.value === null ? "统计暂不可用" : item.detail
                        }
                    />
                ))}
            </MetricStrip>
            {!embedded ? (
                <p className="mt-1 text-xs text-muted-foreground">
                    <time dateTime={queriedAt}>
                        更新于 {formatDateTime(queriedAt, "monthDayIntl")}
                    </time>
                </p>
            ) : null}
        </section>
    )
}
