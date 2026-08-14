import { describe, it, expect, afterEach } from "vitest"
import { act, cleanup, renderHook } from "@testing-library/react"
import * as React from "react"

import type { ResultState } from "@/components/business/feedback"
import { useSettlementResultFocus } from "./use-settlement-result-focus"

function renderWithResult() {
    const { result, rerender } = renderHook(
        ({ state }: { state: ResultState }) => {
            const ref = React.useRef<HTMLDivElement | null>(null)
            useSettlementResultFocus(state, ref)
            return ref
        },
        { initialProps: { state: null as ResultState } },
    )
    return { result, rerender }
}

describe("useSettlementResultFocus", () => {
    afterEach(() => {
        cleanup()
        document.body.innerHTML = ""
    })

    it("does not focus anything while there is no result", () => {
        const { result } = renderWithResult()
        const div = document.createElement("div")
        div.tabIndex = -1
        document.body.appendChild(div)
        act(() => {
            result.current.current = div
        })

        expect(document.activeElement).not.toBe(div)
    })

    it("focuses the result panel on a succeeded result", () => {
        const { result, rerender } = renderWithResult()
        const div = document.createElement("div")
        div.tabIndex = -1
        document.body.appendChild(div)
        act(() => {
            result.current.current = div
        })

        rerender({
            state: {
                status: "succeeded",
                title: "已完成",
                description: "处理结果已记录",
            },
        })

        expect(document.activeElement).toBe(div)
    })

    it("focuses the result panel when the outcome is unknown", () => {
        const { result, rerender } = renderWithResult()
        const div = document.createElement("div")
        div.tabIndex = -1
        document.body.appendChild(div)
        act(() => {
            result.current.current = div
        })

        rerender({
            state: {
                status: "unknown",
                title: "处理结果待确认",
                description: "请勿重复提交",
            },
        })

        expect(document.activeElement).toBe(div)
    })

    it("does not focus for rejected or blocked results", () => {
        const { result, rerender } = renderWithResult()
        const div = document.createElement("div")
        div.tabIndex = -1
        document.body.appendChild(div)
        act(() => {
            result.current.current = div
        })

        rerender({
            state: {
                status: "rejected",
                title: "未完成",
                description: "请重试",
            },
        })
        expect(document.activeElement).not.toBe(div)

        rerender({
            state: {
                status: "blocked",
                title: "暂不可用",
                description: "缺少依据",
            },
        })
        expect(document.activeElement).not.toBe(div)
    })
})
