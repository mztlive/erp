import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import type { QueryObserverOptions } from '@tanstack/react-query'
import {
    fetchHistoryBackfillDetail,
    fetchHistoryBackfillList,
    submitHistoryBackfillCommand,
} from '@/features/history-backfill/api/history-backfill'
import {
    historyBackfillDetailRefetchInterval,
    useHistoryBackfillCommandMutation,
    useHistoryBackfillDetailQuery,
    useHistoryBackfillListQuery,
} from '@/features/history-backfill/hooks/queries'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'
import {
    makeCommittedResult,
    makeDetailView,
    makeJob,
    makeListView,
} from '@/features/history-backfill/test-fixtures'

vi.mock('@/features/history-backfill/api/history-backfill', () => ({
    fetchHistoryBackfillList: vi.fn(),
    fetchHistoryBackfillDetail: vi.fn(),
    submitHistoryBackfillCommand: vi.fn(),
}))

const listParams = {
    view: "active",
    page: 1,
    pageSize: 20,
} as const

describe("useHistoryBackfillListQuery", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it("fetches via fetchHistoryBackfillList and exposes data", async () => {
        const listView = makeListView({ totalCount: 2 })
        vi.mocked(fetchHistoryBackfillList).mockResolvedValue(listView)

        const { result } = renderHookWithProviders(() =>
            useHistoryBackfillListQuery({ ...listParams }),
        )

        expect(result.current.isPending).toBe(true)
        expect(fetchHistoryBackfillList).toHaveBeenCalledWith({
            ...listParams,
        })

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toEqual(listView)
    })

    it("shares cache for identical params and fetches again for new params", async () => {
        vi.mocked(fetchHistoryBackfillList).mockResolvedValue(makeListView())
        const client = createFreshQueryClient()

        renderHookWithProviders(
            () => useHistoryBackfillListQuery({ ...listParams }),
            { queryClient: client },
        )
        const second = renderHookWithProviders(
            () => useHistoryBackfillListQuery({ ...listParams }),
            { queryClient: client },
        )
        await waitFor(() => expect(second.result.current.isSuccess).toBe(true))
        expect(fetchHistoryBackfillList).toHaveBeenCalledTimes(1)

        const nextParams = { ...listParams, page: 2 }
        const third = renderHookWithProviders(
            () => useHistoryBackfillListQuery(nextParams),
            { queryClient: client },
        )
        await waitFor(() => expect(third.result.current.isSuccess).toBe(true))
        expect(fetchHistoryBackfillList).toHaveBeenCalledTimes(2)
        expect(fetchHistoryBackfillList).toHaveBeenLastCalledWith(nextParams)
    })

    it("surfaces query errors", async () => {
        vi.mocked(fetchHistoryBackfillList).mockRejectedValue(
            new Error("network down"),
        )

        const { result } = renderHookWithProviders(() =>
            useHistoryBackfillListQuery({ ...listParams }),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toEqual(new Error("network down"))
    })
})

describe("useHistoryBackfillDetailQuery", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    const detailParams = {
        jobId: "job-1",
        page: 1,
        pageSize: 20,
        section: "facts",
    } as const

    it("is disabled and never fetches when jobId is empty", () => {
        const { result } = renderHookWithProviders(() =>
            useHistoryBackfillDetailQuery({
                jobId: "",
                page: 1,
                pageSize: 20,
            }),
        )

        expect(fetchHistoryBackfillDetail).not.toHaveBeenCalled()
        expect(result.current.isPending).toBe(true)
        expect(result.current.isFetching).toBe(false)
    })

    it("fetches via fetchHistoryBackfillDetail and exposes data", async () => {
        const detailView = makeDetailView()
        vi.mocked(fetchHistoryBackfillDetail).mockResolvedValue(detailView)

        const { result } = renderHookWithProviders(() =>
            useHistoryBackfillDetailQuery({ ...detailParams }),
        )

        expect(fetchHistoryBackfillDetail).toHaveBeenCalledWith({
            ...detailParams,
        })

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toEqual(detailView)
    })

    it("polls running jobs every 8s and stops once completed", async () => {
        vi.mocked(fetchHistoryBackfillDetail).mockResolvedValue(
            makeDetailView({ job: makeJob({ processingStatus: "RUNNING" }) }),
        )
        const client = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useHistoryBackfillDetailQuery({ ...detailParams }),
            { queryClient: client },
        )

        await waitFor(() =>
            expect(result.current.data?.job.processingStatus).toBe("RUNNING"),
        )
        expect(fetchHistoryBackfillDetail).toHaveBeenCalledTimes(1)

        const query = client.getQueryCache().find({
            queryKey: ["history-backfill", "detail", { ...detailParams }],
        })!
        const options = query.options as QueryObserverOptions<
            ReturnType<typeof makeDetailView>,
            Error,
            ReturnType<typeof makeDetailView>,
            ReturnType<typeof makeDetailView>,
            readonly unknown[]
        >
        expect(typeof options.refetchInterval).toBe("function")
        const intervalFn = options.refetchInterval as (
            q: unknown,
            key: unknown,
            c: unknown,
        ) => number | false
        expect(
            intervalFn(
                { state: { data: result.current.data } },
                ["history-backfill"],
                client,
            ),
        ).toBe(8_000)

        // 完成态后不再轮询
        const completedView = makeDetailView()
        const completedInterval = historyBackfillDetailRefetchInterval(
            completedView.job.processingStatus,
        )
        expect(completedInterval).toBe(false)
    })
})

describe("useHistoryBackfillCommandMutation", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    const input = {
        action: "START",
        jobId: "job-1",
        operationId: "op-1",
        idempotencyKey: "idem-1",
    } as const

    it("wires mutationFn to submitHistoryBackfillCommand", async () => {
        const committed = makeCommittedResult()
        vi.mocked(submitHistoryBackfillCommand).mockResolvedValue(committed)

        const { result } = renderHookWithProviders(() =>
            useHistoryBackfillCommandMutation(),
        )

        await act(async () => {
            await result.current.mutateAsync({ ...input })
        })

        expect(submitHistoryBackfillCommand).toHaveBeenCalledWith({ ...input })
    })

    it("invalidates all history-backfill queries on COMMITTED", async () => {
        vi.mocked(submitHistoryBackfillCommand).mockResolvedValue(
            makeCommittedResult(),
        )
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useHistoryBackfillCommandMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync({ ...input })
        })

        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ["history-backfill"],
        })
    })

    it("does not invalidate on BLOCKED results", async () => {
        vi.mocked(submitHistoryBackfillCommand).mockResolvedValue(
            makeCommittedResult({ status: "BLOCKED" }),
        )
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useHistoryBackfillCommandMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync({ ...input })
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})
