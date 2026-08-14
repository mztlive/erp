import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useQueueUrlState } from "./use-queue-url-state"

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    back: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => "/workspace/tasks",
    useParams: () => ({}),
}))

const FOCUS_KEY = "w02.focus-work-item"

describe("useQueueUrlState", () => {
    beforeEach(() => {
        navMocks.searchParams = new URLSearchParams()
        navMocks.replace.mockClear()
        window.sessionStorage.clear()
    })

    it("falls back to defaults on an empty URL", () => {
        const { result } = renderHook(() => useQueueUrlState())

        expect(result.current.approvalBlockers).toBe(false)
        expect(result.current.scope).toBe("mine")
        expect(result.current.family).toBeUndefined()
        expect(result.current.due).toBeUndefined()
        expect(result.current.priorities).toBeUndefined()
        expect(result.current.sort).toBe("priority_due")
        expect(result.current.historyStatus).toBeUndefined()
        expect(result.current.workItemType).toBeUndefined()
        expect(result.current.queryText).toBe("")
        expect(result.current.queueContextId).toBeUndefined()
        expect(result.current.currentWorkItemId).toBeUndefined()
    })

    it("parses every filter from the URL and prefers it over stored focus", () => {
        window.sessionStorage.setItem(FOCUS_KEY, "stored-focus")
        navMocks.searchParams = new URLSearchParams(
            "scope=history&family=finance&due=overdue&priority=1,2&sort=due_asc&status=closed&type=po_review&q=abc&queueContextId=q1&currentWorkItemId=wi-1",
        )
        const { result } = renderHook(() => useQueueUrlState())

        expect(result.current.scope).toBe("history")
        expect(result.current.family).toBe("finance")
        expect(result.current.due).toBe("overdue")
        expect(result.current.priorities).toEqual([1, 2])
        expect(result.current.sort).toBe("due_asc")
        expect(result.current.historyStatus).toBe("CLOSED")
        expect(result.current.workItemType).toBe("po_review")
        expect(result.current.queryText).toBe("abc")
        expect(result.current.queueContextId).toBe("q1")
        expect(result.current.currentWorkItemId).toBe("wi-1")
    })

    it("reads the focus id from session storage when the URL omits it", () => {
        window.sessionStorage.setItem(FOCUS_KEY, "stored-focus")
        const { result } = renderHook(() => useQueueUrlState())

        expect(result.current.currentWorkItemId).toBe("stored-focus")
    })

    it("treats the history scope as completed when no status is present", () => {
        navMocks.searchParams = new URLSearchParams("scope=history")
        const { result } = renderHook(() => useQueueUrlState())

        expect(result.current.historyStatus).toBe("COMPLETED")
    })

    it("replaces the URL with the minimal default query for a no-op update", () => {
        const { result } = renderHook(() => useQueueUrlState())

        act(() => {
            result.current.replaceUrl({})
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/workspace/tasks?scope=mine&sort=priority_due",
            { scroll: false },
        )
    })

    it("persists the selected item id as the focus id and in the URL", () => {
        const { result } = renderHook(() => useQueueUrlState())

        act(() => {
            result.current.replaceUrl({ currentWorkItemId: "wi-9" })
        })

        expect(window.sessionStorage.getItem(FOCUS_KEY)).toBe("wi-9")
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/workspace/tasks?scope=mine&sort=priority_due&currentWorkItemId=wi-9",
            { scroll: false },
        )
    })

    it("clears filters, the query, and the stored focus id on clear", () => {
        window.sessionStorage.setItem(FOCUS_KEY, "wi-1")
        navMocks.searchParams = new URLSearchParams(
            "family=approval&due=today&priority=1&q=abc&currentWorkItemId=wi-1",
        )
        const { result } = renderHook(() => useQueueUrlState())

        act(() => {
            result.current.replaceUrl({
                family: null,
                due: null,
                priorities: null,
                query: null,
                currentWorkItemId: null,
            })
        })

        expect(window.sessionStorage.getItem(FOCUS_KEY)).toBeNull()
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/workspace/tasks?scope=mine&sort=priority_due",
            { scroll: false },
        )
    })

    it("enters and leaves the approval blocker view", () => {
        navMocks.searchParams = new URLSearchParams("view=approval-blockers")
        const { result } = renderHook(() => useQueueUrlState())

        expect(result.current.approvalBlockers).toBe(true)

        act(() => {
            result.current.replaceUrl({ approvalBlockers: false })
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/workspace/tasks?scope=mine&sort=priority_due",
            { scroll: false },
        )
    })

    it("keeps the current item id when overrides omit it", () => {
        navMocks.searchParams = new URLSearchParams("currentWorkItemId=wi-5")
        const { result } = renderHook(() => useQueueUrlState())

        act(() => {
            result.current.replaceUrl({ family: "finance" })
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/workspace/tasks?scope=mine&family=finance&sort=priority_due&currentWorkItemId=wi-5",
            { scroll: false },
        )
    })
})
