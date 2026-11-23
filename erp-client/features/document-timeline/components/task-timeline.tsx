"use client"

import Link from "next/link"
import { useState } from "react"
import { ArrowUpRightIcon, HistoryIcon } from "lucide-react"

import { OptionCombobox, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { SheetTrigger } from "@/components/ui/sheet"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useTimelineQuery } from "../hooks/queries"
import type { TimelineCategory } from "../types"
import { SalesTimeline, timelineCategories } from "./sales-timeline"
import { TimelineEvents } from "./timeline-events"

type TaskTimelineView = "task" | "sales"

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
    const [view, setView] = useState<TaskTimelineView>("task")
    const [selectedSalesId, setSelectedSalesId] = useState<string | null>(null)
    const [salesCategory, setSalesCategory] = useState<
        TimelineCategory | undefined
    >()
    const [includeSalesDetails, setIncludeSalesDetails] = useState(false)

    const idPrefix = `workspace-timeline-${toAutomationIdSegment(taskId)}`
    const query = useTimelineQuery(
        { kind: instance ? "approval" : "task", id: taskId },
        { pageSize: 5 },
        open,
    )
    const first = query.data?.pages[0]
    const salesOrders = first?.salesOrders ?? []
    const hasSalesFlow = salesOrders.length > 0
    const activeSale =
        salesOrders.find((sale) => sale.id === selectedSalesId) ??
        salesOrders[0] ??
        null
    const effectiveView: TaskTimelineView =
        view === "sales" && activeSale ? "sales" : "task"

    const resetAndClose = () => {
        setOpen(false)
        setView("task")
        setSelectedSalesId(null)
    }

    return (
        <QuickPreviewSheet
            idPrefix={`${idPrefix}-sheet`}
            size="detail"
            open={open}
            onOpenChange={(nextOpen) => {
                setOpen(nextOpen)
                if (!nextOpen) {
                    setView("task")
                    setSelectedSalesId(null)
                }
            }}
            identity={
                effectiveView === "sales" && activeSale?.number
                    ? `销售单号：${activeSale.number}`
                    : document.number
                      ? `单号：${document.number}`
                      : undefined
            }
            title={effectiveView === "sales" ? "关联销售全链路" : "办理记录"}
            description={title}
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
                <>
                    <Button
                        id={`${idPrefix}-sheet-footer-close`}
                        variant="outline"
                        onClick={resetAndClose}
                    >
                        关闭
                    </Button>
                    {effectiveView === "sales" && activeSale ? (
                        <Button
                            id={`${idPrefix}-sheet-open-order-${toAutomationIdSegment(activeSale.id)}`}
                            render={
                                <Link
                                    href={`/sales/orders/${encodeURIComponent(activeSale.id)}?section=timeline`}
                                    onClick={resetAndClose}
                                />
                            }
                        >
                            打开销售单
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    ) : null}
                </>
            }
        >
            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-sheet-inline py-5">
                <div className="min-w-0 space-y-5 break-words">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        {hasSalesFlow ? (
                            <div
                                role="tablist"
                                aria-label="动态视图切换"
                                className="inline-flex items-center rounded-md border border-border/80 bg-muted/50 p-0.5"
                            >
                                <Button
                                    id={`${idPrefix}-view-task`}
                                    type="button"
                                    role="tab"
                                    aria-selected={effectiveView === "task"}
                                    size="xs"
                                    variant={
                                        effectiveView === "task"
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    onClick={() => setView("task")}
                                >
                                    当前任务动态
                                </Button>
                                <Button
                                    id={`${idPrefix}-view-sales`}
                                    type="button"
                                    role="tab"
                                    aria-selected={effectiveView === "sales"}
                                    size="xs"
                                    variant={
                                        effectiveView === "sales"
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    onClick={() => {
                                        if (!selectedSalesId && activeSale) {
                                            setSelectedSalesId(activeSale.id)
                                        }
                                        setView("sales")
                                    }}
                                >
                                    关联销售全链路
                                </Button>
                            </div>
                        ) : (
                            <div />
                        )}
                        {effectiveView === "task" ? (
                            <Button
                                id={`${idPrefix}-refresh`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                disabled={query.isFetching}
                                onClick={() => void query.refetch()}
                            >
                                刷新
                            </Button>
                        ) : null}
                    </div>

                    {effectiveView === "task" ? (
                        <>
                            {approval ? (
                                <p className="text-xs text-muted-foreground">
                                    以下为单据最新动态；审批请以本次提交时的资料为准。
                                </p>
                            ) : null}
                            {query.isPending ? (
                                <p
                                    role="status"
                                    className="text-xs text-muted-foreground"
                                >
                                    正在读取记录…
                                </p>
                            ) : query.isError ? (
                                <div
                                    role="alert"
                                    className="space-y-3 text-body-compact"
                                >
                                    <p className="text-destructive">
                                        办理记录暂时无法读取。
                                    </p>
                                    <Button
                                        id={`${idPrefix}-retry`}
                                        type="button"
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
                                    {query.hasNextPage ? (
                                        <Button
                                            id={`${idPrefix}-more`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={query.isFetching}
                                            onClick={() =>
                                                void query.fetchNextPage()
                                            }
                                        >
                                            {query.isFetchingNextPage
                                                ? "正在读取…"
                                                : "查看更早记录"}
                                        </Button>
                                    ) : null}
                                    {hasSalesFlow ? (
                                        <div className="flex flex-wrap gap-3 border-t border-border/60 pt-4">
                                            {salesOrders.map((sale) => (
                                                <Button
                                                    key={sale.id}
                                                    id={`${idPrefix}-sales-${toAutomationIdSegment(sale.id)}`}
                                                    type="button"
                                                    variant="link"
                                                    size="sm"
                                                    className="h-auto whitespace-normal p-0 text-left"
                                                    onClick={() => {
                                                        setSelectedSalesId(
                                                            sale.id,
                                                        )
                                                        setView("sales")
                                                    }}
                                                >
                                                    {salesOrders.length === 1
                                                        ? "查看关联销售全链路"
                                                        : `查看 ${sale.number || "关联销售单"} 全链路`}
                                                </Button>
                                            ))}
                                        </div>
                                    ) : null}
                                </>
                            )}
                        </>
                    ) : activeSale ? (
                        <div className="space-y-4">
                            {salesOrders.length > 1 ? (
                                <div className="flex flex-wrap gap-1.5">
                                    {salesOrders.map((sale) => {
                                        const selected =
                                            sale.id === activeSale.id
                                        return (
                                            <Button
                                                key={sale.id}
                                                id={`${idPrefix}-sales-order-${toAutomationIdSegment(sale.id)}`}
                                                type="button"
                                                size="xs"
                                                variant={
                                                    selected
                                                        ? "secondary"
                                                        : "outline"
                                                }
                                                aria-pressed={selected}
                                                className="num"
                                                onClick={() =>
                                                    setSelectedSalesId(sale.id)
                                                }
                                            >
                                                {sale.number || "关联销售单"}
                                            </Button>
                                        )
                                    })}
                                </div>
                            ) : null}
                            <div className="flex flex-wrap items-center gap-3">
                                <OptionCombobox
                                    id={`${idPrefix}-sales-category`}
                                    options={[
                                        { value: "all", label: "全部环节" },
                                        ...timelineCategories,
                                    ]}
                                    value={salesCategory ?? "all"}
                                    onValueChange={(value) =>
                                        setSalesCategory(
                                            !value || value === "all"
                                                ? undefined
                                                : (value as TimelineCategory),
                                        )
                                    }
                                    allowClear={false}
                                    size="sm"
                                    aria-label="业务环节"
                                    className="w-36"
                                    inputClassName="w-36"
                                />
                                <label
                                    htmlFor={`${idPrefix}-sales-details`}
                                    className="inline-flex cursor-pointer items-center gap-2 text-xs text-muted-foreground"
                                >
                                    <NativeCheckbox
                                        id={`${idPrefix}-sales-details`}
                                        checked={includeSalesDetails}
                                        onChange={(event) =>
                                            setIncludeSalesDetails(
                                                event.target.checked,
                                            )
                                        }
                                    />
                                    包含草稿与资料修改
                                </label>
                            </div>
                            <SalesTimeline
                                key={activeSale.id}
                                salesOrderId={activeSale.id}
                                idPrefix={`${idPrefix}-sales-flow`}
                                embedded
                                fixedCategory={salesCategory}
                                includeDetails={includeSalesDetails}
                                currentDocument={document}
                            />
                        </div>
                    ) : null}
                </div>
            </div>
        </QuickPreviewSheet>
    )
}
