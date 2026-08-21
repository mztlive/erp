import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { ProductListToolbar } from "./product-list-toolbar"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"

afterEach(cleanup)

const productFilterOptionsQuery = {
    data: { categories: [], brands: [], suppliers: [] },
    isPending: false,
    isError: false,
    error: null,
} as unknown as ReturnType<typeof useProductFilterOptionsQuery>

function renderToolbar(open: boolean, chips: unknown[] = []) {
    const noop = vi.fn()
    return render(
        <ProductListToolbar
            searchInputRef={{ current: null }}
            searchDraft=""
            setSearchDraft={noop}
            hasActiveFilters={chips.length > 0}
            clearAllFilters={noop}
            appliedChips={chips as never}
            removeFilter={noop}
            productFilterPanelOpen={open}
            setProductFilterPanelOpen={noop}
            hasStructuredProductFilters={false}
            applyProductFilters={noop}
            resetMoreFilters={noop}
            productKindDraft="all"
            setProductKindDraft={noop}
            lifecycleStatusDraft="all"
            setLifecycleStatusDraft={noop}
            revisionTimingDraft="all"
            setRevisionTimingDraft={noop}
            productListingStatusDraft="all"
            setProductListingStatusDraft={noop}
            productSupplyCoverageDraft="all"
            setProductSupplyCoverageDraft={noop}
            productCategoryIdDraft={null}
            setProductCategoryIdDraft={noop}
            productBrandIdDraft={null}
            setProductBrandIdDraft={noop}
            productSupplierIdDraft={null}
            setProductSupplierIdDraft={noop}
            productSalesPriceMinDraft=""
            setProductSalesPriceMinDraft={noop}
            productSalesPriceMaxDraft=""
            setProductSalesPriceMaxDraft={noop}
            productSalesPriceError={null}
            setProductSalesPriceError={noop}
            productFilterOptionsQuery={productFilterOptionsQuery}
        />,
    )
}

describe("ProductListToolbar", () => {
    it("does not show an inline search submit while more filters are closed", () => {
        renderToolbar(false)

        expect(
            screen.queryByRole("button", { name: "搜索" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
    })

    it("shows only the explicit apply action while more filters are open", () => {
        renderToolbar(true)

        expect(
            screen.queryByRole("button", { name: "搜索" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
        expect(
            screen.getByRole("button", { name: "重置更多条件" }),
        ).toBeDefined()
    })

    it("wires the more-filters toggle with aria-expanded and aria-controls", () => {
        renderToolbar(false)

        const toggle = screen.getByRole("button", { name: /更多筛选/ })
        expect(toggle.getAttribute("aria-expanded")).toBe("false")
        expect(toggle.hasAttribute("aria-controls")).toBe(true)
    })

    it("renders applied chips with clear-all in the secondary row", () => {
        renderToolbar(false, [
            { key: "q", label: "搜索：鞋" },
            { key: "salesPrice", label: "销售价：10 至 99" },
        ])

        expect(screen.getByText("已筛选")).toBeDefined()
        expect(screen.getByRole("button", { name: "清空全部" })).toBeDefined()
        expect(screen.getByText("搜索：鞋")).toBeDefined()
        expect(screen.getByText("销售价：10 至 99")).toBeDefined()
    })
})
