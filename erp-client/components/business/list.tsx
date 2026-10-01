"use client"

import * as React from "react"

import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
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
                                className="min-w-0 w-full sm:w-72 lg:w-80 sm:flex-initial"
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
    /** 表格视图操作；列设置由 DataTable 提供并置于最右侧。 */
    readonly tableActions?: React.ReactNode
    readonly table: React.ReactNode
    readonly footer?: React.ReactNode
}

/**
 * 列表工作面：工具条与结果区沿同一基线排列，以分隔线划分区域。
 * showHeader 时展示结果标题与说明，分页保持表格既有位置。
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
    tableActions,
    table,
    footer,
    className,
    ...props
}: BusinessTableFrameProps) {
    const Heading = headingLevel

    return (
        <TableToolbarScope>
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
                        {toolbar || headerActions ? (
                            <div className="flex flex-wrap items-start justify-between gap-2">
                                <div className="min-w-0 flex-1">{toolbar}</div>
                                <div className="flex shrink-0 items-center gap-2">
                                    {headerActions}
                                </div>
                            </div>
                        ) : null}
                        <TableToolbar isFrameToolbar actions={tableActions}>
                            {selectionBar}
                        </TableToolbar>
                        <div data-slot="business-table-frame-table">
                            {table}
                        </div>
                    </>
                ) : (
                    <>
                        {toolbar ? (
                            <div
                                data-slot="table-frame-toolbar"
                                className="bg-card py-1"
                            >
                                {toolbar}
                            </div>
                        ) : null}
                        <div
                            data-slot="business-table-frame-result"
                            className="overflow-hidden border-y border-border bg-card"
                        >
                            <div className="flex min-h-row-comfortable flex-col gap-2 border-b px-0 py-4 sm:flex-row sm:items-center sm:justify-between">
                                <div className="min-w-0">
                                    <Heading className="text-base font-semibold text-foreground">
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
                                </div>
                            </div>
                            <TableToolbar isFrameToolbar actions={tableActions}>
                                {selectionBar}
                            </TableToolbar>
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
        </TableToolbarScope>
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
    readonly overlayClassName?: string
    /**
     * preview：窄栏 + 整区滚动，适合轻摘要。
     * detail：半屏 + 正文区由子树自管滚动，适合双栏读主记录。
     */
    readonly size?: QuickPreviewSheetSize
    readonly id?: string
    readonly idPrefix?: string
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
    overlayClassName,
    size = "preview",
    id,
    idPrefix,
    ...props
}: QuickPreviewSheetProps) {
    const isDetail = size === "detail"
    const baseId = idPrefix ?? id
    const closeButtonId = baseId ? `${baseId}-close` : undefined

    return (
        <Sheet {...props} open={open} onOpenChange={onOpenChange}>
            <SheetContent
                side="right"
                size={size}
                className={contentClassName}
                overlayClassName={overlayClassName}
                closeButtonId={closeButtonId}
            >
                <SheetHeader className="border-b border-border">
                    {identity ? (
                        <div
                            data-slot="quick-preview-identity"
                            className="text-xs text-muted-foreground"
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
                            className="space-y-6 px-7 py-6"
                        >
                            {children}
                        </div>
                    </ScrollArea>
                )}

                {footer ? (
                    <SheetFooter className="border-t border-border">
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
    type BusinessTableFrameProps,
    type ListToolbarProps,
    type QuickPreviewSheetProps,
    type QuickPreviewSheetSize,
}
