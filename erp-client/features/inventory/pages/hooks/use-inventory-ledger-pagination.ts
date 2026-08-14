"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

import { encodeInventoryCursor } from "@/features/inventory/lib/cursor"
import type { InventoryView } from "@/features/inventory/types"
import type { LedgerPatchUrl } from "./use-inventory-ledger-url-state"

export interface InventoryLedgerPaginationInput {
    view: InventoryView
    pageSize: number
    cursorOffset: number
    patchUrl: LedgerPatchUrl
}

export function useInventoryLedgerPagination({
    view,
    pageSize,
    cursorOffset,
    patchUrl,
}: InventoryLedgerPaginationInput) {
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: Math.floor(cursorOffset / pageSize),
        pageSize,
    })

    const resetPagination = React.useCallback(() => {
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }, [])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            setPagination(next)
            const offset = next.pageIndex * next.pageSize
            patchUrl(
                {
                    cursor:
                        offset === 0
                            ? null
                            : encodeInventoryCursor(view, offset),
                    pageSize: String(next.pageSize),
                },
                { replace: true },
            )
        },
        [patchUrl, view],
    )

    // URL（游标 / pageSize）变化时同步分页状态
    React.useEffect(() => {
        const expectedPageIndex = Math.floor(cursorOffset / pageSize)
        setPagination((current) =>
            current.pageIndex === expectedPageIndex &&
            current.pageSize === pageSize
                ? current
                : { pageIndex: expectedPageIndex, pageSize },
        )
    }, [cursorOffset, pageSize])

    return { pagination, resetPagination, handlePaginationChange }
}
