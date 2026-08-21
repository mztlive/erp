import { act, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { useSupplierOrdersSearchDraft } from "./use-supplier-orders-search-draft"

function renderSearchDraft(q: string | undefined) {
    const ref = { current: null as HTMLInputElement | null }
    const rendered = renderHook(
        ({ currentQ }: { currentQ: string | undefined }) =>
            useSupplierOrdersSearchDraft({
                q: currentQ,
                searchInputRef: ref,
            }),
        { initialProps: { currentQ: q } },
    )
    return { ...rendered, ref }
}

describe("useSupplierOrdersSearchDraft", () => {
    it("starts from the current q", () => {
        const { result } = renderSearchDraft("SFO-9")
        expect(result.current.searchDraft).toBe("SFO-9")
    })

    it("resets the draft when q changes externally while the input is not focused", () => {
        const { result, rerender } = renderSearchDraft("SFO-9")

        act(() => {
            result.current.setSearchDraft("手写内容")
        })
        expect(result.current.searchDraft).toBe("手写内容")

        rerender({ currentQ: "外部清除后" })
        expect(result.current.searchDraft).toBe("外部清除后")
    })

    it("keeps the uncommitted draft while the search input is focused", () => {
        const { result, rerender, ref } = renderSearchDraft("SFO-9")

        act(() => {
            result.current.setSearchDraft("手写内容")
        })

        const input = document.createElement("input")
        ref.current = input
        document.body.appendChild(input)
        input.focus()

        rerender({ currentQ: "外部变化" })
        expect(result.current.searchDraft).toBe("手写内容")
    })

    it("does not commit drafts on its own", () => {
        const { result } = renderSearchDraft(undefined)

        act(() => {
            result.current.setSearchDraft("SFO-9")
        })

        expect(result.current.searchDraft).toBe("SFO-9")
    })
})
