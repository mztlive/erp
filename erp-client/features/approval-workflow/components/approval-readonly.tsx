"use client"

import { ApprovalCopyHistory } from "./copy-history"

import { cn } from "@/lib/utils"
import { BusinessFailureState } from "@/components/business"
import { DefinitionBindingCard } from "./definition-binding-card"
import { ExecutionHistory } from "./execution-history"
import { RuntimeSummary } from "./runtime-summary"
import { isBlockedStatus } from "../display"
import { useApprovalHistoryInfiniteQuery } from "../queries"
import type { DocumentApprovalView } from "../types"

/** 单据查阅使用的审批摘要与历史，不加载决定或恢复操作。 */
export function ApprovalReadonly({
    approval,
    id,
    className,
    presentation = "detail",
}: {
    approval?: DocumentApprovalView
    id: string
    className?: string
    /** 外层已有审批记录标题时，只展示时间轴；受阻说明仍保留。 */
    presentation?: "detail" | "history"
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
                presentation === "detail" ||
                isBlockedStatus(approval.instance.status) ? (
                    <RuntimeSummary instance={approval.instance} compact />
                ) : null
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
                presentation={presentation === "history" ? "timeline" : "card"}
            />
            {instanceId && (
                <ApprovalCopyHistory
                    instanceId={instanceId}
                    idPrefix={`${id}-copies`}
                />
            )}
        </div>
    )
}
