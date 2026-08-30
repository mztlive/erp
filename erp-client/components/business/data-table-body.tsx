"use client"

import type * as React from "react"
import {
    flexRender,
    type ColumnSizingState,
    type Table as TanStackTable,
} from "@tanstack/react-table"

import { BusinessFailureState } from "@/components/business/feedback"
import {
    alignmentClass,
    columnRuntimeWidth,
    columnWidthClass,
    pinningClass,
} from "@/components/business/data-table-layout"
import { Skeleton } from "@/components/ui/skeleton"
import { TableBody, TableCell, TableRow } from "@/components/ui/table"
import { cn } from "@/lib/utils"

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

export function DataTableBody<TData>({
    table,
    rowRefs,
    rowLabel,
    dataLength,
    loading,
    showErrorState,
    errorState,
    errorTitle,
    errorSummary,
    onRetry,
    emptyState,
    emptyTitle,
    emptyDescription,
    emptyAction,
    onRowPreview,
    onRowOpen,
    enableRowSelection,
    highlightedRowId,
    enableColumnResizing,
    columnSizing,
}: {
    table: TanStackTable<TData>
    rowRefs: React.RefObject<Map<string, HTMLTableRowElement>>
    rowLabel?: (row: TData) => string
    dataLength: number
    loading: boolean
    showErrorState: boolean
    errorState?: React.ReactNode
    errorTitle?: React.ReactNode
    errorSummary?: React.ReactNode
    onRetry?: () => void
    emptyState?: React.ReactNode
    emptyTitle?: React.ReactNode
    emptyDescription?: React.ReactNode
    emptyAction?: React.ReactNode
    onRowPreview?: (row: TData) => void
    onRowOpen?: (row: TData) => void
    enableRowSelection: boolean | ((row: never) => boolean)
    highlightedRowId?: string
    enableColumnResizing: boolean
    columnSizing: ColumnSizingState
}) {
    const rows = table.getRowModel().rows
    const visibleColumnCount = table.getVisibleLeafColumns().length
    const interactive = Boolean(onRowPreview || onRowOpen || enableRowSelection)

    const focusRelativeRow = (rowIndex: number, offset: number) => {
        const target = rows[rowIndex + offset]
        if (target) rowRefs.current.get(target.id)?.focus()
    }

    return (
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
            ) : loading && dataLength === 0 ? (
                Array.from({ length: 5 }, (_, rowIndex) => (
                    <TableRow key={`loading-${rowIndex}`}>
                        {table.getVisibleLeafColumns().map((column) => (
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
                            if (node) rowRefs.current.set(row.id, node)
                            else rowRefs.current.delete(row.id)
                        }}
                        data-state={
                            row.getIsSelected() ? "selected" : undefined
                        }
                        data-highlighted={
                            highlightedRowId === row.id ? "true" : undefined
                        }
                        tabIndex={interactive ? 0 : undefined}
                        className={cn(
                            interactive && "cursor-pointer",
                            highlightedRowId === row.id && "bg-muted",
                        )}
                        onClick={(event) => {
                            if (isInteractiveRowTarget(event.target)) return
                            ;(onRowPreview ?? onRowOpen)?.(row.original)
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
                                ;(onRowOpen ?? onRowPreview)?.(row.original)
                            } else if (
                                event.key === " " &&
                                row.getCanSelect()
                            ) {
                                event.preventDefault()
                                row.toggleSelected()
                            }
                        }}
                        aria-label={
                            rowLabel?.(row.original) ?? `第 ${rowIndex + 1} 行`
                        }
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
                                    data-align={
                                        meta?.align === "end"
                                            ? "end"
                                            : undefined
                                    }
                                    className={cn(
                                        alignmentClass(meta?.align),
                                        cell.column.id === "actions" &&
                                            "max-sm:hidden",
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
                                    {emptyTitle ?? "当前筛选没有结果"}
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
    )
}
