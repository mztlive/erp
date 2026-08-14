import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, waitFor, cleanup } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import type { ReactNode } from "react"

vi.mock("@/features/fulfillment-operations/api", () => ({
    fetchFulfillmentQueue: vi.fn(),
    saveFulfillmentOperation: vi.fn(),
    postFulfillmentOperation: vi.fn(),
    resolveUnknownFulfillmentResult: vi.fn(),
}))

import {
    fetchFulfillmentQueue,
    postFulfillmentOperation,
    saveFulfillmentOperation,
} from "@/features/fulfillment-operations/api"
import type {
    FulfillmentQueueFilters,
} from "@/features/fulfillment-operations/api"
import type { FulfillmentQueueView } from "@/features/fulfillment-operations/types"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import {
    fulfillmentKeys,
    useFulfillmentCountQuery,
    useFulfillmentQueueQuery,
    usePostFulfillmentMutation,
    useSaveFulfillmentMutation,
} from "./queries"

const mockedFetchQueue = vi.mocked(fetchFulfillmentQueue)
const mockedSave = vi.mocked(saveFulfillmentOperation)
const mockedPost = vi.mocked(postFulfillmentOperation)

const FILTERS: FulfillmentQueueFilters = { role: "warehouse" }

function makeView(): FulfillmentQueueView {
    return {
        context: {
            position: 0,
            total: 3,
            filterSummary: "全部类型",
            warehouseOptions: [],
            visibleTypes: ["RECEIPT"],
            roleLabel: "仓储经办",
            canExecute: true,
            snapshotUpdatedAt: "2026-08-14T10:00:00.000Z",
        },
        metrics: [],
        operations: [],
        preferences: { autoNextDefault: true },
    }
}

function makeMutationWrapper(client: QueryClient) {
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
    return wrapper
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useFulfillmentQueueQuery", () => {
    it("passes the filters to the queryFn and exposes the loaded view", async () => {
        const view = makeView()
        mockedFetchQueue.mockResolvedValue(view)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useFulfillmentQueueQuery(FILTERS),
            { queryClient: client },
        )
        expect(result.current.isPending).toBe(true)

        await waitFor(() => expect(result.current.isPending).toBe(false))
        expect(result.current.data).toEqual(view)
        expect(mockedFetchQueue).toHaveBeenCalledWith(FILTERS)
        expect(mockedFetchQueue).toHaveBeenCalledTimes(1)
    })

    it("keeps a stable query key for the same filters", async () => {
        mockedFetchQueue.mockResolvedValue(makeView())
        const client = createFreshQueryClient()

        const { result, rerender } = renderHookWithProviders(
            () => useFulfillmentQueueQuery(FILTERS),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        rerender()
        await waitFor(() => expect(result.current.data).toBeDefined())
        const entries = client.getQueryCache().findAll()
        expect(entries).toHaveLength(1)
        expect(entries[0]?.queryKey).toEqual(
            fulfillmentKeys.queue(FILTERS),
        )
    })

    it("refetches when the filters change", async () => {
        mockedFetchQueue.mockResolvedValue(makeView())
        const client = createFreshQueryClient()
        let filters = FILTERS
        const { result, rerender } = renderHookWithProviders(
            () => useFulfillmentQueueQuery(filters),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        filters = { ...FILTERS, q: "SO-1" }
        rerender()
        await waitFor(() =>
            expect(mockedFetchQueue).toHaveBeenCalledWith(filters),
        )
        expect(mockedFetchQueue).toHaveBeenCalledTimes(2)
    })

    it("surfaces fetch errors without data", async () => {
        mockedFetchQueue.mockRejectedValue(new Error("网络异常"))
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useFulfillmentQueueQuery(FILTERS),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.data).toBeUndefined()
    })
})

describe("useFulfillmentCountQuery", () => {
    it("fetches the lane queue and returns the pending count", async () => {
        const view = makeView()
        mockedFetchQueue.mockResolvedValue(view)
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useFulfillmentCountQuery("warehouse"),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.data).toEqual({ pending: 3 }))
        expect(mockedFetchQueue).toHaveBeenCalledWith({ role: "warehouse" })
    })
})

describe("useSaveFulfillmentMutation", () => {
    it("wires mutationFn to the api and invalidates fulfillment queries on success", async () => {
        mockedSave.mockResolvedValue({ editVersion: 2 })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const wrapper = makeMutationWrapper(client)
        const { result } = renderHook(() => useSaveFulfillmentMutation(), {
            wrapper,
        })

        await act(async () => {
            await result.current.mutateAsync({
                operationId: "op_1",
                expectedDocumentVersion: 1,
                expectedSourceVersion: "sv_1",
                idempotencyKey: "key",
                draft: {
                    type: "RECEIPT",
                    warehouseId: "wh_1",
                    warehouseLabel: "中心仓",
                    occurredAt: "2026-08-14T09:00:00.000Z",
                    lines: [],
                },
            })
        })
        expect(mockedSave).toHaveBeenCalledTimes(1)
        await waitFor(() => expect(invalidate).toHaveBeenCalled())
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: fulfillmentKeys.all,
        })
    })
})

describe("usePostFulfillmentMutation", () => {
    it("wires mutationFn to the api and invalidates only on a succeeded result", async () => {
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const wrapper = makeMutationWrapper(client)
        const { result } = renderHook(() => usePostFulfillmentMutation(), {
            wrapper,
        })

        mockedPost.mockResolvedValue({
            status: "unknown",
            message: "处理结果待确认",
            idempotencyKey: "key",
        })
        await act(async () => {
            await result.current.mutateAsync({
                operationId: "op_1",
                expectedSourceVersion: "sv_1",
                expectedDocumentVersion: 1,
                idempotencyKey: "key",
                draft: {
                    type: "RECEIPT",
                    warehouseId: "wh_1",
                    warehouseLabel: "中心仓",
                    occurredAt: "2026-08-14T09:00:00.000Z",
                    lines: [],
                },
            })
        })
        expect(mockedPost).toHaveBeenCalledTimes(1)
        expect(invalidate).not.toHaveBeenCalled()

        mockedPost.mockResolvedValue({
            status: "succeeded",
            outcome: {
                kind: "POSTED",
                operationId: "op_1",
                factType: "PURCHASE_RECEIPT",
                factId: "f_1",
                factNo: "RK-1",
                formalStatus: "POSTED",
                occurredAt: "2026-08-14T09:00:00.000Z",
                operationType: "RECEIPT",
                inventoryDelta: [],
                reservationDelta: [],
                remainingByLine: [],
                acceptanceRequired: false,
                acceptanceNextStep: "由销售登记客户验收",
                inventoryImpactSummary: "中心仓 +10",
                reference: "RK-1",
                salesOrderId: "so_1",
                salesOrderNo: "SO-1",
            },
        })
        await act(async () => {
            await result.current.mutateAsync({
                operationId: "op_1",
                expectedSourceVersion: "sv_1",
                expectedDocumentVersion: 1,
                idempotencyKey: "key-2",
                draft: {
                    type: "RECEIPT",
                    warehouseId: "wh_1",
                    warehouseLabel: "中心仓",
                    occurredAt: "2026-08-14T09:00:00.000Z",
                    lines: [],
                },
            })
        })
        await waitFor(() => expect(invalidate).toHaveBeenCalled())
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: fulfillmentKeys.all,
        })
    })
})
