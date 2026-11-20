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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export function DataTableHeader<TData>({
    table,
    columnSizing,
    columnSizingInfo,
    enableColumnResizing,
    sortingInteractive,
    resize,
    idPrefix,
}: {
    table: TanStackTable<TData>
    columnSizing: ColumnSizingState
    columnSizingInfo: ColumnSizingInfoState
    enableColumnResizing: boolean
    sortingInteractive: boolean
    resize: ColumnResizeHandlers<TData>
    idPrefix?: string
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

                        const pinned = header.column.getIsPinned()
                        const pinnedEdge =
                            pinned === "left" &&
                            header.column.getIsLastColumn("left")
                                ? "left"
                                : pinned === "right" &&
                                    header.column.getIsFirstColumn("right")
                                  ? "right"
                                  : undefined

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
                                data-column-pinned={pinned || undefined}
                                data-pinned-edge={pinnedEdge}
                                data-sorted={sort ? "true" : undefined}
                                data-align={
                                    meta?.align === "end" ? "end" : undefined
                                }
                                className={cn(
                                    "group/head sticky top-0 z-20",
                                    alignmentClass(meta?.align),
                                    columnWidthClass(meta?.width, meta?.role),
                                    pinningClass(pinned, "header"),
                                    sort &&
                                        "bg-row-hover text-primary hover:bg-row-hover hover:text-primary",
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
                                        id={
                                            idPrefix
                                                ? `${idPrefix}-header-${toAutomationIdSegment(header.column.id)}-sort`
                                                : undefined
                                        }
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={cn(
                                            "w-full gap-1 px-0 text-xs font-medium text-foreground/65 hover:bg-transparent hover:text-foreground",
                                            sort &&
                                                "text-primary hover:text-primary",
                                            sortableHeaderClass(meta?.align),
                                        )}
                                        onClick={header.column.getToggleSortingHandler()}
                                        aria-label={`按${meta?.label ?? header.column.id}排序`}
                                    >
                                        {flexRender(
                                            header.column.columnDef.header,
                                            header.getContext(),
                                        )}
                                        <SortIcon
                                            aria-hidden="true"
                                            className={cn(
                                                "size-3.5 transition-opacity",
                                                sort
                                                    ? "opacity-100 text-primary"
                                                    : "opacity-35 group-hover/head:opacity-85",
                                            )}
                                        />
                                    </Button>
                                ) : (
                                    flexRender(
                                        header.column.columnDef.header,
                                        header.getContext(),
                                    )
                                )}

                                {header.column.getCanResize() ? (
                                    <button
                                        id={
                                            idPrefix
                                                ? `${idPrefix}-header-${toAutomationIdSegment(header.column.id)}-resize`
                                                : undefined
                                        }
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
                                            "absolute inset-y-0 right-0 z-20 w-table-resize-target cursor-col-resize touch-none border-0 bg-transparent p-0 select-none after:absolute after:inset-y-2.5 after:right-0 after:w-px after:bg-transparent after:transition-colors group-hover/thead:after:bg-border/60 group-hover/head:after:bg-primary/65 hover:after:inset-y-1 hover:after:w-0.5 hover:after:bg-primary",
                                            columnSizingInfo.isResizingColumn ===
                                                header.column.id &&
                                                "after:inset-y-0 after:w-0.5 after:bg-primary",
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
