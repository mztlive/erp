import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, waitFor, act, cleanup } from "@testing-library/react"

import {
    useCardFundsReviewDefaultUrlSync,
    useCardFundsReviewUrlState,
} from "./use-card-funds-review-url-state"
import { makeQueueView, makeTask } from "./test-data"

vi.mock("next/navigation", () => ({
    useRouter: vi.fn(() => ({
        push: vi.fn(),
        replace: vi.fn(),
        back: vi.fn(),
    })),
    useSearchParams: vi.fn(
        () => new URLSearchParams() as unknown as ReadonlyURLSearchParams,
    ),
    usePathname: vi.fn(() => "/test"),
    useParams: vi.fn(() => ({})),
}))

import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ReadonlyURLSearchParams } from "next/navigation"

const mockedSearchParams = vi.mocked(useSearchParams)
const mockedPathname = vi.mocked(usePathname)
const mockedRouter = vi.mocked(useRouter)

function setupRouter() {
    const router = {
        push: vi.fn(),
        replace: vi.fn(),
        back: vi.fn(),
    }
    mockedRouter.mockReturnValue(
        router as unknown as ReturnType<typeof useRouter>,
    )
    return router
}

beforeEach(() => {
    vi.clearAllMocks()
    mockedSearchParams.mockReturnValue(
        new URLSearchParams() as unknown as ReadonlyURLSearchParams,
    )
    mockedPathname.mockReturnValue("/test")
    setupRouter()
})

afterEach(() => {
    cleanup()
})

describe("useCardFundsReviewUrlState", () => {
    it("applies defaults when no params are present", () => {
        const { result } = renderHook(() => useCardFundsReviewUrlState())
        expect(result.current.scope).toBe("mine")
        expect(result.current.type).toBe("all")
        expect(result.current.status).toBe("OPEN")
        expect(result.current.due).toBe("all")
        expect(result.current.q).toBeUndefined()
        expect(result.current.currentWorkItemId).toBeUndefined()
        expect(result.current.queueContextId).toBe(
            "queue:card-funds-review:mine",
        )
        expect(result.current.autoNext).toBe(true)
        expect(result.current.searchInput).toBe("")
    })

    it("parses full params from the URL", () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                "scope=history&type=delta&status=CLOSED&due=overdue&q=SO-1&currentWorkItemId=wi_9&queueContextId=qc_1&autoNext=0",
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCardFundsReviewUrlState())
        expect(result.current.scope).toBe("history")
        expect(result.current.type).toBe("delta")
        expect(result.current.status).toBe("CLOSED")
        expect(result.current.due).toBe("overdue")
        expect(result.current.q).toBe("SO-1")
        expect(result.current.currentWorkItemId).toBe("wi_9")
        expect(result.current.queueContextId).toBe("qc_1")
        expect(result.current.autoNext).toBe(false)
        expect(result.current.searchInput).toBe("SO-1")
    })

    it("ignores unknown param values by falling back to defaults", () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                "scope=bogus&type=bogus&status=bogus&due=bogus",
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCardFundsReviewUrlState())
        expect(result.current.scope).toBe("mine")
        expect(result.current.type).toBe("all")
        expect(result.current.status).toBe("OPEN")
        expect(result.current.due).toBe("all")
    })

    it("replaceUrl sets, deletes and preserves queueContextId", () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                "scope=mine&queueContextId=qc_1",
            ) as unknown as ReadonlyURLSearchParams,
        )
        mockedPathname.mockReturnValue("/finance/card-funds-review")
        const router = setupRouter()
        const { result } = renderHook(() => useCardFundsReviewUrlState())

        act(() => {
            result.current.replaceUrl({
                type: "opening",
                q: null,
                currentWorkItemId: "wi_2",
            })
        })
        expect(router.replace).toHaveBeenCalledWith(
            "/finance/card-funds-review?scope=mine&queueContextId=qc_1&type=opening&currentWorkItemId=wi_2",
            { scroll: false },
        )
    })

    it("replaceUrl adds the default queueContextId when absent", () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                "scope=mine",
            ) as unknown as ReadonlyURLSearchParams,
        )
        mockedPathname.mockReturnValue("/finance/card-funds-review")
        const router = setupRouter()
        const { result } = renderHook(() => useCardFundsReviewUrlState())

        act(() => {
            result.current.replaceUrl({ due: "today" })
        })
        expect(router.replace).toHaveBeenCalledWith(
            "/finance/card-funds-review?scope=mine&due=today&queueContextId=queue%3Acard-funds-review%3Amine",
            { scroll: false },
        )
    })

    it("replaceUrl restores the current queueContextId when removed", () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                "queueContextId=qc_1",
            ) as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCardFundsReviewUrlState())

        act(() => {
            result.current.replaceUrl({ queueContextId: null })
        })
        // 删除后由兜底逻辑按当前 URL 的 queueContextId 重新补回
        expect(router.replace).toHaveBeenCalledWith(
            "/test?queueContextId=qc_1",
            { scroll: false },
        )
    })

    it("debounces the search input into the q param", async () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams() as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCardFundsReviewUrlState())

        act(() => {
            result.current.setSearchInput("SO-1")
        })
        expect(router.replace).not.toHaveBeenCalled()
        await waitFor(
            () =>
                expect(router.replace).toHaveBeenCalledWith(
                    "/test?q=SO-1&queueContextId=queue%3Acard-funds-review%3Amine",
                    { scroll: false },
                ),
            { timeout: 2000 },
        )
    })

    it("does not write the q param when it already matches", async () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams("q=SO-1") as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        renderHook(() => useCardFundsReviewUrlState())
        await new Promise((resolve) => setTimeout(resolve, 400))
        expect(router.replace).not.toHaveBeenCalled()
    })

    it("setAutoNext persists the toggle into the URL", () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                "queueContextId=qc_1",
            ) as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCardFundsReviewUrlState())

        act(() => {
            result.current.setAutoNext(false)
        })
        expect(result.current.autoNext).toBe(false)
        expect(router.replace).toHaveBeenCalledWith(
            "/test?queueContextId=qc_1&autoNext=0",
            { scroll: false },
        )
    })
})

describe("useCardFundsReviewDefaultUrlSync", () => {
    const view = makeQueueView(makeTask())

    it("fills missing defaults once the queue is ready", async () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams() as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        renderHook(() =>
            useCardFundsReviewDefaultUrlSync({
                queuePending: false,
                view,
                task: view.current,
                taskCount: 1,
                scope: "mine",
                type: "all",
                queueContextId: "queue:card-funds-review:mine",
            }),
        )
        await waitFor(() => expect(router.replace).toHaveBeenCalledTimes(1))
        expect(router.replace).toHaveBeenCalledWith(
            "/test?scope=mine&queueContextId=queue%3Acard-funds-review%3Amine&currentWorkItemId=wi_1",
            { scroll: false },
        )
    })

    it("writes the type param when it is not the default", async () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams() as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        renderHook(() =>
            useCardFundsReviewDefaultUrlSync({
                queuePending: false,
                view,
                task: view.current,
                taskCount: 1,
                scope: "mine",
                type: "delta",
                queueContextId: "queue:card-funds-review:mine",
            }),
        )
        await waitFor(() => expect(router.replace).toHaveBeenCalledTimes(1))
        expect(router.replace).toHaveBeenCalledWith(
            "/test?scope=mine&type=delta&queueContextId=queue%3Acard-funds-review%3Amine&currentWorkItemId=wi_1",
            { scroll: false },
        )
    })

    it("does nothing when all defaults are already present", async () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                "scope=mine&queueContextId=queue:card-funds-review:mine&currentWorkItemId=wi_1",
            ) as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        renderHook(() =>
            useCardFundsReviewDefaultUrlSync({
                queuePending: false,
                view,
                task: view.current,
                taskCount: 1,
                scope: "mine",
                type: "all",
                queueContextId: "queue:card-funds-review:mine",
            }),
        )
        await new Promise((resolve) => setTimeout(resolve, 100))
        expect(router.replace).not.toHaveBeenCalled()
    })

    it("does not write a currentWorkItemId for an empty queue", async () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                "scope=mine&queueContextId=qc_1",
            ) as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        renderHook(() =>
            useCardFundsReviewDefaultUrlSync({
                queuePending: false,
                view: { ...view, tasks: [], current: undefined },
                task: undefined,
                taskCount: 0,
                scope: "mine",
                type: "all",
                queueContextId: "qc_1",
            }),
        )
        await new Promise((resolve) => setTimeout(resolve, 100))
        expect(router.replace).not.toHaveBeenCalled()
    })

    it("waits while the queue is still pending", async () => {
        const router = setupRouter()
        renderHook(() =>
            useCardFundsReviewDefaultUrlSync({
                queuePending: true,
                view: undefined,
                task: undefined,
                taskCount: 0,
                scope: "mine",
                type: "all",
                queueContextId: "queue:card-funds-review:mine",
            }),
        )
        await new Promise((resolve) => setTimeout(resolve, 100))
        expect(router.replace).not.toHaveBeenCalled()
    })
})
