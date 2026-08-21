import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useSellableListFilters } from "./use-sellable-list-filters"

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    back: vi.fn(),
}))

let currentSearchParams = new URLSearchParams()

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
    usePathname: () => "/master-data/sellable-items",
    useSearchParams: () => currentSearchParams,
    useParams: () => ({}),
}))

function renderFilters() {
    const searchInputRef = { current: null as HTMLInputElement | null }
    return renderHook(() => useSellableListFilters(searchInputRef))
}

describe("useSellableListFilters", () => {
    beforeEach(() => {
        navMocks.replace.mockClear()
        navMocks.push.mockClear()
        currentSearchParams = new URLSearchParams()
    })

    it("uses defaults for an empty url", () => {
        const { result } = renderFilters()

        expect(result.current.q).toBe("")
        expect(result.current.productKind).toBeUndefined()
        expect(result.current.productCategoryId).toBeUndefined()
        expect(result.current.supplyRegion).toBeUndefined()
        expect(result.current.hasStructuredSellableFilters).toBe(false)
        expect(result.current.hasAdvancedSellableFilters).toBe(false)
        expect(result.current.sellableFilterPanelOpen).toBe(false)
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it("parses sellable filters from the url", () => {
        currentSearchParams = new URLSearchParams(
            "q=pen&productKind=VOUCHER&productCategoryId=c1&productBrandId=b1" +
                "&productSupplierId=s1&supplyRegion=全国&supplyPreset=nationwide&productSalesPriceMin=5&productSalesPriceMax=50",
        )
        const { result } = renderFilters()

        expect(result.current.q).toBe("pen")
        expect(result.current.productKind).toBe("VOUCHER")
        expect(result.current.productCategoryId).toBe("c1")
        expect(result.current.productBrandId).toBe("b1")
        expect(result.current.productSupplierId).toBe("s1")
        expect(result.current.supplyRegion).toBe("全国")
        expect(result.current.supplyPreset).toBe("nationwide")
        expect(result.current.productSalesPriceMin).toBe("5")
        expect(result.current.productSalesPriceMax).toBe("50")
        expect(result.current.hasStructuredSellableFilters).toBe(true)
        expect(result.current.hasAdvancedSellableFilters).toBe(true)
        // 深链带结构化条件时面板必须自动展开，条件要可见可改
        expect(result.current.sellableFilterPanelOpen).toBe(true)
    })

    it("opens the more-filter panel when product kind is in a deep link", () => {
        currentSearchParams = new URLSearchParams("productKind=PHYSICAL")
        const { result } = renderFilters()

        expect(result.current.hasStructuredSellableFilters).toBe(true)
        expect(result.current.hasAdvancedSellableFilters).toBe(false)
        expect(result.current.sellableFilterPanelOpen).toBe(true)
    })

    it("keeps product kind in the draft until filters are submitted", () => {
        const { result } = renderFilters()

        act(() => result.current.setProductKindDraft("PHYSICAL"))
        act(() => result.current.setSellableFilterPanelOpen(true))

        expect(result.current.productKindDraft).toBe("PHYSICAL")
        expect(navMocks.replace).not.toHaveBeenCalled()

        act(() => result.current.applySellableFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/sellable-items?productKind=PHYSICAL",
            { scroll: false },
        )
        expect(result.current.sellableFilterPanelOpen).toBe(false)
    })

    it("applies a supply shortcut without changing the other filters", () => {
        currentSearchParams = new URLSearchParams("q=pen&productBrandId=b1")
        const { result } = renderFilters()

        act(() => result.current.applySupplyPreset("single-supplier"))

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/sellable-items?q=pen&productBrandId=b1&supplyPreset=single-supplier",
            { scroll: false },
        )
    })

    it("removes a single applied condition without touching the others", () => {
        currentSearchParams = new URLSearchParams(
            "q=pen&productBrandId=b1&supplyRegion=全国",
        )
        const { result } = renderFilters()

        act(() => result.current.removeFilter("productBrandId"))

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/sellable-items?q=pen&supplyRegion=%E5%85%A8%E5%9B%BD",
            { scroll: false },
        )
        expect(result.current.productBrandIdDraft).toBeNull()
    })

    it("removes both sales price bounds as one condition", () => {
        currentSearchParams = new URLSearchParams(
            "productSalesPriceMin=5&productSalesPriceMax=50",
        )
        const { result } = renderFilters()

        act(() => result.current.removeFilter("salesPrice"))

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/sellable-items",
            { scroll: false },
        )
        expect(result.current.productSalesPriceMinDraft).toBe("")
        expect(result.current.productSalesPriceMaxDraft).toBe("")
    })

    it("commits a changed search draft to the url", () => {
        const { result } = renderFilters()

        act(() => result.current.setSearchDraft("paper"))
        act(() => result.current.commitSearch())

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/sellable-items?q=paper",
            { scroll: false },
        )
    })

    it("rejects an invalid sales price range without patching the url", () => {
        const { result } = renderFilters()

        act(() => result.current.setProductSalesPriceMinDraft("20"))
        act(() => result.current.setProductSalesPriceMaxDraft("10"))
        act(() => result.current.applySellableFilters())

        expect(result.current.productSalesPriceError).toBe(
            "最低价不能高于最高价",
        )
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it("applies drafts and drops eligibilityAsOf from the url", () => {
        currentSearchParams = new URLSearchParams("eligibilityAsOf=2026-01-01")
        const { result } = renderFilters()

        act(() => {
            result.current.setProductKindDraft("PHYSICAL")
            result.current.setSupplyRegionDraft("华东")
            result.current.setProductSalesPriceMinDraft("1")
        })
        act(() => result.current.applySellableFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/sellable-items?productKind=PHYSICAL&supplyRegion=%E5%8D%8E%E4%B8%9C&productSalesPriceMin=1",
            { scroll: false },
        )
    })

    it("clears every filter including eligibilityAsOf", () => {
        currentSearchParams = new URLSearchParams(
            "q=x&productKind=PHYSICAL&eligibilityAsOf=2026-01-01&supplyPreset=nationwide",
        )
        const { result } = renderFilters()

        act(() => result.current.clearAllFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/sellable-items",
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe("")
        expect(result.current.supplyRegionDraft).toBe("")
    })

    it("resets only more-filter conditions and preserves search and shortcut", () => {
        currentSearchParams = new URLSearchParams(
            "q=x&productKind=PHYSICAL&productBrandId=b1&supplyPreset=nationwide",
        )
        const { result } = renderFilters()

        act(() => result.current.resetMoreFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/sellable-items?q=x&supplyPreset=nationwide",
            { scroll: false },
        )
        expect(result.current.productKindDraft).toBe("all")
        expect(result.current.productBrandIdDraft).toBeNull()
    })
})
