import { describe, it, expect, vi } from "vitest"
import { renderHook } from "@testing-library/react"

import { useListPageChrome } from "./use-list-page-chrome"

describe("useListPageChrome", () => {
    it("returns stable refs for search, results heading and focused row", () => {
        const { result, rerender } = renderHook(() => useListPageChrome())
        const first = result.current
        expect(first.searchInputRef).toEqual({ current: null })
        expect(first.resultsHeadingRef).toEqual({ current: null })
        expect(first.lastFocusedRowId).toEqual({ current: null })

        rerender()
        expect(result.current.searchInputRef).toBe(first.searchInputRef)
        expect(result.current.resultsHeadingRef).toBe(first.resultsHeadingRef)
        expect(result.current.lastFocusedRowId).toBe(first.lastFocusedRowId)
    })

    it("focuses the search input on the / hotkey", () => {
        const { result } = renderHook(() => useListPageChrome())
        const focus = vi.fn()
        const inputElement = document.createElement("input")
        inputElement.focus = focus
        result.current.searchInputRef.current = inputElement

        window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", cancelable: true }),
        )

        expect(focus).toHaveBeenCalledTimes(1)
    })
})
