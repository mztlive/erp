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
    useTableAppliedChipsHost,
    useTableFilterStatusHost,
    useTableFilterTrayHost,
    useTableInlineFilterStatusHost,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { listFilterText } from "@/lib/ui-text"

const listWorkspaceFilterBarClassName =
    "gap-3 [&_[data-slot=list-toolbar-filters]]:self-start [&_[data-slot=list-toolbar-secondary]]:w-full [&_[data-slot=list-toolbar-secondary]]:flex-col [&_[data-slot=list-toolbar-secondary]]:items-stretch [&_[data-slot=list-toolbar-secondary]]:gap-0 max-sm:[&_[data-slot=list-toolbar-primary]]:sticky max-sm:[&_[data-slot=list-toolbar-primary]]:top-0 max-sm:[&_[data-slot=list-toolbar-primary]]:z-10 max-sm:[&_[data-slot=list-toolbar-primary]]:bg-card max-sm:[&_[data-slot=list-toolbar-primary]]:pb-2 [&_[data-slot=list-toolbar-query-tools]]:sm:flex-wrap [&_[data-slot=list-toolbar-search]]:sm:w-60 [&_[data-slot=list-toolbar-search]]:lg:w-64 [&_[data-slot=list-toolbar-search]]:xl:w-72"

const listWorkspacePopoverFiltersClassName =
    "[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"

export type ListWorkspaceFilterChip = {
    key: string
    label: string
    clearLabel?: string
    onClear?: () => void
}

/** 高级筛选托盘内的分组容器，配合 globals.css 自动生成组间细分割线与左标题布局。 */
export function ListFilterSections({
    children,
    className,
}: {
    children: React.ReactNode
    className?: string
}) {
    return <div className={cn("grid min-w-0 gap-5", className)}>{children}</div>
}

/** 高级筛选托盘内的语义分组（左标题 + 响应式 1/2/3/4 列控件网格）。 */
export function ListFilterSection({
    legend,
    children,
    className,
    gridClassName,
}: {
    legend: string
    children: React.ReactNode
    className?: string
    gridClassName?: string
}) {
    return (
        <fieldset className={cn("min-w-0", className)}>
            <legend className="mb-3 text-xs font-medium">{legend}</legend>
            <div
                className={cn(
                    "grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2",
                    gridClassName,
                )}
            >
                {children}
            </div>
        </fieldset>
    )
}

/** 高级筛选托盘内的双输入区间胶囊（如金额、价格、数量或天数范围）。 */
export function ListFilterRangeField({
    label,
    minId,
    maxId,
    errorId,
    min,
    max,
    minPlaceholder = "最低",
    maxPlaceholder = "最高",
    minAriaLabel,
    maxAriaLabel,
    unit,
    inputMode = "decimal",
    inputRef,
    error,
    onMinChange,
    onMaxChange,
    fieldClassName,
    inputClassName = "h-control w-0 min-w-0 flex-1",
}: {
    label: string
    minId: string
    maxId: string
    errorId: string
    min: string
    max: string
    minPlaceholder?: string
    maxPlaceholder?: string
    minAriaLabel?: string
    maxAriaLabel?: string
    unit?: string
    inputMode?: React.HTMLAttributes<HTMLInputElement>["inputMode"]
    inputRef?: React.Ref<HTMLInputElement>
    error?: string | null
    onMinChange: (value: string) => void
    onMaxChange: (value: string) => void
    fieldClassName?: string
    inputClassName?: string
}) {
    return (
        <div className={cn("min-w-0", fieldClassName)}>
            <ListWorkspaceFilterField label={label}>
                <div className="flex min-w-0 items-center gap-1">
                    <Input
                        id={minId}
                        ref={inputRef}
                        className={inputClassName}
                        value={min}
                        onChange={(event) => onMinChange(event.target.value)}
                        inputMode={inputMode}
                        autoComplete="off"
                        placeholder={minPlaceholder}
                        aria-label={minAriaLabel ?? `最低${label}`}
                        aria-invalid={Boolean(error)}
                        aria-describedby={error ? errorId : undefined}
                    />
                    <span className="shrink-0 text-xs text-muted-foreground/65">
                        —
                    </span>
                    <Input
                        id={maxId}
                        className={inputClassName}
                        value={max}
                        onChange={(event) => onMaxChange(event.target.value)}
                        inputMode={inputMode}
                        autoComplete="off"
                        placeholder={maxPlaceholder}
                        aria-label={maxAriaLabel ?? `最高${label}`}
                        aria-invalid={Boolean(error)}
                        aria-describedby={error ? errorId : undefined}
                    />
                    {unit ? (
                        <span className="shrink-0 pr-1.5 text-xs text-muted-foreground/75">
                            {unit}
                        </span>
                    ) : null}
                </div>
            </ListWorkspaceFilterField>
            {error ? (
                <p
                    id={errorId}
                    className="mt-1 text-xs text-destructive"
                    role="alert"
                >
                    {error}
                </p>
            ) : null}
        </div>
    )
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
                className="shrink-0 text-body-compact text-muted-foreground"
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
    moreCount,
    moreOpen = false,
    onToggleMore,
    morePanelId,
    morePanel,
    morePanelAriaLabel = "更多筛选条件",
    morePresentation = "inline",
    moreSize: _moreSize = "wide",
    commonFilters,
    primaryFilters,
    resultStatus = null,
    chips = [],
    foldedChipKeys,
    onClearChip,
    onClearAll,
    hasPendingChanges = false,
    reactive = false,
    pendingHint = "条件已修改，待查询",
    idleHint,
    statusActions,
    extraPrimary,
    onResetMore,
    moreHint,
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
    resultStatus?: React.ReactNode
    chips?: readonly ListWorkspaceFilterChip[]
    /**
     * 指定折叠在「更多筛选」里的条件 key。
     * 传入后仅在已选条件带展示这些 key，且未显式传 moreCount 时自动以其数量作为计数角标，
     * 避免与主筛选行重复展示。
     */
    foldedChipKeys?: readonly string[]
    onClearChip?: (key: string) => void
    onClearAll?: () => void
    hasPendingChanges?: boolean
    /** 全即时响应式筛选模式：隐藏手动「查询」按钮与待查询标记，改由控件变更直接触发。 */
    reactive?: boolean
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
    const effectivePending = reactive ? false : hasPendingChanges
    const resolvedMoreHint =
        moreHint ??
        (reactive
            ? "选择条件后即时生效，无需点击查询。"
            : "可组合多个条件，点击「查询」后统一生效。")
    const panelId = morePanelId ?? `${idPrefix}-more-panel`
    const triggerId = moreButtonId ?? `${idPrefix}-more`
    const showMore = morePanel != null && onToggleMore != null
    const visibleChips = React.useMemo(
        () =>
            foldedChipKeys
                ? chips.filter((chip) => foldedChipKeys.includes(chip.key))
                : chips,
        [chips, foldedChipKeys],
    )
    const resolvedMoreCount =
        moreCount ?? (foldedChipKeys ? visibleChips.length : 0)
    // 展开「更多筛选」时顶栏主按钮退场，由展开区页脚「查询」作为唯一提交入口。
    const hideTopQuery = reactive || (showMore && moreOpen)

    React.useEffect(() => {
        if (!(showMore && moreOpen)) return
        const onEscape = (event: KeyboardEvent) => {
            if (
                event.key !== "Escape" ||
                event.defaultPrevented ||
                event.isComposing ||
                document.querySelector(
                    '[role="dialog"]:not([data-slot="toast"]), [role="alertdialog"], [role="menu"], [role="listbox"], [data-slot="popover-content"][data-open]',
                )
            )
                return
            event.preventDefault()
            onToggleMore?.()
            document.getElementById(triggerId)?.focus()
        }
        window.addEventListener("keydown", onEscape)
        return () => window.removeEventListener("keydown", onEscape)
    }, [showMore, moreOpen, onToggleMore, triggerId])
    const statusHost = useTableFilterStatusHost()
    const inlineStatusHost = useTableInlineFilterStatusHost()
    const appliedChipsHost = useTableAppliedChipsHost()
    const filterTrayHost = useTableFilterTrayHost()
    const hasInlineChips =
        morePresentation !== "popover" && visibleChips.length > 0
    const compactInlineStatus = Boolean(
        inlineStatusHost &&
        (resultStatus != null ||
            (morePresentation === "popover" && !idleHint)) &&
        !hasInlineChips &&
        !statusActions,
    )
    const portalAppliedChips = Boolean(
        morePresentation === "popover" &&
        appliedChipsHost &&
        visibleChips.length > 0,
    )
    const appliedChips = visibleChips.length ? (
        <>
            <span className="shrink-0 text-muted-foreground/80">
                {morePresentation === "popover"
                    ? listFilterText.selected
                    : "已生效"}
            </span>
            {visibleChips.map((chip) => (
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
        effectivePending ||
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
                {idleHint || effectivePending || statusActions ? (
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 sm:ml-auto">
                        {idleHint || effectivePending ? (
                            <span
                                role="status"
                                className={
                                    effectivePending
                                        ? "rounded-sm bg-warning-soft px-1.5 py-0.5 font-medium text-warning-soft-foreground"
                                        : "hidden text-muted-foreground/75 xl:inline"
                                }
                            >
                                {effectivePending ? pendingHint : idleHint}
                            </span>
                        ) : null}
                        {statusActions}
                    </div>
                ) : null}
            </div>
        ) : null

    const queryButton = reactive ? null : (
        <ListFilterQueryButton
            id={queryButtonId ?? `${idPrefix}-query`}
            hasPendingChanges={effectivePending}
        />
    )

    return (
        <form
            aria-label={formAriaLabel}
            data-pending-query={
                effectivePending || (showMore && moreOpen && !reactive)
                    ? ""
                    : undefined
            }
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                onSubmit()
            }}
        >
            {hideTopQuery && filterTrayHost && !reactive ? (
                <input
                    id={`${idPrefix}-implicit-submit`}
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
                    morePresentation === "popover" &&
                        listWorkspacePopoverFiltersClassName,
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
                                triggerId={triggerId}
                                panelId={panelId}
                                open={moreOpen}
                                count={resolvedMoreCount}
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
                    (morePresentation === "popover" &&
                        visibleChips.length > 0 &&
                        !appliedChipsHost) ||
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
                            {morePresentation === "popover" &&
                            appliedChips &&
                            !appliedChipsHost ? (
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
                                    onApply={onSubmit}
                                    onClose={onToggleMore}
                                    onReset={onResetMore}
                                    resetId={resetMoreButtonId}
                                    hasPendingChanges={effectivePending}
                                    reactive={reactive}
                                    hint={resolvedMoreHint}
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
            {portalAppliedChips && appliedChipsHost
                ? createPortal(
                      <div
                          data-slot="list-filter-applied"
                          className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1.5 text-xs"
                      >
                          {appliedChips}
                      </div>,
                      appliedChipsHost,
                  )
                : null}
            {showMore && filterTrayHost
                ? createPortal(
                      <ListFilterTray
                          idPrefix={idPrefix}
                          panelId={panelId}
                          label={morePanelAriaLabel}
                          open={moreOpen}
                          onApply={onSubmit}
                          onClose={onToggleMore}
                          onReset={onResetMore}
                          resetId={resetMoreButtonId}
                          hasPendingChanges={effectivePending}
                          reactive={reactive}
                          hint={resolvedMoreHint}
                          inPortal
                      >
                          {morePanel}
                      </ListFilterTray>,
                      filterTrayHost,
                  )
                : null}
            {compactInlineStatus && inlineStatusHost
                ? effectivePending || resultStatus != null
                    ? createPortal(
                          <div className="flex items-center gap-2">
                              {effectivePending ? (
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
                              {resultStatus != null ? (
                                  <span
                                      role="status"
                                      className="num shrink-0 whitespace-nowrap text-xs text-muted-foreground"
                                  >
                                      {resultStatus}
                                  </span>
                              ) : null}
                          </div>,
                          inlineStatusHost,
                      )
                    : null
                : statusHost
                  ? createPortal(statusRow, statusHost)
                  : null}
        </form>
    )
}
