import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useSupplierListFilters } from "./use-supplier-list-filters"

const navMocks = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => "/master-data/suppliers",
    useParams: () => ({}),
}))

function queryOf(call: unknown[]): URLSearchParams {
    const url = String(call[0])
    const idx = url.indexOf("?")
    return new URLSearchParams(idx >= 0 ? url.slice(idx + 1) : "")
}

function pathOf(call: unknown[]): string {
    const url = String(call[0])
    const idx = url.indexOf("?")
    return idx >= 0 ? url.slice(0, idx) : url
}

beforeEach(() => {
    navMocks.searchParams = new URLSearchParams()
    navMocks.push.mockClear()
    navMocks.replace.mockClear()
})

describe("useSupplierListFilters", () => {
    it("falls back to defaults on an empty URL", () => {
        const searchInputRef = { current: null }
        const { result } = renderHook(() =>
            useSupplierListFilters(searchInputRef),
        )
        expect(result.current.q).toBe("")
        expect(result.current.lifecycleStatus).toBe("all")
        expect(result.current.supplierCapabilityCodes).toEqual([])
        expect(result.current.supplierQualificationTypes).toEqual([])
        expect(result.current.supplierQualificationHealth).toBeUndefined()
        expect(result.current.metricKey).toBe("all")
        expect(result.current.hasStructuredSupplierFilters).toBe(false)
        expect(result.current.supplierFilterPanelOpen).toBe(false)
    })

    it("parses every filter from the URL and drops invalid enum values", () => {
        navMocks.searchParams = new URLSearchParams(
            "lifecycleStatus=enabled&supplierCapabilityCodes=api,physical,bogus&supplierQualificationTypes=certificate,contract&supplierQualificationHealth=expired&metricKey=disabled",
        )
        const searchInputRef = { current: null }
        const { result } = renderHook(() =>
            useSupplierListFilters(searchInputRef),
        )
        expect(result.current.lifecycleStatus).toBe("enabled")
        expect(result.current.supplierCapabilityCodes).toEqual([
            "api",
            "physical",
        ])
        expect(result.current.supplierQualificationTypes).toEqual([
            "certificate",
            "contract",
        ])
        expect(result.current.supplierQualificationHealth).toBe("expired")
        expect(result.current.metricKey).toBe("disabled")
        expect(result.current.hasStructuredSupplierFilters).toBe(true)
        expect(result.current.supplierFilterPanelOpen).toBe(true)
    })

    it("treats an invalid lifecycleStatus as all", () => {
        navMocks.searchParams = new URLSearchParams("lifecycleStatus=bogus")
        const searchInputRef = { current: null }
        const { result } = renderHook(() =>
            useSupplierListFilters(searchInputRef),
        )
        expect(result.current.lifecycleStatus).toBe("all")
        expect(result.current.hasStructuredSupplierFilters).toBe(false)
    })

    it("applySupplierFilters writes drafts into the URL and resets the page", () => {
        const searchInputRef = { current: null }
        const { result } = renderHook(() =>
            useSupplierListFilters(searchInputRef),
        )
        act(() => {
            result.current.setSearchDraft("茶叶")
            result.current.setLifecycleStatusDraft("disabled")
            result.current.setSupplierCapabilityCodesDraft(["physical"])
            result.current.setSupplierQualificationTypesDraft([
                "food_license",
            ])
            result.current.setSupplierQualificationHealthDraft("valid")
            result.current.changePagination({ pageIndex: 2, pageSize: 20 })
        })
        act(() => {
            result.current.applySupplierFilters()
        })

        const [url] = navMocks.replace.mock.calls.at(-1)!
        expect(pathOf([url])).toBe("/master-data/suppliers")
        const params = queryOf([url])
        expect(params.get("q")).toBe("茶叶")
        expect(params.get("lifecycleStatus")).toBe("disabled")
        expect(params.get("metricKey")).toBe("disabled")
        expect(params.get("supplierCapabilityCodes")).toBe("physical")
        expect(params.get("supplierQualificationTypes")).toBe("food_license")
        expect(params.get("supplierQualificationHealth")).toBe("valid")
        expect(params.has("page")).toBe(false)
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it("clearAllFilters resets drafts, closes the panel and clears the URL", () => {
        navMocks.searchParams = new URLSearchParams(
            "lifecycleStatus=enabled&supplierCapabilityCodes=physical",
        )
        const searchInputRef = { current: null }
        const { result } = renderHook(() =>
            useSupplierListFilters(searchInputRef),
        )
        act(() => {
            result.current.clearAllFilters()
        })
        expect(result.current.searchDraft).toBe("")
        expect(result.current.lifecycleStatusDraft).toBe("all")
        expect(result.current.supplierCapabilityCodesDraft).toEqual([])
        expect(result.current.supplierQualificationTypesDraft).toEqual([])
        expect(result.current.supplierQualificationHealthDraft).toBe("all")
        expect(result.current.supplierFilterPanelOpen).toBe(false)

        const [url] = navMocks.replace.mock.calls[0]!
        const params = queryOf([url])
        expect(params.toString()).toBe("")
    })

    it("commitSearch skips the URL write when the query did not change", () => {
        navMocks.searchParams = new URLSearchParams("q=茶叶")
        const searchInputRef = { current: null }
        const { result } = renderHook(() =>
            useSupplierListFilters(searchInputRef),
        )
        act(() => {
            result.current.setSearchDraft("  茶叶  ")
        })
        act(() => {
            result.current.commitSearch()
        })
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it("commitSearch writes a changed query and clears the page", () => {
        const searchInputRef = { current: null }
        const { result } = renderHook(() =>
            useSupplierListFilters(searchInputRef),
        )
        act(() => {
            result.current.setSearchDraft("礼盒")
        })
        act(() => {
            result.current.commitSearch()
        })
        const [url] = navMocks.replace.mock.calls[0]!
        const params = queryOf([url])
        expect(params.get("q")).toBe("礼盒")
        expect(params.has("page")).toBe(false)
        expect(result.current.pagination.pageIndex).toBe(0)
    })
})
