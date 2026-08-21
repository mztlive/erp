"use client"

import * as React from "react"
import type { LucideIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"

type DivProps = React.ComponentPropsWithoutRef<"div">

interface ListToolbarProps extends Omit<DivProps, "children"> {
    /** 常驻搜索控件。查询值与提交时机由业务层管理。 */
    readonly search?: React.ReactNode
    /** 主筛（≤3）：状态、仓库、到期等日常最高频维度。 */
    readonly filters?: React.ReactNode
    /**
     * 第 2 层：高级筛选入口、来源锁定 FilterChip 等。
     * 独立成行，不与主筛挤在同一视觉行（见 docs/ui-filter-design.md §3）。
     */
    readonly secondary?: React.ReactNode
    /** 当前视图选择、保存和管理入口。 */
    readonly savedView?: React.ReactNode
    /** 计数、清除筛选、队列「自动下一项」等。 */
    readonly actions?: React.ReactNode
}

/**
 * 跨业务域列表工具栏。
 *
 * 组件只排列受控搜索、主筛、次要行、保存视图和动作插槽，不持有查询或路由状态。
 * 布局契约：主行 = search + filters(≤3) + actions；secondary 固定次行。
 */
function ListToolbar({
    search,
    filters,
    secondary,
    savedView,
    actions,
    className,
    "aria-label": ariaLabel = "列表工具栏",
    ...props
}: ListToolbarProps) {
    const hasQueryTools = Boolean(savedView || search || filters)

    return (
        <div
            role="toolbar"
            aria-label={ariaLabel}
            data-slot="list-toolbar"
            className={cn(
                "flex flex-col gap-2",
                // 主行控件统一吃 --spacing-control，避免搜索框/分段/按钮各撑各的高度
                "[&_[data-slot=list-toolbar-query-tools]]:items-center",
                "[&_[data-slot=input-group]]:h-control [&_[data-slot=input-group]]:min-h-0",
                "[&_[data-slot=list-toolbar-filters]>[data-slot=button]]:h-control",
                className,
            )}
            {...props}
        >
            <div
                data-slot="list-toolbar-primary"
                className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between"
            >
                {hasQueryTools ? (
                    <div
                        data-slot="list-toolbar-query-tools"
                        className="flex min-w-0 flex-1 flex-col gap-2 sm:flex-row sm:items-center"
                    >
                        {savedView ? (
                            <div
                                data-slot="list-toolbar-saved-view"
                                className="shrink-0"
                            >
                                {savedView}
                            </div>
                        ) : null}
                        {search ? (
                            <div
                                data-slot="list-toolbar-search"
                                className="min-w-0 w-full flex-1 sm:min-w-search-min"
                            >
                                {search}
                            </div>
                        ) : null}
                        {filters ? (
                            <div
                                data-slot="list-toolbar-filters"
                                className="flex shrink-0 flex-wrap items-center gap-2"
                            >
                                {filters}
                            </div>
                        ) : null}
                    </div>
                ) : null}

                {actions ? (
                    <div className="flex items-stretch gap-3">
                        {hasQueryTools ? (
                            <Separator
                                orientation="vertical"
                                className="hidden bg-border lg:block"
                            />
                        ) : null}
                        <div
                            data-slot="list-toolbar-actions"
                            className="flex flex-wrap items-center gap-2"
                        >
                            {actions}
                        </div>
                    </div>
                ) : null}
            </div>

            {secondary ? (
                <div
                    data-slot="list-toolbar-secondary"
                    className="flex flex-wrap items-center gap-2"
                >
                    {secondary}
                </div>
            ) : null}
        </div>
    )
}

type SelectionScope = "page" | "filtered"

interface SelectionScopeBarProps extends Omit<DivProps, "children"> {
    /** 当前选择是当前页记录，还是全部符合筛选的记录。 */
    readonly scope: SelectionScope
    /** 当前选择快照中的记录数。 */
    readonly selectedCount: number
    /** 当前页可进入选择范围的记录数。 */
    readonly pageItemCount: number
    /** 当前筛选命中的总记录数。 */
    readonly filteredCount: number
    /** 当前筛选摘要；全筛选批量动作可用它再次说明影响范围。 */
    readonly filterSummary?: React.ReactNode
    /** 当前选择范围允许执行的批量动作。 */
    readonly actions?: React.ReactNode
    readonly onScopeChange: (scope: SelectionScope) => void
    readonly onClearSelection?: () => void
    readonly scopeChangeDisabled?: boolean
}

/** 明确区分“本页选择”和“全部筛选结果”的受控选择范围条。 */
function SelectionScopeBar({
    scope,
    selectedCount,
    pageItemCount,
    filteredCount,
    filterSummary,
    actions,
    onScopeChange,
    onClearSelection,
    scopeChangeDisabled = false,
    className,
    "aria-label": ariaLabel = "批量选择范围",
    ...props
}: SelectionScopeBarProps) {
    const isFilteredScope = scope === "filtered"
    const formattedSelectedCount = selectedCount.toLocaleString("zh-CN")
    const formattedPageItemCount = pageItemCount.toLocaleString("zh-CN")
    const formattedFilteredCount = filteredCount.toLocaleString("zh-CN")

    return (
        <div
            role="region"
            aria-label={ariaLabel}
            aria-live="polite"
            data-slot="selection-scope-bar"
            data-scope={scope}
            className={cn(
                "flex flex-col gap-3 rounded-2xl border border-border bg-muted/50 p-3 sm:flex-row sm:items-center sm:justify-between",
                className,
            )}
            {...props}
        >
            <div className="flex min-w-0 flex-wrap items-center gap-2">
                <Badge variant={isFilteredScope ? "info" : "secondary"}>
                    {isFilteredScope
                        ? `全部筛选 ${formattedSelectedCount} 条`
                        : `本页 ${formattedSelectedCount} 条`}
                </Badge>
                <span className="text-sm font-medium text-foreground">
                    {isFilteredScope
                        ? "已选择全部符合当前筛选的记录"
                        : "已选择当前页记录"}
                </span>
                {filterSummary ? (
                    <span
                        data-slot="selection-scope-summary"
                        className="min-w-0 text-sm text-muted-foreground"
                    >
                        {filterSummary}
                    </span>
                ) : null}
                <Button
                    type="button"
                    variant="link"
                    size="sm"
                    disabled={
                        scopeChangeDisabled ||
                        (!isFilteredScope && filteredCount <= selectedCount)
                    }
                    onClick={() =>
                        onScopeChange(isFilteredScope ? "page" : "filtered")
                    }
                >
                    {isFilteredScope
                        ? `仅选择本页 ${formattedPageItemCount} 条`
                        : `选择全部符合当前筛选的 ${formattedFilteredCount} 条`}
                </Button>
            </div>

            {actions || onClearSelection ? (
                <div
                    data-slot="selection-scope-actions"
                    className="flex shrink-0 flex-wrap items-center gap-2"
                >
                    {actions}
                    {onClearSelection ? (
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onClearSelection}
                        >
                            取消选择
                        </Button>
                    ) : null}
                </div>
            ) : null}
        </div>
    )
}

interface StatusMatrixItem {
    /** 同一业务对象内稳定且唯一的状态轴标识。 */
    readonly id: string
    /** 状态轴名称，例如“主状态”“履约”“回款”。 */
    readonly axis: string
    /** 必填的状态文字；状态不得只靠颜色或图标表达。 */
    readonly status: string
    readonly tone?: StatusTone
    readonly icon?: LucideIcon
    readonly description?: React.ReactNode
}

interface StatusMatrixProps extends Omit<
    React.ComponentPropsWithoutRef<"dl">,
    "children"
> {
    readonly items: readonly StatusMatrixItem[]
}

/** 并列展示多个互不覆盖的状态轴，每个状态均包含文字与图标。 */
function StatusMatrix({ items, className, ...props }: StatusMatrixProps) {
    return (
        <div data-slot="status-matrix-container" className="@container">
            <dl
                data-slot="status-matrix"
                className={cn(
                    "grid gap-3 @sm:grid-cols-2 @2xl:grid-cols-3",
                    className,
                )}
                {...props}
            >
                {items.map((item) => (
                    <div
                        key={item.id}
                        data-slot="status-matrix-axis"
                        className="grid grid-cols-2 items-center gap-2 rounded-2xl border border-border bg-muted/50 p-3"
                    >
                        <dt className="min-w-0 text-sm font-medium text-muted-foreground">
                            {item.axis}
                        </dt>
                        <dd className="flex min-w-0 justify-end">
                            <StatusBadge
                                label={item.status}
                                tone={item.tone}
                                icon={item.icon}
                            />
                        </dd>
                        {item.description ? (
                            <dd className="col-span-2 text-sm text-muted-foreground">
                                {item.description}
                            </dd>
                        ) : null}
                    </div>
                ))}
            </dl>
        </div>
    )
}

type BusinessTableHeadingLevel = "h1" | "h2" | "h3"

interface BusinessTableFrameProps extends Omit<
    React.ComponentProps<"section">,
    "children" | "title"
> {
    readonly title: React.ReactNode
    readonly description?: React.ReactNode
    readonly headingLevel?: BusinessTableHeadingLevel
    /** 显示表格自己的结果标题栏；默认继续只向读屏器提供辅助标题。 */
    readonly showHeader?: boolean
    /** 导出、新建等与整张表相关的标题区动作。 */
    readonly headerActions?: React.ReactNode
    readonly toolbar?: React.ReactNode
    readonly selectionBar?: React.ReactNode
    readonly table: React.ReactNode
    readonly footer?: React.ReactNode
}

/**
 * 列表工作面：工具条贴画布；表格单独圆角描边。
 * showHeader 时分页贴在结果卡页脚；否则分页在表外。
 * 页头标题由 PageHeader 承担，这里的 title 仅作辅助标题。
 */
function BusinessTableFrame({
    title,
    description,
    headingLevel = "h2",
    showHeader = false,
    headerActions,
    toolbar,
    selectionBar,
    table,
    footer,
    className,
    ...props
}: BusinessTableFrameProps) {
    const Heading = headingLevel

    return (
        <section
            data-business-component="table-frame"
            className={cn("flex min-w-0 flex-col gap-4", className)}
            {...props}
        >
            {!showHeader ? (
                <>
                    <Heading className="sr-only">{title}</Heading>
                    {description ? (
                        <p className="sr-only">{description}</p>
                    ) : null}
                    {/* items-start：筛选面板展开后表级动作仍留在首行，不被垂直居中拽到面板中间 */}
                    <div className="flex flex-wrap items-start justify-between gap-2">
                        <div className="min-w-0 flex-1">{toolbar}</div>
                        <div className="flex shrink-0 items-center gap-2">
                            {headerActions}
                            <div data-slot="table-frame-view-options" />
                        </div>
                    </div>
                    {selectionBar}
                    <div data-slot="business-table-frame-table">{table}</div>
                </>
            ) : (
                <>
                    {toolbar ? (
                        <div
                            data-slot="table-frame-toolbar"
                            className="rounded-lg border bg-card p-toolbar-inset shadow-xs"
                        >
                            {toolbar}
                        </div>
                    ) : null}
                    {selectionBar}
                    <div
                        data-slot="business-table-frame-result"
                        className="overflow-hidden rounded-lg border bg-card shadow-xs"
                    >
                        <div className="flex min-h-row-comfortable flex-col gap-2 border-b px-table-cell-inline py-toolbar-inset sm:flex-row sm:items-center sm:justify-between">
                            <div className="min-w-0">
                                <Heading className="text-sm font-semibold text-foreground">
                                    {title}
                                </Heading>
                                {description ? (
                                    <p className="mt-0.5 text-xs text-muted-foreground">
                                        {description}
                                    </p>
                                ) : null}
                            </div>
                            <div className="flex shrink-0 items-center gap-2">
                                {headerActions}
                                <div data-slot="table-frame-view-options" />
                            </div>
                        </div>
                        <div
                            data-slot="business-table-frame-table"
                            className="[&_[data-slot=data-table]]:gap-0 [&_[data-slot=data-table-pagination]]:border-t [&_[data-slot=data-table-surface]]:rounded-none [&_[data-slot=data-table-surface]]:border-0"
                        >
                            {table}
                        </div>
                    </div>
                </>
            )}
            {footer}
        </section>
    )
}

type SheetProps = React.ComponentProps<typeof Sheet>
type SheetOpenChangeHandler = NonNullable<SheetProps["onOpenChange"]>

type QuickPreviewSheetSize = "preview" | "detail"

interface QuickPreviewSheetProps extends Omit<
    SheetProps,
    "children" | "defaultOpen" | "onOpenChange" | "open"
> {
    readonly open: boolean
    readonly onOpenChange: SheetOpenChangeHandler
    readonly title: React.ReactNode
    readonly description?: React.ReactNode
    /** 编号、客户等紧邻标题展示的识别信息。 */
    readonly identity?: React.ReactNode
    /** 多维状态等头部摘要。 */
    readonly summary?: React.ReactNode
    readonly children: React.ReactNode
    readonly footer?: React.ReactNode
    readonly contentClassName?: string
    /**
     * preview：窄栏 + 整区滚动，适合轻摘要。
     * detail：半屏 + 正文区由子树自管滚动，适合双栏读主记录。
     */
    readonly size?: QuickPreviewSheetSize
}

/** 受控的右侧预览抽屉；正文与页脚均由业务页面注入。 */
function QuickPreviewSheet({
    open,
    onOpenChange,
    title,
    description,
    identity,
    summary,
    children,
    footer,
    contentClassName,
    size = "preview",
    ...props
}: QuickPreviewSheetProps) {
    const isDetail = size === "detail"

    return (
        <Sheet {...props} open={open} onOpenChange={onOpenChange}>
            <SheetContent side="right" size={size} className={contentClassName}>
                <SheetHeader
                    className={cn(
                        "shrink-0 border-b border-border",
                        isDetail && "space-y-2 py-4",
                    )}
                >
                    {identity ? (
                        <div
                            data-slot="quick-preview-identity"
                            className="text-sm text-muted-foreground"
                        >
                            {identity}
                        </div>
                    ) : null}
                    <SheetTitle>{title}</SheetTitle>
                    {description ? (
                        <SheetDescription>{description}</SheetDescription>
                    ) : null}
                    {summary ? (
                        <div data-slot="quick-preview-summary" className="pt-1">
                            {summary}
                        </div>
                    ) : null}
                </SheetHeader>

                {isDetail ? (
                    <div
                        data-slot="quick-preview-content"
                        className="flex min-h-0 flex-1 flex-col overflow-hidden"
                    >
                        {children}
                    </div>
                ) : (
                    <ScrollArea className="min-h-0 flex-1">
                        <div
                            data-slot="quick-preview-content"
                            className="space-y-5 p-6"
                        >
                            {children}
                        </div>
                    </ScrollArea>
                )}

                {footer ? (
                    <SheetFooter
                        className={cn(
                            "shrink-0 border-t border-border",
                            isDetail &&
                                "flex-row flex-wrap justify-end gap-2 py-3",
                        )}
                    >
                        {footer}
                    </SheetFooter>
                ) : null}
            </SheetContent>
        </Sheet>
    )
}

export {
    BusinessTableFrame,
    ListToolbar,
    QuickPreviewSheet,
    SelectionScopeBar,
    StatusMatrix,
    type BusinessTableFrameProps,
    type ListToolbarProps,
    type QuickPreviewSheetProps,
    type QuickPreviewSheetSize,
    type SelectionScope,
    type SelectionScopeBarProps,
    type StatusMatrixItem,
    type StatusMatrixProps,
}
