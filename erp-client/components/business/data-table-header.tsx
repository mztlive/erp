"use client"

import { ArrowDownIcon, ArrowUpDownIcon, ArrowUpIcon } from "lucide-react"
import {
    flexRender,
    type ColumnSizingInfoState,
    type ColumnSizingState,
    type Table as TanStackTable,
} from "@tanstack/react-table"

import {
    alignmentClass,
    columnRuntimeWidth,
    columnWidthClass,
    pinningClass,
    sortableHeaderClass,
} from "@/components/business/data-table-layout"
import type { ColumnResizeHandlers } from "@/components/business/data-table-resize"
import { Button } from "@/components/ui/button"
import { TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { cn } from "@/lib/utils"

export function DataTableHeader<TData>({
    table,
    columnSizing,
    columnSizingInfo,
    enableColumnResizing,
    sortingInteractive,
    resize,
}: {
    table: TanStackTable<TData>
    columnSizing: ColumnSizingState
    columnSizingInfo: ColumnSizingInfoState
    enableColumnResizing: boolean
    sortingInteractive: boolean
    resize: ColumnResizeHandlers<TData>
}) {
    return (
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
                                    header.subHeaders.length === 0
                                        ? "true"
                                        : undefined
                                }
                                data-column-pinned={
                                    header.column.getIsPinned() || undefined
                                }
                                data-align={
                                    meta?.align === "end" ? "end" : undefined
                                }
                                className={cn(
                                    "relative",
                                    header.column.id === "actions" &&
                                        "max-sm:hidden",
                                    alignmentClass(meta?.align),
                                    columnWidthClass(meta?.width, meta?.role),
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
                                            "w-full px-0 text-xs font-medium text-muted-foreground hover:text-foreground",
                                            sortableHeaderClass(meta?.align),
                                        )}
                                        onClick={header.column.getToggleSortingHandler()}
                                        aria-label={`按${meta?.label ?? header.column.id}排序`}
                                    >
                                        {flexRender(
                                            header.column.columnDef.header,
                                            header.getContext(),
                                        )}
                                        <SortIcon aria-hidden="true" />
                                    </Button>
                                ) : (
                                    flexRender(
                                        header.column.columnDef.header,
                                        header.getContext(),
                                    )
                                )}

                                {header.column.getCanResize() ? (
                                    <button
                                        type="button"
                                        role="separator"
                                        aria-label={`调整${meta?.label ?? header.column.id}列宽`}
                                        aria-orientation="vertical"
                                        aria-valuemin={
                                            header.column.columnDef.minSize ??
                                            40
                                        }
                                        aria-valuemax={
                                            header.column.columnDef.maxSize ??
                                            1_000
                                        }
                                        aria-valuenow={header.column.getSize()}
                                        onDoubleClick={(event) => {
                                            event.stopPropagation()
                                            header.column.resetSize()
                                        }}
                                        onKeyDown={(event) => {
                                            if (
                                                event.key === "Enter" ||
                                                event.key === " "
                                            ) {
                                                event.preventDefault()
                                                header.column.resetSize()
                                                return
                                            }
                                            if (
                                                event.key !== "ArrowLeft" &&
                                                event.key !== "ArrowRight"
                                            ) {
                                                return
                                            }
                                            event.preventDefault()
                                            const minimum =
                                                header.column.columnDef
                                                    .minSize ?? 40
                                            const maximum =
                                                header.column.columnDef
                                                    .maxSize ?? 1_000
                                            const delta =
                                                event.key === "ArrowRight"
                                                    ? 8
                                                    : -8
                                            const size = Math.min(
                                                maximum,
                                                Math.max(
                                                    minimum,
                                                    header.column.getSize() +
                                                        delta,
                                                ),
                                            )
                                            table.setColumnSizing(
                                                (current) => ({
                                                    ...current,
                                                    [header.column.id]: size,
                                                }),
                                            )
                                        }}
                                        onPointerDown={(event) =>
                                            resize.begin(event, header.column)
                                        }
                                        onPointerMove={resize.update}
                                        onPointerUp={resize.end}
                                        onPointerCancel={resize.end}
                                        onLostPointerCapture={resize.end}
                                        className={cn(
                                            "absolute inset-y-0 right-0 z-20 w-table-resize-target cursor-col-resize touch-none border-0 bg-transparent p-0 select-none after:absolute after:inset-y-0 after:right-0 after:w-px after:bg-transparent hover:after:bg-ring",
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
    )
}
