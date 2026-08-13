"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

/** 前端按当前页切片（服务端已按筛选返回全量当前结果）。 */
export function useClientPagedRows<T>(
    rows: readonly T[],
    pagination: PaginationState,
): T[] {
    return React.useMemo(() => {
        const start = pagination.pageIndex * pagination.pageSize
        return rows.slice(start, start + pagination.pageSize)
    }, [pagination.pageIndex, pagination.pageSize, rows])
}
