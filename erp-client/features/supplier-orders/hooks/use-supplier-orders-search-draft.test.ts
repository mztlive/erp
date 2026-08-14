import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { useSupplierOrdersSearchDraft } from "./use-supplier-orders-search-draft"

function renderSearchDraft(q: string | undefined) {
    const updateUrl = vi.fn()
    const rendered = renderHook(
        ({ currentQ }: { currentQ: string | undefined }) =>
            useSupplierOrdersSearchDraft({ q: currentQ, updateUrl }),
        { initialProps: { currentQ: q } },
    )
    return { ...rendered, updateUrl }
}

describe("useSupplierOrdersSearchDraft", () => {
    it("starts from the current q", () => {
        const { result } = renderSearchDraft("SFO-9")
        expect(result.current.searchDraft).toBe("SFO-9")
    })

    it("resets the draft when q changes externally", () => {
        const { result, rerender } = renderSearchDraft("SFO-9")

        act(() => {
            result.current.setSearchDraft("手写内容")
        })
        expect(result.current.searchDraft).toBe("手写内容")

        rerender({ currentQ: "外部清除后" })
        expect(result.current.searchDraft).toBe("外部清除后")
    })

    it("commits the draft on enter and resets the page", () => {
        const { result, updateUrl } = renderSearchDraft(undefined)

        act(() => {
            result.current.setSearchDraft("SFO-9")
        })
        act(() => {
            result.current.commitSearch("SFO-9")
        })

        expect(updateUrl).toHaveBeenCalledWith({ q: "SFO-9", page: 1 })
    })

    it("turns an empty commit into q=undefined", () => {
        const { result, updateUrl } = renderSearchDraft("SFO-9")

        act(() => {
            result.current.commitSearch("")
        })

        expect(updateUrl).toHaveBeenCalledWith({ q: undefined, page: 1 })
    })

    it("commits on blur only when the draft differs from q", () => {
        const { result, updateUrl, rerender } = renderSearchDraft("SFO-9")

        act(() => {
            result.current.commitOnBlur()
        })
        expect(updateUrl).not.toHaveBeenCalled()

        act(() => {
            result.current.setSearchDraft("SFO-10")
        })
        rerender({ currentQ: "SFO-9" })
        act(() => {
            result.current.commitOnBlur()
        })
        expect(updateUrl).toHaveBeenCalledWith({ q: "SFO-10", page: 1 })
    })
})
