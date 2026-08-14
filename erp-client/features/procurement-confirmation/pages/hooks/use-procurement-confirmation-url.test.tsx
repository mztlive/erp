import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, cleanup } from "@testing-library/react"

import {
    useProcurementConfirmationQueueUrlSync,
    useProcurementConfirmationUrl,
} from "./use-procurement-confirmation-url"

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => "/procurement/confirm",
    useParams: () => ({}),
}))

beforeEach(() => {
    navMocks.searchParams = new URLSearchParams()
    navMocks.replace.mockClear()
    navMocks.push.mockClear()
})

afterEach(() => {
    cleanup()
    vi.useRealTimers()
})

function lastReplacedUrl() {
    const [url] = navMocks.replace.mock.lastCall ?? [""]
    return String(url ?? "")
}

function lastReplacedParams() {
    const url = lastReplacedUrl()
    const index = url.indexOf("?")
    return new URLSearchParams(index >= 0 ? url.slice(index + 1) : "")
}

describe("useProcurementConfirmationUrl", () => {
    it("parses an empty URL into defaults", () => {
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        expect(result.current.scope).toBe("mine")
        expect(result.current.due).toBe("active")
        expect(result.current.sort).toBe("due_at")
        expect(result.current.orderNo).toBeUndefined()
        expect(result.current.currentWorkItemId).toBeUndefined()
        expect(result.current.queueContextId).toBe(
            "queue:procurement-confirmation:mine",
        )
        expect(result.current.autoNext).toBe(true)
        expect(result.current.hasActiveFilter).toBe(false)
        expect(result.current.orderNoDraft).toBe("")
        expect(result.current.returnTo).toBe("/procurement/confirm")
    })

    it("parses scope/due/sort/orderNo from the URL", () => {
        navMocks.searchParams = new URLSearchParams(
            "scope=team&due=overdue&sort=submitted_at&orderNo=SO-1",
        )
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        expect(result.current.scope).toBe("team")
        expect(result.current.due).toBe("overdue")
        expect(result.current.sort).toBe("submitted_at")
        expect(result.current.orderNo).toBe("SO-1")
        expect(result.current.hasActiveFilter).toBe(true)
        expect(result.current.orderNoDraft).toBe("SO-1")
    })

    it("reads the legacy task param as currentWorkItemId", () => {
        navMocks.searchParams = new URLSearchParams("task=wi_9")
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        expect(result.current.currentWorkItemId).toBe("wi_9")
    })

    it("honours explicit autoNext=0 over the session default", () => {
        navMocks.searchParams = new URLSearchParams("autoNext=0")
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        expect(result.current.autoNext).toBe(false)
    })

    it("keeps the orderNo draft in sync with the URL", () => {
        const { result, rerender } = renderHook(() =>
            useProcurementConfirmationUrl(),
        )
        expect(result.current.orderNoDraft).toBe("")
        navMocks.searchParams = new URLSearchParams("orderNo=SO-2")
        rerender()
        expect(result.current.orderNoDraft).toBe("SO-2")
    })

    it("debounces orderNo draft edits into a replace URL", () => {
        vi.useFakeTimers()
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        act(() => {
            result.current.setOrderNoDraft("SO-3")
        })
        expect(navMocks.replace).not.toHaveBeenCalled()
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(lastReplacedParams().get("orderNo")).toBe("SO-3")
        expect(lastReplacedParams().has("currentWorkItemId")).toBe(false)
    })

    it("does not write the URL when the debounced draft equals orderNo", () => {
        vi.useFakeTimers()
        renderHook(() => useProcurementConfirmationUrl())
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it("commits the orderNo immediately on commitOrderNo", () => {
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        act(() => {
            result.current.setOrderNoDraft("  SO-4  ")
        })
        act(() => {
            result.current.commitOrderNo()
        })
        expect(lastReplacedParams().get("orderNo")).toBe("SO-4")
    })

    it("clearFilters removes orderNo/due and keeps scope-like params out", () => {
        navMocks.searchParams = new URLSearchParams(
            "orderNo=SO-1&due=today&currentWorkItemId=wi_1&sort=priority",
        )
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        act(() => {
            result.current.clearFilters()
        })
        const params = lastReplacedParams()
        expect(params.has("orderNo")).toBe(false)
        expect(params.has("due")).toBe(false)
        expect(params.has("currentWorkItemId")).toBe(false)
        expect(params.get("sort")).toBe("priority")
    })

    it("toggleAutoNext writes the explicit URL value", () => {
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        act(() => {
            result.current.toggleAutoNext(false)
        })
        expect(lastReplacedParams().get("autoNext")).toBe("0")
        expect(result.current.autoNext).toBe(false)
    })

    it("handleScopeChange drops queueContextId and current item", () => {
        navMocks.searchParams = new URLSearchParams(
            "scope=mine&queueContextId=queue:procurement-confirmation:mine&currentWorkItemId=wi_1",
        )
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        act(() => {
            result.current.handleScopeChange("team")
        })
        const params = lastReplacedParams()
        expect(params.get("scope")).toBe("team")
        expect(params.has("queueContextId")).toBe(false)
        expect(params.has("currentWorkItemId")).toBe(false)
    })

    it("handleDueChange clears due when switching back to active", () => {
        navMocks.searchParams = new URLSearchParams("due=today")
        const { result } = renderHook(() => useProcurementConfirmationUrl())
        act(() => {
            result.current.handleDueChange("active")
        })
        expect(lastReplacedParams().has("due")).toBe(false)
    })
})

describe("useProcurementConfirmationQueueUrlSync", () => {
    function renderSync(overrides: {
        queueReady?: boolean
        tasksLength?: number
        currentTaskWorkItemId?: string
        scope?: "mine" | "team"
    } = {}) {
        return renderHook(() =>
            useProcurementConfirmationQueueUrlSync({
                scope: overrides.scope ?? "mine",
                queueContextId: `queue:procurement-confirmation:${overrides.scope ?? "mine"}`,
                queueReady: overrides.queueReady ?? true,
                tasksLength: overrides.tasksLength ?? 1,
                currentTaskWorkItemId:
                    overrides.currentTaskWorkItemId ?? "wi_1",
            }),
        )
    }

    it("does nothing while the queue is still loading", () => {
        renderSync({ queueReady: false })
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it("fills scope/queueContextId/currentWorkItemId defaults", () => {
        renderSync()
        const params = lastReplacedParams()
        expect(params.get("scope")).toBe("mine")
        expect(params.get("queueContextId")).toBe(
            "queue:procurement-confirmation:mine",
        )
        expect(params.get("currentWorkItemId")).toBe("wi_1")
    })

    it("migrates the legacy task param to currentWorkItemId and drops completed", () => {
        navMocks.searchParams = new URLSearchParams(
            "task=wi_1&completed=1",
        )
        renderSync({ currentTaskWorkItemId: "wi_1" })
        const params = lastReplacedParams()
        expect(params.get("currentWorkItemId")).toBe("wi_1")
        expect(params.has("task")).toBe(false)
        expect(params.has("completed")).toBe(false)
        expect(params.get("scope")).toBe("mine")
    })

    it("skips rewriting when all params are already present", () => {
        navMocks.searchParams = new URLSearchParams(
            "scope=mine&queueContextId=queue:procurement-confirmation:mine&currentWorkItemId=wi_1",
        )
        renderSync()
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it("skips rewriting a settled legacy task param", () => {
        navMocks.searchParams = new URLSearchParams(
            "scope=mine&queueContextId=queue:procurement-confirmation:mine&task=wi_1",
        )
        renderSync({ currentTaskWorkItemId: "wi_1" })
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it("skips rewriting for an empty queue with settled scope and context", () => {
        navMocks.searchParams = new URLSearchParams(
            "scope=mine&queueContextId=queue:procurement-confirmation:mine",
        )
        renderSync({ tasksLength: 0 })
        expect(navMocks.replace).not.toHaveBeenCalled()
    })
})
