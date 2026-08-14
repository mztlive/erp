import { act, cleanup, render, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useIntegrationSearch } from "./use-integration-search"

const onCommitSearch = vi.fn()

beforeEach(() => {
    vi.useFakeTimers()
    onCommitSearch.mockClear()
})

afterEach(() => {
    cleanup()
    vi.useRealTimers()
})

function SearchProbe({
    q,
}: {
    q: string | undefined
}) {
    const { searchInputRef } = useIntegrationSearch({ q, onCommitSearch })
    return <input ref={searchInputRef} aria-label="搜索" data-testid="search-input" />
}

describe("useIntegrationSearch", () => {
    it("initializes the draft from the URL q param", () => {
        const { result } = renderHook(() =>
            useIntegrationSearch({ q: "po-1", onCommitSearch }),
        )
        expect(result.current.searchDraft).toBe("po-1")
    })

    it("syncs the draft when the URL q param changes", () => {
        const { result, rerender } = renderHook(
            ({ q }: { q: string | undefined }) =>
                useIntegrationSearch({ q, onCommitSearch }),
            { initialProps: { q: "po-1" } },
        )
        rerender({ q: "po-2" })
        expect(result.current.searchDraft).toBe("po-2")
    })

    it("commits the draft after the 300ms debounce", () => {
        const { result } = renderHook(
            ({ q }: { q: string | undefined }) =>
                useIntegrationSearch({ q, onCommitSearch }),
            { initialProps: { q: "" } },
        )
        act(() => {
            result.current.setSearchDraft("po")
        })
        expect(onCommitSearch).not.toHaveBeenCalled()
        act(() => {
            vi.advanceTimersByTime(299)
        })
        expect(onCommitSearch).not.toHaveBeenCalled()
        act(() => {
            vi.advanceTimersByTime(1)
        })
        expect(onCommitSearch).toHaveBeenCalledWith("po")
    })

    it("does not commit when the trimmed draft equals the URL q", () => {
        const { result } = renderHook(
            ({ q }: { q: string | undefined }) =>
                useIntegrationSearch({ q, onCommitSearch }),
            { initialProps: { q: "po" } },
        )
        act(() => {
            result.current.setSearchDraft(" po ")
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(onCommitSearch).not.toHaveBeenCalled()
    })

    it("commits null for an emptied draft", () => {
        const { result } = renderHook(
            ({ q }: { q: string | undefined }) =>
                useIntegrationSearch({ q, onCommitSearch }),
            { initialProps: { q: "po" } },
        )
        act(() => {
            result.current.setSearchDraft("")
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(onCommitSearch).toHaveBeenCalledWith(null)
    })

    it("focuses the search input when / is pressed outside form fields", () => {
        render(<SearchProbe q="" />)
        const input = document.querySelector<HTMLInputElement>(
            '[data-testid="search-input"]',
        )
        expect(input).not.toBeNull()
        act(() => {
            document.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", bubbles: true }),
            )
        })
        expect(document.activeElement).toBe(input)
    })

    it("ignores / inside text inputs", () => {
        render(<SearchProbe q="" />)
        const input = document.querySelector<HTMLInputElement>(
            '[data-testid="search-input"]',
        )
        expect(input).not.toBeNull()
        act(() => {
            input?.dispatchEvent(
                new KeyboardEvent("keydown", {
                    key: "/",
                    bubbles: true,
                }),
            )
        })
        expect(document.activeElement).not.toBe(input)
    })
})
