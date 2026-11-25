"use client"

import { Fragment, type ReactNode, type RefObject } from "react"
import {
    flexRender,
    type ColumnSizingState,
    type Table as TanStackTable,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
} from "@/components/business/feedback-states"
import {
    alignmentClass,
    columnRuntimeWidth,
    columnWidthClass,
    pinningClass,
} from "@/components/business/data-table-layout"
import { Skeleton } from "@/components/ui/skeleton"
import { TableBody, TableCell, TableRow } from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
    error,
    errorTitle,
    errorSummary,
    onRetry,
    emptyState,
    emptyTitle,
    emptyDescription,
    emptyAction,
    onRowPreview,
    onRowOpen,
    renderExpandedRow,
    selectOnRowClick,
    enableRowSelection,
    highlightedRowId,
    enableColumnResizing,
    columnSizing,
    idPrefix,
}: {
    table: TanStackTable<TData>
    rowRefs: RefObject<Map<string, HTMLTableRowElement>>
    rowLabel?: (row: TData) => string
    dataLength: number
    loading: boolean
    showErrorState: boolean
    errorState?: ReactNode
    error?: unknown
    errorTitle?: ReactNode
    errorSummary?: ReactNode
    onRetry?: () => void
    emptyState?: ReactNode
    emptyTitle?: ReactNode
    emptyDescription?: ReactNode
    emptyAction?: ReactNode
    onRowPreview?: (row: TData) => void
    onRowOpen?: (row: TData) => void
    renderExpandedRow?: (row: TData) => ReactNode
    selectOnRowClick: boolean
    enableRowSelection: boolean | ((row: never) => boolean)
    highlightedRowId?: string
    enableColumnResizing: boolean
    columnSizing: ColumnSizingState
    idPrefix?: string
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
                        <div className="sticky left-0 w-[calc(100cqw-3rem)] max-w-full whitespace-normal">
                            {errorState ?? (
                                <BusinessFailureState
                                    kind="system"
                                    error={error ?? undefined}
                                    title={
                                        errorTitle ??
                                        (error != null
                                            ? undefined
                                            : "数据加载失败")
                                    }
                                    description={errorSummary}
                                    onRetry={onRetry}
                                    idPrefix={idPrefix}
                                />
                            )}
                        </div>
                    </TableCell>
                </TableRow>
            ) : loading && dataLength === 0 ? (
                Array.from({ length: 5 }, (_, rowIndex) => (
                    <TableRow key={`loading-${rowIndex}`}>
                        {table.getVisibleLeafColumns().map((column) => (
                            <TableCell
                                key={column.id}
                                data-column-id={column.id}
                            >
                                <Skeleton className="h-4 w-full" />
                            </TableCell>
                        ))}
                    </TableRow>
                ))
            ) : (
                rows.map((row, rowIndex) => {
                    const expanded = renderExpandedRow?.(row.original)
                    return (
                        <Fragment key={row.id}>
                            <TableRow
                                id={
                                    idPrefix
                                        ? `${idPrefix}-row-${toAutomationIdSegment(row.id)}`
                                        : undefined
                                }
                                data-row-id={row.id}
                                ref={(node) => {
                                    if (node) rowRefs.current.set(row.id, node)
                                    else rowRefs.current.delete(row.id)
                                }}
                                data-state={
                                    row.getIsSelected() ? "selected" : undefined
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
                                        "bg-row-selected",
                                )}
                                onClick={(event) => {
                                    if (
                                        !(event.target instanceof Node) ||
                                        !event.currentTarget.contains(
                                            event.target,
                                        ) ||
                                        isInteractiveRowTarget(event.target)
                                    )
                                        return
                                    if (selectOnRowClick) {
                                        if (row.getCanSelect())
                                            row.toggleSelected()
                                        return
                                    }
                                    ;(onRowPreview ?? onRowOpen)?.(row.original)
                                }}
                                onKeyDown={(event) => {
                                    if (
                                        !(event.target instanceof Node) ||
                                        !event.currentTarget.contains(
                                            event.target,
                                        ) ||
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
                                        (selectOnRowClick ||
                                            onRowOpen ||
                                            onRowPreview)
                                    ) {
                                        event.preventDefault()
                                        if (selectOnRowClick) {
                                            if (row.getCanSelect())
                                                row.toggleSelected()
                                            return
                                        }
                                        ;(onRowOpen ?? onRowPreview)?.(
                                            row.original,
                                        )
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
                                    const pinned = cell.column.getIsPinned()
                                    const pinnedEdge =
                                        pinned === "left" &&
                                        cell.column.getIsLastColumn("left")
                                            ? "left"
                                            : pinned === "right" &&
                                                cell.column.getIsFirstColumn(
                                                    "right",
                                                )
                                              ? "right"
                                              : undefined
                                    return (
                                        <TableCell
                                            key={cell.id}
                                            data-column-id={cell.column.id}
                                            data-column-pinned={
                                                pinned || undefined
                                            }
                                            data-pinned-edge={pinnedEdge}
                                            data-align={
                                                meta?.align === "end"
                                                    ? "end"
                                                    : undefined
                                            }
                                            className={cn(
                                                alignmentClass(meta?.align),
                                                meta?.numeric &&
                                                    meta?.align !== "end" &&
                                                    "num",
                                                columnWidthClass(
                                                    meta?.width,
                                                    meta?.role,
                                                ),
                                                pinningClass(pinned, "cell"),
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
                            {expanded != null && expanded !== false ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={visibleColumnCount}
                                        className="h-auto whitespace-normal p-0"
                                    >
                                        {expanded}
                                    </TableCell>
                                </TableRow>
                            ) : null}
                        </Fragment>
                    )
                })
            )}

            {!showErrorState && !loading && rows.length === 0 ? (
                <TableRow>
                    <TableCell
                        colSpan={Math.max(visibleColumnCount, 1)}
                        className="h-auto p-6"
                    >
                        <div className="sticky left-0 w-[calc(100cqw-3rem)] max-w-full whitespace-normal">
                            {emptyState ?? (
                                <BusinessEmptyState
                                    kind={
                                        emptyTitle != null
                                            ? "no-data"
                                            : "filter"
                                    }
                                    title={
                                        typeof emptyTitle === "string"
                                            ? emptyTitle
                                            : emptyTitle == null
                                              ? "当前筛选没有结果"
                                              : ""
                                    }
                                    description={
                                        typeof emptyTitle === "string" ||
                                        emptyTitle == null ? (
                                            (emptyDescription ??
                                            (emptyTitle != null
                                                ? ""
                                                : undefined))
                                        ) : (
                                            <>
                                                <span className="block font-heading text-section-title font-semibold tracking-tight text-foreground">
                                                    {emptyTitle}
                                                </span>
                                                {emptyDescription}
                                            </>
                                        )
                                    }
                                    action={emptyAction}
                                    className="px-4 py-6"
                                />
                            )}
                        </div>
                    </TableCell>
                </TableRow>
            ) : null}
        </TableBody>
    )
}
