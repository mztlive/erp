import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, cleanup } from "@testing-library/react"

import {
    useProcurementKeyboardShortcuts,
    type ProcurementKeyboardShortcutsOptions,
} from "./use-procurement-keyboard-shortcuts"

const options = (): ProcurementKeyboardShortcutsOptions => ({
    allowedActions: [
        "START_PROCESSING",
        "SAVE",
        "APPROVE",
        "REJECT",
        "RELEASE_TO_TEAM",
    ],
    searchInputRef: { current: null },
    onSave: vi.fn(),
    onConfirmApprove: vi.fn(),
    onNavigate: vi.fn(),
})

function renderShortcuts(
    overrides: Partial<ProcurementKeyboardShortcutsOptions> = {},
) {
    const handlers = { ...options(), ...overrides }
    const utils = renderHook(
        (props: ProcurementKeyboardShortcutsOptions) =>
            useProcurementKeyboardShortcuts(props),
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

describe("useProcurementKeyboardShortcuts", () => {
    it("saves on Ctrl+S", () => {
        const { handlers } = renderShortcuts()
        pressKey("s", { ctrlKey: true })
        expect(handlers.onSave).toHaveBeenCalledTimes(1)
    })

    it("saves on Meta+S", () => {
        const { handlers } = renderShortcuts()
        pressKey("s", { metaKey: true })
        expect(handlers.onSave).toHaveBeenCalledTimes(1)
    })

    it("opens the approve dialog on Ctrl+Enter when APPROVE is allowed", () => {
        const { handlers } = renderShortcuts()
        pressKey("Enter", { ctrlKey: true })
        expect(handlers.onConfirmApprove).toHaveBeenCalledTimes(1)
    })

    it("opens the approve dialog on Ctrl+Enter once SAVE is available", () => {
        const { handlers } = renderShortcuts({
            allowedActions: ["SAVE", "REJECT"],
        })
        pressKey("Enter", { ctrlKey: true })
        expect(handlers.onConfirmApprove).toHaveBeenCalledTimes(1)
    })

    it("keeps Ctrl+Enter inactive before the operator can work the confirmation", () => {
        const { handlers } = renderShortcuts({
            allowedActions: ["START_PROCESSING", "REJECT"],
        })
        pressKey("Enter", { ctrlKey: true })
        expect(handlers.onConfirmApprove).not.toHaveBeenCalled()
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
        expect(handlers.onConfirmApprove).not.toHaveBeenCalled()
        input.remove()
    })

    it("navigates down on j and up on k", () => {
        const { handlers } = renderShortcuts()
        pressKey("j")
        pressKey("k")
        expect(handlers.onNavigate).toHaveBeenNthCalledWith(1, 1)
        expect(handlers.onNavigate).toHaveBeenNthCalledWith(2, -1)
    })

    it("navigates on arrow keys", () => {
        const { handlers } = renderShortcuts()
        pressKey("ArrowDown")
        pressKey("ArrowUp")
        expect(handlers.onNavigate).toHaveBeenNthCalledWith(1, 1)
        expect(handlers.onNavigate).toHaveBeenNthCalledWith(2, -1)
    })

    it("does not navigate while typing in a field", () => {
        const { handlers } = renderShortcuts()
        const input = document.createElement("input")
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

    it("focuses the orderNo search on /", () => {
        const input = document.createElement("input")
        document.body.appendChild(input)
        renderShortcuts({ searchInputRef: { current: input } })
        pressKey("/")
        expect(document.activeElement).toBe(input)
        input.remove()
    })

    it("updates the shortcuts when props change", () => {
        const { handlers, rerender } = renderShortcuts()
        pressKey("Enter", { ctrlKey: true })
        expect(handlers.onConfirmApprove).toHaveBeenCalledTimes(1)
        rerender({ ...options(), allowedActions: ["START_PROCESSING"] })
        pressKey("Enter", { ctrlKey: true })
        expect(handlers.onConfirmApprove).toHaveBeenCalledTimes(1)
    })
})
