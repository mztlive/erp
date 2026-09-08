"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import { FilterChip } from "@/components/business/filter-chip"
import { ListToolbar } from "@/components/business/list"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export const listWorkspaceFilterBarClassName =
    "gap-4 [&_[data-slot=list-toolbar-filters]]:self-start [&_[data-slot=list-toolbar-secondary]]:w-full [&_[data-slot=list-toolbar-secondary]]:flex-col [&_[data-slot=list-toolbar-secondary]]:items-stretch [&_[data-slot=list-toolbar-secondary]]:gap-0 max-sm:[&_[data-slot=list-toolbar-primary]]:sticky max-sm:[&_[data-slot=list-toolbar-primary]]:top-0 max-sm:[&_[data-slot=list-toolbar-primary]]:z-10 max-sm:[&_[data-slot=list-toolbar-primary]]:bg-card max-sm:[&_[data-slot=list-toolbar-primary]]:pb-2 [&_[data-slot=list-toolbar-query-tools]]:sm:flex-wrap [&_[data-slot=list-toolbar-search]]:lg:w-96"

export type ListWorkspaceFilterChip = {
    key: string
    label: string
    clearLabel?: string
    onClear?: () => void
}

export function ListWorkspaceFilterField({
    htmlFor,
    label,
    children,
    className,
}: {
    htmlFor?: string
    label: string
    children: React.ReactNode
    className?: string
}) {
    return (
        <div className={cn("min-w-0 space-y-1.5", className)}>
            {htmlFor ? (
                <label
                    htmlFor={htmlFor}
                    className="text-xs text-muted-foreground"
                >
                    {label}
                </label>
            ) : (
                <div className="text-xs text-muted-foreground">{label}</div>
            )}
            {children}
        </div>
    )
}

export function ListWorkspaceInlineFilter({
    htmlFor,
    label,
    children,
    className,
}: {
    htmlFor?: string
    label: string
    children: React.ReactNode
    className?: string
}) {
    return (
        <div
            className={cn(
                "flex min-w-0 items-center gap-3 lg:not-first:border-l lg:not-first:pl-6",
                className,
            )}
        >
            <label
                htmlFor={htmlFor}
                className="shrink-0 text-sm text-muted-foreground"
            >
                {label}
            </label>
            {children}
        </div>
    )
}

export function listWorkspaceFilterStatusText({
    loading,
    failed,
    resultCount,
    noun,
    loadingLabel,
}: {
    loading?: boolean
    failed?: boolean
    resultCount?: number
    noun: string
    loadingLabel?: string
}): string {
    if (loading) return "查询中…"
    if (failed) return "查询未完成"
    if (resultCount === undefined) return loadingLabel ?? `正在加载${noun}…`
    return `共 ${resultCount} ${noun}`
}

export function ListWorkspaceFilterBar({
    density = "default",
    idPrefix,
    formAriaLabel,
    onSubmit,
    search,
    moreCount = 0,
    moreOpen = false,
    onToggleMore,
    morePanelId,
    morePanel,
    morePanelAriaLabel = "更多筛选条件",
    commonFilters,
    primaryFilters,
    resultStatus,
    chips = [],
    onClearChip,
    onClearAll,
    hasPendingChanges = false,
    pendingHint = "条件已修改，待查询",
    idleHint,
    extraPrimary,
    onResetMore,
    moreHint = "可组合多个条件，点击「查询」后统一生效。",
    actions,
    className,
    queryButtonId,
    moreButtonId,
    resetMoreButtonId,
    clearButtonId,
}: {
    density?: "default" | "compact"
    idPrefix: string
    formAriaLabel: string
    onSubmit: () => void
    search: React.ReactNode
    moreCount?: number
    moreOpen?: boolean
    onToggleMore?: () => void
    morePanelId?: string
    morePanel?: React.ReactNode
    morePanelAriaLabel?: string
    commonFilters?: React.ReactNode
    /** 常用条件置于搜索之后、查询按钮之前；不影响默认的次行布局。 */
    primaryFilters?: React.ReactNode
    resultStatus: React.ReactNode
    chips?: readonly ListWorkspaceFilterChip[]
    onClearChip?: (key: string) => void
    onClearAll?: () => void
    hasPendingChanges?: boolean
    pendingHint?: string
    idleHint?: string
    extraPrimary?: React.ReactNode
    onResetMore?: () => void
    moreHint?: string
    actions?: React.ReactNode
    className?: string
    queryButtonId?: string
    moreButtonId?: string
    resetMoreButtonId?: string
    clearButtonId?: string
}) {
    const panelId = morePanelId ?? `${idPrefix}-more-panel`
    const showMore = morePanel != null && onToggleMore != null

    return (
        <form
            aria-label={formAriaLabel}
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                onSubmit()
            }}
        >
            <ListToolbar
                className={cn(
                    listWorkspaceFilterBarClassName,
                    density === "compact" && "gap-2",
                    className,
                )}
                search={search}
                filters={
                    <>
                        {primaryFilters}
                        <Button
                            id={queryButtonId ?? `${idPrefix}-query`}
                            type="submit"
                        >
                            <SearchIcon aria-hidden="true" />
                            查询
                        </Button>
                        {extraPrimary}
                        {showMore ? (
                            <Button
                                id={moreButtonId ?? `${idPrefix}-more`}
                                type="button"
                                variant="ghost"
                                aria-expanded={moreOpen}
                                aria-controls={panelId}
                                onClick={onToggleMore}
                            >
                                <FilterIcon aria-hidden="true" />
                                更多筛选
                                {moreCount > 0 ? (
                                    <span
                                        className="rounded bg-muted px-1.5 text-xs tabular-nums"
                                        aria-label={`${moreCount} 项已生效`}
                                    >
                                        {moreCount}
                                    </span>
                                ) : null}
                                <ChevronDownIcon
                                    aria-hidden="true"
                                    className={cn(
                                        "transition-transform",
                                        moreOpen && "rotate-180",
                                    )}
                                />
                            </Button>
                        ) : null}
                    </>
                }
                actions={actions}
                secondary={
                    <div
                        className={cn(
                            "w-full min-w-0",
                            density === "compact" ? "space-y-2" : "space-y-4",
                        )}
                    >
                        {commonFilters ? (
                            <div className="flex min-w-0 flex-col gap-3 lg:flex-row lg:items-center lg:gap-6">
                                {commonFilters}
                            </div>
                        ) : null}
                        {showMore && moreOpen ? (
                            <section
                                id={panelId}
                                aria-label={morePanelAriaLabel}
                                className="rounded-xl border border-border/70 bg-muted/25 p-4"
                            >
                                {morePanel}
                                {onResetMore ? (
                                    <div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3">
                                        <span className="text-xs text-muted-foreground">
                                            {moreHint}
                                        </span>
                                        <Button
                                            id={
                                                resetMoreButtonId ??
                                                `${idPrefix}-reset-more`
                                            }
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            onClick={onResetMore}
                                        >
                                            重置更多条件
                                        </Button>
                                    </div>
                                ) : null}
                            </section>
                        ) : null}
                        <div
                            className={cn(
                                "flex flex-wrap items-center gap-x-3 gap-y-2 text-xs",
                                density === "compact"
                                    ? "pt-1"
                                    : "border-t border-border/60 pt-3",
                            )}
                        >
                            <span
                                role="status"
                                className="shrink-0 text-muted-foreground"
                            >
                                {resultStatus}
                            </span>
                            {chips.length ? (
                                <>
                                    <span className="text-muted-foreground">
                                        已生效
                                    </span>
                                    {chips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            id={`${idPrefix}-chip-${toAutomationIdSegment(chip.key)}`}
                                            label={chip.label}
                                            clearLabel={
                                                chip.clearLabel ??
                                                `移除${chip.label}`
                                            }
                                            onClear={() => {
                                                if (chip.onClear) chip.onClear()
                                                else onClearChip?.(chip.key)
                                            }}
                                        />
                                    ))}
                                    {onClearAll ? (
                                        <Button
                                            id={
                                                clearButtonId ??
                                                `${idPrefix}-clear`
                                            }
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            onClick={onClearAll}
                                        >
                                            清除全部
                                        </Button>
                                    ) : null}
                                </>
                            ) : null}
                            {idleHint || hasPendingChanges ? (
                                <span
                                    role="status"
                                    className={cn(
                                        "sm:ml-auto",
                                        hasPendingChanges
                                            ? "font-medium text-warning-soft-foreground"
                                            : "text-muted-foreground",
                                    )}
                                >
                                    {hasPendingChanges ? pendingHint : idleHint}
                                </span>
                            ) : null}
                        </div>
                    </div>
                }
            />
        </form>
    )
}
