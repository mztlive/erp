"use client"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import { ApprovalQueryFeedback } from "@/features/approval-workflow/components/approval-query-feedback"
import { ApprovalRoute } from "@/features/approval-workflow/components/approval-route"
import { ExecutionHistory } from "@/features/approval-workflow/components/execution-history"
import { displayRound } from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 单栏销售审批首屏展示完整冻结路线，历史意见按需查看。 */
export function WorkspaceApprovalRouteSummary({
    instance,
}: {
    instance: ApprovalRuntimeInstance
}) {
    const historyQuery = useApprovalHistoryInfiniteQuery({
        instanceId: instance.id,
        order: "desc",
    })
    const route = historyQuery.data?.pages[0]?.route
    const history = historyQuery.data?.pages.flatMap((page) => page.items) ?? []
    const prefix = `workspace-approval-route-${toAutomationIdSegment(instance.id)}`

    return (
        <section
            aria-label="审批流程"
            className="space-y-3 border-t border-border/60 px-5 py-4 sm:px-6"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-3">
                    <h3 className="text-sm font-semibold">审批流程</h3>
                    <span className="text-xs text-muted-foreground">
                        {displayRound(
                            route?.roundNo ?? instance.currentRoundNo,
                        )}
                    </span>
                </div>
                <Dialog>
                    <DialogTrigger
                        id={`${prefix}-history`}
                        render={<Button variant="ghost" size="sm" />}
                    >
                        查看审批记录
                    </DialogTrigger>
                    <DialogContent
                        closeButtonId={`${prefix}-history-close`}
                        className="flex max-h-[85dvh] flex-col sm:max-w-2xl"
                    >
                        <DialogHeader className="shrink-0 pr-8">
                            <DialogTitle>审批记录</DialogTitle>
                            <DialogDescription>
                                查看各轮审批的处理人、时间和审批意见。
                            </DialogDescription>
                        </DialogHeader>
                        <div className="min-h-0 space-y-4 overflow-auto">
                            {historyQuery.isPending ? (
                                <p
                                    role="status"
                                    className="text-sm text-muted-foreground"
                                >
                                    正在加载审批记录…
                                </p>
                            ) : history.length > 0 || !historyQuery.isError ? (
                                <ExecutionHistory
                                    id={`${prefix}-executions`}
                                    items={history}
                                    presentation="timeline"
                                    hasMore={historyQuery.hasNextPage}
                                    loadingMore={
                                        historyQuery.isFetchingNextPage
                                    }
                                    onLoadMore={() =>
                                        void historyQuery.fetchNextPage()
                                    }
                                />
                            ) : null}
                            <ApprovalQueryFeedback
                                surface={`${prefix}-dialog`}
                                instanceId={instance.id}
                                historyQuery={historyQuery}
                            />
                        </div>
                    </DialogContent>
                </Dialog>
            </div>
            {route?.nodes.length ? (
                <ApprovalRoute route={route} orientation="responsive" compact />
            ) : historyQuery.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在加载审批流程…
                </p>
            ) : !historyQuery.isError ? (
                <BusinessFailureState
                    id={`${prefix}-missing`}
                    title="完整审批流程暂不可用，请刷新重试"
                    onRetry={() => void historyQuery.refetch()}
                />
            ) : null}
            {historyQuery.isError ? (
                <BusinessFailureState
                    id={`${prefix}-failure`}
                    title="审批流程读取失败，请重试；已显示的流程可能不是最新状态"
                    error={historyQuery.error}
                    onRetry={() => void historyQuery.refetch()}
                />
            ) : null}
        </section>
    )
}
