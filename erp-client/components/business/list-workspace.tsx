"use client"

import * as React from "react"
import NextLink from "next/link"

import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { cn } from "@/lib/utils"

/** Shared Tailwind classes for business list workspaces. */
export const listWorkspaceStyles = {
    page: "max-w-none gap-0 bg-card px-workspace-inline py-page-block md:px-page-inline md:pt-workspace-block-start-lg md:pb-page-block-lg min-[75rem]:px-page-inline-lg",
    header: "flex flex-col items-start justify-between gap-4 pb-page-block md:flex-row md:items-center md:gap-6 md:pb-workspace-header-block-end",
    eyebrow: "mb-2.5 text-xs leading-4.5 text-muted-foreground",
    title: "text-page-title font-semibold tracking-tight",
    description: "mt-1.5 text-body-compact text-muted-foreground",
    headerActions:
        "flex shrink-0 items-center gap-2 max-md:w-full max-md:justify-end [&_[data-slot=button]]:h-control [&_[data-slot=button]]:shadow-none",
    quietButton: "text-body-compact text-muted-foreground shadow-none",
    exportButton: "text-body-compact shadow-none",
    workSurface: "flex min-w-0 flex-1 flex-col",
    viewBar:
        "flex shrink-0 items-center justify-between gap-3 border-b border-border",
    views: "flex min-w-0 gap-5 overflow-x-auto md:gap-workspace-view-gap",
    view: "relative inline-flex min-h-workspace-view shrink-0 cursor-pointer items-center gap-2.25 border-b-2 border-transparent text-body-compact font-medium text-muted-foreground transition-colors duration-150 ease-linear hover:text-foreground focus-visible:rounded-sm focus-visible:outline-2 focus-visible:-outline-offset-3 focus-visible:outline-foreground",
    activeView: "border-foreground font-semibold text-foreground",
    viewCount:
        "min-w-5.5 rounded-md bg-muted px-1.25 py-0 text-center text-xs leading-5 font-medium tabular-nums",
    viewHint:
        "text-xs whitespace-nowrap text-muted-foreground max-[1200px]:hidden",
    toolbar:
        "flex shrink-0 items-start gap-0 py-workspace-toolbar-block md:gap-3 [&_[data-slot=button]]:shadow-none [&_[data-slot=input-group]]:bg-background [&_[data-slot=input-group]]:text-body-compact [&_[data-slot=input-group]]:shadow-none [&_[data-slot=input-group]:focus-within]:outline-2 [&_[data-slot=input-group]:focus-within]:outline-offset-2 [&_[data-slot=input-group]:focus-within]:outline-foreground",
    filters: "min-w-0 flex-1",
    table: [
        "min-h-0 flex-1",
        "[&_[data-slot=data-table]]:h-full [&_[data-slot=data-table]]:gap-0",
        "[&_[data-slot=data-table-surface]]:flex-1 [&_[data-slot=data-table-surface]]:rounded-none [&_[data-slot=data-table-surface]]:border-0",
        "[&_[data-slot=table-row]:focus-visible]:outline-2 [&_[data-slot=table-row]:focus-visible]:-outline-offset-2 [&_[data-slot=table-row]:focus-visible]:outline-foreground",
        "[&_[data-slot=data-table-pagination]]:mt-auto [&_[data-slot=data-table-pagination]]:shrink-0 [&_[data-slot=data-table-pagination]]:border-t [&_[data-slot=data-table-pagination]]:border-border [&_[data-slot=data-table-pagination]]:px-0 [&_[data-slot=data-table-pagination]]:pt-4 [&_[data-slot=data-table-pagination]]:pb-0 [&_[data-slot=data-table-pagination]]:text-xs",
    ].join(" "),
} as const

export const listWorkspaceEmptyStateClassName =
    "rounded-none border-0 bg-transparent px-6 py-16 shadow-none ring-0"

const styles = listWorkspaceStyles

export function ListWorkspaceHeader({
    className,
    eyebrow,
    title,
    description,
    children,
}: {
    className?: string
    eyebrow: string
    title: string
    description?: React.ReactNode
    children?: React.ReactNode
}) {
    return (
        <header className={cn(styles.header, className)}>
            <div className="min-w-0">
                <p className={styles.eyebrow}>{eyebrow}</p>
                <h1 className={styles.title}>{title}</h1>
                {description ? (
                    <div className={styles.description}>{description}</div>
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
    href?: string
    onClick?: () => void
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
    // 单个视图不是切换，不画 tab 行（例如「全部合同」）。
    if (items.length < 2) return null
    return (
        <div className={styles.viewBar}>
            <div className={styles.views} role="group" aria-label={ariaLabel}>
                {items.map((item) => {
                    const className = cn(
                        styles.view,
                        item.active && styles.activeView,
                    )
                    const content = (
                        <>
                            {item.label}
                            {item.count != null ? (
                                <span className={styles.viewCount}>
                                    {item.count}
                                </span>
                            ) : null}
                        </>
                    )
                    return item.href ? (
                        <NextLink
                            key={item.id}
                            id={item.id}
                            href={item.href}
                            aria-current={item.active ? "page" : undefined}
                            className={className}
                            onClick={item.onClick}
                        >
                            {content}
                        </NextLink>
                    ) : (
                        <button
                            key={item.id}
                            id={item.id}
                            type="button"
                            aria-pressed={item.active}
                            className={className}
                            onClick={item.onClick}
                        >
                            {content}
                        </button>
                    )
                })}
            </div>
            {hint ? <span className={styles.viewHint}>{hint}</span> : null}
        </div>
    )
}

export { ListSearchField } from "./list-search-field"
export {
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterBarClassName,
    listWorkspaceFilterStatusText,
    type ListWorkspaceFilterChip,
} from "./list-workspace-filter-bar"

export function ListWorkSurface({
    ariaLabel,
    views,
    toolbar,
    table,
    className,
    tableClassName,
    toolbarClassName,
    selectionBar,
    tableActions,
}: {
    ariaLabel: string
    views?: React.ReactNode
    toolbar?: React.ReactNode
    table: React.ReactNode
    className?: string
    tableClassName?: string
    toolbarClassName?: string
    selectionBar?: React.ReactNode
    tableActions?: React.ReactNode
}) {
    return (
        <TableToolbarScope>
            <section
                className={cn(styles.workSurface, className)}
                data-business-component="table-frame"
                aria-label={ariaLabel}
            >
                {views}
                {toolbar ? (
                    <div
                        className={cn(styles.toolbar, toolbarClassName)}
                        data-slot="list-workspace-toolbar"
                    >
                        <div className={styles.filters}>{toolbar}</div>
                    </div>
                ) : null}
                <TableToolbar isFrameToolbar actions={tableActions}>
                    {selectionBar}
                </TableToolbar>
                <div
                    className={cn(styles.table, tableClassName)}
                    data-slot="business-table-frame-table"
                >
                    {table}
                </div>
            </section>
        </TableToolbarScope>
    )
}
