import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act, waitFor, cleanup } from "@testing-library/react"

vi.mock("@/features/procurement-confirmation/api", () => ({
    fetchProcurementQueue: vi.fn(),
    fetchProcurementRecommendation: vi.fn(),
    fetchProcurementSupplyOptions: vi.fn(),
    saveProcurementConfirmation: vi.fn(),
    completeProcurementDecision: vi.fn(),
}))

vi.mock("@/features/unified-task-queue/queries", () => ({
    unifiedQueueKeys: { all: ["work-items"], view: vi.fn(), counts: vi.fn() },
}))

import {
    completeProcurementDecision,
    fetchProcurementQueue,
    fetchProcurementRecommendation,
    fetchProcurementSupplyOptions,
    saveProcurementConfirmation,
    type QueueFilters,
} from "@/features/procurement-confirmation/api"
import type {
    ProcurementQueueView,
    ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import {
    useCompleteProcurementMutation,
    useProcurementConfirmationQuery,
    useProcurementRecommendationQuery,
    useProcurementSupplyOptionsQuery,
    useSaveProcurementConfirmationMutation,
} from "./queries"

const mockedFetchQueue = vi.mocked(fetchProcurementQueue)
const mockedFetchRecommendation = vi.mocked(fetchProcurementRecommendation)
const mockedFetchSupplyOptions = vi.mocked(fetchProcurementSupplyOptions)
const mockedSave = vi.mocked(saveProcurementConfirmation)
const mockedComplete = vi.mocked(completeProcurementDecision)

const FILTERS: QueueFilters = { scope: "mine" }

function makeQueueView(): ProcurementQueueView {
    return {
        preferences: { autoNextDefault: true },
        context: {
            queueContextId: "queue:procurement-confirmation:mine",
            position: 0,
            total: 0,
            filterSummary: "仅我的 · 有效全部 · 截止优先",
            queueContextUpdatedAt: "2026-08-14T10:00:00.000Z",
        },
        tasks: [],
        emptyReason: "NO_TASKS",
    }
}

function makeRecommendation(): ProcurementRecommendation {
    return {
        confirmationId: "pc_1",
        policyVersion: "v3",
        calculatedAt: "2026-08-14T10:00:00.000Z",
        ready: true,
        lines: [],
        purchaseOrders: [],
        estimatedPurchaseGross: "100.00",
        salesGross: "120.00",
        estimatedGrossMargin: "20.00",
        blockingIssues: [],
        warnings: [],
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useProcurementRecommendationQuery", () => {
    it("fetches via fetchProcurementRecommendation with the confirmationId", async () => {
        const recommendation = makeRecommendation()
        mockedFetchRecommendation.mockResolvedValue(recommendation)
        const client = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useProcurementRecommendationQuery("pc_1"),
            { queryClient: client },
        )
        expect(result.current.isPending).toBe(true)

        await waitFor(() => expect(result.current.isPending).toBe(false))
        expect(result.current.data).toEqual(recommendation)
        expect(mockedFetchRecommendation).toHaveBeenCalledWith("pc_1")
        expect(mockedFetchRecommendation).toHaveBeenCalledTimes(1)
    })

    it("uses a stable key per confirmationId and refetches when it changes", async () => {
        mockedFetchRecommendation.mockResolvedValue(makeRecommendation())
        const client = createFreshQueryClient()
        let confirmationId = "pc_1"
        const { result, rerender } = renderHookWithProviders(
            () => useProcurementRecommendationQuery(confirmationId),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        const entries = client.getQueryCache().findAll()
        expect(entries).toHaveLength(1)
        expect(entries[0]?.queryKey).toEqual([
            "procurement-confirmation",
            "recommendation",
            "pc_1",
        ])

        confirmationId = "pc_2"
        rerender()
        await waitFor(() =>
            expect(mockedFetchRecommendation).toHaveBeenCalledWith("pc_2"),
        )
        expect(mockedFetchRecommendation).toHaveBeenCalledTimes(2)
    })

    it("does not fetch while disabled by an empty id or enabled=false", async () => {
        const client = createFreshQueryClient()
        let confirmationId = "pc_1"
        let enabled = false
        const { result, rerender } = renderHookWithProviders(
            () => useProcurementRecommendationQuery(confirmationId, enabled),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.fetchStatus).toBe("idle"))
        expect(mockedFetchRecommendation).not.toHaveBeenCalled()

        enabled = true
        confirmationId = ""
        rerender()
        await waitFor(() => expect(result.current.fetchStatus).toBe("idle"))
        expect(mockedFetchRecommendation).not.toHaveBeenCalled()

        confirmationId = "pc_1"
        rerender()
        await waitFor(() =>
            expect(mockedFetchRecommendation).toHaveBeenCalledWith("pc_1"),
        )
    })

    it("surfaces fetch errors without data", async () => {
        mockedFetchRecommendation.mockRejectedValue(new Error("网络异常"))
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useProcurementRecommendationQuery("pc_1"),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.data).toBeUndefined()
    })
})

describe("useProcurementSupplyOptionsQuery", () => {
    it("fetches via fetchProcurementSupplyOptions and sorts skuIds in the key", async () => {
        const options = [{ skuId: "sku_a" }]
        mockedFetchSupplyOptions.mockResolvedValue(
            options as Awaited<ReturnType<typeof fetchProcurementSupplyOptions>>,
        )
        const client = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useProcurementSupplyOptionsQuery(["sku_b", "sku_a", ""]),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        expect(result.current.data).toEqual(options)
        expect(mockedFetchSupplyOptions).toHaveBeenCalledWith([
            "sku_b",
            "sku_a",
            "",
        ])
        const entries = client.getQueryCache().findAll()
        expect(entries).toHaveLength(1)
        expect(entries[0]?.queryKey).toEqual([
            "procurement-confirmation",
            "supply-options",
            ["", "sku_a", "sku_b"],
        ])
    })

    it("stays disabled when no sku has a value", async () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useProcurementSupplyOptionsQuery(["", ""]),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.fetchStatus).toBe("idle"))
        expect(mockedFetchSupplyOptions).not.toHaveBeenCalled()
    })

    it("surfaces fetch errors", async () => {
        mockedFetchSupplyOptions.mockRejectedValue(new Error("供给不可用"))
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useProcurementSupplyOptionsQuery(["sku_a"]),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
    })
})

describe("useProcurementConfirmationQuery", () => {
    it("passes filters to fetchProcurementQueue and exposes the view", async () => {
        const view = makeQueueView()
        mockedFetchQueue.mockResolvedValue(view)
        const client = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useProcurementConfirmationQuery(FILTERS),
            { queryClient: client },
        )
        expect(result.current.isPending).toBe(true)

        await waitFor(() => expect(result.current.isPending).toBe(false))
        expect(result.current.data).toEqual(view)
        expect(mockedFetchQueue).toHaveBeenCalledWith(FILTERS)
        expect(mockedFetchQueue).toHaveBeenCalledTimes(1)
    })

    it("keeps a stable key for the same filters", async () => {
        mockedFetchQueue.mockResolvedValue(makeQueueView())
        const client = createFreshQueryClient()

        const { result, rerender } = renderHookWithProviders(
            () => useProcurementConfirmationQuery(FILTERS),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        rerender()
        await waitFor(() => expect(result.current.data).toBeDefined())
        const entries = client.getQueryCache().findAll()
        expect(entries).toHaveLength(1)
        expect(entries[0]?.queryKey).toEqual([
            "procurement-confirmation",
            "queue",
            FILTERS,
        ])
    })

    it("surfaces fetch errors without data", async () => {
        mockedFetchQueue.mockRejectedValue(new Error("队列加载失败"))
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useProcurementConfirmationQuery(FILTERS),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.data).toBeUndefined()
    })
})

describe("useSaveProcurementConfirmationMutation", () => {
    const input = {
        workItemId: "wi_1",
        expectedTaskVersion: "1",
        expectedSubjectVersion: "sv_1",
        confirmationId: "pc_1",
        submissionId: "sub_1",
        expectedEditVersion: 1,
        lines: [],
        idempotencyKey: "key",
    }

    it("wires mutationFn to saveProcurementConfirmation and invalidates on success", async () => {
        mockedSave.mockResolvedValue({ editVersion: 2, taskVersion: "2" })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useSaveProcurementConfirmationMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedSave).toHaveBeenCalledTimes(1)
        expect(mockedSave).toHaveBeenCalledWith(input, expect.anything())
        await waitFor(() => expect(invalidate).toHaveBeenCalled())
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["procurement-confirmation"],
        })
    })

    it("does not invalidate when the save fails", async () => {
        mockedSave.mockRejectedValue(new Error("保存失败"))
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useSaveProcurementConfirmationMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await expect(result.current.mutateAsync(input)).rejects.toThrow(
                "保存失败",
            )
        })
        expect(invalidate).not.toHaveBeenCalled()
    })
})

describe("useCompleteProcurementMutation", () => {
    const input = {
        workItemId: "wi_1",
        expectedTaskVersion: "1",
        expectedSubjectVersion: "sv_1",
        idempotencyKey: "key",
        decision: {
            reviewResult: "APPROVED" as const,
            confirmationId: "pc_1",
            submissionId: "sub_1",
            expectedConfirmationEditVersion: 2,
            salesOrderId: "so_1",
            salesOrderNo: "SO-1",
            subjectHash: "sub_1",
        },
    }

    it("wires mutationFn to completeProcurementDecision", async () => {
        mockedComplete.mockResolvedValue({
            status: "unknown",
            message: "待核实",
            idempotencyKey: "key",
        })
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCompleteProcurementMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(mockedComplete).toHaveBeenCalledTimes(1)
        expect(mockedComplete).toHaveBeenCalledWith(input, expect.anything())
    })

    it("invalidates procurement and unified-queue keys only on a succeeded result", async () => {
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useCompleteProcurementMutation(),
            { queryClient: client },
        )

        mockedComplete.mockResolvedValue({
            status: "failed",
            message: "保存失败",
            code: "CONFLICT",
        })
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(invalidate).not.toHaveBeenCalled()

        mockedComplete.mockResolvedValue({
            status: "succeeded",
            outcome: {
                kind: "APPROVED_AND_SALES_EFFECTIVE",
                procurementConfirmationId: "pc_1",
                salesOrderId: "so_1",
                salesOrderNo: "SO-1",
                submissionId: "sub_1",
                subjectHash: "sub_1",
                salesOrderRevisionId: "rev_1",
                receivableAccountId: "ra_1",
                procurementCreationBasisId: "basis_1",
                reference: "basis_1",
            },
        })
        await act(async () => {
            await result.current.mutateAsync({
                ...input,
                idempotencyKey: "key-2",
            })
        })
        await waitFor(() => expect(invalidate).toHaveBeenCalledTimes(2))
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["procurement-confirmation"],
        })
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["work-items"],
        })
    })
})
