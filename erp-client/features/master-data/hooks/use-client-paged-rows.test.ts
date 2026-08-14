import { describe, it, expect } from "vitest"
import { renderHook } from "@testing-library/react"

import { useClientPagedRows } from "./use-client-paged-rows"

const rows = Array.from({ length: 45 }, (_, index) => index)

describe("useClientPagedRows", () => {
    it("slices the first page from the start", () => {
        const { result } = renderHook(() =>
            useClientPagedRows(rows, { pageIndex: 0, pageSize: 20 }),
        )
        expect(result.current).toHaveLength(20)
        expect(result.current[0]).toBe(0)
        expect(result.current[19]).toBe(19)
    })

    it("slices later pages at the correct offset", () => {
        const { result } = renderHook(() =>
            useClientPagedRows(rows, { pageIndex: 2, pageSize: 20 }),
        )
        expect(result.current).toHaveLength(5)
        expect(result.current[0]).toBe(40)
        expect(result.current[4]).toBe(44)
    })

    it("returns an empty array for empty input", () => {
        const { result } = renderHook(() =>
            useClientPagedRows([], { pageIndex: 0, pageSize: 20 }),
        )
        expect(result.current).toEqual([])
    })

    it("returns an empty slice when the page index is past the end", () => {
        const { result } = renderHook(() =>
            useClientPagedRows(rows, { pageIndex: 9, pageSize: 20 }),
        )
        expect(result.current).toEqual([])
    })

    it("recomputes when rows change", () => {
        const { result, rerender } = renderHook(
            ({ data }: { data: readonly number[] }) =>
                useClientPagedRows(data, { pageIndex: 0, pageSize: 20 }),
            { initialProps: { data: rows } },
        )
        expect(result.current).toHaveLength(20)
        rerender({ data: rows.slice(0, 10) })
        expect(result.current).toHaveLength(10)
    })
})
