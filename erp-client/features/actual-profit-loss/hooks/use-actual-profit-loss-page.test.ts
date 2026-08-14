import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act, cleanup, waitFor } from "@testing-library/react"

// React 19 act() requires this flag in non-browser test environments.
;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT =
    true

import { renderHookWithProviders } from "@/features/test-utils"
import { useActualProfitLossPage } from "@/features/actual-profit-loss/hooks/use-actual-profit-loss-page"
import { resolvePeriod } from "@/features/actual-profit-loss/lib/url-state"
import {
    configuredBasis,
    fakeCostEntries,
    fakeExportJob,
    makeRow,
    makeView,
    unconfiguredBasis,
} from "@/features/actual-profit-loss/hooks/test-fixtures"

const nav = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: nav.push,
        replace: nav.replace,
        back: nav.back,
    }),
    useSearchParams: () => nav.searchParams,
    usePathname: () => "/test",
    useParams: () => ({}),
}))

const api = vi.hoisted(() => ({
    fetchPeriodBasisConfig: vi.fn(),
    fetchProfitLossView: vi.fn(),
    fetchCostEntriesForRow: vi.fn(),
    startProfitLossExport: vi.fn(),
}))

vi.mock("@/features/actual-profit-loss/api", () => api)

function urlWithBasis(): string {
    return "periodBasis=sales_revenue_recognition_date"
}

function expectedHref(base: string, extra?: Record<string, string | null>): string {
    const next = new URLSearchParams(base)
    for (const [key, value] of Object.entries(extra ?? {})) {
        if (value == null) next.delete(key)
        else next.set(key, value)
    }
    const qs = next.toString()
    return qs ? `/test?${qs}` : "/test"
}

beforeEach(() => {
    nav.push.mockClear()
    nav.replace.mockClear()
    nav.searchParams = new URLSearchParams()
    api.fetchPeriodBasisConfig.mockReset().mockResolvedValue(configuredBasis)
    api.fetchProfitLossView.mockReset().mockResolvedValue(makeView())
    api.fetchCostEntriesForRow.mockReset().mockResolvedValue(fakeCostEntries)
    api.startProfitLossExport.mockReset().mockResolvedValue(fakeExportJob)
    URL.createObjectURL = vi.fn(() => "blob:mock")
    URL.revokeObjectURL = vi.fn()
})

afterEach(() => {
    cleanup()
})

describe("useActualProfitLossPage", () => {
    it("derives defaults for period, coverage and dimension from an empty URL", async () => {
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        const expected = resolvePeriod("month-to-date")
        expect(result.current.from).toBe(expected.from)
        expect(result.current.to).toBe(expected.to)
        expect(result.current.periodBasisUrl).toBe("")
        expect(result.current.periodBasisValid).toBe(false)
        expect(result.current.coverage).toBe("covered")
        expect(result.current.dimension).toBe("sales_order")
        expect(result.current.searchInput).toBe("")
        expect(result.current.hasFilters).toBe(false)
    })

    it("writes the configured period basis into the URL when it is missing", async () => {
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(nav.replace).toHaveBeenCalled())
        expect(nav.replace).toHaveBeenCalledWith(
            expectedHref("", {
                periodBasis: "sales_revenue_recognition_date",
                from: result.current.from,
                to: result.current.to,
            }),
        )
    })

    it("does not rewrite the URL when a period basis is already present", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        await act(async () => {
            await new Promise((resolve) => setTimeout(resolve, 50))
        })
        expect(nav.replace).not.toHaveBeenCalled()
    })

    it("blocks analysis when no basis is configured and none is chosen", async () => {
        api.fetchPeriodBasisConfig.mockReset().mockResolvedValue(unconfiguredBasis)
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        expect(result.current.analysisBlocked).toBe(true)
        expect(result.current.analysisReady).toBe(false)
        expect(api.fetchProfitLossView).not.toHaveBeenCalled()
    })

    it("enables analysis with an explicitly chosen basis even without configuration", async () => {
        api.fetchPeriodBasisConfig.mockReset().mockResolvedValue(unconfiguredBasis)
        nav.searchParams = new URLSearchParams(
            "periodBasis=sales_revenue_recognition_date&from=2026-01-01&to=2026-01-31",
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.analysisReady).toBe(true))
        await waitFor(() => expect(result.current.viewQuery.isSuccess).toBe(true))
        expect(api.fetchProfitLossView).toHaveBeenCalledWith({
            from: "2026-01-01",
            to: "2026-01-31",
            periodBasis: "sales_revenue_recognition_date",
            scopeId: "org-hq-finance",
            coverage: "covered",
            customerId: undefined,
            salesOrderId: undefined,
            dimension: "sales_order",
            q: undefined,
            sort: "actualProfitLossNet:asc",
            pageSize: 20,
        })
    })

    it("flows every supported URL filter into the view query and sorting", async () => {
        nav.searchParams = new URLSearchParams(
            "periodBasis=cost_occurred_date&from=2026-02-01&to=2026-02-28" +
                "&coverage=all&dimension=customer&q=abc" +
                "&sort=actualProfitLossNet:desc&customerId=c9&salesOrderId=so9",
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(api.fetchProfitLossView).toHaveBeenCalled())
        expect(api.fetchProfitLossView).toHaveBeenCalledWith({
            from: "2026-02-01",
            to: "2026-02-28",
            periodBasis: "cost_occurred_date",
            scopeId: "org-hq-finance",
            coverage: "all",
            customerId: "c9",
            salesOrderId: "so9",
            dimension: "customer",
            q: "abc",
            sort: "actualProfitLossNet:desc",
            pageSize: 20,
        })
        expect(result.current.tableSorting).toEqual([
            { id: "actualProfitLossNet", desc: true },
        ])
        expect(result.current.hasFilters).toBe(true)
    })

    it("falls back to defaults for unrecognized URL values", async () => {
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&coverage=banana&dimension=banana`,
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        expect(result.current.coverage).toBe("covered")
        expect(result.current.dimension).toBe("sales_order")
    })

    it("debounces search input into the q URL parameter", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        nav.replace.mockClear()
        act(() => {
            result.current.setSearchInput("hello")
        })
        expect(nav.replace).not.toHaveBeenCalled()
        await waitFor(() => expect(nav.replace).toHaveBeenCalled(), {
            timeout: 2000,
        })
        expect(nav.replace).toHaveBeenCalledWith(
            expectedHref(urlWithBasis(), { q: "hello" }),
        )
    })

    it("does not touch the URL when the debounced search input equals the URL value", async () => {
        nav.searchParams = new URLSearchParams(`${urlWithBasis()}&q=same`)
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        nav.replace.mockClear()
        act(() => {
            result.current.setSearchInput("same")
        })
        await act(async () => {
            await new Promise((resolve) => setTimeout(resolve, 400))
        })
        expect(nav.replace).not.toHaveBeenCalled()
    })

    it("focuses the search input on the '/' keyboard shortcut and respects modifiers", async () => {
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        const focus = vi.fn()
        act(() => {
            result.current.searchInputRef.current = {
                focus,
            } as unknown as HTMLInputElement
        })
        act(() => {
            window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }))
        })
        expect(focus).toHaveBeenCalledTimes(1)
        act(() => {
            window.dispatchEvent(
                new KeyboardEvent("keydown", { key: "/", metaKey: true }),
            )
        })
        expect(focus).toHaveBeenCalledTimes(1)
    })

    it("writes a header sort change into the URL and resets the page", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        nav.replace.mockClear()
        act(() => {
            result.current.handleTableSortingChange([
                { id: "marginRate", desc: true },
            ])
        })
        expect(nav.replace).toHaveBeenCalledWith(
            expectedHref(urlWithBasis(), { sort: "marginRate:desc" }),
        )
    })

    it("removes q/customerId/salesOrderId/coverage when clearing filters", async () => {
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&q=abc&customerId=c1&salesOrderId=so1&coverage=uncovered`,
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        expect(result.current.hasFilters).toBe(true)
        nav.replace.mockClear()
        act(() => {
            result.current.clearFilters()
        })
        expect(nav.replace).toHaveBeenCalledWith(expectedHref(urlWithBasis()))
    })

    it("updates the coverage filter through its handler", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        nav.replace.mockClear()
        act(() => {
            result.current.handleCoverageChange("uncovered")
        })
        expect(nav.replace).toHaveBeenCalledWith(
            expectedHref(urlWithBasis(), { coverage: "uncovered" }),
        )
    })

    it("updates the dimension through its handler", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        nav.replace.mockClear()
        act(() => {
            result.current.handleDimensionChange("customer")
        })
        expect(nav.replace).toHaveBeenCalledWith(
            expectedHref(urlWithBasis(), { dimension: "customer" }),
        )
    })

    it("opens the cost detail for drillable rows and derives the selected entry", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.viewQuery.isSuccess).toBe(true))
        const row = result.current.data?.rows.items[0]
        expect(row).toBeDefined()
        act(() => {
            result.current.openCostDetail(row!)
        })
        expect(result.current.costDetailRow?.rowId).toBe(row?.rowId)
        expect(result.current.selectedCostEntryId).toBe("ce-1")
        await waitFor(() =>
            expect(result.current.costEntriesQuery.isSuccess).toBe(true),
        )
        expect(api.fetchCostEntriesForRow).toHaveBeenCalledWith(["ce-1", "ce-2"])
        await waitFor(() =>
            expect(result.current.selectedEntry?.costEntryId).toBe("ce-1"),
        )
    })

    it("ignores rows without the cost_entry drilldown", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        const noDrill = makeRow({ allowedDrilldowns: [], costEntryIds: [] })
        act(() => {
            result.current.openCostDetail(noDrill)
        })
        expect(result.current.costDetailRow).toBeNull()
        expect(result.current.selectedCostEntryId).toBeNull()
        expect(api.fetchCostEntriesForRow).not.toHaveBeenCalled()
    })

    it("refetches the view and basis queries on manual refresh", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.viewQuery.isSuccess).toBe(true))
        const viewCalls = api.fetchProfitLossView.mock.calls.length
        await act(async () => {
            await result.current.handleRefresh()
        })
        expect(api.fetchProfitLossView.mock.calls.length).toBe(viewCalls + 1)
        expect(api.fetchPeriodBasisConfig.mock.calls.length).toBe(2)
        expect(result.current.refreshFailed).toBeNull()
    })

    it("retains last data without a failure label when refresh hits a fetch error", async () => {
        // Default client options (no throwOnError): refetch() resolves on error,
        // so handleRefresh keeps the previous view data and sets no failure label.
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.viewQuery.isSuccess).toBe(true))
        api.fetchProfitLossView.mockRejectedValueOnce(new Error("boom"))
        await act(async () => {
            await result.current.handleRefresh()
        })
        expect(result.current.refreshFailed).toBeNull()
        expect(result.current.viewQuery.isError).toBe(true)
        expect(result.current.data).toEqual(makeView())
    })

    it("starts an export with the active query and view, then records the job", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.viewQuery.isSuccess).toBe(true))
        await act(async () => {
            await result.current.handleExport()
        })
        expect(api.startProfitLossExport).toHaveBeenCalledTimes(1)
        const input = api.startProfitLossExport.mock.calls[0][0] as {
            query: { periodBasis: string; scopeId: string }
            view: unknown
            coverage: string
        }
        expect(input.query.periodBasis).toBe("sales_revenue_recognition_date")
        expect(input.query.scopeId).toBe("org-hq-finance")
        expect(input.coverage).toBe("covered")
        expect(input.view).toEqual(result.current.data)
        expect(result.current.exportJob).toEqual(fakeExportJob)
        expect(result.current.exportFailed).toBeNull()
    })

    it("records a failure message when export fails", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.viewQuery.isSuccess).toBe(true))
        api.startProfitLossExport.mockRejectedValueOnce(new Error("boom"))
        await act(async () => {
            await result.current.handleExport()
        })
        expect(result.current.exportFailed).toBe("boom")
        expect(result.current.exportJob).toBeNull()
    })

    it("does not export without permission or data", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        api.fetchProfitLossView.mockReset().mockResolvedValue(
            makeView({
                fieldPermissions: {
                    canViewRevenue: true,
                    canViewCost: true,
                    canViewProfit: true,
                    canExport: false,
                },
            }),
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.viewQuery.isSuccess).toBe(true))
        await act(async () => {
            await result.current.handleExport()
        })
        expect(api.startProfitLossExport).not.toHaveBeenCalled()
        expect(result.current.exportJob).toBeNull()
    })
})
