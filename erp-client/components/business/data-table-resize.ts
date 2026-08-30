"use client"

import * as React from "react"
import type {
    Column,
    ColumnSizingInfoState,
    ColumnSizingState,
    OnChangeFn,
} from "@tanstack/react-table"

export const emptyColumnSizingInfo: ColumnSizingInfoState = {
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

export type ColumnResizeHandlers<TData> = {
    begin: (
        event: React.PointerEvent<HTMLElement>,
        column: Column<TData>,
    ) => void
    update: (event: React.PointerEvent<HTMLElement>) => void
    end: (event: React.PointerEvent<HTMLElement>) => void
}

/** 指针拖拽列宽的会话状态；键盘列宽调整由表头组件负责。 */
export function useColumnResize<TData>(
    setColumnSizing: OnChangeFn<ColumnSizingState>,
    setColumnSizingInfo: OnChangeFn<ColumnSizingInfoState>,
): ColumnResizeHandlers<TData> {
    const sessionRef = React.useRef<ColumnResizeSession | null>(null)

    const begin = React.useCallback(
        (event: React.PointerEvent<HTMLElement>, column: Column<TData>) => {
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
            sessionRef.current = {
                pointerId: event.pointerId,
                columnId: column.id,
                startOffset: event.clientX,
                startSize,
                minSize: Number.isFinite(parsedMinSize) ? parsedMinSize : 0,
                maxSize: Number.isFinite(parsedMaxSize)
                    ? parsedMaxSize
                    : undefined,
                direction: computedStyle.direction === "rtl" ? -1 : 1,
            }
            setColumnSizingInfo({
                columnSizingStart: [[column.id, startSize]],
                deltaOffset: 0,
                deltaPercentage: 0,
                isResizingColumn: column.id,
                startOffset: event.clientX,
                startSize,
            })
        },
        [setColumnSizingInfo],
    )

    const update = React.useCallback(
        (event: React.PointerEvent<HTMLElement>) => {
            const session = sessionRef.current
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
                    session.startSize === 0
                        ? 0
                        : deltaOffset / session.startSize,
                isResizingColumn: session.columnId,
                startOffset: session.startOffset,
                startSize: session.startSize,
            })
        },
        [setColumnSizing, setColumnSizingInfo],
    )

    const end = React.useCallback(
        (event: React.PointerEvent<HTMLElement>) => {
            const session = sessionRef.current
            if (!session || session.pointerId !== event.pointerId) return
            sessionRef.current = null
            setColumnSizingInfo(emptyColumnSizingInfo)
        },
        [setColumnSizingInfo],
    )

    return { begin, update, end }
}
