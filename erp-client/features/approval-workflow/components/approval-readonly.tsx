"use client"

import { cn } from "@/lib/utils"
import { BusinessFailureState } from "@/components/business"
import { DefinitionBindingCard } from "./definition-binding-card"
import { ExecutionHistory } from "./execution-history"
import { RuntimeSummary } from "./runtime-summary"
import { useApprovalHistoryInfiniteQuery } from "../queries"
import type { DocumentApprovalView } from "../types"

/** 单据查阅使用的审批摘要与历史，不加载决定或恢复操作。 */
export function ApprovalReadonly({
    approval,
    id,
    className,
}: {
    approval?: DocumentApprovalView
    id: string
    className?: string
}) {
    const instanceId = approval?.instance?.id
    const history = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "" },
        Boolean(instanceId),
    )
    if (!approval)
        return <p className="text-sm text-muted-foreground">暂无审批记录。</p>
    return (
        <div className={cn("space-y-4", className)}>
            {approval.instance ? (
                <RuntimeSummary instance={approval.instance} compact />
            ) : (
                <DefinitionBindingCard
                    definition={approval.definition}
                    compact
                />
            )}
            {history.isError ? (
                <BusinessFailureState
                    id={`${id}-retry`}
                    title="审批历史加载失败"
                    error={history.error}
                    onRetry={() => void history.refetch()}
                />
            ) : null}
            <ExecutionHistory
                id={`${id}-history`}
                items={
                    history.data?.pages.flatMap((page) => page.items) ??
                    approval.recentHistory ??
                    []
                }
                hasMore={history.hasNextPage}
                loadingMore={history.isFetchingNextPage}
                onLoadMore={
                    history.hasNextPage
                        ? () => {
                              void history.fetchNextPage()
                          }
                        : undefined
                }
                compact
            />
        </div>
    )
}
