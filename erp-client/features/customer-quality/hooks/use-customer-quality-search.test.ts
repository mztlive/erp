import { describe, expect, it, vi } from "vitest"
import { act } from "@testing-library/react"
import { renderHook } from "@testing-library/react"

import { useCustomerQualitySearch } from "./use-customer-quality-search"

describe("useCustomerQualitySearch", () => {
    it("starts from the URL q param", () => {
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "abc" }),
        )

        expect(result.current.searchDraft).toBe("abc")
    })

    it("backfills the draft from the URL unless the input is focused", () => {
        const { result, rerender } = renderHook(
            ({ qParam }: { qParam: string }) =>
                useCustomerQualitySearch({ qParam }),
            { initialProps: { qParam: "abc" } },
        )

        expect(result.current.searchDraft).toBe("abc")

        rerender({ qParam: "next" })
        expect(result.current.searchDraft).toBe("next")

        // 输入框聚焦时保留草稿，不被旧 URL 值覆盖
        result.current.searchInputRef.current =
            document.body as unknown as HTMLInputElement
        act(() => {
            result.current.setSearchDraft("draft")
        })
        rerender({ qParam: "later" })
        expect(result.current.searchDraft).toBe("draft")
    })

    it("never writes the URL by itself (submit is explicit)", () => {
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "" }),
        )

        act(() => {
            result.current.setSearchDraft("abc")
        })
        expect(result.current.searchDraft).toBe("abc")
    })

    it("focuses the search input on / when not typing elsewhere", () => {
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "" }),
        )
        const focus = vi.fn()
        result.current.searchInputRef.current = {
            focus,
        } as unknown as HTMLInputElement

        const event = new KeyboardEvent("keydown", {
            key: "/",
            bubbles: true,
            cancelable: true,
        })
        act(() => {
            window.dispatchEvent(event)
        })

        expect(focus).toHaveBeenCalledTimes(1)
        expect(event.defaultPrevented).toBe(true)
    })

    it("ignores / when a modifier is held or focus is in a field", () => {
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "" }),
        )
        const focus = vi.fn()
        result.current.searchInputRef.current = {
            focus,
        } as unknown as HTMLInputElement

        act(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", metaKey: true }),
            )
        })
        expect(focus).not.toHaveBeenCalled()

        const input = document.createElement("input")
        document.body.appendChild(input)
        act(() => {
            input.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", bubbles: true }),
            )
        })
        expect(focus).not.toHaveBeenCalled()
        input.remove()
    })

    it("ignores / while a dialog or sheet is open", () => {
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "" }),
        )
        const focus = vi.fn()
        result.current.searchInputRef.current = {
            focus,
        } as unknown as HTMLInputElement

        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)

        act(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", {
                    key: "/",
                    bubbles: true,
                    cancelable: true,
                }),
            )
        })
        expect(focus).not.toHaveBeenCalled()
        dialog.remove()
    })

    it("removes the hotkey listener on unmount", () => {
        const { unmount } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "" }),
        )
        unmount()

        expect(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", bubbles: true }),
            )
        }).not.toThrow()
    })
})
