"use client"

import { ChevronRightIcon, HistoryIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Spinner } from "@/components/ui/spinner"
import type { ContractImportTask } from "@/features/contracts/api/upload"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

const STATUS = {
    ready: { label: "等待识别", variant: "secondary" },
    processing: { label: "正在识别", variant: "secondary" },
    failed: { label: "导入失败", variant: "destructive" },
    succeeded: { label: "已归档", variant: "success" },
} as const

export function ContractImportHistory({
    items,
    total,
    loading,
    page,
    selectedId,
    currentTask,
    disabled,
    onSelect,
    onPageChange,
}: {
    items: ContractImportTask[]
    total: number
    loading: boolean
    page: number
    selectedId: string
    currentTask?: ContractImportTask
    disabled: boolean
    onSelect: (id: string) => void
    onPageChange: (page: number) => void
}) {
    return (
        <Collapsible className="border-y border-border">
            <CollapsibleTrigger
                id="contract-import-history-toggle"
                className="group flex w-full items-center gap-3 py-4 text-left text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
                <HistoryIcon
                    className="size-4 text-muted-foreground"
                    aria-hidden="true"
                />
                最近导入
                <Badge variant="secondary">{total}</Badge>
                <ChevronRightIcon
                    className="ml-auto size-4 transition-transform group-data-panel-open:rotate-90"
                    aria-hidden="true"
                />
            </CollapsibleTrigger>
            <CollapsibleContent>
                <div className="space-y-2 pb-4">
                    {loading ? (
                        <p className="flex items-center gap-2 py-3 text-sm text-muted-foreground">
                            <Spinner />
                            加载导入记录…
                        </p>
                    ) : null}
                    {!loading && items.length === 0 ? (
                        <p className="py-3 text-sm text-muted-foreground">
                            暂无导入记录
                        </p>
                    ) : null}
                    {items.map((record) => {
                        const item =
                            currentTask?.id === record.id ? currentTask : record
                        const status = STATUS[item.status]
                        return (
                            <button
                                id={`contract-import-select-${toAutomationIdSegment(item.id)}`}
                                key={item.id}
                                type="button"
                                disabled={disabled}
                                aria-pressed={selectedId === item.id}
                                onClick={() => onSelect(item.id)}
                                className={cn(
                                    "flex w-full min-w-0 items-center gap-3 rounded-lg px-3 py-3 text-left transition-colors outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
                                    selectedId === item.id && "bg-muted/60",
                                )}
                            >
                                <span className="min-w-0 flex-1">
                                    <span
                                        className="block truncate text-sm font-medium"
                                        title={item.file_name}
                                    >
                                        {item.file_name}
                                    </span>
                                    <span className="mt-1 block text-xs text-muted-foreground">
                                        {item.page_count} 页
                                        {item.started_at
                                            ? ` · 开始于 ${formatDateTime(new Date(item.started_at * 1000).toISOString())}`
                                            : " · 尚未开始识别"}
                                    </span>
                                </span>
                                <Badge variant={status.variant}>
                                    {status.label}
                                </Badge>
                            </button>
                        )
                    })}
                    {total > 20 || page > 1 ? (
                        <div className="flex items-center justify-end gap-2 pt-2">
                            <span className="mr-auto text-xs text-muted-foreground">
                                第 {page} 页 · 共 {total} 条
                            </span>
                            <Button
                                id="contract-import-page-previous"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={page <= 1 || loading}
                                onClick={() => onPageChange(page - 1)}
                            >
                                上一页
                            </Button>
                            <Button
                                id="contract-import-page-next"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={page * 20 >= total || loading}
                                onClick={() => onPageChange(page + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    ) : null}
                </div>
            </CollapsibleContent>
        </Collapsible>
    )
}
