import { act, renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

// useSearchInput 依赖的查询模块在本文件中不发起请求，占位即可。
vi.mock("@/features/entity-selectors/api/index", () => ({}))
vi.mock("@/features/sales-orders/api", () => ({}))
vi.mock("@/features/master-data/api", () => ({}))

import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

describe("useSearchInput", () => {
    afterEach(() => {
        vi.useRealTimers()
    })

    it("starts empty and only exposes the debounced, trimmed query", () => {
        vi.useFakeTimers()
        const { result } = renderHook(() => useSearchInput())
        expect(result.current.input).toBe("")

        act(() => {
            result.current.onSearchChange("  胶水 ")
        })
        expect(result.current.input).toBe("")

        act(() => {
            vi.advanceTimersByTime(250)
        })
        expect(result.current.input).toBe("胶水")
    })

    it("drops intermediate input when typing faster than the delay", () => {
        vi.useFakeTimers()
        const { result } = renderHook(() => useSearchInput())

        act(() => {
            result.current.onSearchChange("a")
        })
        act(() => {
            vi.advanceTimersByTime(100)
        })
        act(() => {
            result.current.onSearchChange("ab")
        })
        act(() => {
            vi.advanceTimersByTime(100)
        })
        expect(result.current.input).toBe("")

        act(() => {
            vi.advanceTimersByTime(150)
        })
        expect(result.current.input).toBe("ab")
    })
})
