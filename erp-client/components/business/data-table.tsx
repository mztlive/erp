"use client"

import * as React from "react"
import { createPortal } from "react-dom"
import {
    ArrowDownIcon,
    ArrowUpDownIcon,
    ArrowUpIcon,
    ChevronRightIcon,
} from "lucide-react"
import {
    flexRender,
    getCoreRowModel,
    getFilteredRowModel,
    getPaginationRowModel,
    getSortedRowModel,
    useReactTable,
    type Column,
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

import { BusinessFailureState } from "@/components/business/feedback"
import {
    DataTablePagination,
    DataTableViewOptions,
} from "@/components/business/data-table-controls"
import type { DataTableProps } from "@/components/business/data-table-contracts"
import {
    alignmentClass,
    columnRuntimeWidth,
    columnWidthClass,
    pinningClass,
    sortableHeaderClass,
} from "@/components/business/data-table-layout"
import type {
    DataTableAlignment,
    DataTableColumnWidth,
    DataTableLayout,
} from "@/components/business/data-table-layout"
import { useControlledTableState } from "@/components/business/data-table-state"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Skeleton } from "@/components/ui/skeleton"
import { Spinner } from "@/components/ui/spinner"
import {
    Table,
    TableBody,
    TableCaption,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { cn } from "@/lib/utils"

const emptyColumnSizingInfo: ColumnSizingInfoState = {
    columnSizingStart: [],
    deltaOffset: null,
    deltaPercentage: null,
    isResizingColumn: false,
    startOffset: null,
    startSize: null,
}

type ColumnResizeSession = {
    pointerId: number
    columnId: string
    startOffset: number
    startSize: number
    minSize: number
    maxSize?: number
    direction: 1 | -1
}

const rowInteractiveTargetSelector = [
    "button",
    "a",
    "input",
    "select",
    "textarea",
    '[contenteditable]:not([contenteditable="false"])',
    '[role="button"]',
    '[role="checkbox"]',
    '[role="combobox"]',
    '[role="link"]',
    '[role="menuitem"]',
    '[role="option"]',
    '[role="radio"]',
    '[role="slider"]',
    '[role="spinbutton"]',
    '[role="switch"]',
    '[role="tab"]',
    '[role="textbox"]',
].join(",")

function isInteractiveRowTarget(target: EventTarget | null) {
    return (
        target instanceof Element &&
        target.closest(rowInteractiveTargetSelector) !== null
    )
}

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
    const resizeSessionRef = React.useRef<ColumnResizeSession | null>(null)

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
                <div
                    className="flex items-center justify-center"
                    onClick={(event) => event.stopPropagation()}
                >
                    <Checkbox
                        checked={table.getIsAllPageRowsSelected()}
                        indeterminate={table.getIsSomePageRowsSelected()}
                        onCheckedChange={(checked) =>
                            table.toggleAllPageRowsSelected(checked === true)
                        }
                        aria-label="选择当前页全部记录"
                    />
                </div>
            ),
            cell: ({ row }) => (
                <div
                    className="flex items-center justify-center"
                    onClick={(event) => event.stopPropagation()}
                >
                    <Checkbox
                        checked={row.getIsSelected()}
                        indeterminate={row.getIsSomeSelected()}
                        disabled={!row.getCanSelect()}
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

    const visibleColumnSignature = table
        .getVisibleLeafColumns()
        .map((column) => column.id)
        .join("|")
    const pinnedColumnSignature = [
        table
            .getLeftLeafColumns()
            .filter((column) => column.getIsVisible())
            .map((column) => column.id)
            .join("|"),
        table
            .getRightLeafColumns()
            .filter((column) => column.getIsVisible())
            .map((column) => column.id)
            .join("|"),
    ].join("::")

    // HTML table 会按容器和内容分配剩余空间。固定列偏移必须读取真实列宽，
    // 不能使用 TanStack 的逻辑默认宽度，否则窄屏和缩放下会产生间隙或覆盖。
    React.useLayoutEffect(() => {
        const surface = tableSurfaceRef.current
        if (!surface) return

        const observedHeaders = new Set<Element>()
        const resizeObserver = new ResizeObserver(() =>
            syncPinnedColumnOffsets(),
        )

        function observeLeafHeaders() {
            surface
                ?.querySelectorAll<HTMLElement>(
                    '[data-column-leaf="true"][data-column-id]',
                )
                .forEach((header) => {
                    if (observedHeaders.has(header)) return
                    observedHeaders.add(header)
                    resizeObserver.observe(header)
                })
        }

        function syncPinnedColumnOffsets() {
            if (!surface) return

            const widths = new Map<string, number>()
            surface
                .querySelectorAll<HTMLElement>(
                    '[data-column-leaf="true"][data-column-id]',
                )
                .forEach((header) => {
                    const columnId = header.dataset.columnId
                    if (columnId)
                        widths.set(
                            columnId,
                            header.getBoundingClientRect().width,
                        )
                })

            const positions = new Map<
                string,
                { side: "left" | "right"; offset: number }
            >()
            let leftOffset = 0
            table
                .getLeftLeafColumns()
                .filter((column) => column.getIsVisible())
                .forEach((column) => {
                    positions.set(column.id, {
                        side: "left",
                        offset: leftOffset,
                    })
                    leftOffset += widths.get(column.id) ?? 0
                })

            let rightOffset = 0
            table
                .getRightLeafColumns()
                .filter((column) => column.getIsVisible())
                .reverse()
                .forEach((column) => {
                    positions.set(column.id, {
                        side: "right",
                        offset: rightOffset,
                    })
                    rightOffset += widths.get(column.id) ?? 0
                })

            surface
                .querySelectorAll<HTMLElement>("[data-column-id]")
                .forEach((element) => {
                    const columnId = element.dataset.columnId
                    const position = columnId
                        ? positions.get(columnId)
                        : undefined
                    element.style.left =
                        position?.side === "left" ? `${position.offset}px` : ""
                    element.style.right =
                        position?.side === "right" ? `${position.offset}px` : ""
                })
        }

        const mutationObserver = new MutationObserver(() => {
            observeLeafHeaders()
            syncPinnedColumnOffsets()
        })

        observeLeafHeaders()
        resizeObserver.observe(surface)
        mutationObserver.observe(surface, { childList: true, subtree: true })
        syncPinnedColumnOffsets()

        return () => {
            resizeObserver.disconnect()
            mutationObserver.disconnect()
        }
    }, [pinnedColumnSignature, table, visibleColumnSignature])

    const beginColumnResize = (
        event: React.PointerEvent<HTMLDivElement>,
        column: Column<TData>,
    ) => {
        if (event.button !== 0) return

        const headerCell = event.currentTarget.closest("th")
        if (!headerCell) return

        event.preventDefault()
        event.stopPropagation()
        event.currentTarget.setPointerCapture(event.pointerId)

        const startSize = headerCell.getBoundingClientRect().width
        const computedStyle = window.getComputedStyle(headerCell)
        const parsedMinSize = Number.parseFloat(computedStyle.minWidth)
        const parsedMaxSize = Number.parseFloat(computedStyle.maxWidth)
        const direction = computedStyle.direction === "rtl" ? -1 : 1

        resizeSessionRef.current = {
            pointerId: event.pointerId,
            columnId: column.id,
            startOffset: event.clientX,
            startSize,
            minSize: Number.isFinite(parsedMinSize) ? parsedMinSize : 0,
            maxSize: Number.isFinite(parsedMaxSize) ? parsedMaxSize : undefined,
            direction,
        }
        setColumnSizingInfo({
            columnSizingStart: [[column.id, startSize]],
            deltaOffset: 0,
            deltaPercentage: 0,
            isResizingColumn: column.id,
            startOffset: event.clientX,
            startSize,
        })
    }

    const updateColumnResize = (event: React.PointerEvent<HTMLDivElement>) => {
        const session = resizeSessionRef.current
        if (!session || session.pointerId !== event.pointerId) return

        const deltaOffset =
            (event.clientX - session.startOffset) * session.direction
        const unconstrainedSize = session.startSize + deltaOffset
        const nextSize = Math.max(
            session.minSize,
            session.maxSize === undefined
                ? unconstrainedSize
                : Math.min(session.maxSize, unconstrainedSize),
        )

        setColumnSizing((current) => ({
            ...current,
            [session.columnId]: nextSize,
        }))
        setColumnSizingInfo({
            columnSizingStart: [[session.columnId, session.startSize]],
            deltaOffset,
            deltaPercentage:
                session.startSize === 0 ? 0 : deltaOffset / session.startSize,
            isResizingColumn: session.columnId,
            startOffset: session.startOffset,
            startSize: session.startSize,
        })
    }

    const endColumnResize = (event: React.PointerEvent<HTMLDivElement>) => {
        const session = resizeSessionRef.current
        if (!session || session.pointerId !== event.pointerId) return

        resizeSessionRef.current = null
        setColumnSizingInfo(emptyColumnSizingInfo)
    }

    const rows = table.getRowModel().rows
    const visibleColumnCount = table.getVisibleLeafColumns().length
    const interactive = Boolean(onRowPreview || onRowOpen || enableRowSelection)
    const showErrorState =
        errorState !== undefined ||
        errorTitle !== undefined ||
        errorSummary !== undefined ||
        onRetry !== undefined

    const focusRelativeRow = (rowIndex: number, offset: number) => {
        const target = rows[rowIndex + offset]
        if (target) rowRefs.current.get(target.id)?.focus()
    }

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
                    <TableHeader>
                        {table.getHeaderGroups().map((headerGroup) => (
                            <TableRow key={headerGroup.id}>
                                {headerGroup.headers.map((header) => {
                                    const sort = header.column.getIsSorted()
                                    const meta = header.column.columnDef.meta
                                    const runtimeWidth =
                                        columnSizing[header.column.id]
                                    const SortIcon =
                                        sort === "asc"
                                            ? ArrowUpIcon
                                            : sort === "desc"
                                              ? ArrowDownIcon
                                              : ArrowUpDownIcon

                                    return (
                                        <TableHead
                                            key={header.id}
                                            colSpan={header.colSpan}
                                            data-column-id={header.column.id}
                                            data-column-leaf={
                                                header.subHeaders.length === 0
                                                    ? "true"
                                                    : undefined
                                            }
                                            data-column-pinned={
                                                header.column.getIsPinned() ||
                                                undefined
                                            }
                                            data-align={
                                                meta?.align === "end"
                                                    ? "end"
                                                    : undefined
                                            }
                                            className={cn(
                                                "relative",
                                                header.column.id ===
                                                    "actions" &&
                                                    "max-sm:hidden",
                                                alignmentClass(meta?.align),
                                                columnWidthClass(
                                                    meta?.width,
                                                    meta?.role,
                                                ),
                                                pinningClass(
                                                    header.column.getIsPinned(),
                                                    "header",
                                                ),
                                            )}
                                            style={{
                                                width: columnRuntimeWidth(
                                                    enableColumnResizing,
                                                    meta?.role,
                                                    runtimeWidth,
                                                ),
                                            }}
                                            aria-sort={
                                                sort === "asc"
                                                    ? "ascending"
                                                    : sort === "desc"
                                                      ? "descending"
                                                      : sortingInteractive &&
                                                          header.column.getCanSort()
                                                        ? "none"
                                                        : undefined
                                            }
                                        >
                                            {header.isPlaceholder ? null : sortingInteractive &&
                                              header.column.getCanSort() ? (
                                                <Button
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    className={cn(
                                                        // TableHead 本身是 text-xs；size="xs" 也是 text-xs，
                                                        // 不要再抬成 text-sm，否则排序列会比其它列头大一号。
                                                        "w-full px-0 text-xs font-medium text-muted-foreground hover:text-foreground",
                                                        sortableHeaderClass(
                                                            meta?.align,
                                                        ),
                                                    )}
                                                    onClick={header.column.getToggleSortingHandler()}
                                                    aria-label={`按${meta?.label ?? header.column.id}排序`}
                                                >
                                                    {flexRender(
                                                        header.column.columnDef
                                                            .header,
                                                        header.getContext(),
                                                    )}
                                                    <SortIcon aria-hidden="true" />
                                                </Button>
                                            ) : (
                                                flexRender(
                                                    header.column.columnDef
                                                        .header,
                                                    header.getContext(),
                                                )
                                            )}

                                            {header.column.getCanResize() ? (
                                                <div
                                                    role="separator"
                                                    aria-label={`调整${meta?.label ?? header.column.id}列宽`}
                                                    aria-orientation="vertical"
                                                    onDoubleClick={(event) => {
                                                        event.stopPropagation()
                                                        header.column.resetSize()
                                                    }}
                                                    onPointerDown={(event) =>
                                                        beginColumnResize(
                                                            event,
                                                            header.column,
                                                        )
                                                    }
                                                    onPointerMove={
                                                        updateColumnResize
                                                    }
                                                    onPointerUp={
                                                        endColumnResize
                                                    }
                                                    onPointerCancel={
                                                        endColumnResize
                                                    }
                                                    onLostPointerCapture={
                                                        endColumnResize
                                                    }
                                                    className={cn(
                                                        "absolute inset-y-0 right-0 z-20 w-table-resize-target cursor-col-resize touch-none select-none after:absolute after:inset-y-0 after:right-0 after:w-px after:bg-transparent hover:after:bg-ring",
                                                        columnSizingInfo.isResizingColumn ===
                                                            header.column.id &&
                                                            "after:bg-ring",
                                                    )}
                                                />
                                            ) : null}
                                        </TableHead>
                                    )
                                })}
                            </TableRow>
                        ))}
                    </TableHeader>

                    <TableBody>
                        {showErrorState ? (
                            <TableRow>
                                <TableCell
                                    colSpan={Math.max(visibleColumnCount, 1)}
                                    className="h-auto p-4 sm:p-6"
                                >
                                    {errorState ?? (
                                        <BusinessFailureState
                                            kind="system"
                                            title={errorTitle ?? "数据加载失败"}
                                            description={errorSummary}
                                            onRetry={onRetry}
                                        />
                                    )}
                                </TableCell>
                            </TableRow>
                        ) : loading && data.length === 0 ? (
                            Array.from({ length: 5 }, (_, rowIndex) => (
                                <TableRow key={`loading-${rowIndex}`}>
                                    {table
                                        .getVisibleLeafColumns()
                                        .map((column) => (
                                            <TableCell key={column.id}>
                                                <Skeleton className="h-4 w-full" />
                                            </TableCell>
                                        ))}
                                </TableRow>
                            ))
                        ) : (
                            rows.map((row, rowIndex) => (
                                <TableRow
                                    key={row.id}
                                    data-row-id={row.id}
                                    ref={(node) => {
                                        if (node)
                                            rowRefs.current.set(row.id, node)
                                        else rowRefs.current.delete(row.id)
                                    }}
                                    data-state={
                                        row.getIsSelected()
                                            ? "selected"
                                            : undefined
                                    }
                                    data-highlighted={
                                        highlightedRowId === row.id
                                            ? "true"
                                            : undefined
                                    }
                                    tabIndex={interactive ? 0 : undefined}
                                    className={cn(
                                        interactive && "cursor-pointer",
                                        highlightedRowId === row.id &&
                                            "bg-muted",
                                    )}
                                    onClick={(event) => {
                                        if (
                                            isInteractiveRowTarget(event.target)
                                        )
                                            return
                                        // 单击行：优先快速预览；未接预览时回落到详情入口，
                                        // 避免触屏用户（无键盘 Enter）对只有 onRowOpen 的页面无从打开。
                                        ;(onRowPreview ?? onRowOpen)?.(
                                            row.original,
                                        )
                                    }}
                                    onKeyDown={(event) => {
                                        if (
                                            isInteractiveRowTarget(event.target)
                                        )
                                            return

                                        if (event.key === "ArrowDown") {
                                            event.preventDefault()
                                            focusRelativeRow(rowIndex, 1)
                                        } else if (event.key === "ArrowUp") {
                                            event.preventDefault()
                                            focusRelativeRow(rowIndex, -1)
                                        } else if (
                                            event.key === "Enter" &&
                                            (onRowOpen || onRowPreview)
                                        ) {
                                            event.preventDefault()
                                            const rowAction =
                                                onRowOpen ?? onRowPreview
                                            rowAction?.(row.original)
                                        } else if (
                                            event.key === " " &&
                                            row.getCanSelect()
                                        ) {
                                            event.preventDefault()
                                            row.toggleSelected()
                                        }
                                    }}
                                    aria-label={
                                        rowLabel?.(row.original) ??
                                        `第 ${rowIndex + 1} 行`
                                    }
                                >
                                    {row.getVisibleCells().map((cell) => {
                                        const meta = cell.column.columnDef.meta
                                        const runtimeWidth =
                                            columnSizing[cell.column.id]

                                        return (
                                            <TableCell
                                                key={cell.id}
                                                data-column-id={cell.column.id}
                                                data-column-pinned={
                                                    cell.column.getIsPinned() ||
                                                    undefined
                                                }
                                                data-align={
                                                    meta?.align === "end"
                                                        ? "end"
                                                        : undefined
                                                }
                                                className={cn(
                                                    alignmentClass(meta?.align),
                                                    cell.column.id ===
                                                        "actions" &&
                                                        "max-sm:hidden",
                                                    // align="end" 已由 [data-align="end"] 统一带上 num，这里只处理非 end 的等宽数字列。
                                                    meta?.numeric &&
                                                        meta?.align !== "end" &&
                                                        "num",
                                                    columnWidthClass(
                                                        meta?.width,
                                                        meta?.role,
                                                    ),
                                                    pinningClass(
                                                        cell.column.getIsPinned(),
                                                        "cell",
                                                    ),
                                                )}
                                                style={{
                                                    width: columnRuntimeWidth(
                                                        enableColumnResizing,
                                                        meta?.role,
                                                        runtimeWidth,
                                                    ),
                                                }}
                                            >
                                                {flexRender(
                                                    cell.column.columnDef.cell,
                                                    cell.getContext(),
                                                )}
                                            </TableCell>
                                        )
                                    })}
                                </TableRow>
                            ))
                        )}

                        {!showErrorState && !loading && rows.length === 0 ? (
                            <TableRow>
                                <TableCell
                                    colSpan={Math.max(visibleColumnCount, 1)}
                                    className="h-auto p-6"
                                >
                                    {emptyState ?? (
                                        <div className="flex flex-col items-center gap-2 py-2 text-center">
                                            <p className="text-sm text-muted-foreground">
                                                {emptyTitle ??
                                                    "当前筛选没有结果"}
                                            </p>
                                            {emptyDescription != null ? (
                                                <p className="text-xs text-muted-foreground">
                                                    {emptyDescription}
                                                </p>
                                            ) : null}
                                            {emptyAction != null ? (
                                                <div className="flex flex-wrap justify-center gap-2 pt-1">
                                                    {emptyAction}
                                                </div>
                                            ) : null}
                                        </div>
                                    )}
                                </TableCell>
                            </TableRow>
                        ) : null}
                    </TableBody>
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
