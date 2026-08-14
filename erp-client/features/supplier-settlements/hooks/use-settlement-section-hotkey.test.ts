import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { cleanup, renderHook } from "@testing-library/react"

import { useSettlementSectionHotkey } from "./use-settlement-section-hotkey"

function pressKey(key: string, init: KeyboardEventInit = {}) {
    window.dispatchEvent(
        new KeyboardEvent("keydown", { key, cancelable: true, ...init }),
    )
}

describe("useSettlementSectionHotkey", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    afterEach(() => {
        cleanup()
        document.body.innerHTML = ""
    })

    it("switches to the differences section when d is pressed", () => {
        const patchUrl = vi.fn()
        renderHook(() => useSettlementSectionHotkey(patchUrl))

        pressKey("d")

        expect(patchUrl).toHaveBeenCalledWith({ section: "differences" })
    })

    it("prevents the default behaviour for the d key", () => {
        const patchUrl = vi.fn()
        renderHook(() => useSettlementSectionHotkey(patchUrl))
        const event = new KeyboardEvent("keydown", {
            key: "d",
            cancelable: true,
        })
        window.dispatchEvent(event)

        expect(event.defaultPrevented).toBe(true)
        expect(patchUrl).toHaveBeenCalledTimes(1)
    })

    it("ignores other keys and modifier combinations", () => {
        const patchUrl = vi.fn()
        renderHook(() => useSettlementSectionHotkey(patchUrl))

        pressKey("a")
        pressKey("d", { metaKey: true })
        pressKey("d", { ctrlKey: true })

        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("ignores key presses inside inputs, selects and editable areas", () => {
        const patchUrl = vi.fn()
        renderHook(() => useSettlementSectionHotkey(patchUrl))

        const input = document.createElement("input")
        const select = document.createElement("select")
        const textarea = document.createElement("textarea")
        const contentEditable = document.createElement("div")
        contentEditable.contentEditable = "true"
        // jsdom 未实现 isContentEditable，测试环境补上让守卫生效
        Object.defineProperty(contentEditable, "isContentEditable", {
            value: true,
        })
        document.body.append(input, select, textarea, contentEditable)

        for (const target of [input, select, textarea, contentEditable]) {
            target.dispatchEvent(
                new KeyboardEvent("keydown", {
                    key: "d",
                    cancelable: true,
                    bubbles: true,
                }),
            )
        }

        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("removes the listener on unmount", () => {
        const patchUrl = vi.fn()
        const { unmount } = renderHook(() =>
            useSettlementSectionHotkey(patchUrl),
        )

        unmount()
        pressKey("d")

        expect(patchUrl).not.toHaveBeenCalled()
    })
})
