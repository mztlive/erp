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
        expect(result.current.mappingType).toBeUndefined()
        expect(result.current.jobId).toBeUndefined()
        expect(result.current.queueContextId).toBe("queue:W17:mall-sync")
        expect(result.current.hasActiveFilters).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.mappingTypeDraft).toBe("all")
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

    it("degrades an invalid mappingType to the default", () => {
        mocks.params = new URLSearchParams("view=mapping&mappingType=bogus")
        const { result } = renderHook(() => useMallSyncUrlState())
        expect(result.current.mappingType).toBeUndefined()
        expect(result.current.mappingTypeDraft).toBe("all")
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.panelOpen).toBe(false)
    })

    it("opens the panel on an initial deep link with structured filters", () => {
        mocks.params = new URLSearchParams(
            "view=mapping&mappingType=CUSTOMER",
        )
        const { result } = renderHook(() => useMallSyncUrlState())
        expect(result.current.mappingType).toBe("CUSTOMER")
        expect(result.current.mappingTypeDraft).toBe("CUSTOMER")
        expect(result.current.hasStructuredFilters).toBe(true)
        expect(result.current.panelOpen).toBe(true)
    })

    it("keeps the search draft in sync with the q param", () => {
        mocks.params = new URLSearchParams("q=abc")
        const { result, rerender } = renderHook(() => useMallSyncUrlState())
        expect(result.current.searchDraft).toBe("abc")
        mocks.params = new URLSearchParams("q=xyz")
        rerender()
        expect(result.current.searchDraft).toBe("xyz")
    })

    it("does not write the URL while the draft is being edited", () => {
        const { result } = renderHook(() => useMallSyncUrlState())
        act(() => {
            result.current.setSearchDraft("xyz")
        })
        expect(mocks.replace).not.toHaveBeenCalled()
        expect(mocks.push).not.toHaveBeenCalled()
    })

    it("applies q and mappingType in one replace without page params", () => {
        mocks.params = new URLSearchParams("view=mapping&mappingTaskId=mt-1")
        const { result } = renderHook(() => useMallSyncUrlState())
        act(() => {
            result.current.setSearchDraft("  abc  ")
            result.current.setMappingTypeDraft("CUSTOMER")
        })
        act(() => {
            result.current.applyFilters()
        })
        expect(mocks.replace).toHaveBeenCalledWith(
            "/mall-sync?view=mapping&mappingTaskId=mt-1&q=abc&mappingType=CUSTOMER",
            { scroll: false },
        )
        expect(mocks.push).not.toHaveBeenCalled()
        expect(result.current.panelOpen).toBe(false)
    })

    it("omits default values from the URL on apply", () => {
        mocks.params = new URLSearchParams("view=mapping&q=abc")
        const { result } = renderHook(() => useMallSyncUrlState())
        act(() => {
            result.current.setSearchDraft("")
            result.current.setMappingTypeDraft("all")
        })
        act(() => {
            result.current.applyFilters()
        })
        expect(mocks.replace).toHaveBeenCalledWith(
            "/mall-sync?view=mapping",
            { scroll: false },
        )
    })

    it("keeps the panel closed after apply even when the URL backfills", () => {
        mocks.params = new URLSearchParams(
            "view=mapping&mappingType=CUSTOMER",
        )
        const { result, rerender } = renderHook(() => useMallSyncUrlState())
        expect(result.current.panelOpen).toBe(true)
        act(() => {
            result.current.applyFilters()
        })
        expect(result.current.panelOpen).toBe(false)
        // URL 回填（q 变化）只同步 Draft，不重新展开面板
        mocks.params = new URLSearchParams(
            "view=mapping&mappingType=CUSTOMER&q=abc",
        )
        rerender()
        expect(result.current.searchDraft).toBe("abc")
        expect(result.current.panelOpen).toBe(false)
    })

    it("resets only structured filters and keeps q on resetMoreFilters", () => {
        mocks.params = new URLSearchParams(
            "view=mapping&q=abc&mappingType=CUSTOMER",
        )
        const { result } = renderHook(() => useMallSyncUrlState())
        act(() => {
            result.current.resetMoreFilters()
        })
        expect(result.current.mappingTypeDraft).toBe("all")
        expect(mocks.replace).toHaveBeenCalledWith(
            "/mall-sync?view=mapping&q=abc",
            { scroll: false },
        )
    })

    it("removes a single condition and clears the paired work item", () => {
        mocks.params = new URLSearchParams(
            "view=mapping&mappingTaskId=m1&workItemId=w1&currentWorkItemId=w1",
        )
        const { result } = renderHook(() => useMallSyncUrlState())
        act(() => {
            result.current.removeFilter("mappingTaskId")
        })
        expect(mocks.replace).toHaveBeenCalledWith(
            "/mall-sync?view=mapping",
            { scroll: false },
        )
    })

    it("clears all filters and keeps the view", () => {
        mocks.params = new URLSearchParams(
            "view=mapping&mappingTaskId=m1&workItemId=w1&currentWorkItemId=w1&q=abc&jobId=j1&snapshotId=s1&differenceId=d1&mappingType=CUSTOMER",
        )
        const { result } = renderHook(() => useMallSyncUrlState())
        act(() => {
            result.current.clearAllFilters()
        })
        expect(result.current.searchDraft).toBe("")
        expect(result.current.mappingTypeDraft).toBe("all")
        expect(result.current.panelOpen).toBe(false)
        expect(mocks.replace).toHaveBeenCalledWith("/mall-sync?view=mapping", {
            scroll: false,
        })
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

    it("does not steal focus on / while a dialog is open", () => {
        const { result } = renderHook(() => useMallSyncUrlState())
        const input = document.createElement("input")
        document.body.appendChild(input)
        result.current.searchInputRef.current = input
        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)

        act(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", bubbles: true }),
            )
        })
        expect(document.activeElement).not.toBe(input)

        dialog.remove()
        input.remove()
    })
})
