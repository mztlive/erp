import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, waitFor } from "@testing-library/react"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

import {
    fetchCustomerQuality,
    fetchCustomerQualityPeriodPolicy,
    startCustomerQualityExport,
} from "../api/customer-quality"
import type {
    CustomerQualityPeriodPolicy,
    CustomerQualityQuery,
    CustomerQualityView,
} from "../types"
import {
    useCustomerQualityPeriodPolicyQuery,
    useCustomerQualityQuery,
    useRefreshCustomerQualityMutation,
    useStartCustomerQualityExportMutation,
} from "./queries"

vi.mock("../api/customer-quality", () => ({
    fetchCustomerQuality: vi.fn(),
    fetchCustomerQualityPeriodPolicy: vi.fn(),
    startCustomerQualityExport: vi.fn(),
}))

const mockedFetchView = vi.mocked(fetchCustomerQuality)
const mockedFetchPolicy = vi.mocked(fetchCustomerQualityPeriodPolicy)
const mockedStartExport = vi.mocked(startCustomerQualityExport)

beforeEach(() => {
    vi.clearAllMocks()
})

const baseQuery: CustomerQualityQuery = {
    from: "2026-01-01",
    to: "2026-06-30",
    periodBasis: "BUSINESS_DATE",
    periodSelectionSource: "SERVER_DEFAULT",
    scopeId: "scope:team:sales-east",
    fundsReview: "all",
    sort: "salesGrossAmount:desc",
    page: 1,
    pageSize: 20,
}

const policyFixture: CustomerQualityPeriodPolicy = {
    hasDefault: true,
    from: "2026-01-01",
    to: "2026-08-01",
    periodBasis: "BUSINESS_DATE",
    timezone: "Asia/Shanghai",
    selectionSource: "SERVER_DEFAULT",
}

const viewFixture: CustomerQualityView = {
    scope: { id: "scope:team:sales-east", label: "华东", permissionVersion: "v1" },
    period: {
        from: "2026-01-01",
        to: "2026-06-30",
        basis: "BUSINESS_DATE",
        timezone: "Asia/Shanghai",
        selectionSource: "SERVER_DEFAULT",
    },
    freshness: {
        projectedAt: "2026-07-01T10:00:00+08:00",
        sourceWatermark: "outbox:cq:2026-07-01T10:00:00+08:00",
        state: "fresh",
    },
    coverage: {
        cardFundsReviewRate: "8/10",
        cardFundsReviewPercent: 80,
        reviewedVoucherOrderCount: 8,
        requiredVoucherOrderCount: 10,
        cardFundsState: "partial",
        costCoveredNetRevenue: "800.00",
        costUncoveredNetRevenue: "200.00",
        costCoverageRate: "80.0%",
        costCoveragePercent: 80,
        costCoverageState: "partial",
        costBasis: "ACTUAL",
    },
    metrics: [],
    dimensions: [],
    customers: { items: [], total: 0, filteredTotal: 0 },
    filterSummary: "全部",
    canExport: true,
    tagRuleCatalog: {
        scale: { ruleVersion: "v1", explanation: "e", labels: {} },
        profit: { ruleVersion: "v1", explanation: "e", labels: {} },
        risk: { ruleVersion: "v1", explanation: "e", labels: {} },
    },
}

describe("useCustomerQualityPeriodPolicyQuery", () => {
    it("caches under the period-policy key and calls the api with the scenario", async () => {
        mockedFetchPolicy.mockResolvedValue(policyFixture)
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useCustomerQualityPeriodPolicyQuery("stale"),
            { queryClient },
        )

        await waitFor(() =>
            expect(result.current.data).toEqual(policyFixture),
        )
        expect(
            queryClient.getQueryData([
                "customer-quality",
                "period-policy",
                "stale",
            ]),
        ).toEqual(policyFixture)
        expect(mockedFetchPolicy).toHaveBeenCalledWith({ scenario: "stale" })
    })

    it("uses the default scenario slot when scenario is omitted", async () => {
        mockedFetchPolicy.mockResolvedValue(policyFixture)
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useCustomerQualityPeriodPolicyQuery(),
            { queryClient },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(
            queryClient.getQueryData([
                "customer-quality",
                "period-policy",
                "default",
            ]),
        ).toEqual(policyFixture)
        expect(mockedFetchPolicy).toHaveBeenCalledWith({
            scenario: undefined,
        })
    })

    it("surfaces errors from the policy request", async () => {
        mockedFetchPolicy.mockRejectedValue(new Error("down"))
        const { result } = renderHookWithProviders(
            () => useCustomerQualityPeriodPolicyQuery(),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useCustomerQualityQuery", () => {
    it("fetches once under the view key and passes the query to the api", async () => {
        mockedFetchView.mockResolvedValue(viewFixture)
        const queryClient = createFreshQueryClient()

        const { result, rerender } = renderHookWithProviders(
            () => useCustomerQualityQuery(baseQuery),
            { queryClient },
        )

        await waitFor(() => expect(result.current.data).toEqual(viewFixture))
        expect(
            queryClient.getQueryData(["customer-quality", "view", baseQuery]),
        ).toEqual(viewFixture)
        expect(mockedFetchView).toHaveBeenCalledWith(baseQuery)

        rerender()
        await waitFor(() => expect(result.current.data).toEqual(viewFixture))
        expect(mockedFetchView).toHaveBeenCalledTimes(1)
    })

    it("stays idle when the query is null or the period is incomplete", () => {
        mockedFetchView.mockResolvedValue(viewFixture)

        const { result: nullResult } = renderHookWithProviders(() =>
            useCustomerQualityQuery(null),
        )
        expect(nullResult.current.fetchStatus).toBe("idle")
        expect(mockedFetchView).not.toHaveBeenCalled()

        const { result: emptyResult } = renderHookWithProviders(() =>
            useCustomerQualityQuery({ ...baseQuery, from: "", to: "" }),
        )
        expect(emptyResult.current.fetchStatus).toBe("idle")
        expect(mockedFetchView).not.toHaveBeenCalled()
    })

    it("surfaces query errors", async () => {
        mockedFetchView.mockRejectedValue(new Error("down"))
        const { result } = renderHookWithProviders(() =>
            useCustomerQualityQuery(baseQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useStartCustomerQualityExportMutation", () => {
    it("wires mutationFn to the export api", async () => {
        const job = {
            jobId: "j1",
            status: "queued" as const,
            total: 10,
            completed: 0,
            filterSummary: "s",
            period: { from: "a", to: "b" },
            permissionVersion: "v1",
            projectionWatermark: "w",
            amountBasisNote: "n",
        }
        mockedStartExport.mockResolvedValue(job)

        const { result } = renderHookWithProviders(() =>
            useStartCustomerQualityExportMutation(),
        )

        const input = {
            query: baseQuery,
            filterSummary: "s",
            projectionWatermark: "w",
            permissionVersion: "v1",
            rowCount: 10,
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedStartExport.mock.calls[0]?.[0]).toEqual(input)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
    })

    it("keeps the error on the mutation state when the export fails", async () => {
        mockedStartExport.mockRejectedValue(new Error("export down"))
        const { result } = renderHookWithProviders(() =>
            useStartCustomerQualityExportMutation(),
        )

        await act(async () => {
            await result.current
                .mutateAsync({
                    query: baseQuery,
                    filterSummary: "s",
                    projectionWatermark: "w",
                    permissionVersion: "v1",
                    rowCount: 10,
                })
                .catch(() => undefined)
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useRefreshCustomerQualityMutation", () => {
    it("invalidates every customer-quality query", async () => {
        const queryClient = createFreshQueryClient()
        const spy = vi.spyOn(queryClient, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useRefreshCustomerQualityMutation(),
            { queryClient },
        )

        await act(async () => {
            await result.current.mutateAsync()
        })

        expect(spy).toHaveBeenCalledWith({
            queryKey: ["customer-quality"],
        })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
    })
})
