import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act } from "@testing-library/react"

import { useExecutionProjectionSearch } from "./use-execution-projection-search"

describe("useExecutionProjectionSearch", () => {
    beforeEach(() => {
        vi.useFakeTimers()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it("初始草稿取 URL 中的 q", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionSearch({ q: "SO-1", replaceParams: vi.fn() }),
        )
        expect(result.current.searchDraft).toBe("SO-1")
    })

    it("输入 300ms 防抖后把搜索词与分页写回 URL", () => {
        const replaceParams = vi.fn()
        const { result } = renderHook(() =>
            useExecutionProjectionSearch({ q: "", replaceParams }),
        )

        act(() => {
            result.current.setSearchDraft("abc")
        })
        expect(replaceParams).not.toHaveBeenCalled()
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(replaceParams).toHaveBeenCalledWith({ q: "abc", page: "1" })
    })

    it("草稿与 q 一致（含首尾空白）时不写 URL", () => {
        const replaceParams = vi.fn()
        const { result } = renderHook(() =>
            useExecutionProjectionSearch({ q: "abc", replaceParams }),
        )
        act(() => {
            result.current.setSearchDraft(" abc ")
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(replaceParams).not.toHaveBeenCalled()
    })

    it("清空草稿时以 null 移除 q 参数", () => {
        const replaceParams = vi.fn()
        const { result } = renderHook(() =>
            useExecutionProjectionSearch({ q: "abc", replaceParams }),
        )
        act(() => {
            result.current.setSearchDraft("   ")
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(replaceParams).toHaveBeenCalledWith({ q: null, page: "1" })
    })

    it("卸载后不再触发未到期的防抖写回", () => {
        const replaceParams = vi.fn()
        const { result, unmount } = renderHook(() =>
            useExecutionProjectionSearch({ q: "", replaceParams }),
        )
        act(() => {
            result.current.setSearchDraft("abc")
        })
        unmount()
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(replaceParams).not.toHaveBeenCalled()
    })

    it("q 变化且输入框未聚焦时回填草稿", () => {
        const { result, rerender } = renderHook(
            ({ q }: { q: string }) =>
                useExecutionProjectionSearch({ q, replaceParams: vi.fn() }),
            { initialProps: { q: "" } },
        )
        rerender({ q: "NEW" })
        expect(result.current.searchDraft).toBe("NEW")
    })

    it("输入框聚焦时 q 变化不覆盖草稿", () => {
        const input = document.createElement("input")
        document.body.appendChild(input)
        const { result, rerender } = renderHook(
            ({ q }: { q: string }) =>
                useExecutionProjectionSearch({ q, replaceParams: vi.fn() }),
            { initialProps: { q: "" } },
        )
        result.current.searchInputRef.current = input
        input.focus()
        act(() => {
            rerender({ q: "NEW" })
        })
        expect(result.current.searchDraft).toBe("")
        document.body.removeChild(input)
    })

    it("按 / 聚焦搜索输入框，输入控件内不抢占", () => {
        const input = document.createElement("input")
        document.body.appendChild(input)
        const { result } = renderHook(() =>
            useExecutionProjectionSearch({ q: "", replaceParams: vi.fn() }),
        )
        result.current.searchInputRef.current = input

        act(() => {
            window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }))
        })
        expect(document.activeElement).toBe(input)

        input.blur()
        const typing = document.createElement("textarea")
        document.body.appendChild(typing)
        typing.focus()
        act(() => {
            typing.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", bubbles: true }),
            )
        })
        expect(document.activeElement).toBe(typing)

        document.body.removeChild(input)
        document.body.removeChild(typing)
    })
})
