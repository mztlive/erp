import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, cleanup } from "@testing-library/react"

import { useFulfillmentKeyboardShortcuts } from "./use-fulfillment-keyboard-shortcuts"

const options = () => ({
    dirty: false,
    canPost: true,
    formalPending: false,
    canExecute: true,
    supportsSave: true,
    onSave: vi.fn(),
    onConfirm: vi.fn(),
    onNavigate: vi.fn(),
    onToggleShortcuts: vi.fn(),
})

function renderShortcuts(overrides: Partial<ReturnType<typeof options>> = {}) {
    const handlers = { ...options(), ...overrides }
    const utils = renderHook(
        (props: ReturnType<typeof options>) =>
            useFulfillmentKeyboardShortcuts(props),
        { initialProps: handlers },
    )
    return { ...utils, handlers }
}

function pressKey(key: string, init: KeyboardEventInit = {}) {
    act(() => {
        window.dispatchEvent(
            new KeyboardEvent("keydown", {
                key,
                bubbles: true,
                ...init,
            }),
        )
    })
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useFulfillmentKeyboardShortcuts", () => {
    it("saves the draft on Ctrl+S when the user can execute", () => {
        const { handlers } = renderShortcuts()
        pressKey("s", { ctrlKey: true })
        expect(handlers.onSave).toHaveBeenCalledTimes(1)
    })

    it("does not save on Ctrl+S for read-only roles", () => {
        const { handlers } = renderShortcuts({ canExecute: false })
        pressKey("s", { ctrlKey: true })
        expect(handlers.onSave).not.toHaveBeenCalled()
    })

    it("opens the confirm dialog on Ctrl+Enter when posting is allowed", () => {
        const { handlers } = renderShortcuts()
        pressKey("Enter", { ctrlKey: true })
        expect(handlers.onConfirm).toHaveBeenCalledTimes(1)
    })

    it("does not confirm while a formal action is pending", () => {
        const { handlers } = renderShortcuts({ formalPending: true })
        pressKey("Enter", { ctrlKey: true })
        expect(handlers.onConfirm).not.toHaveBeenCalled()
    })

    it("navigates down on j and up on k", () => {
        const { handlers } = renderShortcuts()
        pressKey("j")
        pressKey("k")
        expect(handlers.onNavigate).toHaveBeenNthCalledWith(1, 1)
        expect(handlers.onNavigate).toHaveBeenNthCalledWith(2, -1)
    })

    it("blocks navigation while the draft is dirty", () => {
        const { handlers } = renderShortcuts({ dirty: true })
        pressKey("j")
        expect(handlers.onNavigate).not.toHaveBeenCalled()
    })

    it("toggles the shortcut help on ?", () => {
        const { handlers } = renderShortcuts()
        pressKey("?")
        expect(handlers.onToggleShortcuts).toHaveBeenCalledTimes(1)
    })

    it("keeps Ctrl+Enter inactive inside input fields", () => {
        const { handlers } = renderShortcuts()
        const input = document.createElement("input")
        document.body.appendChild(input)
        act(() => {
            input.dispatchEvent(
                new KeyboardEvent("keydown", {
                    key: "Enter",
                    ctrlKey: true,
                    bubbles: true,
                }),
            )
        })
        expect(handlers.onConfirm).not.toHaveBeenCalled()
        input.remove()
    })

    it("ignores j/k while typing in a field", () => {
        const { handlers } = renderShortcuts()
        const input = document.createElement("textarea")
        document.body.appendChild(input)
        act(() => {
            input.dispatchEvent(
                new KeyboardEvent("keydown", {
                    key: "j",
                    bubbles: true,
                }),
            )
        })
        expect(handlers.onNavigate).not.toHaveBeenCalled()
        input.remove()
    })

    it("keeps ArrowDown on queue buttons native (no hijack)", () => {
        const { handlers } = renderShortcuts()
        const button = document.createElement("button")
        document.body.appendChild(button)
        act(() => {
            button.dispatchEvent(
                new KeyboardEvent("keydown", {
                    key: "ArrowDown",
                    bubbles: true,
                }),
            )
        })
        expect(handlers.onNavigate).not.toHaveBeenCalled()
        button.remove()
    })
})
