"use client"

import * as React from "react"
import {
  ArrowDownIcon,
  ArrowUpDownIcon,
  ArrowUpIcon,
  ChevronsLeftIcon,
  ChevronsRightIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  Columns3Icon,
  PanelLeftIcon,
  PanelRightIcon,
  PinOffIcon,
} from "lucide-react"
import {
  flexRender,
  functionalUpdate,
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
  type OnChangeFn,
  type PaginationState,
  type Row,
  type RowData,
  type RowSelectionState,
  type SortingState,
  type Table as TanStackTable,
  type Updater,
  type VisibilityState,
} from "@tanstack/react-table"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
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

type DataTableAlignment = "start" | "center" | "end"
type DataTableColumnWidth =
  "default" | "reference" | "status" | "amount" | "quantity" | "rate" | "tracks"
type DataTableLayout = "inset" | "flush"

const dataTableColumnWidthClasses: Record<DataTableColumnWidth, string> = {
  default: "w-table-column-default min-w-table-column-default-min",
  reference: "w-table-column-reference min-w-table-column-reference-min",
  status: "w-table-column-status min-w-table-column-status-min",
  amount: "w-table-column-amount min-w-table-column-amount-min",
  quantity: "w-table-column-quantity min-w-table-column-quantity-min",
  rate: "w-table-column-rate min-w-table-column-rate-min",
  tracks: "w-table-column-tracks min-w-table-column-tracks-min",
}

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
    role?: "selection"
  }
}

type ControlledTableStateProps<T> = {
  value?: T
  defaultValue: T
  onChange?: (value: T) => void
}

function useControlledTableState<T>({
  value,
  defaultValue,
  onChange,
}: ControlledTableStateProps<T>): [T, OnChangeFn<T>] {
  const [internalValue, setInternalValue] = React.useState(defaultValue)
  const currentValue = value ?? internalValue

  const handleChange = React.useCallback(
    (updater: Updater<T>) => {
      const nextValue = functionalUpdate(updater, currentValue)
      if (value === undefined) setInternalValue(nextValue)
      onChange?.(nextValue)
    },
    [currentValue, onChange, value]
  )

  return [currentValue, handleChange]
}

type DataTableProps<TData> = {
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
  enableColumnResizing?: boolean
  enableColumnPinning?: boolean
  /** inset 自带卡片内距与圆角边框；flush 由外部框架提供边界。 */
  layout?: DataTableLayout
  density?: "compact" | "comfortable"
  striped?: boolean
  loading?: boolean
  emptyState?: React.ReactNode
  renderToolbar?: (table: TanStackTable<TData>) => React.ReactNode
  showColumnVisibility?: boolean
  showPagination?: boolean
  pageSizeOptions?: readonly number[]
  /** 单击非交互区域时打开快速预览。 */
  onRowPreview?: (row: TData) => void
  /** 聚焦行后按 Enter 优先打开详情；鼠标入口应由业务列提供明确按钮或链接。 */
  onRowOpen?: (row: TData) => void
  className?: string
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
  enableColumnResizing = true,
  enableColumnPinning = true,
  layout = "flush",
  density = "compact",
  striped = false,
  loading = false,
  emptyState,
  renderToolbar,
  showColumnVisibility = true,
  showPagination = true,
  pageSizeOptions = [20, 50, 100],
  onRowPreview,
  onRowOpen,
  className,
}: DataTableProps<TData>) {
  const rowRefs = React.useRef(new Map<string, HTMLTableRowElement>())
  const tableSurfaceRef = React.useRef<HTMLDivElement>(null)
  const resizeSessionRef = React.useRef<ColumnResizeSession | null>(null)

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
            onCheckedChange={(checked) => row.toggleSelected(checked === true)}
            aria-label={`选择 ${rowLabel?.(row.original) ?? row.id}`}
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
    [rowLabel]
  )

  const resolvedColumns = React.useMemo(
    () => (enableRowSelection ? [selectionColumn, ...columns] : columns),
    [columns, enableRowSelection, selectionColumn]
  )

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
      columnPinning,
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
    const resizeObserver = new ResizeObserver(() => syncPinnedColumnOffsets())

    function observeLeafHeaders() {
      surface
        ?.querySelectorAll<HTMLElement>(
          '[data-column-leaf="true"][data-column-id]'
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
          '[data-column-leaf="true"][data-column-id]'
        )
        .forEach((header) => {
          const columnId = header.dataset.columnId
          if (columnId)
            widths.set(columnId, header.getBoundingClientRect().width)
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
          positions.set(column.id, { side: "left", offset: leftOffset })
          leftOffset += widths.get(column.id) ?? 0
        })

      let rightOffset = 0
      table
        .getRightLeafColumns()
        .filter((column) => column.getIsVisible())
        .reverse()
        .forEach((column) => {
          positions.set(column.id, { side: "right", offset: rightOffset })
          rightOffset += widths.get(column.id) ?? 0
        })

      surface
        .querySelectorAll<HTMLElement>("[data-column-id]")
        .forEach((element) => {
          const columnId = element.dataset.columnId
          const position = columnId ? positions.get(columnId) : undefined
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
    column: Column<TData>
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
        : Math.min(session.maxSize, unconstrainedSize)
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

  const focusRelativeRow = (rowIndex: number, offset: number) => {
    const target = rows[rowIndex + offset]
    if (target) rowRefs.current.get(target.id)?.focus()
  }

  return (
    <section
      data-slot="data-table"
      data-layout={layout}
      className={cn(
        "space-y-3",
        layout === "inset" && "p-table-frame-inset",
        className
      )}
      aria-busy={loading}
    >
      {renderToolbar || showColumnVisibility ? (
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="min-w-0 flex-1">{renderToolbar?.(table)}</div>
          {showColumnVisibility ? <DataTableViewOptions table={table} /> : null}
        </div>
      ) : null}

      <div
        ref={tableSurfaceRef}
        className={cn(
          "overflow-x-auto bg-card",
          layout === "inset" ? "rounded-lg border" : "border-y"
        )}
      >
        {loading && data.length > 0 ? (
          <div
            role="status"
            className="flex items-center gap-2 border-b bg-surface-sunken px-3 py-2 text-xs text-muted-foreground"
          >
            <Spinner />
            正在刷新，当前内容会保留
          </div>
        ) : null}

        <Table
          data-density={density}
          data-striped={striped ? "true" : undefined}
        >
          <TableCaption className="sr-only">{caption}</TableCaption>
          <TableHeader>
            {table.getHeaderGroups().map((headerGroup) => (
              <TableRow key={headerGroup.id}>
                {headerGroup.headers.map((header) => {
                  const sort = header.column.getIsSorted()
                  const meta = header.column.columnDef.meta
                  const runtimeWidth = columnSizing[header.column.id]
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
                        header.subHeaders.length === 0 ? "true" : undefined
                      }
                      data-column-pinned={
                        header.column.getIsPinned() || undefined
                      }
                      data-align={meta?.align === "end" ? "end" : undefined}
                      className={cn(
                        "relative",
                        header.column.id === "actions" && "max-sm:hidden",
                        alignmentClass(meta?.align),
                        columnWidthClass(meta?.width, meta?.role),
                        pinningClass(header.column.getIsPinned(), "header")
                      )}
                      style={{
                        width:
                          enableColumnResizing && !meta?.role
                            ? runtimeWidth
                            : undefined,
                      }}
                      aria-sort={
                        sort === "asc"
                          ? "ascending"
                          : sort === "desc"
                            ? "descending"
                            : header.column.getCanSort()
                              ? "none"
                              : undefined
                      }
                    >
                      {header.isPlaceholder ? null : header.column.getCanSort() ? (
                        <Button
                          type="button"
                          variant="ghost"
                          size="xs"
                          className={cn(
                            "w-full px-0",
                            sortableHeaderClass(meta?.align)
                          )}
                          onClick={header.column.getToggleSortingHandler()}
                          aria-label={`按${meta?.label ?? header.column.id}排序`}
                        >
                          {flexRender(
                            header.column.columnDef.header,
                            header.getContext()
                          )}
                          <SortIcon aria-hidden="true" />
                        </Button>
                      ) : (
                        flexRender(
                          header.column.columnDef.header,
                          header.getContext()
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
                            beginColumnResize(event, header.column)
                          }
                          onPointerMove={updateColumnResize}
                          onPointerUp={endColumnResize}
                          onPointerCancel={endColumnResize}
                          onLostPointerCapture={endColumnResize}
                          className={cn(
                            "absolute inset-y-0 right-0 z-20 w-table-resize-target cursor-col-resize touch-none select-none after:absolute after:inset-y-0 after:right-0 after:w-px after:bg-transparent hover:after:bg-ring",
                            columnSizingInfo.isResizingColumn ===
                              header.column.id && "after:bg-ring"
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
            {loading && data.length === 0
              ? Array.from({ length: 5 }, (_, rowIndex) => (
                  <TableRow key={`loading-${rowIndex}`}>
                    {table.getVisibleLeafColumns().map((column) => (
                      <TableCell key={column.id}>
                        <Skeleton className="h-4 w-full" />
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              : rows.map((row, rowIndex) => (
                  <TableRow
                    key={row.id}
                    ref={(node) => {
                      if (node) rowRefs.current.set(row.id, node)
                      else rowRefs.current.delete(row.id)
                    }}
                    data-state={row.getIsSelected() ? "selected" : undefined}
                    tabIndex={interactive ? 0 : undefined}
                    className={interactive ? "cursor-default" : undefined}
                    onClick={(event) => {
                      if (isInteractiveRowTarget(event.target)) return
                      onRowPreview?.(row.original)
                    }}
                    onKeyDown={(event) => {
                      if (isInteractiveRowTarget(event.target)) return

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
                        const rowAction = onRowOpen ?? onRowPreview
                        rowAction?.(row.original)
                      } else if (event.key === " " && row.getCanSelect()) {
                        event.preventDefault()
                        row.toggleSelected()
                      }
                    }}
                    aria-label={rowLabel?.(row.original) ?? row.id}
                  >
                    {row.getVisibleCells().map((cell) => {
                      const meta = cell.column.columnDef.meta
                      const runtimeWidth = columnSizing[cell.column.id]

                      return (
                        <TableCell
                          key={cell.id}
                          data-column-id={cell.column.id}
                          data-column-pinned={
                            cell.column.getIsPinned() || undefined
                          }
                          data-align={meta?.align === "end" ? "end" : undefined}
                          className={cn(
                            alignmentClass(meta?.align),
                            cell.column.id === "actions" && "max-sm:hidden",
                            meta?.numeric && "num",
                            columnWidthClass(meta?.width, meta?.role),
                            pinningClass(cell.column.getIsPinned(), "cell")
                          )}
                          style={{
                            width:
                              enableColumnResizing && !meta?.role
                                ? runtimeWidth
                                : undefined,
                          }}
                        >
                          {flexRender(
                            cell.column.columnDef.cell,
                            cell.getContext()
                          )}
                        </TableCell>
                      )
                    })}
                  </TableRow>
                ))}

            {!loading && rows.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={Math.max(visibleColumnCount, 1)}
                  className="h-auto p-6"
                >
                  {emptyState ?? (
                    <p className="text-center text-sm text-muted-foreground">
                      当前筛选没有结果
                    </p>
                  )}
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </div>

      {showPagination ? (
        <DataTablePagination table={table} pageSizeOptions={pageSizeOptions} />
      ) : null}
    </section>
  )
}

function DataTableViewOptions<TData>({
  table,
}: {
  table: TanStackTable<TData>
}) {
  const columns = table
    .getAllLeafColumns()
    .filter((column) => column.getCanHide())
  const visibleColumnCount = columns.filter((column) =>
    column.getIsVisible()
  ).length

  if (columns.length === 0) return null

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="max-sm:hidden"
          />
        }
      >
        <Columns3Icon data-icon="inline-start" aria-hidden="true" />
        列设置
      </PopoverTrigger>
      <PopoverContent align="end" className="w-auto min-w-72">
        <div className="space-y-3">
          <div>
            <div className="text-sm font-medium">列设置</div>
            <p className="text-xs text-muted-foreground">
              调整顺序、显隐和固定位置
            </p>
          </div>
          <div className="space-y-1">
            {columns.map((column, index) => {
              const label = column.columnDef.meta?.label ?? column.id
              const pinned = column.getIsPinned()

              return (
                <div
                  key={column.id}
                  className="flex items-center gap-1 rounded-lg px-1 py-1 hover:bg-accent"
                >
                  <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 text-sm">
                    <Checkbox
                      checked={column.getIsVisible()}
                      disabled={
                        column.getIsVisible() && visibleColumnCount === 1
                      }
                      onCheckedChange={(checked) =>
                        column.toggleVisibility(checked === true)
                      }
                    />
                    <span className="truncate">{label}</span>
                  </label>
                  <div
                    role="group"
                    aria-label={`${label}列设置`}
                    className="flex"
                  >
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-xs"
                      onClick={() => moveColumn(table, columns, index, -1)}
                      disabled={index === 0}
                      aria-label={`前移${label}列`}
                    >
                      <ArrowUpIcon aria-hidden="true" />
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-xs"
                      onClick={() => moveColumn(table, columns, index, 1)}
                      disabled={index === columns.length - 1}
                      aria-label={`后移${label}列`}
                    >
                      <ArrowDownIcon aria-hidden="true" />
                    </Button>
                    {column.getCanPin() ? (
                      <>
                        <Button
                          type="button"
                          variant={pinned === "left" ? "secondary" : "ghost"}
                          size="icon-xs"
                          onClick={() => column.pin("left")}
                          disabled={!column.getIsVisible()}
                          aria-label={`固定${label}列到左侧`}
                        >
                          <PanelLeftIcon aria-hidden="true" />
                        </Button>
                        <Button
                          type="button"
                          variant={pinned === "right" ? "secondary" : "ghost"}
                          size="icon-xs"
                          onClick={() => column.pin("right")}
                          disabled={!column.getIsVisible()}
                          aria-label={`固定${label}列到右侧`}
                        >
                          <PanelRightIcon aria-hidden="true" />
                        </Button>
                        {pinned ? (
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            onClick={() => column.pin(false)}
                            aria-label={`取消固定${label}列`}
                          >
                            <PinOffIcon aria-hidden="true" />
                          </Button>
                        ) : null}
                      </>
                    ) : null}
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}

function moveColumn<TData>(
  table: TanStackTable<TData>,
  configurableColumns: ReturnType<TanStackTable<TData>["getAllLeafColumns"]>,
  index: number,
  offset: -1 | 1
) {
  const targetIndex = index + offset
  if (targetIndex < 0 || targetIndex >= configurableColumns.length) return

  const reorderedIds = configurableColumns.map((column) => column.id)
  const [movedId] = reorderedIds.splice(index, 1)
  reorderedIds.splice(targetIndex, 0, movedId)

  const nextMovableIds = [...reorderedIds]
  table.setColumnOrder(
    table
      .getAllLeafColumns()
      .map((column) =>
        column.getCanHide() ? (nextMovableIds.shift() ?? column.id) : column.id
      )
  )
}

function DataTablePagination<TData>({
  table,
  pageSizeOptions,
}: {
  table: TanStackTable<TData>
  pageSizeOptions: readonly number[]
}) {
  const { pageIndex, pageSize } = table.getState().pagination
  const pageCount = table.getPageCount()
  const rowCount = table.getRowCount()
  const selectedCount = Object.keys(table.getState().rowSelection).length

  return (
    <div
      data-slot="data-table-pagination"
      className="grid gap-3 text-sm sm:flex sm:items-center sm:gap-2"
    >
      <div className="flex min-w-0 items-center justify-between gap-3 sm:contents">
        <div className="min-w-0 text-muted-foreground sm:mr-auto">
          <span className="num">共 {rowCount.toLocaleString("zh-CN")} 条</span>
          {selectedCount > 0 ? (
            <span className="num">
              ，已选择 {selectedCount.toLocaleString("zh-CN")} 条
            </span>
          ) : null}
        </div>

        <label className="flex shrink-0 items-center gap-2 text-muted-foreground">
          每页
          <NativeSelect
            size="sm"
            value={String(pageSize)}
            onChange={(event) => table.setPageSize(Number(event.target.value))}
            aria-label="每页记录数"
          >
            {pageSizeOptions.map((size) => (
              <NativeSelectOption key={size} value={size}>
                {size}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </label>
      </div>

      <div className="flex items-center justify-between gap-3 sm:contents">
        <span className="num min-w-24 text-center text-muted-foreground">
          第 {pageCount === 0 ? 0 : pageIndex + 1} / {pageCount} 页
        </span>
        <div role="group" aria-label="翻页" className="flex items-center gap-1">
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            onClick={() => table.firstPage()}
            disabled={!table.getCanPreviousPage()}
            aria-label="第一页"
          >
            <ChevronsLeftIcon aria-hidden="true" />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            onClick={() => table.previousPage()}
            disabled={!table.getCanPreviousPage()}
            aria-label="上一页"
          >
            <ChevronLeftIcon aria-hidden="true" />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            onClick={() => table.nextPage()}
            disabled={!table.getCanNextPage()}
            aria-label="下一页"
          >
            <ChevronRightIcon aria-hidden="true" />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            onClick={() => table.lastPage()}
            disabled={!table.getCanNextPage()}
            aria-label="最后一页"
          >
            <ChevronsRightIcon aria-hidden="true" />
          </Button>
        </div>
      </div>
    </div>
  )
}

function alignmentClass(alignment: DataTableAlignment = "start") {
  if (alignment === "end") return "text-right"
  if (alignment === "center") return "text-center"
  return "text-left"
}

function sortableHeaderClass(alignment: DataTableAlignment = "start") {
  if (alignment === "end") {
    return "flex-row-reverse justify-start text-right"
  }
  if (alignment === "center") return "justify-center text-center"
  return "justify-start text-left"
}

function pinningClass(
  pinned: false | "left" | "right",
  area: "header" | "cell"
) {
  if (!pinned) return undefined
  return area === "header"
    ? "sticky z-10 bg-table-header"
    : "sticky z-10 bg-inherit"
}

function columnWidthClass(
  width: DataTableColumnWidth = "default",
  role?: "selection"
) {
  if (role === "selection") {
    return "w-table-column-selection min-w-table-column-selection max-w-table-column-selection"
  }

  return dataTableColumnWidthClasses[width]
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
