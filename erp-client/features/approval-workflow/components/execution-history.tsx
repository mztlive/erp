"use client"

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

import { displayExecutionStatus, displayRound } from "../display"
import type { ApprovalHistoryItem } from "../types"

/**
 * 按轮次分组、按执行序号排序的审批历史。
 *
 * 同一节点跨轮次必须显示多条记录，不得按 node_key 去重。
 * 首屏只消费受控 recent_history，更多记录由游标接口加载。
 */
export function ExecutionHistory({
    items,
    hasMore = false,
    loadingMore = false,
    onLoadMore,
}: {
    items: readonly ApprovalHistoryItem[]
    hasMore?: boolean
    loadingMore?: boolean
    onLoadMore?: () => void
}) {
    const rounds = groupByRound(items)

    return (
        <Card size="sm">
            <CardHeader>
                <CardTitle>审批历史</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
                {rounds.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        暂无审批记录
                    </p>
                ) : (
                    rounds.map((round) => (
                        <section key={round.roundNo} className="space-y-2">
                            <h3 className="text-sm font-medium">
                                {displayRound(round.roundNo)}
                            </h3>
                            <ol className="space-y-2">
                                {round.items.map((item) => (
                                    <li
                                        key={item.executionId}
                                        className="rounded-md border border-border px-3 py-2 text-sm"
                                    >
                                        <p>
                                            {item.nodeName} ·{" "}
                                            {displayExecutionStatus(
                                                item.result,
                                            )}
                                        </p>
                                        {item.assigneeName || item.decidedBy ? (
                                            <p className="text-muted-foreground">
                                                {item.decidedBy ??
                                                    item.assigneeName}
                                            </p>
                                        ) : null}
                                        {item.decisionReason ? (
                                            <p className="text-muted-foreground">
                                                {item.decisionReason}
                                            </p>
                                        ) : null}
                                    </li>
                                ))}
                            </ol>
                        </section>
                    ))
                )}
                {hasMore && onLoadMore ? (
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={loadingMore}
                        onClick={onLoadMore}
                    >
                        {loadingMore ? "加载中" : "加载更多"}
                    </Button>
                ) : null}
            </CardContent>
        </Card>
    )
}

/**
 * 按轮次分组并按 execution_no 排序。保留跨轮次同一节点的多条记录。
 */
export const groupByRound = (
    items: readonly ApprovalHistoryItem[],
): readonly { roundNo: number; items: ApprovalHistoryItem[] }[] => {
    const grouped = new Map<number, ApprovalHistoryItem[]>()
    for (const item of items) {
        const bucket = grouped.get(item.roundNo) ?? []
        bucket.push(item)
        grouped.set(item.roundNo, bucket)
    }
    return [...grouped.entries()]
        .sort(([left], [right]) => left - right)
        .map(([roundNo, roundItems]) => ({
            roundNo,
            items: [...roundItems].sort(
                (left, right) => left.executionNo - right.executionNo,
            ),
        }))
}
