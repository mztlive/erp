import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { act } from "@testing-library/react"
import { renderHook } from "@testing-library/react"

import { useCustomerQualitySearch } from "./use-customer-quality-search"

const mocks = vi.hoisted(() => ({
    patchUrl: vi.fn(),
}))

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    vi.useRealTimers()
})

describe("useCustomerQualitySearch", () => {
    it("starts from the URL q param", () => {
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "abc", patchUrl: mocks.patchUrl }),
        )

        expect(result.current.searchInput).toBe("abc")
    })

    it("debounces input changes into the URL q param", () => {
        vi.useFakeTimers()
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "", patchUrl: mocks.patchUrl }),
        )

        act(() => {
            result.current.setSearchInput("abc")
        })
        expect(mocks.patchUrl).not.toHaveBeenCalled()

        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(mocks.patchUrl).toHaveBeenCalledWith({ q: "abc" })
    })

    it("writes null for whitespace-only input", () => {
        vi.useFakeTimers()
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "abc", patchUrl: mocks.patchUrl }),
        )

        act(() => {
            result.current.setSearchInput("   ")
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })

        expect(mocks.patchUrl).toHaveBeenCalledWith({ q: null })
    })

    it("does not write the URL when the trimmed input equals the URL value", () => {
        vi.useFakeTimers()
        renderHook(() =>
            useCustomerQualitySearch({ qParam: "abc", patchUrl: mocks.patchUrl }),
        )

        act(() => {
            vi.advanceTimersByTime(300)
        })

        expect(mocks.patchUrl).not.toHaveBeenCalled()
    })

    it("backfills the input from the URL unless the input is focused", () => {
        const { result, rerender } = renderHook(
            ({ qParam }: { qParam: string }) =>
                useCustomerQualitySearch({
                    qParam,
                    patchUrl: mocks.patchUrl,
                }),
            { initialProps: { qParam: "abc" } },
        )

        expect(result.current.searchInput).toBe("abc")

        rerender({ qParam: "next" })
        expect(result.current.searchInput).toBe("next")

        // 输入框聚焦时保留草稿，不被旧 URL 值覆盖
        result.current.searchInputRef.current =
            document.body as unknown as HTMLInputElement
        act(() => {
            result.current.setSearchInput("draft")
        })
        rerender({ qParam: "later" })
        expect(result.current.searchInput).toBe("draft")
    })

    it("focuses the search input on / when not typing elsewhere", () => {
        const { result } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "", patchUrl: mocks.patchUrl }),
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
            useCustomerQualitySearch({ qParam: "", patchUrl: mocks.patchUrl }),
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

    it("removes the hotkey listener on unmount", () => {
        const { unmount } = renderHook(() =>
            useCustomerQualitySearch({ qParam: "", patchUrl: mocks.patchUrl }),
        )
        unmount()

        expect(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", bubbles: true }),
            )
        }).not.toThrow()
    })
})
