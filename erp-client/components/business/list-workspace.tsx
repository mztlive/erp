"use client"

import * as React from "react"

import { cn } from "@/lib/utils"

/** Shared Tailwind classes for business list workspaces. */
export const listWorkspaceStyles = {
    page: "max-w-none gap-0 bg-card px-4 py-5 md:px-6 md:pt-7 md:pb-6 min-[75rem]:px-8",
    header: "flex flex-col items-start justify-between gap-4 pb-5 md:flex-row md:items-center md:gap-6 md:pb-[30px]",
    eyebrow: "mb-2.5 text-xs leading-[18px] text-muted-foreground",
    title: "text-[26px] leading-9 font-semibold tracking-[-0.6px]",
    description: "mt-1.5 text-[13px] leading-[22px] text-muted-foreground",
    headerActions:
        "flex shrink-0 items-center gap-2 max-md:w-full max-md:justify-end [&_[data-slot=button]]:h-9 [&_[data-slot=button]]:text-[13px] [&_[data-slot=button]]:shadow-none",
    quietButton: "text-[13px] text-muted-foreground shadow-none",
    exportButton: "text-[13px] shadow-none",
    workSurface: "flex min-w-0 flex-1 flex-col",
    viewBar:
        "flex shrink-0 items-center justify-between gap-3 border-b border-border",
    views: "flex min-w-0 gap-5 overflow-x-auto md:gap-[26px]",
    view: "relative inline-flex min-h-[46px] shrink-0 cursor-pointer items-center gap-[9px] border-b-2 border-transparent text-[13px] font-medium text-muted-foreground transition-[color] duration-[120ms] ease-linear hover:text-foreground focus-visible:rounded-[3px] focus-visible:outline-2 focus-visible:-outline-offset-3 focus-visible:outline-foreground",
    activeView: "border-foreground font-semibold text-foreground",
    viewCount:
        "min-w-[22px] rounded-[5px] bg-muted px-[5px] py-0 text-center text-xs leading-5 font-medium tabular-nums",
    viewHint:
        "text-xs whitespace-nowrap text-muted-foreground max-[1200px]:hidden",
    toolbar:
        "flex shrink-0 items-start gap-0 py-[18px] md:gap-3 [&_[data-slot=button]]:text-[13px] [&_[data-slot=button]]:shadow-none [&_[data-slot=input-group]]:bg-background [&_[data-slot=input-group]]:text-[13px] [&_[data-slot=input-group]]:shadow-none [&_[data-slot=input-group]:focus-within]:outline-2 [&_[data-slot=input-group]:focus-within]:outline-offset-2 [&_[data-slot=input-group]:focus-within]:outline-foreground",
    filters: "min-w-0 flex-1",
    columnSettings: "shrink-0 empty:hidden",
    table: [
        "min-h-0 flex-1",
        "[&_[data-slot=data-table]]:h-full [&_[data-slot=data-table]]:gap-0",
        "[&_[data-slot=data-table-surface]]:flex-1 [&_[data-slot=data-table-surface]]:rounded-none [&_[data-slot=data-table-surface]]:border-0",
        "[&_[data-slot=table-head]]:h-10 [&_[data-slot=table-head]]:py-0 [&_[data-slot=table-head]]:text-xs [&_[data-slot=table-head]]:font-medium [&_[data-slot=table-head]]:tracking-normal",
        "[&_[data-slot=table-cell]]:h-16 [&_[data-slot=table-cell]]:py-2.5",
        "[&_[data-slot=table-row]:focus-visible]:outline-2 [&_[data-slot=table-row]:focus-visible]:-outline-offset-2 [&_[data-slot=table-row]:focus-visible]:outline-foreground",
        "[&_[data-slot=data-table-pagination]]:mt-auto [&_[data-slot=data-table-pagination]]:shrink-0 [&_[data-slot=data-table-pagination]]:border-t [&_[data-slot=data-table-pagination]]:border-border [&_[data-slot=data-table-pagination]]:px-0 [&_[data-slot=data-table-pagination]]:pt-4 [&_[data-slot=data-table-pagination]]:pb-0 [&_[data-slot=data-table-pagination]]:text-xs",
    ].join(" "),
} as const

export const listWorkspaceEmptyStateClassName =
    "rounded-none border-0 bg-transparent px-6 py-16 shadow-none ring-0"

const styles = listWorkspaceStyles

export function ListWorkspaceHeader({
    eyebrow,
    title,
    description,
    children,
}: {
    eyebrow: string
    title: string
    description?: React.ReactNode
    children?: React.ReactNode
}) {
    return (
        <header className={styles.header}>
            <div>
                <p className={styles.eyebrow}>{eyebrow}</p>
                <h1 className={styles.title}>{title}</h1>
                {description ? (
                    <p className={styles.description}>{description}</p>
                ) : null}
            </div>
            {children ? (
                <div className={styles.headerActions}>{children}</div>
            ) : null}
        </header>
    )
}

export type ListWorkspaceViewItem = {
    id: string
    label: string
    count?: React.ReactNode
    active: boolean
    onClick: () => void
}

export function ListWorkspaceViews({
    ariaLabel,
    items,
    hint,
}: {
    ariaLabel: string
    items: readonly ListWorkspaceViewItem[]
    hint?: string
}) {
    if (items.length === 0) return null
    return (
        <div className={styles.viewBar}>
            <div className={styles.views} role="group" aria-label={ariaLabel}>
                {items.map((item) => (
                    <button
                        key={item.id}
                        id={item.id}
                        type="button"
                        aria-pressed={item.active}
                        className={cn(
                            styles.view,
                            item.active && styles.activeView,
                        )}
                        onClick={item.onClick}
                    >
                        {item.label}
                        {item.count != null ? (
                            <span className={styles.viewCount}>
                                {item.count}
                            </span>
                        ) : null}
                    </button>
                ))}
            </div>
            {hint ? <span className={styles.viewHint}>{hint}</span> : null}
        </div>
    )
}

export function ListWorkSurface({
    ariaLabel,
    views,
    toolbar,
    table,
    tableClassName,
    selectionBar,
}: {
    ariaLabel: string
    views?: React.ReactNode
    toolbar?: React.ReactNode
    table: React.ReactNode
    tableClassName?: string
    selectionBar?: React.ReactNode
}) {
    return (
        <section
            className={styles.workSurface}
            data-business-component="table-frame"
            aria-label={ariaLabel}
        >
            {views}
            <div className={styles.toolbar} data-slot="list-workspace-toolbar">
                <div className={styles.filters}>{toolbar}</div>
                <div
                    className={styles.columnSettings}
                    data-slot="table-frame-view-options"
                />
            </div>
            {selectionBar}
            <div
                className={cn(styles.table, tableClassName)}
                data-slot="business-table-frame-table"
            >
                {table}
            </div>
        </section>
    )
}
