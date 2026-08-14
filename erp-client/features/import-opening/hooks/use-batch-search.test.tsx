import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useBatchSearch } from "@/features/import-opening/hooks/use-batch-search"

beforeEach(() => {
    vi.useFakeTimers()
})

afterEach(() => {
    vi.useRealTimers()
})

function setup(q?: string) {
    const patchUrl = vi.fn()
    const view = renderHook(
        ({ q }: { q?: string }) => useBatchSearch({ q, patchUrl }),
        { initialProps: { q } },
    )
    return { patchUrl, ...view }
}

describe("useBatchSearch", () => {
    it("initializes the draft from the URL q", () => {
        const { result } = setup("abc")
        expect(result.current.qDraft).toBe("abc")
    })

    it("syncs the draft when the URL q changes without writing back", () => {
        const { patchUrl, result, rerender } = setup("old")
        rerender({ q: "new" })
        expect(result.current.qDraft).toBe("new")
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("debounces draft edits into a URL write with page reset", () => {
        const { patchUrl, result } = setup()
        act(() => {
            result.current.setQDraft("  B-01 ")
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(patchUrl).toHaveBeenCalledWith({ q: "B-01", page: 1 })
    })

    it("clears q when the draft becomes blank", () => {
        const { patchUrl, result } = setup("B-01")
        act(() => {
            result.current.setQDraft("   ")
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(patchUrl).toHaveBeenCalledWith({ q: undefined, page: 1 })
    })

    it("does not write while edits keep arriving within the debounce window", () => {
        const { patchUrl, result } = setup()
        act(() => {
            result.current.setQDraft("a")
        })
        act(() => {
            vi.advanceTimersByTime(200)
            result.current.setQDraft("ab")
        })
        act(() => {
            vi.advanceTimersByTime(200)
        })
        expect(patchUrl).not.toHaveBeenCalled()
        act(() => {
            vi.advanceTimersByTime(100)
        })
        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith({ q: "ab", page: 1 })
    })

    it("focuses the search input when / is pressed outside a field", () => {
        const { result } = setup()
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input

        act(() => {
            window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }))
        })
        expect(document.activeElement).toBe(input)

        input.remove()
    })

    it("ignores / when a modifier key is held", () => {
        const { result } = setup()
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input

        act(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", ctrlKey: true }),
            )
        })
        expect(document.activeElement).not.toBe(input)

        input.remove()
    })

    it("ignores / when typing inside an input", () => {
        const { result } = setup()
        const target = document.createElement("input")
        document.body.appendChild(target)
        result.current.searchInputRef.current = document.createElement("input")
        target.focus()

        act(() => {
            target.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", bubbles: true }),
            )
        })
        expect(document.activeElement).toBe(target)

        target.remove()
    })
})
