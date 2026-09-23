import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { ApiErrorException } from "@/lib/api/errors"
import { useActualProfitLossPage } from "./use-actual-profit-loss-page"
import { makeQuery, makeRow, makeView } from "./test-fixtures"

const state = vi.hoisted(() => ({
    view: {} as Record<string, unknown>,
    cost: {} as Record<string, unknown>,
    query: {} as Record<string, unknown>,
    export: {} as Record<string, unknown>,
    client: { removeQueries: vi.fn() },
}))
vi.mock("@tanstack/react-query", () => ({ useQueryClient: () => state.client }))
vi.mock("@/lib/historical-directory", () => ({
    useHistoricalDirectory: () => ({
        data: undefined,
        isError: false,
        isFetching: false,
    }),
}))
vi.mock("./queries", () => ({
    usePeriodBasisConfigQuery: () => ({ isSuccess: true }),
    useProfitLossViewQuery: () => state.view,
    useCostEntriesForRowQuery: () => state.cost,
    useStartProfitLossExportMutation: () => state.export,
}))
vi.mock("./use-profit-loss-url-state", () => ({
    useProfitLossUrlState: () => ({
        query: state.query,
        patchUrl: vi.fn(),
        analysisReady: true,
        attributionUserIds: [],
        attributionOrgUnitIds: [],
        costTypes: [],
        qParam: "",
        coverage: "covered",
    }),
}))

describe("profit-loss authorization recovery", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        state.query = { ...makeQuery() }
        state.view = {
            data: makeView({ scopeVersion: "v1" }),
            error: null,
            isSuccess: true,
            isFetching: false,
            dataUpdatedAt: 1,
        }
        state.cost = { error: null }
        state.export = { error: null, reset: vi.fn() }
    })
    afterEach(cleanup)

    it("removes stale report and open detail when cost authorization is revoked, then accepts only a fresh authorized result", () => {
        const { result, rerender } = renderHook(useActualProfitLossPage)
        act(() => result.current.openCostDetail(makeRow()))
        expect(result.current.costDetailRow).not.toBeNull()
        state.cost.error = new ApiErrorException({
            kind: "Auth",
            status: 403,
            message: "范围已撤销",
        })
        rerender()
        expect(result.current.data).toBeUndefined()
        expect(result.current.costDetailRow).toBeNull()
        expect(state.client.removeQueries).toHaveBeenCalledWith({
            queryKey: ["actual-profit-loss"],
            type: "inactive",
        })
        state.cost.error = null
        rerender()
        expect(result.current.data).toBeUndefined()
        state.view = {
            ...state.view,
            data: makeView({ scopeVersion: "v2" }),
            dataUpdatedAt: Date.now() + 1000,
        }
        rerender()
        expect(result.current.data?.scopeVersion).toBe("v2")
        expect(result.current.scopeError).toBeFalsy()
    })

    it("closes old cost detail when a successful query resolves a different scope version", () => {
        const { result, rerender } = renderHook(useActualProfitLossPage)
        act(() => result.current.openCostDetail(makeRow()))
        state.view = { ...state.view, data: makeView({ scopeVersion: "v2" }) }
        rerender()
        expect(result.current.costDetailRow).toBeNull()
        expect(result.current.data?.scopeVersion).toBe("v2")
    })

    it("binds export to the displayed report version and hides stale data on a conflict", async () => {
        const conflict = new ApiErrorException({
            kind: "Http",
            status: 409,
            message: "DATA_SCOPE_CHANGED",
        })
        const mutateAsync = vi.fn(async () => {
            state.export.error = conflict
            throw conflict
        })
        state.export.mutateAsync = mutateAsync
        const { result } = renderHook(useActualProfitLossPage)
        await act(async () => {
            await result.current.handleExport()
        })
        expect(mutateAsync).toHaveBeenCalledWith({
            query: { ...state.query, scopeVersion: "v1" },
        })
        expect(result.current.data).toBeUndefined()
        expect(result.current.scopeError).toBe(conflict)
    })
})
