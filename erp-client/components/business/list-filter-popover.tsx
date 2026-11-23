"use client"

import type * as React from "react"
import { FilterIcon, RotateCcwIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible"
import { LoadingButton } from "@/components/ui/loading-button"
import { listFilterText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

/** 查询主按钮：有待提交改动或位于展开托盘页脚时用主蓝强调，顶栏空闲态退为中性轻按钮避免与筛选高亮打架。 */
export function ListFilterQueryButton({
    id,
    hasPendingChanges = false,
    emphasize = false,
}: {
    id: string
    hasPendingChanges?: boolean
    emphasize?: boolean
}) {
    const isPrimary = hasPendingChanges || emphasize
    return (
        <LoadingButton
            id={id}
            type="submit"
            variant={isPrimary ? "default" : "ghost"}
            className={cn(
                "shadow-none",
                isPrimary
                    ? "px-3.5"
                    : "px-2.5 font-normal text-muted-foreground hover:bg-muted/80 hover:text-foreground",
            )}
        >
            {listFilterText.query}
        </LoadingButton>
    )
}

/** 高级筛选触发按钮：平时无框轻按钮，展开或有已选条件时用轻冰蓝底 + 轻数字标记，保持工具栏视觉克制。 */
export function ListFilterTrigger({
    triggerId,
    panelId,
    open,
    count,
    onToggle,
}: {
    triggerId: string
    panelId: string
    open: boolean
    count: number
    onToggle: () => void
}) {
    return (
        <Button
            id={triggerId}
            type="button"
            variant="ghost"
            aria-expanded={open}
            aria-controls={panelId}
            className={cn(
                count > 0 || open
                    ? "bg-sidebar-accent/75 text-sidebar-accent-foreground hover:bg-sidebar-accent"
                    : "font-normal text-foreground/75 hover:bg-muted/80 hover:text-foreground",
            )}
            onClick={onToggle}
        >
            <FilterIcon aria-hidden="true" className="size-3.5" />
            {open ? listFilterText.collapse : listFilterText.more}
            {count > 0 ? (
                <span
                    className="rounded-sm bg-primary/12 px-1.5 text-tiny leading-4 font-semibold tabular-nums text-primary"
                    aria-label={`${count} 项已生效`}
                >
                    {count}
                </span>
            ) : null}
        </Button>
    )
}

/**
 * 飞书多维表格风格通栏展开式高级筛选区。
 * - 不遮挡表格、不产生浮层嵌套；保持挂载（hidden 切换）以保留未应用草稿；
 * - 通过 portal 挂载到列表工作面时自带独立 <form>，退回工具栏内部渲染时复用外层表单避免嵌套。
 */
export function ListFilterTray({
    idPrefix,
    panelId,
    label,
    open,
    onApply,
    onClose,
    onReset,
    resetId,
    hasPendingChanges = false,
    reactive = false,
    hint,
    inPortal = true,
    children,
}: {
    idPrefix: string
    panelId: string
    label: string
    open: boolean
    onApply: () => void
    onClose?: () => void
    onReset?: () => void
    resetId?: string
    hasPendingChanges?: boolean
    /** 即时筛选模式：条件选择后立即生效，隐藏手点「查询」按钮并改为收起「完成」。 */
    reactive?: boolean
    hint?: string
    /** true：已挂载到外层通栏槽（独立 form）；false：内联在父 form 内部（使用 div）。 */
    inPortal?: boolean
    children: React.ReactNode
}) {
    const content = (
        <>
            <div
                data-slot="list-filter-tray-body"
                className={cn(
                    "max-h-[min(26rem,44dvh)] overflow-y-auto overscroll-contain pb-2.5",
                    inPortal ? "pt-1" : "pt-2.5",
                )}
            >
                {children}
            </div>
            <div
                data-slot="list-filter-tray-footer"
                className="flex flex-wrap items-center justify-between gap-2 border-t border-border/40 pt-3 pb-3.5"
            >
                {hint ? (
                    <span className="min-w-0 truncate text-xs text-muted-foreground/75">
                        {hint}
                    </span>
                ) : null}
                <div className="ml-auto flex shrink-0 items-center gap-1.5">
                    {onReset ? (
                        <Button
                            id={resetId ?? `${idPrefix}-reset-more`}
                            type="button"
                            variant="ghost"
                            className="text-muted-foreground shadow-none hover:bg-muted/80 hover:text-primary"
                            onClick={onReset}
                        >
                            <RotateCcwIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                            {listFilterText.resetMore}
                        </Button>
                    ) : null}
                    {reactive ? (
                        <Button
                            id={`${idPrefix}-more-done`}
                            type="button"
                            variant="outline"
                            className="shadow-none"
                            onClick={onClose ?? onApply}
                        >
                            完成
                        </Button>
                    ) : (
                        <ListFilterQueryButton
                            id={`${idPrefix}-more-apply`}
                            hasPendingChanges={hasPendingChanges}
                            emphasize
                        />
                    )}
                </div>
            </div>
        </>
    )

    return (
        <Collapsible
            open={open}
            className="min-w-0 has-[>[data-slot=list-filter-tray-panel][hidden]]:hidden"
        >
            <CollapsibleContent
                keepMounted
                data-slot="list-filter-tray-panel"
                className={cn(
                    "group/tray h-(--collapsible-panel-height) overflow-hidden transition-[height,opacity] ease-out",
                    "data-open:duration-200 data-closed:duration-150 data-closed:ease-in",
                    "data-starting-style:h-0 data-starting-style:opacity-0",
                    "data-ending-style:h-0 data-ending-style:opacity-0",
                    "motion-reduce:transition-none",
                )}
            >
                <section
                    id={panelId}
                    data-slot="list-filter-tray"
                    data-open={open ? "" : undefined}
                    data-placement={inPortal ? "edge" : "embedded"}
                    aria-label={label}
                    className={cn(
                        "min-w-0 transition-transform ease-out",
                        "group-data-open/tray:duration-200 group-data-closed/tray:pointer-events-none group-data-closed/tray:duration-150 group-data-closed/tray:ease-in",
                        "group-data-[starting-style]/tray:-translate-y-1.5 group-data-[ending-style]/tray:-translate-y-1",
                        "motion-reduce:transition-none",
                        inPortal
                            ? "border-b border-border/75 bg-card px-4"
                            : "rounded-lg border border-border/75 bg-card px-4",
                    )}
                >
                    {inPortal ? (
                        <form
                            aria-label={label}
                            className="flex min-w-0 flex-col"
                            onSubmit={(event) => {
                                event.preventDefault()
                                event.stopPropagation()
                                onApply()
                            }}
                        >
                            {content}
                        </form>
                    ) : (
                        <div className="flex min-w-0 flex-col">{content}</div>
                    )}
                </section>
            </CollapsibleContent>
        </Collapsible>
    )
}
