"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, RotateCcwIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { listFilterText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

/** 查询主按钮：收起态（顶栏）与展开态（托盘页脚）共用同一组件，保证外观与待查询高亮完全一致。 */
export function ListFilterQueryButton({
    id,
    hasPendingChanges = false,
}: {
    id: string
    hasPendingChanges?: boolean
}) {
    return (
        <LoadingButton
            id={id}
            type="submit"
            variant={hasPendingChanges ? "default" : "ghost"}
            className={cn(
                "shadow-none",
                hasPendingChanges
                    ? "px-3.5"
                    : "bg-sidebar-accent/55 px-3 text-primary hover:bg-sidebar-accent hover:text-primary",
            )}
        >
            {listFilterText.query}
        </LoadingButton>
    )
}

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
            {open ? listFilterText.collapse : listFilterText.more}
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
    onApply,
    onReset,
    resetId,
    hasPendingChanges = false,
    hint = "可组合多个条件，点击「查询」后统一生效",
    inPortal = true,
    children,
}: {
    idPrefix: string
    panelId: string
    label: string
    open: boolean
    onOpenChange: (open: boolean) => void
    onApply: () => void
    onReset?: () => void
    resetId?: string
    hasPendingChanges?: boolean
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
                <span className="min-w-0 truncate text-xs text-muted-foreground/75">
                    {hint}
                </span>
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
                    <ListFilterQueryButton
                        id={`${idPrefix}-more-apply`}
                        hasPendingChanges={hasPendingChanges}
                    />
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
