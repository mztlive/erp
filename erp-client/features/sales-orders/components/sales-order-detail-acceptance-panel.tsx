"use client"

import { useQuery } from "@tanstack/react-query"
import { BusinessFailureState } from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { buildOrderProgress } from "@/features/sales-orders/lib/acceptance-model"
import { AcceptanceProgressTable } from "./acceptance-progress-table"
import { AcceptanceHistoryList } from "./acceptance-history-list"

/** 验收查阅不消费登记 URL，也不挂载登记或冲正表单。 */
export function AcceptancePanel({ order }: { order: SalesOrderDetailView }) {
    const query = useQuery({
        queryKey: [...salesOrderKeys.acceptanceRoot(order.id), "readonly"],
        staleTime: 0,
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({ salesOrderId: order.id }),
    })
    if (query.isPending) return <p role="status">正在加载验收记录…</p>
    if (query.isError)
        return (
            <BusinessFailureState
                id="sales-order-acceptance-retry"
                title="验收记录加载失败"
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
    if (!query.data)
        return (
            <p className="text-sm text-muted-foreground">
                当前未查到可查看的验收记录。
            </p>
        )
    return (
        <div className="space-y-5">
            <p className="text-sm text-muted-foreground">
                本单交付数量、验收进度与历史记录。
            </p>
            <AcceptanceProgressTable
                progress={buildOrderProgress(query.data.salesLines)}
            />
            <AcceptanceHistoryList
                history={query.data.history}
                canReverse={false}
                onReverse={() => undefined}
            />
        </div>
    )
}
