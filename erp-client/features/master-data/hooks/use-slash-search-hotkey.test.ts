import { describe, it, expect, vi, afterEach } from "vitest"
import { cleanup, renderHook } from "@testing-library/react"

import { useSlashSearchHotkey } from "./use-slash-search-hotkey"

function dispatchSlash(target: EventTarget | null = document.body) {
    const event = new KeyboardEvent("keydown", {
        key: "/",
        bubbles: true,
        cancelable: true,
    })
    target?.dispatchEvent?.(event)
    return event
}

afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
})

describe("useSlashSearchHotkey", () => {
    it("focuses the search input when / is pressed outside an input", () => {
        const focus = vi.fn()
        const input = { current: { focus } } as never
        renderHook(() => useSlashSearchHotkey(input))

        const event = dispatchSlash()

        expect(focus).toHaveBeenCalledTimes(1)
        expect(event.defaultPrevented).toBe(true)
    })

    it("ignores / when the target is a text input", () => {
        const focus = vi.fn()
        const input = { current: { focus } } as never
        renderHook(() => useSlashSearchHotkey(input))

        const inputElement = document.createElement("input")
        const event = dispatchSlash(inputElement)

        expect(focus).not.toHaveBeenCalled()
        expect(event.defaultPrevented).toBe(false)
    })

    it("ignores / when the target is a textarea", () => {
        const focus = vi.fn()
        const input = { current: { focus } } as never
        renderHook(() => useSlashSearchHotkey(input))

        const textarea = document.createElement("textarea")
        const event = dispatchSlash(textarea)

        expect(focus).not.toHaveBeenCalled()
        expect(event.defaultPrevented).toBe(false)
    })

    it("ignores / while a dialog or sheet is open", () => {
        const focus = vi.fn()
        const input = { current: { focus } } as never
        renderHook(() => useSlashSearchHotkey(input))

        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        vi.spyOn(document, "querySelector").mockReturnValue(dialog)

        const event = dispatchSlash()

        expect(focus).not.toHaveBeenCalled()
        expect(event.defaultPrevented).toBe(false)
    })

    it("does nothing for other keys", () => {
        const focus = vi.fn()
        const input = { current: { focus } } as never
        renderHook(() => useSlashSearchHotkey(input))

        window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "Enter", cancelable: true }),
        )

        expect(focus).not.toHaveBeenCalled()
    })

    it("removes the listener on unmount", () => {
        const focus = vi.fn()
        const input = { current: { focus } } as never
        const { unmount } = renderHook(() => useSlashSearchHotkey(input))
        unmount()

        const event = dispatchSlash()

        expect(focus).not.toHaveBeenCalled()
        expect(event.defaultPrevented).toBe(false)
    })
})
