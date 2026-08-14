import { describe, it, expect, vi, beforeEach } from "vitest"
import { waitFor } from "@testing-library/react"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

import { unifiedQueueKeys, useUnifiedTaskCountQuery, useUnifiedTaskQueueQuery } from "./queries"
import type { UnifiedQueueFilters } from "../types"

const api = vi.hoisted(() => ({
    fetchUnifiedTaskQueue: vi.fn(),
    fetchUnifiedTaskQueueCounts: vi.fn(),
}))

vi.mock("../api/work-items", () => api)

const filters: UnifiedQueueFilters = {
    scope: "mine",
    family: "finance",
    due: "today",
    priorities: [1, 2],
    sort: "due_asc",
    query: "采购单",
    queueContextId: "queue-1",
    currentWorkItemId: "wi-1",
    viewerKey: "u1:role1",
}

const queueView = {
    queueContextId: "queue-1",
    total: 2,
    items: [],
}

const counts = { mine: 1, team: 2, overdue: 0, total: 3 }

describe("unified queue query keys", () => {
    it("shares the work-item key space and layers view/count keys", () => {
        expect(unifiedQueueKeys.all).toEqual(["work-items"])
        expect(unifiedQueueKeys.view(filters)).toEqual([
            "work-items",
            "unified-queue",
            filters,
        ])
        expect(unifiedQueueKeys.counts()).toEqual(["work-items", "mine-count"])
    })
})

describe("useUnifiedTaskQueueQuery", () => {
    beforeEach(() => {
        api.fetchUnifiedTaskQueue.mockReset().mockResolvedValue(queueView)
    })

    it("fetches the queue with the given filters and exposes the view", async () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useUnifiedTaskQueueQuery(filters),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))

        expect(result.current.data).toEqual(queueView)
        expect(api.fetchUnifiedTaskQueue).toHaveBeenCalledWith(filters)
        expect(client.getQueryCache().getAll()[0]?.queryKey).toEqual([
            "work-items",
            "unified-queue",
            filters,
        ])
    })

    it("surfaces fetch failures as query errors", async () => {
        api.fetchUnifiedTaskQueue.mockReset().mockRejectedValue(new Error("boom"))
        const { result } = renderHookWithProviders(() =>
            useUnifiedTaskQueueQuery(filters),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useUnifiedTaskCountQuery", () => {
    beforeEach(() => {
        api.fetchUnifiedTaskQueueCounts.mockReset().mockResolvedValue(counts)
    })

    it("fetches open counts under the fixed key", async () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useUnifiedTaskCountQuery(),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))

        expect(result.current.data).toEqual(counts)
        expect(api.fetchUnifiedTaskQueueCounts).toHaveBeenCalledTimes(1)
        expect(client.getQueryCache().getAll()[0]?.queryKey).toEqual([
            "work-items",
            "mine-count",
        ])
    })

    it("surfaces fetch failures as query errors", async () => {
        api.fetchUnifiedTaskQueueCounts.mockReset().mockRejectedValue(
            new Error("boom"),
        )
        const { result } = renderHookWithProviders(() =>
            useUnifiedTaskCountQuery(),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})
