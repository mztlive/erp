import type * as React from "react"
import type {
    ColumnDef,
    ColumnFiltersState,
    ColumnOrderState,
    ColumnPinningState,
    ColumnSizingInfoState,
    ColumnSizingState,
    PaginationState,
    Row,
    RowSelectionState,
    SortingState,
    Table as TanStackTable,
    VisibilityState,
} from "@tanstack/react-table"

import type { DataTableLayout } from "@/components/business/data-table-layout"

export type DataTableProps<TData> = {
    id?: string
    idPrefix?: string
    data: TData[]
    columns: ColumnDef<TData, unknown>[]
    /** 必须使用 ERP 稳定 ID；业务单号和外部单号不能作为行身份。 */
    getRowId: (row: TData, index: number, parent?: Row<TData>) => string
    /** 服务端准确总数；用于普通页面分页，不用于商城同步游标。 */
    rowCount: number
    rowLabel?: (row: TData) => string
    caption?: string

    sorting?: SortingState
    defaultSorting?: SortingState
    onSortingChange?: (sorting: SortingState) => void
    pagination?: PaginationState
    defaultPagination?: PaginationState
    onPaginationChange?: (pagination: PaginationState) => void
    rowSelection?: RowSelectionState
    defaultRowSelection?: RowSelectionState
    onRowSelectionChange?: (selection: RowSelectionState) => void
    columnVisibility?: VisibilityState
    defaultColumnVisibility?: VisibilityState
    onColumnVisibilityChange?: (visibility: VisibilityState) => void
    columnOrder?: ColumnOrderState
    defaultColumnOrder?: ColumnOrderState
    onColumnOrderChange?: (order: ColumnOrderState) => void
    columnPinning?: ColumnPinningState
    defaultColumnPinning?: ColumnPinningState
    onColumnPinningChange?: (pinning: ColumnPinningState) => void
    columnSizing?: ColumnSizingState
    defaultColumnSizing?: ColumnSizingState
    onColumnSizingChange?: (sizing: ColumnSizingState) => void
    columnSizingInfo?: ColumnSizingInfoState
    onColumnSizingInfoChange?: (sizingInfo: ColumnSizingInfoState) => void
    columnFilters?: ColumnFiltersState
    defaultColumnFilters?: ColumnFiltersState
    onColumnFiltersChange?: (filters: ColumnFiltersState) => void
    globalFilter?: string
    defaultGlobalFilter?: string
    onGlobalFilterChange?: (filter: string) => void

    /** ERP 列表默认由 TanStack Query 拉取服务端页。 */
    manualPagination?: boolean
    manualSorting?: boolean
    manualFiltering?: boolean
    enableRowSelection?: boolean | ((row: Row<TData>) => boolean)
    /** 默认 true；false 时勾选一行会清掉其它行。 */
    enableMultiRowSelection?: boolean
    enableColumnResizing?: boolean
    enableColumnPinning?: boolean
    /** inset 自带卡片内距与圆角边框；flush 由外部框架提供边界。 */
    layout?: DataTableLayout
    /** 默认 comfortable，与公司商品池行高/单元格留白一致；高密度账表再显式传 compact。 */
    density?: "compact" | "comfortable"
    striped?: boolean
    loading?: boolean
    /**
     * 查询失败时的整表错误内容（优先级最高）。页面把 isError 时构造的
     * BusinessFailureState 传进来，避免「系统故障」被误报成「当前筛选没有结果」。
     */
    errorState?: React.ReactNode
    /** 内置错误块的标题；默认「数据加载失败」。 */
    errorTitle?: React.ReactNode
    /** 内置错误块的说明；错误说下一步不说原理。 */
    errorSummary?: React.ReactNode
    /** 内置错误块的重试回调；渲染「重试」按钮。 */
    onRetry?: () => void
    /** 空态标题；默认「当前筛选没有结果」。 */
    emptyTitle?: React.ReactNode
    /** 空态说明；默认空。 */
    emptyDescription?: React.ReactNode
    /** 空态引导动作（如「清除筛选」「新建销售单」）。 */
    emptyAction?: React.ReactNode
    /** 正在刷新时显示「正在刷新，当前内容会保留」提示条；轮询页可关闭避免噪声。 */
    showRefreshingBanner?: boolean
    refreshingLabel?: React.ReactNode
    emptyState?: React.ReactNode
    renderToolbar?: (table: TanStackTable<TData>) => React.ReactNode
    showColumnVisibility?: boolean
    showPagination?: boolean
    pageSizeOptions?: readonly number[]
    /** 单击非交互区域时打开快速预览。 */
    onRowPreview?: (row: TData) => void
    /** 聚焦行后按 Enter 优先打开详情；鼠标入口应由业务列提供明确按钮或链接。 */
    onRowOpen?: (row: TData) => void
    /** 刚产生的业务行，用于归档/创建后把视线落到新事实。 */
    highlightedRowId?: string
    className?: string
}
