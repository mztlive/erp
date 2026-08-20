"use client"

import {
    ArrowDownIcon,
    ArrowUpIcon,
    ChevronsLeftIcon,
    ChevronsRightIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    Settings2Icon,
    PanelLeftIcon,
    PanelRightIcon,
    PinOffIcon,
} from "lucide-react"
import type { Table as TanStackTable } from "@tanstack/react-table"

import { OptionCombobox } from "@/components/business/option-combobox"
import type { DataTableLayout } from "@/components/business/data-table-layout"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"

function DataTableViewOptions<TData>({
    table,
}: {
    table: TanStackTable<TData>
}) {
    const columns = table
        .getAllLeafColumns()
        .filter((column) => column.getCanHide())
    const visibleColumnCount = columns.filter((column) =>
        column.getIsVisible(),
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
                <Settings2Icon data-icon="inline-start" aria-hidden="true" />
                视图
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
                            const label =
                                column.columnDef.meta?.label ?? column.id
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
                                                column.getIsVisible() &&
                                                visibleColumnCount === 1
                                            }
                                            onCheckedChange={(checked) =>
                                                column.toggleVisibility(
                                                    checked === true,
                                                )
                                            }
                                        />
                                        <span className="truncate">
                                            {label}
                                        </span>
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
                                            onClick={() =>
                                                moveColumn(
                                                    table,
                                                    columns,
                                                    index,
                                                    -1,
                                                )
                                            }
                                            disabled={index === 0}
                                            aria-label={`前移${label}列`}
                                        >
                                            <ArrowUpIcon aria-hidden="true" />
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            onClick={() =>
                                                moveColumn(
                                                    table,
                                                    columns,
                                                    index,
                                                    1,
                                                )
                                            }
                                            disabled={
                                                index === columns.length - 1
                                            }
                                            aria-label={`后移${label}列`}
                                        >
                                            <ArrowDownIcon aria-hidden="true" />
                                        </Button>
                                        {column.getCanPin() ? (
                                            <>
                                                <Button
                                                    type="button"
                                                    variant={
                                                        pinned === "left"
                                                            ? "secondary"
                                                            : "ghost"
                                                    }
                                                    size="icon-xs"
                                                    onClick={() =>
                                                        column.pin("left")
                                                    }
                                                    disabled={
                                                        !column.getIsVisible()
                                                    }
                                                    aria-label={`固定${label}列到左侧`}
                                                >
                                                    <PanelLeftIcon aria-hidden="true" />
                                                </Button>
                                                <Button
                                                    type="button"
                                                    variant={
                                                        pinned === "right"
                                                            ? "secondary"
                                                            : "ghost"
                                                    }
                                                    size="icon-xs"
                                                    onClick={() =>
                                                        column.pin("right")
                                                    }
                                                    disabled={
                                                        !column.getIsVisible()
                                                    }
                                                    aria-label={`固定${label}列到右侧`}
                                                >
                                                    <PanelRightIcon aria-hidden="true" />
                                                </Button>
                                                {pinned ? (
                                                    <Button
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-xs"
                                                        onClick={() =>
                                                            column.pin(false)
                                                        }
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
    offset: -1 | 1,
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
                column.getCanHide()
                    ? (nextMovableIds.shift() ?? column.id)
                    : column.id,
            ),
    )
}

function DataTablePagination<TData>({
    table,
    pageSizeOptions,
    layout = "flush",
}: {
    table: TanStackTable<TData>
    pageSizeOptions: readonly number[]
    layout?: DataTableLayout
}) {
    const { pageIndex, pageSize } = table.getState().pagination
    const pageCount = table.getPageCount()
    const rowCount = table.getRowCount()
    const selectedCount = Object.keys(table.getState().rowSelection).length

    return (
        <div
            data-slot="data-table-pagination"
            className={cn(
                "flex flex-col gap-3 text-sm sm:flex-row sm:items-center sm:justify-between",
                layout === "inset" && "px-(--card-spacing)",
            )}
        >
            <label className="flex shrink-0 items-center gap-2 text-muted-foreground">
                <OptionCombobox
                    size="sm"
                    value={String(pageSize)}
                    onValueChange={(next) => {
                        if (next == null) return
                        table.setPageSize(Number(next))
                    }}
                    options={pageSizeOptions.map((size) => ({
                        value: String(size),
                        label: String(size),
                    }))}
                    allowClear={false}
                    aria-label="每页记录数"
                    className="w-[5.5rem]"
                />
                条 / 页
                {selectedCount > 0 ? (
                    <span className="num">
                        · 已选择 {selectedCount.toLocaleString("zh-CN")} 条
                    </span>
                ) : (
                    <span className="num">
                        · 共 {rowCount.toLocaleString("zh-CN")} 条
                    </span>
                )}
            </label>

            <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2 sm:justify-end">
                <span className="num shrink-0 text-muted-foreground">
                    第 {pageCount === 0 ? 0 : pageIndex + 1} / {pageCount} 页
                </span>

                <div
                    role="group"
                    aria-label="翻页"
                    className="flex items-center gap-1"
                >
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

export { DataTablePagination, DataTableViewOptions }
