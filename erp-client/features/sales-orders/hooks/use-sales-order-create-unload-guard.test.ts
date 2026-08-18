import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook } from "@testing-library/react"

import { useSalesOrderCreateUnloadGuard } from "./use-sales-order-create-unload-guard"

describe("useSalesOrderCreateUnloadGuard", () => {
    beforeEach(() => {
        vi.spyOn(window, "addEventListener")
        vi.spyOn(window, "removeEventListener")
    })

    afterEach(() => {
        vi.restoreAllMocks()
    })

    it("does not register a listener while the form is clean", () => {
        renderHook(() => useSalesOrderCreateUnloadGuard(false))

        expect(window.addEventListener).not.toHaveBeenCalledWith(
            "beforeunload",
            expect.any(Function),
        )
    })

    it("registers a beforeunload listener when the form becomes dirty", () => {
        const { rerender } = renderHook(
            ({ dirty }: { dirty: boolean }) =>
                useSalesOrderCreateUnloadGuard(dirty),
            { initialProps: { dirty: false } },
        )

        rerender({ dirty: true })

        expect(window.addEventListener).toHaveBeenCalledWith(
            "beforeunload",
            expect.any(Function),
        )
    })

    it("removes the listener when the form becomes clean again", () => {
        const { rerender } = renderHook(
            ({ dirty }: { dirty: boolean }) =>
                useSalesOrderCreateUnloadGuard(dirty),
            { initialProps: { dirty: true } },
        )

        const handler = vi
            .mocked(window.addEventListener)
            .mock.calls.find(([event]) => event === "beforeunload")?.[1]

        rerender({ dirty: false })

        expect(window.removeEventListener).toHaveBeenCalledWith(
            "beforeunload",
            handler,
        )
    })

    it("removes the listener on unmount", () => {
        const { unmount } = renderHook(() =>
            useSalesOrderCreateUnloadGuard(true),
        )

        const handler = vi
            .mocked(window.addEventListener)
            .mock.calls.find(([event]) => event === "beforeunload")?.[1]

        unmount()

        expect(window.removeEventListener).toHaveBeenCalledWith(
            "beforeunload",
            handler,
        )
    })

    it("prevents default and sets a return value on the event", () => {
        renderHook(() => useSalesOrderCreateUnloadGuard(true))

        const handler = vi
            .mocked(window.addEventListener)
            .mock.calls.find(([event]) => event === "beforeunload")?.[1] as (
            event: BeforeUnloadEvent,
        ) => void

        const event = {
            preventDefault: vi.fn(),
        } as unknown as BeforeUnloadEvent
        handler(event)

        expect(event.preventDefault).toHaveBeenCalled()
        expect(event.returnValue).toBe("当前输入尚未提交，刷新后将丢失。")
    })
})
