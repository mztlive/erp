"use client"

import type { ReactNode } from "react"
import { ChevronDownIcon, FilterIcon, XIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { listFilterText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

/** 锚定查询栏的高级筛选；仅正文滚动，应用操作始终可见。 */
export function ListFilterPopover({
    idPrefix,
    triggerId,
    queryId,
    panelId,
    label,
    open,
    onOpenChange,
    count,
    size = "wide",
    onApply,
    onReset,
    resetId,
    children,
}: {
    idPrefix: string
    triggerId: string
    queryId: string
    panelId: string
    label: string
    open: boolean
    onOpenChange: (open: boolean) => void
    count: number
    size?: "compact" | "wide"
    onApply: () => void
    onReset?: () => void
    resetId?: string
    children: ReactNode
}) {
    return (
        <Popover
            open={open}
            onOpenChange={(nextOpen, details) => {
                // 外部「查询」仍应提交本次全部草稿，不能先被外点取消清掉。
                const target = details.event.target
                if (
                    !nextOpen &&
                    target instanceof Element &&
                    target.closest("button")?.id === queryId
                ) {
                    details.cancel()
                    return
                }
                onOpenChange(nextOpen)
            }}
        >
            <PopoverTrigger
                render={
                    <Button id={triggerId} type="button" variant="outline" />
                }
                aria-controls={panelId}
            >
                <FilterIcon aria-hidden="true" />
                {listFilterText.more}
                {count > 0 && (
                    <span
                        className="rounded bg-muted px-1.5 text-xs tabular-nums"
                        aria-label={`${count} 项已生效`}
                    >
                        {count}
                    </span>
                )}
                <ChevronDownIcon
                    aria-hidden="true"
                    className={cn("transition-transform", open && "rotate-180")}
                />
            </PopoverTrigger>
            <PopoverContent
                id={panelId}
                aria-label={label}
                align="end"
                sideOffset={8}
                className={cn(
                    "max-h-[min(44rem,var(--available-height))] max-w-[calc(100vw-2rem)] gap-0 overflow-hidden rounded-xl p-0",
                    size === "compact" ? "w-80" : "w-[36rem]",
                )}
            >
                <div className="flex shrink-0 items-center justify-between border-b px-4 py-3">
                    <PopoverTitle className="text-sm font-semibold">
                        {listFilterText.more}
                    </PopoverTitle>
                    <Button
                        id={`${idPrefix}-more-close`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label={listFilterText.close}
                        onClick={() => onOpenChange(false)}
                    >
                        <XIcon aria-hidden="true" />
                    </Button>
                </div>
                <form
                    className="flex min-h-0 flex-col"
                    aria-label={label}
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        onApply()
                    }}
                >
                    <div className="min-h-0 overflow-y-auto overscroll-contain p-4">
                        {children}
                    </div>
                    <div className="flex shrink-0 items-center gap-2 border-t bg-popover px-4 py-3">
                        {onReset && (
                            <Button
                                id={resetId ?? `${idPrefix}-reset-more`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                onClick={onReset}
                            >
                                {listFilterText.reset}
                            </Button>
                        )}
                        <div className="ml-auto flex gap-2">
                            <Button
                                id={`${idPrefix}-more-cancel`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => onOpenChange(false)}
                            >
                                {listFilterText.cancel}
                            </Button>
                            <Button
                                id={`${idPrefix}-more-apply`}
                                type="submit"
                                size="sm"
                            >
                                {listFilterText.apply}
                            </Button>
                        </div>
                    </div>
                </form>
            </PopoverContent>
        </Popover>
    )
}
