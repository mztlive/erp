"use client"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { ApprovalCopyHistory } from "@/features/approval-workflow/components/copy-history"
import { ApprovalRoute } from "@/features/approval-workflow/components/approval-route"
import { DefinitionBindingCard } from "@/features/approval-workflow/components/definition-binding-card"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { SalesOrderDetailView } from "../api/sales-orders"
import { SalesOrderApprovalHistory } from "./sales-order-approval-history"
import { SalesOrderApprovalSummary } from "./sales-order-approval-summary"

/** 销售单只读审批记录：最新轮次优先，决定仍在工作台办理。 */
export function ApprovalPanel({ order }: { order: SalesOrderDetailView }) {
    const approval = order.approval
    const instance = approval?.instance
    const history = useApprovalHistoryInfiniteQuery(
        { instanceId: instance?.id ?? "", order: "desc" },
        Boolean(instance),
    )
    if (!approval) {
        return <p className="text-sm text-muted-foreground">暂无审批记录。</p>
    }
    if (!instance) {
        return (
            <DefinitionBindingCard definition={approval.definition} compact />
        )
    }
    // 详情中的首屏历史从最早记录开始；只有完整时才能用作倒序列表的回退。
    const items =
        history.data?.pages.flatMap((page) => page.items) ??
        (approval.historyHasMore ? [] : approval.recentHistory)
    const hasMore = history.hasNextPage ?? approval.historyHasMore
    return (
        <div className="space-y-6">
            <SalesOrderApprovalSummary instance={instance} />
            {history.data?.pages[0]?.route ? (
                <section className="space-y-4" aria-label="本次审批路线">
                    <h3 className="text-sm font-medium">审批人链条</h3>
                    <ApprovalRoute route={history.data.pages[0].route} />
                </section>
            ) : null}
            {history.isError ? (
                <BusinessFailureState
                    id="sales-order-approval-history-retry"
                    title="审批记录加载失败"
                    error={history.error}
                    onRetry={() => void history.refetch()}
                />
            ) : null}
            {history.isPending && items.length === 0 ? (
                <p role="status" className="py-4 text-sm text-muted-foreground">
                    正在读取审批记录…
                </p>
            ) : items.length > 0 ? (
                <SalesOrderApprovalHistory
                    key={`${instance.id}-${instance.currentRoundNo}`}
                    instance={instance}
                    items={items}
                    hasMore={hasMore}
                />
            ) : !history.isError ? (
                <p className="text-sm text-muted-foreground">暂无审批记录。</p>
            ) : null}
            {history.hasNextPage ? (
                <Button
                    id="sales-order-approval-history-load-more"
                    variant="outline"
                    size="sm"
                    disabled={history.isFetchingNextPage}
                    onClick={() => void history.fetchNextPage()}
                >
                    {history.isFetchingNextPage ? "正在读取…" : "加载更早记录"}
                </Button>
            ) : null}
            <ApprovalCopyHistory
                instanceId={instance.id}
                idPrefix="sales-order-approval-copies"
            />
        </div>
    )
}
