"use client"

import * as React from "react"

import { cn } from "@/lib/utils"

type ToolbarScope = {
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
            summary,
            settings,
            setSummary,
            setSettings,
            tables,
            register,
        }),
        [summary, settings, tables, register],
    )
    return <ToolbarContext value={value}>{children}</ToolbarContext>
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
    }
}

/** 表格上方的统一工具栏：左侧数量或选择操作，右侧视图操作和列设置。 */
export function TableToolbar({
    children,
    actions,
    columnSettings,
    className,
    isFrameToolbar = false,
}: {
    children?: React.ReactNode
    actions?: React.ReactNode
    columnSettings?: React.ReactNode
    className?: string
    isFrameToolbar?: boolean
}) {
    const scope = React.useContext(ToolbarContext)
    return (
        <div
            data-slot="table-toolbar"
            className={cn(
                "flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 py-2.5 text-[13px] [&:not(:has([data-table-toolbar-content]:not(:empty)))]:hidden",
                "[&_[data-slot=button]]:h-8 [&_[data-slot=button]]:text-[13px] [&_[data-slot=button]]:shadow-none",
                className,
            )}
        >
            <div
                data-slot="table-toolbar-summary"
                ref={
                    isFrameToolbar && children == null
                        ? scope?.setSummary
                        : undefined
                }
                data-table-toolbar-content=""
                data-summary-provided={children != null ? "true" : undefined}
                className="min-w-0 empty:hidden"
            >
                {children}
            </div>
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
                    className="shrink-0 empty:hidden"
                >
                    {columnSettings}
                </div>
            </div>
        </div>
    )
}
