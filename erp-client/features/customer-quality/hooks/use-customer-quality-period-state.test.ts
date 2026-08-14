import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, waitFor } from "@testing-library/react"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { fetchCustomerQualityPeriodPolicy } from "../api/customer-quality"
import type { CustomerQualityPeriodPolicy } from "../types"
import { useCustomerQualityPeriodState } from "./use-customer-quality-period-state"

const mocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: mocks.push,
        replace: mocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => mocks.searchParams,
    usePathname: () => "/analytics/customer-quality",
    useParams: () => ({}),
}))

vi.mock("../api/customer-quality", () => ({
    fetchCustomerQuality: vi.fn(),
    fetchCustomerQualityPeriodPolicy: vi.fn(),
    startCustomerQualityExport: vi.fn(),
}))

const mockedFetchPolicy = vi.mocked(fetchCustomerQualityPeriodPolicy)

const defaultPolicy: CustomerQualityPeriodPolicy = {
    hasDefault: true,
    from: "2026-01-01",
    to: "2026-08-01",
    periodBasis: "BUSINESS_DATE",
    timezone: "Asia/Shanghai",
    customerQualityPeriodPolicyId: "pol-1",
    customerQualityPeriodPolicyVersion: 3,
    selectionSource: "SERVER_DEFAULT",
    presets: [
        { id: "ytd", label: "今年", from: "2026-01-01", to: "2026-08-01" },
    ],
}

type Args = Parameters<typeof useCustomerQualityPeriodState>[0]

function renderPeriodState(args: Partial<Args> = {}) {
    const patchUrl = vi.fn()
    let currentArgs: Args = {
        fromParam: null,
        toParam: null,
        fundsReview: "all",
        qParam: "",
        sort: "salesGrossAmount:desc",
        scopeId: "scope:team:sales-east",
        pagination: { pageIndex: 0, pageSize: 20 },
        patchUrl,
        ...args,
    }
    const queryClient = createFreshQueryClient()
    const rendered = renderHookWithProviders(
        () => useCustomerQualityPeriodState(currentArgs),
        { queryClient },
    )
    const rerender = (next: Partial<Args> = {}) => {
        currentArgs = { ...currentArgs, ...next }
        rendered.rerender()
    }
    return { ...rendered, rerender, patchUrl, queryClient }
}

beforeEach(() => {
    vi.clearAllMocks()
    mocks.searchParams = new URLSearchParams()
})

describe("useCustomerQualityPeriodState", () => {
    it("writes the server default period into the URL exactly once", async () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result, rerender } = renderPeriodState()

        await waitFor(() =>
            expect(mocks.replace).toHaveBeenCalledWith(
                "/analytics/customer-quality?from=2026-01-01&to=2026-08-01&customerQualityPeriodPolicyId=pol-1&customerQualityPeriodPolicyVersion=3&periodPreset=ytd",
            ),
        )
        expect(result.current.periodWriteDone).toBe(true)

        rerender()
        await waitFor(() => expect(result.current.periodWriteDone).toBe(true))
        expect(mocks.replace).toHaveBeenCalledTimes(1)
    })

    it("requires the period blocker when the policy has no default", async () => {
        mockedFetchPolicy.mockResolvedValue({
            hasDefault: false,
            timezone: "Asia/Shanghai",
        })
        const { result } = renderPeriodState()

        await waitFor(() => expect(result.current.periodWriteDone).toBe(true))
        expect(result.current.needsPeriodBlocker).toBe(true)
        expect(mocks.replace).not.toHaveBeenCalled()
    })

    it("skips the URL write when from/to params already exist", async () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result } = renderPeriodState({
            fromParam: "2026-01-01",
            toParam: "2026-06-30",
        })

        await waitFor(() => expect(result.current.periodWriteDone).toBe(true))
        expect(mocks.replace).not.toHaveBeenCalled()
        expect(result.current.needsPeriodBlocker).toBe(false)
    })

    it("keeps the blocker inactive while the policy is still loading", () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result } = renderPeriodState()

        expect(result.current.needsPeriodBlocker).toBe(false)
    })

    it("derives the analysis query from explicit URL params and the policy", async () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        mocks.searchParams = new URLSearchParams(
            "periodSelectionSource=EXPLICIT&customerQualityPeriodPolicyId=pol-url&customerQualityPeriodPolicyVersion=9",
        )
        const { result } = renderPeriodState({
            fromParam: "2026-01-01",
            toParam: "2026-06-30",
            fundsReview: "reviewed_only",
            businessType: "VOUCHER",
            scaleTag: "s1",
            profitTag: "p1",
            riskTag: "r1",
            qParam: "abc",
            sort: "overdueGross:desc",
            chartDimension: "scale",
            chartCode: "s1",
            customerId: "c1",
            pagination: { pageIndex: 1, pageSize: 20 },
            scenario: "stale",
        })

        await waitFor(() =>
            expect(result.current.analysisQuery).toEqual({
                from: "2026-01-01",
                to: "2026-06-30",
                periodBasis: "EXPLICIT",
                periodSelectionSource: "EXPLICIT",
                customerQualityPeriodPolicyId: "pol-url",
                customerQualityPeriodPolicyVersion: 9,
                scopeId: "scope:team:sales-east",
                fundsReview: "reviewed_only",
                businessType: "VOUCHER",
                scaleTag: "s1",
                profitTag: "p1",
                riskTag: "r1",
                q: "abc",
                sort: "overdueGross:desc",
                page: 2,
                pageSize: 20,
                chartDimension: "scale",
                chartCode: "s1",
                customerId: "c1",
                scenario: "stale",
            }),
        )
    })

    it("falls back to the policy id and version when absent from the URL", async () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result } = renderPeriodState({
            fromParam: "2026-01-01",
            toParam: "2026-06-30",
            periodPreset: "ytd",
        })

        await waitFor(() =>
            expect(result.current.analysisQuery).toMatchObject({
                customerQualityPeriodPolicyId: "pol-1",
                customerQualityPeriodPolicyVersion: 3,
                periodSelectionSource: "CONFIGURED_PRESET",
                periodBasis: "BUSINESS_DATE",
                page: 1,
            }),
        )
    })

    it("returns a null analysis query without a complete period", () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result } = renderPeriodState({ fromParam: "2026-01-01" })

        expect(result.current.analysisQuery).toBeNull()
    })

    it("marks the period selection EXPLICIT when the policy has no default", async () => {
        mockedFetchPolicy.mockResolvedValue({
            hasDefault: false,
            timezone: "Asia/Shanghai",
        })
        const { result } = renderPeriodState({
            fromParam: "2026-01-01",
            toParam: "2026-06-30",
        })

        await waitFor(() =>
            expect(result.current.periodSelectionSource).toBe("EXPLICIT"),
        )
        expect(result.current.needsPeriodBlocker).toBe(false)
    })

    it("flags an invalid period only when from is after to", () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result, rerender } = renderPeriodState({
            fromParam: "2026-08-01",
            toParam: "2026-01-01",
        })

        expect(result.current.periodInvalid).toBe(true)

        rerender({
            fromParam: "2026-01-01",
            toParam: "2026-01-01",
        })
        expect(result.current.periodInvalid).toBe(false)
    })

    it("applyExplicitPeriod writes the chosen explicit period", () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result, patchUrl } = renderPeriodState()

        act(() => {
            result.current.setExplicitFrom("2026-02-01")
            result.current.setExplicitTo("2026-03-01")
        })
        act(() => {
            result.current.applyExplicitPeriod()
        })

        expect(patchUrl).toHaveBeenCalledWith({
            from: "2026-02-01",
            to: "2026-03-01",
            periodSelectionSource: "EXPLICIT",
            periodPreset: null,
            customerQualityPeriodPolicyId: null,
            customerQualityPeriodPolicyVersion: null,
        })
    })

    it("applyExplicitPeriod does nothing for empty or reversed periods", () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result, patchUrl } = renderPeriodState()

        act(() => {
            result.current.applyExplicitPeriod()
        })
        expect(patchUrl).not.toHaveBeenCalled()

        act(() => {
            result.current.setExplicitFrom("2026-03-01")
            result.current.setExplicitTo("2026-02-01")
        })
        act(() => {
            result.current.applyExplicitPeriod()
        })
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("applyPreset writes the preset with the policy id and version", async () => {
        mockedFetchPolicy.mockResolvedValue(defaultPolicy)
        const { result, patchUrl } = renderPeriodState()
        await waitFor(() => expect(result.current.periodPolicy).toBeTruthy())

        act(() => {
            result.current.applyPreset("ytd", "2026-01-01", "2026-08-01")
        })

        expect(patchUrl).toHaveBeenCalledWith({
            from: "2026-01-01",
            to: "2026-08-01",
            periodPreset: "ytd",
            periodSelectionSource: "CONFIGURED_PRESET",
            customerQualityPeriodPolicyId: "pol-1",
            customerQualityPeriodPolicyVersion: "3",
        })
    })
})
