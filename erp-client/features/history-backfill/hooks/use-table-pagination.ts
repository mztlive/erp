"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

/**
 * 表格分页状态：本地持有，URL 页码变化时同步回 pageIndex。
 */
export function useTablePagination(page: number, pageSize: number) {
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: Math.max(0, page - 1),
        pageSize,
    })

    React.useEffect(() => {
        setPagination((p) => ({
            ...p,
            pageIndex: Math.max(0, page - 1),
        }))
    }, [page])

    return [pagination, setPagination] as const
}
