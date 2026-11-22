"use client"

import { useQuery } from "@tanstack/react-query"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
} from "@/components/business"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { buildOrderProgress } from "@/features/sales-orders/lib/acceptance-model"
import { AcceptanceHistoryList } from "./acceptance-history-list"
import {
    AcceptanceProgressTable,
    AcceptanceSummaryMetrics,
} from "./acceptance-progress-table"

/** 验收查阅不消费登记 URL，也不挂载登记或冲正表单；与可操作态共用纯白详情布局。 */
export function AcceptancePanel({ order }: { order: SalesOrderDetailView }) {
    const query = useQuery({
        queryKey: [...salesOrderKeys.acceptanceRoot(order.id), "readonly"],
        staleTime: 0,
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({ salesOrderId: order.id }),
    })
    if (query.isPending)
        return (
            <DetailWorkspaceSection width="form">
                <p role="status" className="text-xs text-muted-foreground">
                    正在加载验收记录…
                </p>
            </DetailWorkspaceSection>
        )
    if (query.isError)
        return (
            <DetailWorkspaceSection width="form">
                <BusinessFailureState
                    id="sales-order-acceptance-retry"
                    title="验收记录加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </DetailWorkspaceSection>
        )
    if (!query.data)
        return (
            <DetailWorkspaceSection width="form">
                <p className="text-xs text-muted-foreground">
                    当前未查到可查看的验收记录。
                </p>
            </DetailWorkspaceSection>
        )
    const progress = buildOrderProgress(query.data.salesLines)
    const pendingHint =
        progress.pendingFactCount > 0
            ? `还有 ${progress.pendingFactCount} 批待客户验收，由${order.ownerName || "负责销售"}登记。`
            : undefined

    return (
        <>
            <AcceptanceSummaryMetrics progress={progress} />

            <AcceptanceProgressTable
                surface="detail"
                progress={progress}
                showSummary={false}
                pendingHint={pendingHint}
            />

            <AcceptanceHistoryList
                surface="detail"
                history={query.data.history}
                canReverse={false}
                onReverse={() => undefined}
                showGuidance={false}
            />
        </>
    )
}
