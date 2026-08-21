import { act, renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import { useBatchSearchDraft } from "@/features/import-opening/hooks/use-batch-search"

function setup(q?: string) {
    const view = renderHook(
        ({ value }: { value?: string }) => useBatchSearchDraft(value ?? ""),
        { initialProps: { value: q } },
    )
    return view
}

afterEach(() => {
    document.body.innerHTML = ""
})

describe("useBatchSearchDraft", () => {
    it("initializes the draft from the URL q", () => {
        const { result } = setup("abc")
        expect(result.current.qDraft).toBe("abc")
    })

    it("syncs the draft when the URL q changes without writing back", () => {
        const { result, rerender } = setup("old")
        rerender({ value: "new" })
        expect(result.current.qDraft).toBe("new")
    })

    it("never writes the URL while the draft changes", () => {
        const { result } = setup()
        act(() => {
            result.current.setQDraft("  B-01 ")
        })
        expect(result.current.qDraft).toBe("  B-01 ")
    })

    it("protects an in-progress draft while the search input is focused", () => {
        const { result, rerender } = setup("old")
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input
        input.focus()

        act(() => {
            result.current.setQDraft("typed")
        })
        rerender({ value: "new" })
        expect(result.current.qDraft).toBe("typed")

        input.remove()
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

    it("ignores / while a dialog or sheet is open", () => {
        const { result } = setup()
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input
        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)

        act(() => {
            window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }))
        })
        expect(document.activeElement).not.toBe(input)

        dialog.remove()
        input.remove()
    })
})
