import { describe, expect, it } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { useTablePagination } from '@/features/history-backfill/hooks/use-table-pagination'

describe("useTablePagination", () => {
    it("derives the initial pageIndex from the URL page", () => {
        const { result } = renderHook(
            ({ page }: { page: number }) => useTablePagination(page, 20),
            { initialProps: { page: 3 } },
        )

        expect(result.current[0]).toEqual({ pageIndex: 2, pageSize: 20 })
    })

    it("clamps the initial page to at least 1", () => {
        const { result } = renderHook(() => useTablePagination(0, 20))

        expect(result.current[0].pageIndex).toBe(0)
    })

    it("syncs pageIndex when the URL page changes", () => {
        const { result, rerender } = renderHook(
            ({ page }: { page: number }) => useTablePagination(page, 20),
            { initialProps: { page: 3 } },
        )

        rerender({ page: 1 })
        expect(result.current[0].pageIndex).toBe(0)

        rerender({ page: 9 })
        expect(result.current[0].pageIndex).toBe(8)
    })

    it("lets the table update local pagination state", () => {
        const { result } = renderHook(() => useTablePagination(1, 20))

        act(() => {
            result.current[1]({ pageIndex: 4, pageSize: 20 })
        })

        expect(result.current[0]).toEqual({ pageIndex: 4, pageSize: 20 })
    })
})
