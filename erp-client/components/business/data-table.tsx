"use client"

import * as React from "react"
import { createPortal } from "react-dom"
import { ChevronRightIcon } from "lucide-react"
import {
    getCoreRowModel,
    getFilteredRowModel,
    getPaginationRowModel,
    getSortedRowModel,
    useReactTable,
    type ColumnDef,
    type ColumnFiltersState,
    type ColumnOrderState,
    type ColumnPinningState,
    type ColumnSizingInfoState,
    type ColumnSizingState,
    type PaginationState,
    type RowData,
    type RowSelectionState,
    type SortingState,
    type VisibilityState,
} from "@tanstack/react-table"

import { DataTableBody } from "@/components/business/data-table-body"
import {
    DataTablePagination,
    DataTableViewOptions,
} from "@/components/business/data-table-controls"
import type { DataTableProps } from "@/components/business/data-table-contracts"
import { DataTableHeader } from "@/components/business/data-table-header"
import type {
    DataTableAlignment,
    DataTableColumnWidth,
    DataTableLayout,
} from "@/components/business/data-table-layout"
import { usePinnedColumnOffsets } from "@/components/business/data-table-pinning"
import {
    emptyColumnSizingInfo,
    useColumnResize,
} from "@/components/business/data-table-resize"
import { useControlledTableState } from "@/components/business/data-table-state"
import { Checkbox } from "@/components/ui/checkbox"
import { Spinner } from "@/components/ui/spinner"
import { Table, TableCaption } from "@/components/ui/table"
import { cn } from "@/lib/utils"

declare module "@tanstack/react-table" {
    // 泛型名称必须与 TanStack 的可合并声明保持同一形状。
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    interface ColumnMeta<TData extends RowData, TValue> {
        /** 用于列显隐菜单和无障碍排序名称。 */
        label?: string
        /** 只开放语义对齐，不接受任意样式。 */
        align?: DataTableAlignment
        /** 金额、数量、比例与日期使用等宽数字。 */
        numeric?: boolean
        /** 只声明业务语义宽度；实际尺寸来自主题 token。 */
        width?: DataTableColumnWidth
        /** 系统列使用固定的语义布局，不开放业务列尺寸配置。 */
        role?: "selection" | "preview"
    }
}

function DataTable<TData>({
    data,
    columns,
    getRowId,
    rowCount,
    rowLabel,
    caption = "业务数据列表",
    sorting: controlledSorting,
    defaultSorting = [],
    onSortingChange,
    pagination: controlledPagination,
    defaultPagination = { pageIndex: 0, pageSize: 50 },
    onPaginationChange,
    rowSelection: controlledRowSelection,
    defaultRowSelection = {},
    onRowSelectionChange,
    columnVisibility: controlledColumnVisibility,
    defaultColumnVisibility = {},
    onColumnVisibilityChange,
    columnOrder: controlledColumnOrder,
    defaultColumnOrder = [],
    onColumnOrderChange,
    columnPinning: controlledColumnPinning,
    defaultColumnPinning = {},
    onColumnPinningChange,
    columnSizing: controlledColumnSizing,
    defaultColumnSizing = {},
    onColumnSizingChange,
    columnSizingInfo: controlledColumnSizingInfo,
    onColumnSizingInfoChange,
    columnFilters: controlledColumnFilters,
    defaultColumnFilters = [],
    onColumnFiltersChange,
    globalFilter: controlledGlobalFilter,
    defaultGlobalFilter = "",
    onGlobalFilterChange,
    manualPagination = true,
    manualSorting = true,
    manualFiltering = true,
    enableRowSelection = false,
    enableMultiRowSelection = true,
    enableColumnResizing = true,
    enableColumnPinning = true,
    layout = "flush",
    density = "comfortable",
    striped = false,
    loading = false,
    errorState,
    errorTitle,
    errorSummary,
    onRetry,
    emptyTitle,
    emptyDescription,
    emptyAction,
    showRefreshingBanner = true,
    refreshingLabel = "正在刷新，当前内容会保留",
    emptyState,
    renderToolbar,
    showColumnVisibility = true,
    showPagination = true,
    pageSizeOptions = [20, 50, 100],
    onRowPreview,
    onRowOpen,
    highlightedRowId,
    className,
}: DataTableProps<TData>) {
    const rowRefs = React.useRef(new Map<string, HTMLTableRowElement>())
    const tableSurfaceRef = React.useRef<HTMLDivElement>(null)

    React.useEffect(() => {
        if (!highlightedRowId) return
        rowRefs.current
            .get(highlightedRowId)
            ?.scrollIntoView({ block: "nearest" })
    }, [highlightedRowId, data])

    const [sorting, setSorting] = useControlledTableState({
        value: controlledSorting,
        defaultValue: defaultSorting,
        onChange: onSortingChange,
    })
    const [pagination, setPagination] = useControlledTableState({
        value: controlledPagination,
        defaultValue: defaultPagination,
        onChange: onPaginationChange,
    })
    const [rowSelection, setRowSelection] = useControlledTableState({
        value: controlledRowSelection,
        defaultValue: defaultRowSelection,
        onChange: onRowSelectionChange,
    })
    const [columnVisibility, setColumnVisibility] = useControlledTableState({
        value: controlledColumnVisibility,
        defaultValue: defaultColumnVisibility,
        onChange: onColumnVisibilityChange,
    })
    const [columnOrder, setColumnOrder] = useControlledTableState({
        value: controlledColumnOrder,
        defaultValue: defaultColumnOrder,
        onChange: onColumnOrderChange,
    })
    const [columnPinning, setColumnPinning] = useControlledTableState({
        value: controlledColumnPinning,
        defaultValue: defaultColumnPinning,
        onChange: onColumnPinningChange,
    })
    const [columnSizing, setColumnSizing] = useControlledTableState({
        value: controlledColumnSizing,
        defaultValue: defaultColumnSizing,
        onChange: onColumnSizingChange,
    })
    const [columnSizingInfo, setColumnSizingInfo] = useControlledTableState({
        value: controlledColumnSizingInfo,
        defaultValue: emptyColumnSizingInfo,
        onChange: onColumnSizingInfoChange,
    })
    const [columnFilters, setColumnFilters] = useControlledTableState({
        value: controlledColumnFilters,
        defaultValue: defaultColumnFilters,
        onChange: onColumnFiltersChange,
    })
    const [globalFilter, setGlobalFilter] = useControlledTableState({
        value: controlledGlobalFilter,
        defaultValue: defaultGlobalFilter,
        onChange: onGlobalFilterChange,
    })

    // 筛选变化后总行数变少时，把越界页码钳回最后一个有效页，
    // 避免「共 N 条」与空表格并存（页码由页面写回 URL 或本地状态）。
    React.useEffect(() => {
        if (!manualPagination || rowCount <= 0) return
        const pageCount = Math.ceil(rowCount / pagination.pageSize)
        if (pagination.pageIndex >= pageCount) {
            setPagination({
                pageIndex: Math.max(0, pageCount - 1),
                pageSize: pagination.pageSize,
            })
        }
    }, [
        manualPagination,
        pagination.pageIndex,
        pagination.pageSize,
        rowCount,
        setPagination,
    ])

    // 服务端排序（manualSorting）只有页面接了 onSortingChange 才是真实交互；
    // 否则列头排序按钮只会翻转箭头、数据不动，属于误导性伪交互，隐藏排序入口。
    const sortingInteractive = !manualSorting || onSortingChange !== undefined

    const selectionColumn = React.useMemo<ColumnDef<TData, unknown>>(
        () => ({
            id: "__selection",
            header: ({ table }) => (
                <div className="flex items-center justify-center">
                    <Checkbox
                        checked={table.getIsAllPageRowsSelected()}
                        indeterminate={table.getIsSomePageRowsSelected()}
                        onClick={(event) => event.stopPropagation()}
                        onCheckedChange={(checked) =>
                            table.toggleAllPageRowsSelected(checked === true)
                        }
                        aria-label="选择当前页全部记录"
                    />
                </div>
            ),
            cell: ({ row }) => (
                <div className="flex items-center justify-center">
                    <Checkbox
                        checked={row.getIsSelected()}
                        indeterminate={row.getIsSomeSelected()}
                        disabled={!row.getCanSelect()}
                        onClick={(event) => event.stopPropagation()}
                        onCheckedChange={(checked) =>
                            row.toggleSelected(checked === true)
                        }
                        aria-label={`选择 ${rowLabel?.(row.original) ?? `第 ${row.index + 1} 行`}`}
                    />
                </div>
            ),
            enableHiding: false,
            enablePinning: false,
            enableResizing: false,
            enableSorting: false,
            meta: {
                label: "选择",
                align: "center",
                role: "selection",
            },
        }),
        [rowLabel],
    )

    const previewColumn = React.useMemo<ColumnDef<TData, unknown>>(
        () => ({
            id: "__preview",
            header: () => <span className="sr-only">打开预览</span>,
            cell: () => (
                <ChevronRightIcon
                    aria-hidden="true"
                    className="text-muted-foreground"
                />
            ),
            enableHiding: false,
            enableResizing: false,
            enableSorting: false,
            meta: {
                label: "预览",
                align: "end",
                role: "preview",
            },
        }),
        [],
    )

    const resolvedColumns = React.useMemo(() => {
        const withSelection = enableRowSelection
            ? [selectionColumn, ...columns]
            : columns
        return onRowPreview ? [...withSelection, previewColumn] : withSelection
    }, [
        columns,
        enableRowSelection,
        onRowPreview,
        previewColumn,
        selectionColumn,
    ])

    const resolvedColumnPinning = React.useMemo(() => {
        let next = columnPinning
        if (enableRowSelection) {
            const left = next.left ?? []
            if (!left.includes("__selection")) {
                next = { ...next, left: ["__selection", ...left] }
            }
        }
        if (!onRowPreview) return next
        const right = next.right ?? []
        if (right.includes("__preview")) return next
        return { ...next, right: [...right, "__preview"] }
    }, [columnPinning, enableRowSelection, onRowPreview])

    // TanStack Table 返回不稳定函数引用；React Compiler 只需跳过这个 hook。
    // eslint-disable-next-line react-hooks/incompatible-library
    const table = useReactTable({
        data,
        columns: resolvedColumns,
        getRowId,
        rowCount,
        state: {
            sorting,
            pagination,
            rowSelection,
            columnVisibility,
            columnOrder,
            columnPinning: resolvedColumnPinning,
            columnSizing,
            columnSizingInfo,
            columnFilters,
            globalFilter,
        },
        onSortingChange: setSorting,
        onPaginationChange: setPagination,
        onRowSelectionChange: setRowSelection,
        onColumnVisibilityChange: setColumnVisibility,
        onColumnOrderChange: setColumnOrder,
        onColumnPinningChange: setColumnPinning,
        onColumnSizingChange: setColumnSizing,
        onColumnSizingInfoChange: setColumnSizingInfo,
        onColumnFiltersChange: setColumnFilters,
        onGlobalFilterChange: setGlobalFilter,
        getCoreRowModel: getCoreRowModel(),
        getSortedRowModel: getSortedRowModel(),
        getFilteredRowModel: getFilteredRowModel(),
        getPaginationRowModel: getPaginationRowModel(),
        manualPagination,
        manualSorting,
        manualFiltering,
        enableRowSelection,
        enableMultiRowSelection,
        enableColumnResizing,
        enableColumnPinning,
        columnResizeMode: "onChange",
        autoResetPageIndex: false,
    })

    usePinnedColumnOffsets(table, tableSurfaceRef)
    const resize = useColumnResize<TData>(setColumnSizing, setColumnSizingInfo)
    const rows = table.getRowModel().rows
    const showErrorState =
        errorState !== undefined ||
        errorTitle !== undefined ||
        errorSummary !== undefined ||
        onRetry !== undefined

    const [frameViewOptionsSlot, setFrameViewOptionsSlot] =
        React.useState<Element | null>(null)
    React.useLayoutEffect(() => {
        if (layout !== "flush" || renderToolbar || !showColumnVisibility) {
            setFrameViewOptionsSlot(null)
            return
        }
        const frame = tableSurfaceRef.current?.closest(
            "[data-business-component=table-frame]",
        )
        setFrameViewOptionsSlot(
            frame?.querySelector("[data-slot=table-frame-view-options]") ??
                null,
        )
    }, [layout, renderToolbar, showColumnVisibility])

    const hostViewOptionsInFrame = Boolean(frameViewOptionsSlot)
    const showToolbarRow =
        Boolean(renderToolbar) ||
        (showColumnVisibility && !hostViewOptionsInFrame)
    const viewOptions = showColumnVisibility ? (
        <DataTableViewOptions table={table} />
    ) : null

    return (
        <section
            data-slot="data-table"
            data-layout={layout}
            className={cn(
                "[--row-hover:oklch(0.98_0_0)] dark:[--row-hover:oklch(0.245_0_0)]",
                // inset：外层统一内边距。
                // flush：表格独立圆角描边；分页在表外。结果卡片（showHeader）
                // 会把 gap 收成 0，让分页贴成卡内页脚。
                layout === "inset" && "flex flex-col gap-3 p-table-frame-inset",
                layout === "flush" && "flex flex-col gap-3",
                className,
            )}
            aria-busy={loading}
        >
            {hostViewOptionsInFrame && viewOptions && frameViewOptionsSlot
                ? createPortal(viewOptions, frameViewOptionsSlot)
                : null}
            {showToolbarRow ? (
                <div
                    className={cn(
                        "flex flex-wrap items-center justify-between gap-2",
                    )}
                >
                    <div className="min-w-0 flex-1">
                        {renderToolbar?.(table)}
                    </div>
                    {!hostViewOptionsInFrame ? viewOptions : null}
                </div>
            ) : null}

            <div
                ref={tableSurfaceRef}
                data-slot="data-table-surface"
                className="overflow-hidden rounded-lg border bg-card"
            >
                {loading && data.length > 0 && showRefreshingBanner ? (
                    <div
                        role="status"
                        className="flex items-center gap-2 border-b border-grid bg-surface-sunken px-3 py-2 text-xs text-muted-foreground"
                    >
                        <Spinner />
                        {refreshingLabel}
                    </div>
                ) : null}

                <Table
                    data-density={density}
                    data-striped={striped ? "true" : undefined}
                    data-placeholder={
                        showErrorState || (!loading && rows.length === 0)
                            ? ""
                            : undefined
                    }
                >
                    <TableCaption className="sr-only">{caption}</TableCaption>
                    <DataTableHeader
                        table={table}
                        columnSizing={columnSizing}
                        columnSizingInfo={columnSizingInfo}
                        enableColumnResizing={enableColumnResizing}
                        sortingInteractive={sortingInteractive}
                        resize={resize}
                    />
                    <DataTableBody
                        table={table}
                        rowRefs={rowRefs}
                        rowLabel={rowLabel}
                        dataLength={data.length}
                        loading={loading}
                        showErrorState={showErrorState}
                        errorState={errorState}
                        errorTitle={errorTitle}
                        errorSummary={errorSummary}
                        onRetry={onRetry}
                        emptyState={emptyState}
                        emptyTitle={emptyTitle}
                        emptyDescription={emptyDescription}
                        emptyAction={emptyAction}
                        onRowPreview={onRowPreview}
                        onRowOpen={onRowOpen}
                        enableRowSelection={enableRowSelection}
                        highlightedRowId={highlightedRowId}
                        enableColumnResizing={enableColumnResizing}
                        columnSizing={columnSizing}
                    />
                </Table>
            </div>

            {showPagination ? (
                <DataTablePagination
                    table={table}
                    pageSizeOptions={pageSizeOptions}
                    layout={layout}
                />
            ) : null}
        </section>
    )
}

export {
    DataTable,
    DataTablePagination,
    DataTableViewOptions,
    type DataTableAlignment,
    type DataTableColumnWidth,
    type DataTableLayout,
    type DataTableProps,
}

export type {
    ColumnDef,
    ColumnFiltersState,
    ColumnOrderState,
    ColumnPinningState,
    ColumnSizingInfoState,
    ColumnSizingState,
    PaginationState,
    RowSelectionState,
    SortingState,
    VisibilityState,
}
