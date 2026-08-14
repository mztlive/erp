import { describe, expect, it } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useBatchPagination } from "@/features/import-opening/hooks/use-batch-pagination"

describe("useBatchPagination", () => {
    it("derives pageIndex from the URL page (1-based)", () => {
        const { result } = renderHook(({ page }: { page: number }) =>
            useBatchPagination(page), { initialProps: { page: 3 } },
        )
        expect(result.current.pagination).toEqual({
            pageIndex: 2,
            pageSize: 20,
        })
    })

    it("clamps sub-one page values to the first page", () => {
        const { result } = renderHook(({ page }: { page: number }) =>
            useBatchPagination(page), { initialProps: { page: 0 } },
        )
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it("syncs pageIndex when the URL page changes", () => {
        const { result, rerender } = renderHook(({ page }: { page: number }) =>
            useBatchPagination(page), { initialProps: { page: 1 } },
        )
        rerender({ page: 5 })
        expect(result.current.pagination).toEqual({
            pageIndex: 4,
            pageSize: 20,
        })
    })

    it("preserves the pageSize on sync", () => {
        const { result, rerender } = renderHook(({ page }: { page: number }) =>
            useBatchPagination(page), { initialProps: { page: 1 } },
        )
        act(() => {
            result.current.setPagination({ pageIndex: 2, pageSize: 50 })
        })
        rerender({ page: 2 })
        expect(result.current.pagination).toEqual({
            pageIndex: 1,
            pageSize: 50,
        })
    })

    it("lets the table drive the local pageIndex", () => {
        const { result } = renderHook(() => useBatchPagination(1))
        act(() => {
            result.current.setPagination({ pageIndex: 4, pageSize: 20 })
        })
        expect(result.current.pagination.pageIndex).toBe(4)
    })
})
