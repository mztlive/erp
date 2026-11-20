"use client"

import * as React from "react"
import { createPortal } from "react-dom"

import { FilterChip } from "@/components/business/filter-chip"
import {
    ListFilterQueryButton,
    ListFilterTray,
    ListFilterTrigger,
} from "@/components/business/list-filter-popover"
import { ListToolbar } from "@/components/business/list"
import {
    useTableFilterStatusHost,
    useTableFilterTrayHost,
    useTableInlineFilterStatusHost,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { listFilterText } from "@/lib/ui-text"

const listWorkspaceFilterBarClassName =
    "gap-3 [&_[data-slot=list-toolbar-filters]]:self-start [&_[data-slot=list-toolbar-secondary]]:w-full [&_[data-slot=list-toolbar-secondary]]:flex-col [&_[data-slot=list-toolbar-secondary]]:items-stretch [&_[data-slot=list-toolbar-secondary]]:gap-0 max-sm:[&_[data-slot=list-toolbar-primary]]:sticky max-sm:[&_[data-slot=list-toolbar-primary]]:top-0 max-sm:[&_[data-slot=list-toolbar-primary]]:z-10 max-sm:[&_[data-slot=list-toolbar-primary]]:bg-card max-sm:[&_[data-slot=list-toolbar-primary]]:pb-2 [&_[data-slot=list-toolbar-query-tools]]:sm:flex-wrap [&_[data-slot=list-toolbar-search]]:sm:w-64 [&_[data-slot=list-toolbar-search]]:lg:w-72 [&_[data-slot=list-toolbar-search]]:xl:w-80"

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
        <div
            data-slot="list-filter-field"
            className={cn("min-w-0 space-y-1.5", className)}
        >
            {htmlFor ? (
                <label
                    data-slot="list-filter-field-label"
                    htmlFor={htmlFor}
                    className="block text-xs text-muted-foreground"
                >
                    {label}
                </label>
            ) : (
                <div
                    data-slot="list-filter-field-label"
                    className="text-xs text-muted-foreground"
                >
                    {label}
                </div>
            )}
            <div data-slot="list-filter-field-control" className="min-w-0">
                {children}
            </div>
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
    morePresentation = "inline",
    moreSize: _moreSize = "wide",
    commonFilters,
    primaryFilters,
    resultStatus,
    chips = [],
    onClearChip,
    onClearAll,
    hasPendingChanges = false,
    pendingHint = "条件已修改，待查询",
    idleHint,
    statusActions,
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
    /** 浮动筛选按页面启用，保留其余列表的既有展开形式。 */
    morePresentation?: "inline" | "popover"
    moreSize?: "compact" | "wide" | "medium"
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
    /** 跟在结果说明右侧，例如展示方式切换、导出。 */
    statusActions?: React.ReactNode
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
    // 展开「更多筛选」时顶栏主按钮退场，由展开区页脚「查询」作为唯一提交入口。
    const hideTopQuery = showMore && moreOpen
    const statusHost = useTableFilterStatusHost()
    const inlineStatusHost = useTableInlineFilterStatusHost()
    const filterTrayHost = useTableFilterTrayHost()
    const hasInlineChips = morePresentation !== "popover" && chips.length > 0
    const compactInlineStatus = Boolean(
        inlineStatusHost &&
        resultStatus != null &&
        !hasInlineChips &&
        !statusActions,
    )
    const appliedChips = chips.length ? (
        <>
            <span className="shrink-0 text-muted-foreground/80">
                {morePresentation === "popover"
                    ? listFilterText.selected
                    : "已生效"}
            </span>
            {chips.map((chip) => (
                <FilterChip
                    key={chip.key}
                    id={`${idPrefix}-chip-${toAutomationIdSegment(chip.key)}`}
                    label={chip.label}
                    clearLabel={chip.clearLabel ?? `移除${chip.label}`}
                    onClear={() => {
                        if (chip.onClear) chip.onClear()
                        else onClearChip?.(chip.key)
                    }}
                />
            ))}
            {onClearAll ? (
                <Button
                    id={clearButtonId ?? `${idPrefix}-clear`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="h-6 px-1.5 text-xs font-normal text-muted-foreground hover:bg-transparent hover:text-primary"
                    onClick={onClearAll}
                >
                    {morePresentation === "popover"
                        ? listFilterText.clear
                        : "清除全部"}
                </Button>
            ) : null}
        </>
    ) : null
    const statusRow =
        resultStatus ||
        (morePresentation !== "popover" && appliedChips) ||
        idleHint ||
        hasPendingChanges ||
        statusActions ? (
            <div
                className={cn(
                    "flex flex-wrap items-center gap-x-3 gap-y-2 text-xs",
                    statusHost
                        ? undefined
                        : density === "compact"
                          ? "pt-1"
                          : "border-t border-border/60 pt-3",
                )}
            >
                {resultStatus != null ? (
                    <span
                        role="status"
                        className="shrink-0 text-muted-foreground"
                    >
                        {resultStatus}
                    </span>
                ) : null}
                {morePresentation !== "popover" ? appliedChips : null}
                {idleHint || hasPendingChanges || statusActions ? (
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 sm:ml-auto">
                        {idleHint || hasPendingChanges ? (
                            <span
                                role="status"
                                className={
                                    hasPendingChanges
                                        ? "rounded-sm bg-warning-soft px-1.5 py-0.5 font-medium text-warning-soft-foreground"
                                        : "hidden text-muted-foreground/75 xl:inline"
                                }
                            >
                                {hasPendingChanges ? pendingHint : idleHint}
                            </span>
                        ) : null}
                        {statusActions}
                    </div>
                ) : null}
            </div>
        ) : null

    const queryButton = (
        <ListFilterQueryButton
            id={queryButtonId ?? `${idPrefix}-query`}
            hasPendingChanges={hasPendingChanges}
        />
    )

    return (
        <form
            aria-label={formAriaLabel}
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                onSubmit()
            }}
        >
            {hideTopQuery && filterTrayHost ? (
                <input
                    type="submit"
                    value=""
                    tabIndex={-1}
                    aria-hidden="true"
                    className="sr-only"
                />
            ) : null}
            <ListToolbar
                className={cn(
                    listWorkspaceFilterBarClassName,
                    density === "compact" && "gap-2",
                    className,
                )}
                search={search}
                filters={
                    showMore ? (
                        <>
                            {primaryFilters}
                            {extraPrimary}
                            <ListFilterTrigger
                                triggerId={moreButtonId ?? `${idPrefix}-more`}
                                panelId={panelId}
                                open={moreOpen}
                                count={moreCount}
                                onToggle={onToggleMore}
                            />
                            {!hideTopQuery ? queryButton : null}
                        </>
                    ) : (
                        <>
                            {primaryFilters}
                            {queryButton}
                            {extraPrimary}
                        </>
                    )
                }
                actions={actions}
                secondary={
                    commonFilters ||
                    (morePresentation === "popover" && chips.length > 0) ||
                    (showMore && !filterTrayHost) ||
                    !statusHost ? (
                        <div
                            className={cn(
                                "w-full min-w-0",
                                density === "compact"
                                    ? "space-y-2"
                                    : "space-y-3",
                            )}
                        >
                            {commonFilters ? (
                                <div className="flex min-w-0 flex-col gap-3 lg:flex-row lg:items-center lg:gap-6">
                                    {commonFilters}
                                </div>
                            ) : null}
                            {morePresentation === "popover" && appliedChips ? (
                                <div
                                    data-slot="list-filter-applied"
                                    className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1.5 pt-0.5 text-xs"
                                >
                                    {appliedChips}
                                </div>
                            ) : null}
                            {showMore && !filterTrayHost ? (
                                <ListFilterTray
                                    idPrefix={idPrefix}
                                    panelId={panelId}
                                    label={morePanelAriaLabel}
                                    open={moreOpen}
                                    onOpenChange={(nextOpen) => {
                                        if (nextOpen !== moreOpen)
                                            onToggleMore?.()
                                    }}
                                    onApply={onSubmit}
                                    onReset={onResetMore}
                                    resetId={resetMoreButtonId}
                                    hasPendingChanges={hasPendingChanges}
                                    hint={moreHint}
                                    inPortal={false}
                                >
                                    {morePanel}
                                </ListFilterTray>
                            ) : null}
                            {!statusHost ? statusRow : null}
                        </div>
                    ) : undefined
                }
            />
            {showMore && filterTrayHost
                ? createPortal(
                      <ListFilterTray
                          idPrefix={idPrefix}
                          panelId={panelId}
                          label={morePanelAriaLabel}
                          open={moreOpen}
                          onOpenChange={(nextOpen) => {
                              if (nextOpen !== moreOpen) onToggleMore?.()
                          }}
                          onApply={onSubmit}
                          onReset={onResetMore}
                          resetId={resetMoreButtonId}
                          hasPendingChanges={hasPendingChanges}
                          hint={moreHint}
                          inPortal
                      >
                          {morePanel}
                      </ListFilterTray>,
                      filterTrayHost,
                  )
                : null}
            {compactInlineStatus && inlineStatusHost
                ? createPortal(
                      <div className="flex items-center gap-2">
                          {hasPendingChanges ? (
                              <span
                                  role="status"
                                  title={pendingHint}
                                  className="inline-flex items-center gap-1 rounded-full bg-warning-soft px-2 py-0.5 text-tiny leading-4 font-medium whitespace-nowrap text-warning-soft-foreground"
                              >
                                  <span
                                      aria-hidden="true"
                                      className="size-1.5 rounded-full bg-warning"
                                  />
                                  待查询
                              </span>
                          ) : null}
                          <span
                              role="status"
                              className="num shrink-0 whitespace-nowrap text-xs text-muted-foreground"
                          >
                              {resultStatus}
                          </span>
                      </div>,
                      inlineStatusHost,
                  )
                : statusHost
                  ? createPortal(statusRow, statusHost)
                  : null}
        </form>
    )
}
