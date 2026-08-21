import { describe, it, expect, afterEach } from "vitest"
import { renderHook, act } from "@testing-library/react"

import { useExecutionProjectionSearch } from "./use-execution-projection-search"

afterEach(() => {
    document.body.innerHTML = ""
})

describe("useExecutionProjectionSearch", () => {
    it("初始草稿取 URL 中的 q", () => {
        const { result } = renderHook(() => useExecutionProjectionSearch("SO-1"))
        expect(result.current.searchDraft).toBe("SO-1")
    })

    it("输入只改草稿，不写 URL（提交由筛选表单统一承担）", () => {
        const { result } = renderHook(() => useExecutionProjectionSearch(""))
        act(() => {
            result.current.setSearchDraft("abc")
        })
        expect(result.current.searchDraft).toBe("abc")
    })

    it("q 变化且输入框未聚焦时回填草稿", () => {
        const { result, rerender } = renderHook(
            ({ q }: { q: string }) => useExecutionProjectionSearch(q),
            { initialProps: { q: "" } },
        )
        rerender({ q: "NEW" })
        expect(result.current.searchDraft).toBe("NEW")
    })

    it("输入框聚焦时 q 变化不覆盖草稿", () => {
        const input = document.createElement("input")
        document.body.appendChild(input)
        const { result, rerender } = renderHook(
            ({ q }: { q: string }) => useExecutionProjectionSearch(q),
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
        const { result } = renderHook(() => useExecutionProjectionSearch(""))
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

    it("弹层或抽屉打开时 / 不聚焦搜索框", () => {
        const input = document.createElement("input")
        document.body.appendChild(input)
        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)
        const { result } = renderHook(() => useExecutionProjectionSearch(""))
        result.current.searchInputRef.current = input
        input.focus()
        input.blur()

        act(() => {
            window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }))
        })
        expect(document.activeElement).not.toBe(input)

        document.body.removeChild(input)
        document.body.removeChild(dialog)
    })
})
