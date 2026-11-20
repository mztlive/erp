"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, RotateCcwIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { listFilterText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

/** 高级筛选触发按钮：平时无框轻按钮，展开或有已选条件时高亮飞书冰蓝。 */
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
                    ? "bg-sidebar-accent text-sidebar-accent-foreground hover:bg-sidebar-accent"
                    : "text-foreground/80 hover:bg-muted/80 hover:text-foreground",
            )}
            onClick={onToggle}
        >
            <FilterIcon aria-hidden="true" className="size-3.5" />
            {listFilterText.more}
            {count > 0 ? (
                <span
                    className="rounded-full bg-primary px-1.5 text-tiny leading-4 font-medium tabular-nums text-primary-foreground"
                    aria-label={`${count} 项已生效`}
                >
                    {count}
                </span>
            ) : null}
            <ChevronDownIcon
                aria-hidden="true"
                className={cn(
                    "size-3.5 transition-transform duration-150",
                    open && "rotate-180",
                )}
            />
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
    onOpenChange,
    count,
    onApply,
    onReset,
    resetId,
    hint = "可组合多个条件，点击「应用」后生效",
    inPortal = true,
    children,
}: {
    idPrefix: string
    panelId: string
    label: string
    open: boolean
    onOpenChange: (open: boolean) => void
    count: number
    onApply: () => void
    onReset?: () => void
    resetId?: string
    hint?: string
    /** true：已挂载到外层通栏槽（独立 form）；false：内联在父 form 内部（使用 div）。 */
    inPortal?: boolean
    children: React.ReactNode
}) {
    const sectionRef = React.useRef<HTMLElement | null>(null)

    React.useEffect(() => {
        if (!open) return
        const node = sectionRef.current
        if (!node) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key === "Escape" && !event.defaultPrevented) {
                onOpenChange(false)
            }
        }
        node.addEventListener("keydown", onKeyDown)
        return () => node.removeEventListener("keydown", onKeyDown)
    }, [open, onOpenChange])

    const content = (
        <>
            <div
                data-slot="list-filter-tray-body"
                className={cn(
                    "max-h-[min(26rem,44dvh)] overflow-y-auto overscroll-contain pt-2.5 pb-2.5",
                    inPortal && "border-t border-border/45",
                )}
            >
                {children}
            </div>
            <div
                data-slot="list-filter-tray-footer"
                className="flex flex-wrap items-center justify-between gap-2 border-t border-border/45 py-2"
            >
                <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                    {count > 0 ? (
                        <span className="inline-flex items-center rounded-full bg-sidebar-accent px-2 py-0.5 text-tiny leading-4 font-medium tabular-nums text-sidebar-accent-foreground">
                            已生效 {count} 项
                        </span>
                    ) : null}
                    <span className="truncate text-muted-foreground/75">
                        {hint}
                    </span>
                </div>
                <div className="ml-auto flex shrink-0 items-center gap-1">
                    {onReset ? (
                        <Button
                            id={resetId ?? `${idPrefix}-reset-more`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="text-muted-foreground hover:bg-muted/80 hover:text-primary"
                            onClick={onReset}
                        >
                            <RotateCcwIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                            {listFilterText.reset}
                        </Button>
                    ) : null}
                    <Button
                        id={`${idPrefix}-more-cancel`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="text-foreground/80 hover:bg-muted/80 hover:text-foreground"
                        onClick={() => onOpenChange(false)}
                    >
                        收起
                    </Button>
                    <LoadingButton
                        id={`${idPrefix}-more-apply`}
                        type="submit"
                        size="sm"
                        className="ml-0.5 px-3.5"
                    >
                        {listFilterText.apply}
                    </LoadingButton>
                </div>
            </div>
        </>
    )

    return (
        <section
            ref={sectionRef}
            id={panelId}
            data-slot="list-filter-tray"
            data-placement={inPortal ? "edge" : "embedded"}
            aria-label={label}
            hidden={!open}
            className={cn(
                "min-w-0",
                !open && "hidden",
                open &&
                    "animate-in fade-in-0 slide-in-from-top-1 duration-150 motion-reduce:animate-none",
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
    )
}
