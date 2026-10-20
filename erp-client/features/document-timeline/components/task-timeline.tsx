"use client"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useTimelineQuery } from "../hooks/queries"
import type { TimelineSource } from "../types"
import { TimelineEvents } from "./timeline-events"
import { SalesTimelineSheet } from "./sales-timeline"

export function TaskTimeline({
    taskId,
    approval = false,
    instance = false,
}: {
    taskId: string
    approval?: boolean
    instance?: boolean
}) {
    const [expanded, setExpanded] = useState(!approval)
    const [source, setSource] = useState<TimelineSource | null>(null)
    const idPrefix = `workspace-timeline-${toAutomationIdSegment(taskId)}`
    const query = useTimelineQuery(
        { kind: instance ? "approval" : "task", id: taskId },
        { pageSize: 5 },
        expanded,
    )
    const first = query.data?.pages[0]
    return (
        <section
            className="mt-4 rounded-lg border border-border/60 bg-background p-4"
            aria-label={approval ? "相关业务动态" : "办理动态"}
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-sm font-medium">
                    {approval ? "相关业务动态" : "办理动态"}
                </h3>
                <Button
                    id={`${idPrefix}-toggle`}
                    size="sm"
                    variant="ghost"
                    aria-expanded={expanded}
                    aria-controls={`${idPrefix}-content`}
                    onClick={() => setExpanded(!expanded)}
                >
                    {expanded ? "收起" : "展开"}
                </Button>
            </div>
            {expanded && (
                <div id={`${idPrefix}-content`} className="mt-4 space-y-4">
                    {approval && (
                        <p className="text-xs text-muted-foreground">
                            动态展示实际发生的操作，本次审批依据仍以提交资料为准。
                        </p>
                    )}
                    {query.isPending ? (
                        <p className="text-sm text-muted-foreground">
                            正在读取动态…
                        </p>
                    ) : query.isError ? (
                        <div
                            role="alert"
                            className="flex flex-wrap items-center gap-3 text-sm"
                        >
                            <p className="text-destructive">
                                动态暂时无法读取。
                            </p>
                            <Button
                                id={`${idPrefix}-retry`}
                                variant="outline"
                                size="sm"
                                onClick={() => void query.refetch()}
                            >
                                重试
                            </Button>
                        </div>
                    ) : (
                        <TimelineEvents
                            events={
                                query.data?.pages.flatMap(
                                    (page) => page.items,
                                ) ?? []
                            }
                            idPrefix={idPrefix}
                        />
                    )}
                    <div className="flex flex-wrap items-center gap-3">
                        {query.hasNextPage && (
                            <Button
                                id={`${idPrefix}-more`}
                                variant="outline"
                                size="sm"
                                disabled={query.isFetching}
                                onClick={() => void query.fetchNextPage()}
                            >
                                查看更早记录
                            </Button>
                        )}
                        {first?.salesOrders?.map((sale) => (
                            <Button
                                key={sale.id}
                                id={`${idPrefix}-sales-${toAutomationIdSegment(sale.id)}`}
                                variant="link"
                                size="sm"
                                onClick={() => setSource(sale)}
                            >
                                {first.salesOrders?.length === 1
                                    ? "查看销售单全流程"
                                    : `查看 ${sale.number || "关联销售单"} 全流程`}
                            </Button>
                        ))}
                    </div>
                </div>
            )}
            <SalesTimelineSheet
                source={source}
                onClose={() => setSource(null)}
                idPrefix={idPrefix}
            />
        </section>
    )
}
