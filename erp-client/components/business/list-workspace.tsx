"use client"

import * as React from "react"
import NextLink from "next/link"

import {
    ListDataTable,
    type ListDataTableEmptyCopy,
    type ListDataTableProps,
} from "@/components/business/list-data-table"
import {
    ListSelectionBar,
    useFrozenSelectionSnapshot,
    type ListSelectionBarDisplay,
    type ListSelectionBarProps,
} from "@/components/business/list-selection-bar"
import {
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles,
} from "@/components/business/list-workspace-styles"
import {
    PageActions,
    PageScaffold,
    type PageAction,
} from "@/components/business/page"
import {
    TableToolbar,
    TableToolbarAppliedChipsHost,
    TableToolbarFilterTrayHost,
    TableToolbarInlineControls,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { cn } from "@/lib/utils"

export { listWorkspaceEmptyStateClassName, listWorkspaceStyles }

const styles = listWorkspaceStyles

export function ListWorkspaceHeader({
    className,
    eyebrow,
    title,
    description,
    views,
    children,
}: {
    className?: string
    eyebrow?: string
    title: string
    description?: React.ReactNode
    /** 短视图切换（如启停状态 2~4 项）可并入页头同行，压缩表头前纵向层数。 */
    views?: React.ReactNode
    children?: React.ReactNode
}) {
    if (views) {
        return (
            <header
                data-slot="list-workspace-header"
                data-inline-views=""
                className={cn(
                    "flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-0 border-b border-border/60 bg-card md:min-h-12 md:flex-nowrap",
                    className,
                )}
            >
                <div className="flex min-w-0 flex-1 items-center gap-x-4 max-md:contents">
                    <div className="min-w-0 shrink-0 py-2.5 md:py-0">
                        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                            <h1 className={styles.title}>{title}</h1>
                            {eyebrow ? (
                                <span className={styles.eyebrow}>
                                    {eyebrow}
                                </span>
                            ) : null}
                        </div>
                        {description ? (
                            <div className={styles.description}>
                                {description}
                            </div>
                        ) : null}
                    </div>
                    <div className="relative min-w-0 empty:hidden max-md:order-3 max-md:w-full md:pl-4 md:before:absolute md:before:left-0 md:before:top-1/2 md:before:h-4 md:before:w-px md:before:-translate-y-1/2 md:before:bg-border/80">
                        {views}
                    </div>
                </div>
                {children ? (
                    <div
                        className={cn(
                            styles.headerActions,
                            "py-2 max-md:order-2 max-md:w-auto md:py-0",
                        )}
                    >
                        {children}
                    </div>
                ) : null}
            </header>
        )
    }

    return (
        <header
            data-slot="list-workspace-header"
            className={cn(styles.header, className)}
        >
            <div className="min-w-0">
                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                    <h1 className={styles.title}>{title}</h1>
                    {eyebrow ? (
                        <span className={styles.eyebrow}>{eyebrow}</span>
                    ) : null}
                </div>
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

const GENERIC_VIEW_HINT_RE = /^(选择|点击).+查看详情$/

export function ListWorkspaceViews({
    ariaLabel,
    items,
    hint,
    variant = "bar",
}: {
    ariaLabel: string
    items: readonly ListWorkspaceViewItem[]
    hint?: string
    /** inline：并入页头同行展示，贴齐页头底线；bar：独立通栏视图行。 */
    variant?: "bar" | "inline"
}) {
    // 单个视图不是切换，不画 tab 行（例如「全部合同」）。
    if (items.length < 2) return null
    const visibleHint =
        hint && !GENERIC_VIEW_HINT_RE.test(hint.trim()) ? hint : undefined
    const isInline = variant === "inline"
    return (
        <div
            className={
                isInline
                    ? "flex min-w-0 items-center justify-between gap-3"
                    : styles.viewBar
            }
            data-slot="list-workspace-views"
            data-variant={variant}
        >
            <div
                className={cn(styles.views, isInline && "gap-5")}
                role="group"
                aria-label={ariaLabel}
            >
                {items.map((item) => {
                    const className = cn(
                        styles.view,
                        isInline && "md:min-h-12",
                        item.active && styles.activeView,
                    )
                    const content = (
                        <>
                            {item.label}
                            {item.count != null ? (
                                <span
                                    className={cn(
                                        styles.viewCount,
                                        item.active &&
                                            "font-medium text-primary",
                                    )}
                                >
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
            {visibleHint ? (
                <span className={styles.viewHint}>{visibleHint}</span>
            ) : null}
        </div>
    )
}

export { ListSearchField } from "./list-search-field"
export {
    ListFilterRangeField,
    ListFilterSection,
    ListFilterSections,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
    type ListWorkspaceFilterChip,
} from "./list-workspace-filter-bar"
export {
    ListDataTable,
    type ListDataTableEmptyCopy,
    type ListDataTableProps,
    ListSelectionBar,
    useFrozenSelectionSnapshot,
    type ListSelectionBarDisplay,
    type ListSelectionBarProps,
}

export type ListWorkSurfaceProps = {
    ariaLabel: string
    views?: React.ReactNode
    toolbar?: React.ReactNode
    table: React.ReactNode
    className?: string
    tableClassName?: string
    toolbarClassName?: string
    selectionBar?: React.ReactNode
    /** 按需显隐 selectionBar 时传入，配合表格工具栏播放平滑展开/收起过渡。 */
    selectionBarOpen?: boolean
    tableActions?: React.ReactNode
}

export function ListWorkSurface({
    ariaLabel,
    views,
    toolbar,
    table,
    className,
    tableClassName,
    toolbarClassName,
    selectionBar,
    selectionBarOpen,
    tableActions,
}: ListWorkSurfaceProps) {
    return (
        <TableToolbarScope>
            <section
                data-slot="list-work-surface"
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
                        <TableToolbarInlineControls actions={tableActions} />
                    </div>
                ) : null}
                <TableToolbarAppliedChipsHost />
                <TableToolbarFilterTrayHost />
                <TableToolbar
                    isFrameToolbar
                    animateCollapse={selectionBarOpen !== undefined}
                    childrenOpen={selectionBarOpen}
                    hideControls={Boolean(toolbar)}
                    actions={toolbar ? undefined : tableActions}
                    wrapperClassName="bg-card px-4 pb-2.5"
                    className={
                        selectionBar
                            ? "rounded-lg border border-primary/20 bg-sidebar-accent/55 px-3"
                            : "rounded-lg border border-border/70 bg-surface-sunken/75 px-3"
                    }
                >
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

export type ListWorkspacePageAction = PageAction & {
    /**
     * 页头动作视觉层级：
     * - quiet：无框轻按钮（导入、导出等次要低频动作），自动应用 ghost + quietButton；
     * - primary：实心主蓝按钮（页头唯一主动作，如新建）。
     */
    tier?: "quiet" | "primary"
}

export type ListWorkspaceViewsConfig = {
    ariaLabel: string
    items: readonly ListWorkspaceViewItem[]
    hint?: string
}

export type ListWorkspaceViewsPlacement = "auto" | "inline" | "bar"

export type ListWorkspacePageProps = {
    title: string
    eyebrow?: string
    description?: React.ReactNode
    /**
     * 视图切换：
     * - 传结构化对象 `{ ariaLabel, items, hint }` 时，auto 模式按项数自动排版（2~4 项内联页头，≥5 项独立通栏视图行，<2 项自动隐藏）；
     * - 也可直接传自定义 ReactNode（如各领域封装的指标视图条）。
     */
    views?: React.ReactNode | ListWorkspaceViewsConfig
    viewsPlacement?: ListWorkspaceViewsPlacement
    /** 页头操作按钮列表；与 headerActions 二选一或组合使用。 */
    actions?: readonly ListWorkspacePageAction[]
    /** 自定义页头右侧动作区（如带 Popover 的说明按钮）。 */
    headerActions?: React.ReactNode
    alerts?: React.ReactNode
    notices?: React.ReactNode
    resultsLabel?: string
    resultsHeadingRef?: React.RefObject<HTMLHeadingElement | null>
    resultsHeadingId?: string
    /** 首屏初始加载态，渲染页头与标准工具栏/表格骨架。 */
    loading?: boolean
    /** 传入时自动渲染 ListWorkSurface；未传时可直接在 children 中放置自定义工作面。 */
    surface?: ListWorkSurfaceProps
    children?: React.ReactNode
    className?: string
}

function isStructuredViews(
    views: ListWorkspacePageProps["views"],
): views is ListWorkspaceViewsConfig {
    return (
        typeof views === "object" &&
        views !== null &&
        !React.isValidElement(views) &&
        "items" in views &&
        Array.isArray(views.items)
    )
}

/** 将页头次要动作统一归一化为飞书轻按钮（ghost + quietButton）。 */
export function listWorkspaceQuietAction(action: PageAction): PageAction {
    return {
        ...action,
        variant: "ghost",
        className: cn(styles.quietButton, action.className),
    }
}

function normalizePageAction(action: ListWorkspacePageAction): PageAction {
    const { tier, ...rest } = action
    if (tier === "quiet") {
        return listWorkspaceQuietAction(rest)
    }
    if (tier === "primary") {
        return {
            ...rest,
            variant: rest.variant ?? "default",
        }
    }
    return rest
}

/**
 * 飞书多维表格风格纯白通栏列表页壳：
 * - 统一装配 PageScaffold(.erp-list-workspace-page) + ListWorkspaceHeader + 首屏骨架 + 告警/反馈 + 读屏结果标题 + ListWorkSurface；
 * - 2~4 项结构化视图默认内联页头，页头次要按钮通过 `tier: "quiet"` 一键降噪。
 */
export function ListWorkspacePage({
    title,
    eyebrow,
    description,
    views,
    viewsPlacement = "auto",
    actions,
    headerActions,
    alerts,
    notices,
    resultsLabel,
    resultsHeadingRef,
    resultsHeadingId,
    loading = false,
    surface,
    children,
    className,
}: ListWorkspacePageProps) {
    let headerViews: React.ReactNode = null
    let barViews: React.ReactNode = null

    if (isStructuredViews(views)) {
        if (views.items.length >= 2) {
            const inline =
                viewsPlacement === "inline" ||
                (viewsPlacement === "auto" && views.items.length <= 4)
            const rendered = (
                <ListWorkspaceViews
                    ariaLabel={views.ariaLabel}
                    items={views.items}
                    hint={views.hint}
                    variant={inline ? "inline" : "bar"}
                />
            )
            if (inline) headerViews = rendered
            else barViews = rendered
        }
    } else if (views != null) {
        if (viewsPlacement === "bar") barViews = views
        else headerViews = views
    }

    if (loading) {
        return (
            <PageScaffold
                density="compact"
                className={cn(styles.page, className)}
            >
                <ListWorkspaceHeader
                    eyebrow={eyebrow}
                    title={title}
                    description={description}
                />
                <div
                    className="h-10 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
                <div className="h-96 animate-pulse bg-muted" aria-busy />
            </PageScaffold>
        )
    }

    const normalizedActions = actions?.map(normalizePageAction)
    const hasActions =
        (normalizedActions != null && normalizedActions.length > 0) ||
        headerActions != null

    return (
        <PageScaffold density="compact" className={cn(styles.page, className)}>
            <ListWorkspaceHeader
                eyebrow={eyebrow}
                title={title}
                description={description}
                views={headerViews}
            >
                {hasActions ? (
                    <>
                        {headerActions}
                        {normalizedActions && normalizedActions.length > 0 ? (
                            <PageActions
                                actions={normalizedActions}
                                size="sm"
                            />
                        ) : null}
                    </>
                ) : null}
            </ListWorkspaceHeader>
            {alerts}
            {notices}
            {resultsLabel ? (
                <h2
                    id={resultsHeadingId}
                    ref={resultsHeadingRef}
                    tabIndex={-1}
                    className="sr-only outline-none"
                >
                    {resultsLabel}
                </h2>
            ) : null}
            {surface ? (
                <ListWorkSurface
                    {...surface}
                    views={surface.views ?? barViews}
                />
            ) : null}
            {children}
        </PageScaffold>
    )
}
