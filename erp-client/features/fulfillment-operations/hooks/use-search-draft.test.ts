import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { useSearchDraft } from "./use-search-draft"

describe("useSearchDraft", () => {
    it("starts from the committed value", () => {
        const { result } = renderHook(() => useSearchDraft("SO-1", vi.fn()))
        expect(result.current.searchDraft).toBe("SO-1")
    })

    it("starts empty when there is no committed value", () => {
        const { result } = renderHook(() => useSearchDraft(undefined, vi.fn()))
        expect(result.current.searchDraft).toBe("")
    })

    it("updates the draft while typing without committing", () => {
        const onCommit = vi.fn()
        const { result } = renderHook(() => useSearchDraft(undefined, onCommit))

        act(() => {
            result.current.setSearchDraft("SO-")
        })
        act(() => {
            result.current.setSearchDraft("SO-2")
        })

        expect(result.current.searchDraft).toBe("SO-2")
        expect(onCommit).not.toHaveBeenCalled()
    })

    it("commits a trimmed, changed value", () => {
        const onCommit = vi.fn()
        const { result } = renderHook(() => useSearchDraft("SO-1", onCommit))

        act(() => {
            result.current.setSearchDraft("  SO-2  ")
        })
        act(() => {
            result.current.commitSearch()
        })

        expect(onCommit).toHaveBeenCalledTimes(1)
        expect(onCommit).toHaveBeenCalledWith("SO-2")
    })

    it("commits null when the draft is cleared to whitespace", () => {
        const onCommit = vi.fn()
        const { result } = renderHook(() => useSearchDraft("SO-1", onCommit))

        act(() => {
            result.current.setSearchDraft("   ")
        })
        act(() => {
            result.current.commitSearch()
        })

        expect(onCommit).toHaveBeenCalledTimes(1)
        expect(onCommit).toHaveBeenCalledWith(null)
    })

    it("does not commit when the trimmed draft equals the committed value", () => {
        const onCommit = vi.fn()
        const { result } = renderHook(() => useSearchDraft("SO-1", onCommit))

        act(() => {
            result.current.setSearchDraft(" SO-1 ")
        })
        act(() => {
            result.current.commitSearch()
        })

        expect(onCommit).not.toHaveBeenCalled()
    })

    it("resyncs the draft when the committed value changes", () => {
        const onCommit = vi.fn()
        let committed: string | undefined = "SO-1"
        const { result, rerender } = renderHook(() =>
            useSearchDraft(committed, onCommit),
        )

        act(() => {
            result.current.setSearchDraft("SO-9")
        })

        committed = "SO-2"
        rerender()

        expect(result.current.searchDraft).toBe("SO-2")
        expect(onCommit).not.toHaveBeenCalled()
    })
})
