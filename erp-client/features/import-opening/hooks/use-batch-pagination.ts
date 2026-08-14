"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

/** 列表分页状态：URL page 为唯一事实来源，URL 变化时同步回分页器。 */
export function useBatchPagination(page: number) {
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: Math.max(0, page - 1),
        pageSize: 20,
    })

    React.useEffect(() => {
        setPagination((p) => ({
            ...p,
            pageIndex: Math.max(0, page - 1),
        }))
    }, [page])

    return { pagination, setPagination }
}
