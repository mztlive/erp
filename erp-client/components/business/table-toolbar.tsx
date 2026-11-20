"use client"

import * as React from "react"

import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible"
import { cn } from "@/lib/utils"

type ToolbarScope = {
    filterStatus: HTMLDivElement | null
    setFilterStatus: React.Dispatch<React.SetStateAction<HTMLDivElement | null>>
    inlineFilterStatus: HTMLDivElement | null
    setInlineFilterStatus: React.Dispatch<
        React.SetStateAction<HTMLDivElement | null>
    >
    appliedChips: HTMLDivElement | null
    setAppliedChips: React.Dispatch<React.SetStateAction<HTMLDivElement | null>>
    filterTray: HTMLDivElement | null
    setFilterTray: React.Dispatch<React.SetStateAction<HTMLDivElement | null>>
    filters: ReadonlySet<object>
    registerFilter: (filter: object) => () => void
    summary: HTMLDivElement | null
    settings: HTMLDivElement | null
    setSummary: React.Dispatch<React.SetStateAction<HTMLDivElement | null>>
    setSettings: React.Dispatch<React.SetStateAction<HTMLDivElement | null>>
    tables: ReadonlySet<object>
    register: (table: object) => () => void
}

const ToolbarContext = React.createContext<ToolbarScope | null>(null)

/** 每个列表框架独立管理挂载点；嵌套框架和多表不得共用列设置。 */
export function TableToolbarScope({ children }: { children: React.ReactNode }) {
    const [filterStatus, setFilterStatus] =
        React.useState<HTMLDivElement | null>(null)
    const [inlineFilterStatus, setInlineFilterStatus] =
        React.useState<HTMLDivElement | null>(null)
    const [appliedChips, setAppliedChips] =
        React.useState<HTMLDivElement | null>(null)
    const [filterTray, setFilterTray] = React.useState<HTMLDivElement | null>(
        null,
    )
    const [filters, setFilters] = React.useState<ReadonlySet<object>>(
        () => new Set(),
    )
    const registerFilter = React.useCallback((filter: object) => {
        setFilters((current) => new Set(current).add(filter))
        return () =>
            setFilters((current) => {
                const next = new Set(current)
                next.delete(filter)
                return next
            })
    }, [])
    const [summary, setSummary] = React.useState<HTMLDivElement | null>(null)
    const [settings, setSettings] = React.useState<HTMLDivElement | null>(null)
    const [tables, setTables] = React.useState<ReadonlySet<object>>(
        () => new Set(),
    )
    const register = React.useCallback((table: object) => {
        setTables((current) => new Set(current).add(table))
        return () =>
            setTables((current) => {
                const next = new Set(current)
                next.delete(table)
                return next
            })
    }, [])
    const value = React.useMemo(
        () => ({
            filterStatus,
            setFilterStatus,
            inlineFilterStatus,
            setInlineFilterStatus,
            appliedChips,
            setAppliedChips,
            filterTray,
            setFilterTray,
            filters,
            registerFilter,
            summary,
            settings,
            setSummary,
            setSettings,
            tables,
            register,
        }),
        [
            filterStatus,
            inlineFilterStatus,
            appliedChips,
            filterTray,
            filters,
            registerFilter,
            summary,
            settings,
            tables,
            register,
        ],
    )
    return <ToolbarContext value={value}>{children}</ToolbarContext>
}

/** 将查询结果和已生效条件放入所属列表的表格工具栏；独立筛选栏保留原位。 */
export function useTableFilterStatusHost() {
    const scope = React.useContext(ToolbarContext)
    const identity = React.useRef({})
    const register = scope?.registerFilter
    React.useLayoutEffect(() => register?.(identity.current), [register])
    return scope?.filters.size === 1 ? scope.filterStatus : null
}

/** 筛选栏空闲时，将「共 N 条」轻量挂载到筛选行右侧（与列设置同行），避免额外空白工具条。 */
export function useTableInlineFilterStatusHost() {
    const scope = React.useContext(ToolbarContext)
    return scope?.filters.size === 1 ? scope.inlineFilterStatus : null
}

/** 通栏已选条件条挂载点（紧贴主筛选栏正下方，跨全宽展示，避免挤压右侧工具区）。 */
export function useTableAppliedChipsHost() {
    const scope = React.useContext(ToolbarContext)
    return scope?.filters.size === 1 ? scope.appliedChips : null
}

/** 通栏高级筛选展开区挂载点（位于主筛选栏正下方、与表头同宽）。 */
export function useTableFilterTrayHost() {
    const scope = React.useContext(ToolbarContext)
    return scope?.filters.size === 1 ? scope.filterTray : null
}

/** 列表工作面内的已选条件薄底带，无已选条件时自动隐藏不占空间。 */
export function TableToolbarAppliedChipsHost() {
    const scope = React.useContext(ToolbarContext)
    return (
        <div
            data-slot="list-work-surface-applied-chips"
            ref={scope?.setAppliedChips}
            className="shrink-0 border-b border-border/60 bg-card px-4 pb-2.5 empty:hidden"
        />
    )
}

/** 列表工作面内的高级筛选通栏展开槽，空时自动隐藏不占空间。 */
export function TableToolbarFilterTrayHost() {
    const scope = React.useContext(ToolbarContext)
    return (
        <div
            data-slot="list-work-surface-filter-tray"
            ref={scope?.setFilterTray}
            className="shrink-0 empty:hidden"
        />
    )
}

export function useTableToolbarHost(enabled: boolean) {
    const scope = React.useContext(ToolbarContext)
    const identity = React.useRef({})
    const register = scope?.register
    React.useLayoutEffect(() => register?.(identity.current), [register])
    const ownsToolbar =
        enabled &&
        scope?.tables.size === 1 &&
        scope.tables.has(identity.current)
    return {
        summary: ownsToolbar ? scope.summary : null,
        settings: ownsToolbar ? scope.settings : null,
        hasFilterStatus: Boolean(
            ownsToolbar &&
            (scope.filterStatus || scope.inlineFilterStatus) &&
            scope.filters.size === 1,
        ),
    }
}

/** 与筛选栏并排的右侧工具区：空闲计数、视图动作、列设置合并为一行。 */
export function TableToolbarInlineControls({
    actions,
    className,
}: {
    actions?: React.ReactNode
    className?: string
}) {
    const scope = React.useContext(ToolbarContext)
    return (
        <div
            data-slot="table-toolbar-inline-controls"
            className={cn(
                "ml-auto flex min-h-control shrink-0 flex-wrap items-center justify-end gap-2.5 [&:not(:has([data-table-toolbar-content]:not(:empty)))]:hidden",
                "[&>[data-table-toolbar-content]:not(:empty)+[data-table-toolbar-content]:not(:empty)]:border-l [&>[data-table-toolbar-content]:not(:empty)+[data-table-toolbar-content]:not(:empty)]:border-border/75 [&>[data-table-toolbar-content]:not(:empty)+[data-table-toolbar-content]:not(:empty)]:pl-2.5",
                "[&_[data-slot=button]]:h-control-sm [&_[data-slot=button]]:text-body-compact [&_[data-slot=button]]:shadow-none",
                className,
            )}
        >
            <div
                data-slot="table-toolbar-inline-status"
                ref={scope?.setInlineFilterStatus}
                data-table-toolbar-content=""
                className="flex items-center text-xs text-muted-foreground empty:hidden"
            />
            {actions != null ? (
                <div
                    data-slot="table-toolbar-actions"
                    data-table-toolbar-content=""
                    className="flex flex-wrap items-center gap-2 empty:hidden"
                >
                    {actions}
                </div>
            ) : null}
            <div
                data-slot="table-toolbar-column-settings"
                ref={scope?.setSettings}
                data-table-toolbar-content=""
                className="shrink-0 empty:hidden"
            />
        </div>
    )
}

/** 表格上方的统一工具栏：左侧数量或选择操作，右侧视图操作和列设置。 */
export function TableToolbar({
    children,
    childrenOpen,
    actions,
    columnSettings,
    className,
    isFrameToolbar = false,
    hideControls = false,
    animateCollapse = false,
}: {
    children?: React.ReactNode
    /** 控制左侧 children（如批量操作栏）是否展开；省略时按 children 是否非空判定。 */
    childrenOpen?: boolean
    actions?: React.ReactNode
    columnSettings?: React.ReactNode
    className?: string
    isFrameToolbar?: boolean
    /** 右侧视图操作与列设置已合并至筛选栏时隐藏，仅在有批量选择或条件提示时展开本条。 */
    hideControls?: boolean
    /** 开启后在空/非空切换时播放高度与透明度过渡动画。 */
    animateCollapse?: boolean
}) {
    const scope = React.useContext(ToolbarContext)
    const toolbarRef = React.useRef<HTMLDivElement | null>(null)
    const [hasPortalContent, setHasPortalContent] = React.useState(false)

    const hasDirectChildren = children != null && children !== false
    const isChildrenVisible = hasDirectChildren && (childrenOpen ?? true)
    const hasDirectControls =
        !hideControls &&
        ((actions != null && actions !== false) ||
            (columnSettings != null && columnSettings !== false))

    const open = isChildrenVisible || hasDirectControls || hasPortalContent
    // 当整条工具栏处于收起退场动画时保留 children 以避免退场瞬间塌成空白条；
    // 若工具栏因其他槽位（如筛选状态）仍保持展开，则不渲染已关闭的 children。
    const renderedChildren = animateCollapse
        ? isChildrenVisible || (!open && hasDirectChildren)
            ? children
            : null
        : isChildrenVisible
          ? children
          : null

    React.useLayoutEffect(() => {
        if (!animateCollapse) return
        const root = toolbarRef.current
        if (!root) return
        const checkPortalContent = () => {
            const slots = root.querySelectorAll<HTMLElement>(
                '[data-table-toolbar-portal="true"]',
            )
            let next = false
            for (const slot of slots) {
                if (slot.childNodes.length > 0) {
                    next = true
                    break
                }
            }
            setHasPortalContent(next)
        }
        checkPortalContent()
        const observer = new MutationObserver(checkPortalContent)
        observer.observe(root, { childList: true, subtree: true })
        return () => observer.disconnect()
    }, [animateCollapse])

    const toolbarContent = (
        <div
            ref={toolbarRef}
            data-slot="table-toolbar"
            className={cn(
                "flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 py-table-toolbar-block text-body-compact",
                !animateCollapse &&
                    "[&:not(:has([data-table-toolbar-content]:not(:empty)))]:hidden",
                animateCollapse &&
                    "transition-transform ease-out group-data-open/toolbar:duration-200 group-data-closed/toolbar:pointer-events-none group-data-closed/toolbar:duration-150 group-data-closed/toolbar:ease-in group-data-[starting-style]/toolbar:-translate-y-1 group-data-[ending-style]/toolbar:-translate-y-1 motion-reduce:transition-none",
                "[&_[data-slot=button]]:h-control-sm [&_[data-slot=button]]:text-body-compact [&_[data-slot=button]]:shadow-none",
                className,
            )}
        >
            {isFrameToolbar ? (
                <div
                    data-slot="table-toolbar-filter-status"
                    ref={scope?.setFilterStatus}
                    data-table-toolbar-content=""
                    data-table-toolbar-portal="true"
                    className={cn(
                        "min-w-0 basis-full empty:hidden sm:basis-auto",
                        !hasDirectChildren && "flex-1",
                    )}
                />
            ) : null}
            <div
                data-slot="table-toolbar-summary"
                ref={
                    isFrameToolbar && !hasDirectChildren
                        ? scope?.setSummary
                        : undefined
                }
                data-table-toolbar-content=""
                data-table-toolbar-portal={
                    isFrameToolbar && !hasDirectChildren ? "true" : undefined
                }
                data-summary-provided={hasDirectChildren ? "true" : undefined}
                className="min-w-0 empty:hidden"
            >
                {renderedChildren}
            </div>
            {!hideControls ? (
                <div className="ml-auto flex max-w-full flex-wrap items-center justify-end gap-2">
                    <div
                        data-slot="table-toolbar-actions"
                        data-table-toolbar-content=""
                        className="flex flex-wrap items-center gap-2 empty:hidden"
                    >
                        {actions}
                    </div>
                    <div
                        data-slot="table-toolbar-column-settings"
                        ref={isFrameToolbar ? scope?.setSettings : undefined}
                        data-table-toolbar-content=""
                        data-table-toolbar-portal={
                            isFrameToolbar ? "true" : undefined
                        }
                        className="shrink-0 empty:hidden"
                    >
                        {columnSettings}
                    </div>
                </div>
            ) : null}
        </div>
    )

    if (!animateCollapse) return toolbarContent

    return (
        <Collapsible
            open={open}
            className="shrink-0 has-[>[data-slot=table-toolbar-panel][hidden]]:hidden"
        >
            <CollapsibleContent
                keepMounted
                data-slot="table-toolbar-panel"
                className={cn(
                    "group/toolbar h-(--collapsible-panel-height) overflow-hidden transition-[height,opacity] ease-out",
                    "data-open:duration-200 data-closed:duration-150 data-closed:ease-in",
                    "data-starting-style:h-0 data-starting-style:opacity-0",
                    "data-ending-style:h-0 data-ending-style:opacity-0",
                    "motion-reduce:transition-none",
                )}
            >
                {toolbarContent}
            </CollapsibleContent>
        </Collapsible>
    )
}
