import { afterEach, describe, expect, it, vi } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useLedgerSearch } from "./use-ledger-search"

function setup(qParam = "") {
    return renderHook(
        (props: { qParam: string }) => useLedgerSearch(props),
        { initialProps: { qParam } },
    )
}

afterEach(() => {
    document.body.innerHTML = ""
})

describe("useLedgerSearch", () => {
    it("initialises the draft from the URL q param", () => {
        const { result } = setup("SKU-1")
        expect(result.current.searchDraft).toBe("SKU-1")
    })

    it("draft changes stay local and never write the URL", () => {
        const { result } = setup()
        act(() => {
            result.current.setSearchDraft("SKU-9")
        })
        expect(result.current.searchDraft).toBe("SKU-9")
    })

    it("syncs the draft when the URL q param changes", () => {
        const { result, rerender } = setup("OLD")
        expect(result.current.searchDraft).toBe("OLD")
        rerender({ qParam: "NEW" })
        expect(result.current.searchDraft).toBe("NEW")
    })

    it("keeps the focused draft while the URL q changes elsewhere", () => {
        const { result, rerender } = setup("OLD")
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input
        input.focus()
        rerender({ qParam: "NEW" })
        expect(result.current.searchDraft).toBe("OLD")
        document.body.removeChild(input)
    })

    it('focuses the search input on "/" unless modifiers or editable targets are active', () => {
        const { result } = setup()
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input
        const focusSpy = vi.spyOn(input, "focus")

        window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", cancelable: true }),
        )
        expect(focusSpy).toHaveBeenCalledTimes(1)

        focusSpy.mockClear()
        window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", metaKey: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", ctrlKey: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", altKey: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        const other = document.createElement("input")
        document.body.appendChild(other)
        other.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", bubbles: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        const area = document.createElement("textarea")
        document.body.appendChild(area)
        area.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", bubbles: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        const select = document.createElement("select")
        document.body.appendChild(select)
        select.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", bubbles: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        document.body.removeChild(input)
        document.body.removeChild(other)
        document.body.removeChild(area)
        document.body.removeChild(select)
    })

    it('ignores "/" while a dialog or sheet is open', () => {
        const { result } = setup()
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input
        const focusSpy = vi.spyOn(input, "focus")

        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)
        window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", cancelable: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()
        dialog.remove()

        const sheet = document.createElement("div")
        sheet.setAttribute("data-slot", "sheet")
        document.body.appendChild(sheet)
        window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "/", cancelable: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        document.body.removeChild(sheet)
        document.body.removeChild(input)
    })
})
