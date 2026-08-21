import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { SellableListToolbar } from "./sellable-list-toolbar"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"

afterEach(cleanup)

const productFilterOptionsQuery = {
    data: { categories: [], brands: [], suppliers: [] },
    isPending: false,
    isError: false,
    error: null,
} as unknown as ReturnType<typeof useProductFilterOptionsQuery>

function renderToolbar(open: boolean) {
    const noop = vi.fn()
    return render(
        <SellableListToolbar
            searchInputRef={{ current: null }}
            searchDraft=""
            setSearchDraft={noop}
            hasActiveFilters={false}
            clearAllFilters={noop}
            appliedChips={[]}
            removeFilter={noop}
            supplyPreset="all"
            supplyPresetCounts={{
                all: 6,
                "single-supplier": 2,
                nationwide: 4,
            }}
            applySupplyPreset={noop}
            sellableFilterPanelOpen={open}
            setSellableFilterPanelOpen={noop}
            hasStructuredSellableFilters={false}
            applySellableFilters={noop}
            resetMoreFilters={noop}
            supplyRegionDraft=""
            setSupplyRegionDraft={noop}
            productKindDraft="all"
            setProductKindDraft={noop}
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

describe("SellableListToolbar", () => {
    it("does not show an inline search submit while more filters are closed", () => {
        renderToolbar(false)

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.queryByRole("button", { name: "应用全部筛选" }),
        ).toBeNull()
    })

    it("shows only the explicit apply action while more filters are open", () => {
        renderToolbar(true)

        expect(
            screen.queryByRole("button", { name: "应用搜索与筛选" }),
        ).toBeNull()
        expect(
            screen.getByRole("button", { name: "应用全部筛选" }),
        ).toBeDefined()
    })
})
