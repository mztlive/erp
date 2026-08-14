import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    params: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: mocks.push,
        replace: mocks.replace,
        back: mocks.back,
    }),
    useSearchParams: () => mocks.params,
    usePathname: () => "/mall-sync",
    useParams: () => ({}),
}))

import { useMallSyncUrlState } from "./use-mall-sync-url-state"

beforeEach(() => {
    vi.clearAllMocks()
    mocks.params = new URLSearchParams()
})

describe("useMallSyncUrlState", () => {
    it("falls back to defaults with empty params", () => {
        const { result } = renderHook(() => useMallSyncUrlState())
        expect(result.current.view).toBe("overview")
        expect(result.current.q).toBe("")
        expect(result.current.jobId).toBeUndefined()
        expect(result.current.queueContextId).toBe("queue:W17:mall-sync")
        expect(result.current.hasActiveFilters).toBe(false)
    })

    it("parses every object param from the URL", () => {
        mocks.params = new URLSearchParams(
            "view=jobs&jobId=j1&q=abc&snapshotId=s1&mappingTaskId=m1&workItemId=w1&differenceId=d1",
        )
        const { result } = renderHook(() => useMallSyncUrlState())
        expect(result.current.view).toBe("jobs")
        expect(result.current.q).toBe("abc")
        expect(result.current.jobId).toBe("j1")
        expect(result.current.snapshotId).toBe("s1")
        expect(result.current.mappingTaskId).toBe("m1")
        expect(result.current.workItemId).toBe("w1")
        expect(result.current.differenceId).toBe("d1")
        expect(result.current.hasActiveFilters).toBe(true)
    })

    it("falls back to currentWorkItemId for the work item id", () => {
        mocks.params = new URLSearchParams("currentWorkItemId=w2")
        const { result } = renderHook(() => useMallSyncUrlState())
        expect(result.current.workItemId).toBe("w2")
    })

    it("falls back to overview for an unknown view", () => {
        mocks.params = new URLSearchParams("view=bogus")
        const { result } = renderHook(() => useMallSyncUrlState())
        expect(result.current.view).toBe("overview")
    })

    it("keeps the search input in sync with the q param", () => {
        mocks.params = new URLSearchParams("q=abc")
        const { result, rerender } = renderHook(() => useMallSyncUrlState())
        expect(result.current.searchInput).toBe("abc")
        mocks.params = new URLSearchParams("q=xyz")
        rerender()
        expect(result.current.searchInput).toBe("xyz")
    })

    it("debounces search input into the URL with replace", () => {
        vi.useFakeTimers()
        try {
            mocks.params = new URLSearchParams("view=jobs&jobId=j1&q=abc")
            const { result } = renderHook(() => useMallSyncUrlState())

            act(() => {
                result.current.setSearchInput("xyz")
            })
            act(() => {
                result.current.setSearchInput("abc")
            })
            act(() => {
                vi.advanceTimersByTime(350)
            })
            // 回到原值：防抖回调不应写 URL
            expect(mocks.replace).not.toHaveBeenCalled()

            act(() => {
                result.current.setSearchInput("  xyz  ")
            })
            act(() => {
                vi.advanceTimersByTime(300)
            })
            expect(mocks.replace).toHaveBeenCalledWith(
                "/mall-sync?view=jobs&jobId=j1&q=xyz",
            )

            act(() => {
                result.current.setSearchInput("")
            })
            act(() => {
                vi.advanceTimersByTime(300)
            })
            expect(mocks.replace).toHaveBeenLastCalledWith(
                "/mall-sync?view=jobs&jobId=j1",
            )
        } finally {
            vi.useRealTimers()
        }
    })

    it("clears all filters and keeps the view", () => {
        mocks.params = new URLSearchParams(
            "view=mapping&mappingTaskId=m1&workItemId=w1&currentWorkItemId=w1&q=abc&jobId=j1&snapshotId=s1&differenceId=d1",
        )
        const { result } = renderHook(() => useMallSyncUrlState())
        act(() => {
            result.current.clearAllFilters()
        })
        expect(result.current.searchInput).toBe("")
        expect(mocks.replace).toHaveBeenCalledWith("/mall-sync?view=mapping")
        expect(mocks.push).not.toHaveBeenCalled()
    })

    it("builds the object param cleanup patch per target view", () => {
        const { result } = renderHook(() => useMallSyncUrlState())
        expect(result.current.clearObjectParamsForView("jobs")).toEqual({
            snapshotId: null,
            mappingTaskId: null,
            workItemId: null,
            currentWorkItemId: null,
            differenceId: null,
        })
        expect(result.current.clearObjectParamsForView("mapping")).toEqual({
            jobId: null,
            snapshotId: null,
            differenceId: null,
        })
        expect(result.current.clearObjectParamsForView("overview")).toEqual({
            jobId: null,
            snapshotId: null,
            mappingTaskId: null,
            workItemId: null,
            currentWorkItemId: null,
            differenceId: null,
        })
    })

    it("pushes (not replaces) when patchUrl is called without replace", () => {
        const { result } = renderHook(() => useMallSyncUrlState())
        act(() => {
            result.current.patchUrl({ view: "history" })
        })
        expect(mocks.push).toHaveBeenCalledWith("/mall-sync?view=history")
        expect(mocks.replace).not.toHaveBeenCalled()
    })

    it("focuses the search input on / outside editable elements", () => {
        const { result } = renderHook(() => useMallSyncUrlState())
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input

        act(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", bubbles: true }),
            )
        })
        expect(document.activeElement).toBe(input)

        // 在可编辑元素内按 / 不应抢焦点
        const other = document.createElement("input")
        document.body.appendChild(other)
        other.focus()
        act(() => {
            other.dispatchEvent(
                new KeyboardEvent("keydown", {
                    key: "/",
                    bubbles: true,
                    cancelable: true,
                }),
            )
        })
        expect(document.activeElement).toBe(other)

        input.remove()
        other.remove()
    })
})
