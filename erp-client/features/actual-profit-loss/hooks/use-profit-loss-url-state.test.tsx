import { act, cleanup, renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useProfitLossUrlState } from "@/features/actual-profit-loss/hooks/use-profit-loss-url-state"
import type { ProfitLossPeriodBasisConfig } from "@/features/actual-profit-loss/types"

const navigation = vi.hoisted(() => ({
    pathname: "/finance/actual-profit-loss",
    searchParams: new URLSearchParams(),
    replace: vi.fn(),
    push: vi.fn(),
}))

vi.mock("next/navigation", () => ({
    usePathname: () => navigation.pathname,
    useRouter: () => ({
        replace: navigation.replace,
        push: navigation.push,
    }),
    useSearchParams: () => navigation.searchParams,
}))

const basisConfig: ProfitLossPeriodBasisConfig = {
    configuredPeriodBasis: "recognized_at",
    allowedPeriodBases: [
        {
            code: "recognized_at",
            label: "确认时间",
            explanation: "按收入确认时间归属",
        },
    ],
    configurationVersion: "v1",
}

describe("useProfitLossUrlState", () => {
    beforeEach(() => {
        navigation.searchParams = new URLSearchParams()
        navigation.replace.mockReset()
        navigation.push.mockReset()
    })

    afterEach(cleanup)

    it("keeps an exact unknown historical group across reload and removes it independently", () => {
        navigation.searchParams = new URLSearchParams({
            periodBasis: "recognized_at",
            attributionGroup: "attribution_org:",
            dimension: "sales_order",
            coverage: "all",
        })
        const { result, rerender } = renderHook(() =>
            useProfitLossUrlState({ basisConfig, basisResolved: true }),
        )
        expect(result.current.query?.attributionGroup).toBe("attribution_org:")
        expect(result.current.hasFilters).toBe(true)
        act(() => result.current.removeFilter("attributionGroup"))
        let url = new URL(
            navigation.replace.mock.lastCall![0],
            "http://localhost",
        )
        expect(url.searchParams.has("attributionGroup")).toBe(false)
        expect(url.searchParams.get("coverage")).toBe("all")
        navigation.searchParams.set(
            "attributionGroup",
            "attribution_org:old-org",
        )
        rerender()
        act(() => result.current.clearAllFilters())
        url = new URL(navigation.replace.mock.lastCall![0], "http://localhost")
        expect(url.searchParams.has("attributionGroup")).toBe(false)
    })

    it("includes one-based page and page size in the server query", () => {
        navigation.searchParams = new URLSearchParams({
            from: "2026-08-01",
            to: "2026-08-31",
            periodBasis: "recognized_at",
            page: "2",
            pageSize: "50",
            scopeVersion: "scope-v1",
        })

        const { result } = renderHook(() =>
            useProfitLossUrlState({ basisConfig, basisResolved: true }),
        )

        expect(result.current.query).toMatchObject({
            from: "2026-08-01",
            to: "2026-08-31",
            periodBasis: "recognized_at",
            page: 2,
            pageSize: 50,
        })

        act(() => {
            result.current.setPagination({ pageIndex: 2, pageSize: 50 })
        })
        expect(navigation.replace).toHaveBeenCalledWith(
            "/finance/actual-profit-loss?from=2026-08-01&to=2026-08-31&periodBasis=recognized_at&page=3&pageSize=50&scopeVersion=scope-v1",
            { scroll: false },
        )
    })

    it("writes the configured basis without deleting an explicit period", async () => {
        navigation.searchParams = new URLSearchParams({
            from: "2026-07-01",
            to: "2026-07-31",
        })

        renderHook(() =>
            useProfitLossUrlState({ basisConfig, basisResolved: true }),
        )

        await waitFor(() => {
            expect(navigation.replace).toHaveBeenCalledWith(
                "/finance/actual-profit-loss?from=2026-07-01&to=2026-07-31&periodBasis=recognized_at",
                { scroll: false },
            )
        })
    })
    it("clears retired fulfillment filters and grouping from bookmarked URLs", () => {
        navigation.searchParams = new URLSearchParams(
            "periodBasis=recognized_at&fulfillmentMode=direct&dimension=cost_type&page=3",
        )
        const { result } = renderHook(() =>
            useProfitLossUrlState({ basisConfig, basisResolved: true }),
        )
        expect(result.current.query?.dimension).toBe("sales_order")
        expect(result.current.query).not.toHaveProperty("fulfillmentModes")
        expect(navigation.replace).toHaveBeenCalledWith(
            "/finance/actual-profit-loss?periodBasis=recognized_at",
            { scroll: false },
        )
    })
    it("applies and clears historical identity filters through the same URL state", () => {
        navigation.searchParams = new URLSearchParams(
            "periodBasis=recognized_at&attributionUserIds=sales-a&attributionOrgUnitIds=old-org&dimension=attribution_user&page=2&scopeVersion=old",
        )
        const { result, rerender } = renderHook(() =>
            useProfitLossUrlState({ basisConfig, basisResolved: true }),
        )
        expect(result.current.query).toMatchObject({
            attributionUserIds: ["sales-a"],
            attributionOrgUnitIds: ["old-org"],
            dimension: "attribution_user",
            scopeVersion: "old",
        })
        act(() => result.current.setAttributionUsersDraft(["sales-b"]))
        expect(result.current.query?.attributionUserIds).toEqual(["sales-a"])
        expect(result.current.hasPendingChanges).toBe(true)
        act(() => result.current.applyFilters())
        let url = new URL(
            navigation.replace.mock.lastCall![0],
            "http://localhost",
        )
        expect(url.searchParams.get("attributionUserIds")).toBe("sales-b")
        expect(url.searchParams.has("page")).toBe(false)
        expect(url.searchParams.has("scopeVersion")).toBe(false)
        navigation.searchParams = url.searchParams
        rerender()
        expect(result.current.query?.attributionUserIds).toEqual(["sales-b"])
        act(() => result.current.clearAllFilters())
        url = new URL(navigation.replace.mock.lastCall![0], "http://localhost")
        expect(url.searchParams.has("attributionUserIds")).toBe(false)
        expect(url.searchParams.has("attributionOrgUnitIds")).toBe(false)
    })
})
