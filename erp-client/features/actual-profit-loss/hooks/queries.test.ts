import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, waitFor } from "@testing-library/react"

// React 19 act() requires this flag in non-browser test environments.
;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT =
    true

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import {
    useCostEntriesForRowQuery,
    usePeriodBasisConfigQuery,
    useProfitLossViewQuery,
    useStartProfitLossExportMutation,
} from "@/features/actual-profit-loss/hooks/queries"
import {
    configuredBasis,
    fakeCostEntries,
    fakeExportJob,
    makeQuery,
    makeView,
} from "@/features/actual-profit-loss/hooks/test-fixtures"

const api = vi.hoisted(() => ({
    fetchPeriodBasisConfig: vi.fn(),
    fetchProfitLossView: vi.fn(),
    fetchCostEntriesForRow: vi.fn(),
    startProfitLossExport: vi.fn(),
}))

vi.mock("@/features/actual-profit-loss/api", () => api)

describe("usePeriodBasisConfigQuery", () => {
    beforeEach(() => {
        api.fetchPeriodBasisConfig.mockReset().mockResolvedValue(configuredBasis)
    })

    it("resolves the period basis config and passes the default query", async () => {
        const { result } = renderHookWithProviders(() =>
            usePeriodBasisConfigQuery(),
        )
        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toEqual(configuredBasis)
        expect(api.fetchPeriodBasisConfig).toHaveBeenCalledWith({})
    })

    it("forwards a scenario query and keeps the layered query key", async () => {
        const client = createFreshQueryClient()
        renderHookWithProviders(
            () => usePeriodBasisConfigQuery({ scenario: "missing" }),
            { queryClient: client },
        )
        await waitFor(() => expect(client.getQueryCache().getAll()).toHaveLength(1))
        expect(api.fetchPeriodBasisConfig).toHaveBeenCalledWith({
            scenario: "missing",
        })
        expect(client.getQueryCache().getAll()[0]?.queryKey).toEqual([
            "actual-profit-loss",
            "period-basis",
            { scenario: "missing" },
        ])
    })

    it("surfaces fetch failures as query errors", async () => {
        api.fetchPeriodBasisConfig.mockReset().mockRejectedValue(new Error("boom"))
        const { result } = renderHookWithProviders(() =>
            usePeriodBasisConfigQuery(),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useProfitLossViewQuery", () => {
    beforeEach(() => {
        api.fetchProfitLossView.mockReset().mockResolvedValue(makeView())
    })

    it("fetches with the given query when enabled and exposes data", async () => {
        const query = makeQuery({ coverage: "all" })
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useProfitLossViewQuery(query, true),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(api.fetchProfitLossView).toHaveBeenCalledWith(query)
        expect(result.current.data).toEqual(makeView())
        expect(client.getQueryCache().getAll()[0]?.queryKey).toEqual([
            "actual-profit-loss",
            "view",
            query,
        ])
    })

    it("stays idle without fetching when disabled with a null query", () => {
        const { result } = renderHookWithProviders(() =>
            useProfitLossViewQuery(null, false),
        )
        expect(result.current.fetchStatus).toBe("idle")
        expect(api.fetchProfitLossView).not.toHaveBeenCalled()
    })

    it("surfaces fetch failures as query errors", async () => {
        api.fetchProfitLossView.mockReset().mockRejectedValue(new Error("boom"))
        const { result } = renderHookWithProviders(() =>
            useProfitLossViewQuery(makeQuery(), true),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useCostEntriesForRowQuery", () => {
    beforeEach(() => {
        api.fetchCostEntriesForRow.mockReset().mockResolvedValue(fakeCostEntries)
    })

    it("fetches the row's cost entries with the ids in given order", async () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCostEntriesForRowQuery(["ce-2", "ce-1"]),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(api.fetchCostEntriesForRow).toHaveBeenCalledWith(["ce-2", "ce-1"])
        expect(result.current.data).toEqual(fakeCostEntries)
        expect(client.getQueryCache().getAll()[0]?.queryKey).toEqual([
            "actual-profit-loss",
            "cost-entries",
            "ce-1,ce-2",
        ])
    })

    it("does not fetch and stays idle for an empty id list", () => {
        const { result } = renderHookWithProviders(() =>
            useCostEntriesForRowQuery([]),
        )
        expect(result.current.fetchStatus).toBe("idle")
        expect(api.fetchCostEntriesForRow).not.toHaveBeenCalled()
    })

    it("surfaces fetch failures as query errors", async () => {
        api.fetchCostEntriesForRow.mockReset().mockRejectedValue(new Error("boom"))
        const { result } = renderHookWithProviders(() =>
            useCostEntriesForRowQuery(["ce-1"]),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useStartProfitLossExportMutation", () => {
    beforeEach(() => {
        api.startProfitLossExport.mockReset().mockResolvedValue(fakeExportJob)
    })

    it("calls the export api with the mutation input and returns the job", async () => {
        const { result } = renderHookWithProviders(() =>
            useStartProfitLossExportMutation(),
        )
        const input = {
            query: makeQuery(),
            view: makeView(),
            coverage: "covered" as const,
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(api.startProfitLossExport).toHaveBeenCalledTimes(1)
        expect(api.startProfitLossExport.mock.calls[0][0]).toEqual(input)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toEqual(fakeExportJob)
    })

    it("propagates api failures through the mutation error state", async () => {
        api.startProfitLossExport.mockReset().mockRejectedValue(new Error("boom"))
        const { result } = renderHookWithProviders(() =>
            useStartProfitLossExportMutation(),
        )
        const input = {
            query: makeQuery(),
            view: makeView(),
            coverage: "covered" as const,
        }
        await act(async () => {
            await expect(result.current.mutateAsync(input)).rejects.toThrow(
                "boom",
            )
        })
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})
