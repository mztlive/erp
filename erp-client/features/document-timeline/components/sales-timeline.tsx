"use client"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { QuickPreviewSheet } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useTimelineQuery } from "../hooks/queries"
import type { TimelineCategory, TimelineSource } from "../types"
import { TimelineEvents } from "./timeline-events"

const categories: Array<{ value: TimelineCategory; label: string }> = [
    { value: "sales", label: "销售" },
    { value: "approval", label: "审批" },
    { value: "procurement", label: "采购" },
    { value: "fulfillment", label: "履约" },
    { value: "receivable", label: "客户票款" },
    { value: "payable", label: "供应商票款" },
    { value: "returns", label: "退货与调整" },
]

export function SalesTimeline({
    salesOrderId,
    idPrefix,
    compact = false,
    onOpenFull,
}: {
    salesOrderId: string
    idPrefix: string
    compact?: boolean
    onOpenFull?: () => void
}) {
    const [category, setCategory] = useState<TimelineCategory | undefined>()
    const [includeDetails, setIncludeDetails] = useState(false)
    const query = useTimelineQuery(
        { kind: "sales", id: salesOrderId },
        { category, includeDetails, pageSize: compact ? 5 : 30 },
    )
    const events = query.data?.pages.flatMap((page) => page.items) ?? []
    return (
        <section
            className="space-y-5"
            aria-label={compact ? "最近动态" : "销售单全流程"}
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <h3 className="text-sm font-medium">
                    {compact ? "最近动态" : "全流程时间线"}
                </h3>
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
            {!compact && (
                <div className="flex flex-wrap items-center gap-3">
                    <label htmlFor={`${idPrefix}-category`} className="text-sm">
                        业务环节
                    </label>
                    <NativeSelect
                        id={`${idPrefix}-category`}
                        value={category ?? ""}
                        onChange={(event) =>
                            setCategory(
                                (event.target.value || undefined) as
                                    | TimelineCategory
                                    | undefined,
                            )
                        }
                    >
                        <NativeSelectOption value="">
                            全部环节
                        </NativeSelectOption>
                        {categories.map((item) => (
                            <NativeSelectOption
                                key={item.value}
                                value={item.value}
                            >
                                {item.label}
                            </NativeSelectOption>
                        ))}
                    </NativeSelect>
                    <label
                        htmlFor={`${idPrefix}-details`}
                        className="inline-flex items-center gap-2 text-sm"
                    >
                        <NativeCheckbox
                            id={`${idPrefix}-details`}
                            checked={includeDetails}
                            onChange={(event) =>
                                setIncludeDetails(event.target.checked)
                            }
                        />
                        包含草稿与资料修改
                    </label>
                </div>
            )}
            {query.isPending ? (
                <p className="text-sm text-muted-foreground">正在读取动态…</p>
            ) : query.isError ? (
                <div
                    role="alert"
                    className="space-y-2 text-sm text-destructive"
                >
                    <p>动态读取失败，请重试。当前单据内容仍可继续查看。</p>
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
                    events={compact ? events.slice(0, 5) : events}
                    idPrefix={idPrefix}
                />
            )}
            {!compact && query.hasNextPage && (
                <Button
                    id={`${idPrefix}-more`}
                    variant="outline"
                    size="sm"
                    disabled={query.isFetching}
                    onClick={() => void query.fetchNextPage()}
                >
                    {query.isFetchingNextPage ? "正在读取…" : "加载更早记录"}
                </Button>
            )}
            {compact && onOpenFull && (
                <Button
                    id={`${idPrefix}-full`}
                    variant="link"
                    className="h-auto p-0"
                    onClick={onOpenFull}
                >
                    查看全流程
                </Button>
            )}
            {!compact && query.data && (
                <p className="border-t pt-3 text-xs leading-5 text-muted-foreground">
                    {query.data.pages[0]?.notice}
                </p>
            )}
        </section>
    )
}

export function SalesTimelineSheet({
    source,
    onClose,
    idPrefix,
}: {
    source: TimelineSource | null
    onClose: () => void
    idPrefix: string
}) {
    return (
        <QuickPreviewSheet
            idPrefix={`${idPrefix}-sheet`}
            size="detail"
            open={Boolean(source)}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            title="销售单全流程"
            identity={source?.number ? `销售单号：${source.number}` : undefined}
            footer={
                <>
                    <Button
                        id={`${idPrefix}-sheet-footer-close`}
                        variant="outline"
                        onClick={onClose}
                    >
                        关闭
                    </Button>
                    {source && (
                        <a
                            id={`${idPrefix}-sheet-open-order-${toAutomationIdSegment(source.id)}`}
                            href={`/sales/orders/${encodeURIComponent(source.id)}?section=timeline`}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="text-sm text-primary"
                        >
                            打开销售单
                        </a>
                    )}
                </>
            }
        >
            <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
                {source && (
                    <SalesTimeline
                        key={source.id}
                        salesOrderId={source.id}
                        idPrefix={`${idPrefix}-sheet-timeline`}
                    />
                )}
            </div>
        </QuickPreviewSheet>
    )
}
