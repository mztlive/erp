import { describe, expect, it } from "vitest"

import { toSellablePickerListQuery } from "./sellable-sku-picker-query"

const pagination = { pageIndex: 1, pageSize: 20 }

describe("sales-order toSellablePickerListQuery", () => {
    it("maps nationwide preset to the nationwide supply region", () => {
        const query = toSellablePickerListQuery(
            { q: " 礼盒 ", supplyPreset: "nationwide" },
            pagination,
        )
        expect(query).toEqual({
            q: "礼盒",
            productKind: undefined,
            productCategoryId: undefined,
            productBrandId: undefined,
            productSupplierId: undefined,
            supplyRegion: "全国",
            productSalesPriceMin: undefined,
            productSalesPriceMax: undefined,
            maxSupplierCount: undefined,
            page: 2,
            pageSize: 20,
        })
    })

    it("keeps an explicit region ahead of the nationwide preset", () => {
        const query = toSellablePickerListQuery(
            {
                q: "",
                supplyPreset: "nationwide",
                supplyRegion: "北京",
            },
            pagination,
        )
        expect(query.supplyRegion).toBe("北京")
    })

    it("maps the single-supplier preset to a supplier-count cap", () => {
        const query = toSellablePickerListQuery(
            { q: "", supplyPreset: "single-supplier" },
            { pageIndex: 0, pageSize: 50 },
        )
        expect(query.maxSupplierCount).toBe(1)
        expect(query.page).toBe(1)
        expect(query.pageSize).toBe(50)
    })
})
