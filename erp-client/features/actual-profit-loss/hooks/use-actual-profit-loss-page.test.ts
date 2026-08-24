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

/** 筛选写入 URL 统一 replace 且 scroll:false（docs/ui-filter-design.md §5.3）。 */
function expectedReplace(
    base: string,
    extra?: Record<string, string | null>,
): [string, { scroll: false }] {
    return [expectedHref(base, extra), { scroll: false }]
}

const STRUCTURED_URL =
    "&benefitScenario=节日福利&fulfillmentMode=电子交付,公司仓发&costType=printing,logistics"

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
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.filterPanelOpen).toBe(false)
        expect(result.current.benefitScenarioDraft).toBe("")
        expect(result.current.fulfillmentModesDraft).toEqual([])
        expect(result.current.costTypesDraft).toEqual([])
        expect(result.current.appliedChips).toEqual([])
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
            benefitScenario: undefined,
            fulfillmentModes: undefined,
            costTypes: undefined,
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
                `&sort=actualProfitLossNet:desc&customerId=c9&salesOrderId=so9${STRUCTURED_URL}`,
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
            benefitScenario: "节日福利",
            fulfillmentModes: ["公司仓发", "电子交付"],
            costTypes: ["logistics", "printing"],
            dimension: "customer",
            q: "abc",
            sort: "actualProfitLossNet:desc",
            pageSize: 20,
        })
        expect(result.current.tableSorting).toEqual([
            { id: "actualProfitLossNet", desc: true },
        ])
        expect(result.current.hasFilters).toBe(true)
        expect(result.current.hasStructuredFilters).toBe(true)
        expect(result.current.appliedChips).toEqual(
            expect.arrayContaining([
                { key: "q", label: "搜索：abc" },
                { key: "coverage", label: "覆盖：全部覆盖状态" },
                { key: "customerId", label: "客户锁定" },
                { key: "salesOrderId", label: "销售单锁定" },
                { key: "benefitScenario", label: "福利场景：节日福利" },
                { key: "fulfillmentMode:公司仓发", label: "履约方式：公司仓发" },
                { key: "fulfillmentMode:电子交付", label: "履约方式：电子交付" },
                { key: "costType:logistics", label: "成本类型：logistics" },
                { key: "costType:printing", label: "成本类型：printing" },
            ]),
        )
    })

    it("shows the business label for a locked customer chip when the row is visible", async () => {
        api.fetchProfitLossView.mockReset().mockResolvedValue(
            makeView({
                rows: {
                    dimension: "sales_order",
                    items: [
                        makeRow({
                            customerId: "c9",
                            customerLabel: "示例客户",
                            objectId: "so9",
                            identityLabel: "SO-2026-0009",
                        }),
                    ],
                    total: 1,
                },
            }),
        )
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&customerId=c9&salesOrderId=so9`,
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.viewQuery.isSuccess).toBe(true))
        expect(result.current.appliedChips).toEqual(
            expect.arrayContaining([
                { key: "customerId", label: "示例客户" },
                { key: "salesOrderId", label: "SO-2026-0009" },
            ]),
        )
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

    it("keeps the search draft local until an explicit apply action", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        const viewCalls = api.fetchProfitLossView.mock.calls.length
        nav.replace.mockClear()
        act(() => {
            result.current.setSearchInput("hello")
        })
        await act(async () => {
            await new Promise((resolve) => setTimeout(resolve, 400))
        })
        expect(nav.replace).not.toHaveBeenCalled()
        expect(api.fetchProfitLossView.mock.calls.length).toBe(viewCalls)
        expect(result.current.searchInput).toBe("hello")
        act(() => {
            result.current.applyFilters()
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), { q: "hello" }),
        )
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("applies search and structured drafts in one URL patch and closes the panel", async () => {
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&benefitScenario=节日福利`,
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        expect(result.current.filterPanelOpen).toBe(true)
        nav.replace.mockClear()
        act(() => {
            result.current.setSearchInput("abc")
            result.current.setFulfillmentModesDraft(["电子交付"])
            result.current.setCostTypesDraft(["printing"])
        })
        act(() => {
            result.current.applyFilters()
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), {
                benefitScenario: "节日福利",
                q: "abc",
                fulfillmentMode: "电子交付",
                costType: "printing",
            }),
        )
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("does not re-expand the panel when the URL backfills after a submit", async () => {
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&benefitScenario=节日福利`,
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        act(() => {
            result.current.applyFilters()
        })
        expect(result.current.filterPanelOpen).toBe(false)
        // 模拟提交成功后 URL 回填：只同步草稿，不得重新展开面板
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&benefitScenario=节日福利&q=abc`,
        )
        act(() => {
            result.current.setSearchInput("")
        })
        expect(result.current.filterPanelOpen).toBe(false)
        expect(result.current.searchInput).toBe("abc")
        expect(result.current.benefitScenarioDraft).toBe("节日福利")
    })

    it("keeps an in-progress search draft when other URL filters change", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result, rerender } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        act(() => {
            result.current.setSearchInput("hello")
        })
        nav.replace.mockClear()
        act(() => {
            result.current.handleCoverageChange("uncovered")
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), { coverage: "uncovered" }),
        )
        expect(result.current.searchInput).toBe("hello")
        // 模拟 router.replace 已生效：后续 patch 基于更新后的 URL 构建（Next.js 行为）
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&coverage=uncovered`,
        )
        rerender()
        nav.replace.mockClear()
        act(() => {
            result.current.applyFilters()
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), {
                coverage: "uncovered",
                q: "hello",
            }),
        )
    })

    it("clears all filters but keeps dimension and sort when clearing", async () => {
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&q=abc&customerId=c1&salesOrderId=so1` +
                `&coverage=uncovered&dimension=customer&sort=actualProfitLossNet:desc${STRUCTURED_URL}`,
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        expect(result.current.hasFilters).toBe(true)
        nav.replace.mockClear()
        act(() => {
            result.current.clearAllFilters()
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), {
                dimension: "customer",
                sort: "actualProfitLossNet:desc",
            }),
        )
        expect(result.current.searchInput).toBe("")
        expect(result.current.benefitScenarioDraft).toBe("")
        expect(result.current.fulfillmentModesDraft).toEqual([])
        expect(result.current.costTypesDraft).toEqual([])
        expect(result.current.filterPanelOpen).toBe(false)
    })

    it("resetMoreFilters clears structured conditions but keeps q and coverage", async () => {
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&q=abc&coverage=uncovered${STRUCTURED_URL}`,
        )
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        expect(result.current.filterPanelOpen).toBe(true)
        nav.replace.mockClear()
        act(() => {
            result.current.resetMoreFilters()
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), {
                q: "abc",
                coverage: "uncovered",
            }),
        )
        expect(result.current.filterPanelOpen).toBe(true)
        expect(result.current.searchInput).toBe("abc")
    })

    it("removeFilter removes a single applied condition", async () => {
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&q=abc&customerId=c1&benefitScenario=节日福利`,
        )
        const { result, rerender } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        nav.replace.mockClear()
        act(() => {
            result.current.removeFilter("customerId")
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), {
                q: "abc",
                benefitScenario: "节日福利",
            }),
        )
        // 模拟 router.replace 已生效：后续 patch 基于更新后的 URL 构建（Next.js 行为）
        nav.searchParams = new URLSearchParams(
            `${urlWithBasis()}&q=abc&benefitScenario=节日福利`,
        )
        rerender()
        nav.replace.mockClear()
        act(() => {
            result.current.removeFilter("q")
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), { benefitScenario: "节日福利" }),
        )
        expect(result.current.searchInput).toBe("")
    })

    it("does not write the default coverage value to the URL", async () => {
        nav.searchParams = new URLSearchParams(urlWithBasis())
        const { result } = renderHookWithProviders(() =>
            useActualProfitLossPage(),
        )
        await waitFor(() => expect(result.current.basisQuery.isSuccess).toBe(true))
        nav.replace.mockClear()
        act(() => {
            result.current.handleCoverageChange("covered")
        })
        expect(nav.replace).toHaveBeenCalledWith(...expectedReplace(urlWithBasis()))
        act(() => {
            result.current.handleCoverageChange("all")
        })
        expect(nav.replace).toHaveBeenCalledWith(
            ...expectedReplace(urlWithBasis(), { coverage: "all" }),
        )
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

    it("does not focus the background search while a dialog or sheet is open", async () => {
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
        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)
        try {
            act(() => {
                window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }))
            })
            expect(focus).not.toHaveBeenCalled()
        } finally {
            document.body.removeChild(dialog)
        }
        act(() => {
            window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }))
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
            ...expectedReplace(urlWithBasis(), { sort: "marginRate:desc" }),
        )
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
            ...expectedReplace(urlWithBasis(), { coverage: "uncovered" }),
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
            ...expectedReplace(urlWithBasis(), { dimension: "customer" }),
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
        expect(result.current.exportFailed).toBe("未能生成导出文件，请稍后重试。")
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
