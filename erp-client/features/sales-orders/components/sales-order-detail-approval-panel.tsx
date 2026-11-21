"use client"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { ApprovalCopyHistory } from "@/features/approval-workflow/components/copy-history"
import { ApprovalRoute } from "@/features/approval-workflow/components/approval-route"
import { DefinitionBindingCard } from "@/features/approval-workflow/components/definition-binding-card"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import { getErrorPresentation } from "@/lib/api/errors"
import type { SalesOrderDetailView } from "../api/sales-orders"
import { SalesOrderApprovalHistory } from "./sales-order-approval-history"
import { SalesOrderApprovalSummary } from "./sales-order-approval-summary"

const historyImpact = "销售单详情仍可查看。"

/** 销售单只读审批记录：最新轮次优先，决定仍在工作台办理。 */
export function ApprovalPanel({
    order,
    idPrefix = "sales-order-approval",
    orientation = "vertical",
}: {
    order: SalesOrderDetailView
    idPrefix?: string
    orientation?: "vertical" | "horizontal"
}) {
    const approval = order.approval
    const instance = approval?.instance
    const history = useApprovalHistoryInfiniteQuery(
        { instanceId: instance?.id ?? "", order: "desc" },
        Boolean(instance),
        historyImpact,
    )
    if (!approval) {
        return <p className="text-xs text-muted-foreground">暂无审批记录。</p>
    }
    if (!instance) {
        return (
            <DefinitionBindingCard definition={approval.definition} compact />
        )
    }
    // 详情中的首屏历史从最早记录开始；只有完整时才能用作倒序列表的回退。
    const historyFailure = history.isError
        ? getErrorPresentation(history.error)
        : undefined
    const items =
        history.data?.pages.flatMap((page) => page.items) ??
        (approval.historyHasMore ? [] : approval.recentHistory)
    const hasMore = history.hasNextPage ?? approval.historyHasMore
    return (
        <div className="space-y-6">
            <SalesOrderApprovalSummary
                instance={instance}
                idPrefix={idPrefix}
            />
            {history.data?.pages[0]?.route ? (
                <section className="space-y-4" aria-label="本次审批路线">
                    <h3 className="font-medium">
                        审批链条{" "}
                        <span className="ml-2 text-xs font-normal text-muted-foreground">
                            共 {history.data.pages[0].route.nodes.length} 个节点
                        </span>
                    </h3>
                    <ApprovalRoute
                        route={history.data.pages[0].route}
                        orientation={orientation}
                    />
                </section>
            ) : null}
            {history.isError ? (
                <BusinessFailureState
                    id={`${idPrefix}-history-retry`}
                    title={items.length > 0 ? "审批记录刷新失败" : undefined}
                    readContext={{
                        subject: "审批记录",
                        impact: historyImpact,
                    }}
                    error={history.error}
                    onRetry={() => void history.refetch()}
                />
            ) : null}
            {orientation === "horizontal" && history.isPending ? (
                <p role="status" className="text-xs text-muted-foreground">
                    正在读取完整审批链…
                </p>
            ) : null}
            {orientation === "horizontal" &&
            history.isSuccess &&
            !history.data.pages[0]?.route ? (
                <p className="text-xs text-muted-foreground">
                    本次审批链暂不可用，请刷新后重试。
                </p>
            ) : null}
            <details
                open={orientation === "vertical" ? true : undefined}
                className="space-y-4"
            >
                <summary
                    id={`${idPrefix}-records-toggle`}
                    className={
                        orientation === "horizontal"
                            ? "cursor-pointer text-body-compact font-medium"
                            : "hidden"
                    }
                >
                    查看审批记录与处理意见
                </summary>
                {history.isPending && items.length === 0 ? (
                    <p
                        role="status"
                        className="py-4 text-xs text-muted-foreground"
                    >
                        正在读取审批记录…
                    </p>
                ) : items.length > 0 ? (
                    <SalesOrderApprovalHistory
                        key={`${instance.id}-${instance.currentRoundNo}`}
                        instance={instance}
                        idPrefix={idPrefix}
                        items={items}
                        hasMore={hasMore}
                    />
                ) : !history.isError ? (
                    <p className="text-xs text-muted-foreground">
                        暂无审批记录。
                    </p>
                ) : null}
                {history.hasNextPage && !historyFailure?.accessDenied ? (
                    <Button
                        id={`${idPrefix}-history-load-more`}
                        variant="outline"
                        size="sm"
                        disabled={history.isFetchingNextPage}
                        onClick={() => void history.fetchNextPage()}
                    >
                        {history.isFetchingNextPage
                            ? "正在读取…"
                            : "加载更早记录"}
                    </Button>
                ) : null}
                <ApprovalCopyHistory
                    instanceId={instance.id}
                    idPrefix={`${idPrefix}-copies`}
                />
            </details>
        </div>
    )
}
