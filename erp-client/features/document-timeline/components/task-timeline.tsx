"use client"
import { useState } from "react"
import { HistoryIcon } from "lucide-react"
import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { SheetTrigger } from "@/components/ui/sheet"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useTimelineQuery } from "../hooks/queries"
import type { TimelineSource } from "../types"
import { TimelineEvents } from "./timeline-events"
import { SalesTimelineSheet } from "./sales-timeline"

export function TaskTimeline({
    taskId,
    approval = false,
    instance = false,
    title,
    document,
}: {
    taskId: string
    approval?: boolean
    instance?: boolean
    title: string
    document: { type: string; id: string; number: string }
}) {
    const [open, setOpen] = useState(false)
    const [source, setSource] = useState<TimelineSource | null>(null)
    const idPrefix = `workspace-timeline-${toAutomationIdSegment(taskId)}`
    const query = useTimelineQuery(
        { kind: instance ? "approval" : "task", id: taskId },
        { pageSize: 5 },
        open,
    )
    const first = query.data?.pages[0]
    return (
        <QuickPreviewSheet
            idPrefix={`${idPrefix}-sheet`}
            size="detail"
            open={open}
            onOpenChange={(nextOpen) => {
                setOpen(nextOpen)
                if (!nextOpen) setSource(null)
            }}
            title="办理记录"
            description={
                <>
                    {title}
                    {document.number && (
                        <span className="num mt-1 block">
                            单号：{document.number}
                        </span>
                    )}
                </>
            }
            trigger={
                <SheetTrigger
                    id={`${idPrefix}-trigger`}
                    render={<Button variant="ghost" size="sm" />}
                >
                    <HistoryIcon aria-hidden="true" />
                    办理记录
                </SheetTrigger>
            }
            footer={
                <Button
                    id={`${idPrefix}-sheet-footer-close`}
                    variant="outline"
                    onClick={() => {
                        setOpen(false)
                        setSource(null)
                    }}
                >
                    关闭
                </Button>
            }
        >
            <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-7 py-6">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <p className="text-xs text-muted-foreground">
                        按时间倒序展示
                    </p>
                    <Button
                        id={`${idPrefix}-refresh`}
                        variant="ghost"
                        size="sm"
                        disabled={query.isFetching}
                        onClick={() => void query.refetch()}
                    >
                        刷新
                    </Button>
                </div>
                {approval && (
                    <p className="text-xs text-muted-foreground">
                        记录展示实际发生的操作，本次审批依据仍以提交资料为准。
                    </p>
                )}
                {query.isPending ? (
                    <p role="status" className="text-sm text-muted-foreground">
                        正在读取记录…
                    </p>
                ) : query.isError ? (
                    <div role="alert" className="space-y-3 text-sm">
                        <p className="text-destructive">
                            办理记录暂时无法读取。
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
                    <>
                        <TimelineEvents
                            events={
                                query.data?.pages.flatMap(
                                    (page) => page.items,
                                ) ?? []
                            }
                            idPrefix={idPrefix}
                            groupByDate
                            currentDocument={document}
                        />
                        {query.hasNextPage && (
                            <Button
                                id={`${idPrefix}-more`}
                                variant="outline"
                                size="sm"
                                disabled={query.isFetching}
                                onClick={() => void query.fetchNextPage()}
                            >
                                {query.isFetchingNextPage
                                    ? "正在读取…"
                                    : "查看更早记录"}
                            </Button>
                        )}
                        {first?.salesOrders && first.salesOrders.length > 0 && (
                            <div className="flex flex-wrap gap-3 border-t pt-4">
                                {first.salesOrders.map((sale) => (
                                    <Button
                                        key={sale.id}
                                        id={`${idPrefix}-sales-${toAutomationIdSegment(sale.id)}`}
                                        variant="link"
                                        size="sm"
                                        className="h-auto whitespace-normal p-0 text-left"
                                        onClick={() => setSource(sale)}
                                    >
                                        {first.salesOrders?.length === 1
                                            ? "查看销售单全流程"
                                            : `查看 ${sale.number || "关联销售单"} 全流程`}
                                    </Button>
                                ))}
                            </div>
                        )}
                    </>
                )}
                <SalesTimelineSheet
                    source={source}
                    onClose={() => setSource(null)}
                    idPrefix={`${idPrefix}-sales-flow`}
                />
            </div>
        </QuickPreviewSheet>
    )
}
