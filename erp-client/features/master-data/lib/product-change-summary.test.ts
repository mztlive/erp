import { describe, expect, it } from "vitest"
import { createProductDefaults } from "./product-editor-model"
import { productChangeSummary } from "./product-change-summary"

function baseline() {
    const value = createProductDefaults(false)
    return {
        ...value,
        fields: {
            ...value.fields,
            carouselImages: [...value.fields.carouselImages],
            carouselFileAssetIds: { ...value.fields.carouselFileAssetIds },
            skus: [
                {
                    ...value.fields.skus[0],
                    skuId: "sku-1",
                    name: "礼盒",
                    salePrice: "9007199254740993.0001",
                    mainImage: "tea.png",
                },
            ],
        },
    }
}

describe("productChangeSummary", () => {
    it("retains decimal strings beyond Number precision in price changes", () => {
        const before = baseline()
        const next = structuredClone(before)
        next.fields.skus[0].salePrice = "9007199254740993.0002"
        expect(productChangeSummary(before, next)).toEqual([
            "礼盒 · 销售价：9007199254740993.0001 → 9007199254740993.0002",
        ])
    })
    it("matches SKU identity across reordering and includes removed rows", () => {
        const before = baseline()
        before.fields.skus.push({
            ...before.fields.skus[0],
            skuId: "sku-2",
            name: "小礼盒",
            salePrice: "10.00",
        })
        const next = structuredClone(before)
        next.fields.skus.reverse()
        next.fields.skus[0].salePrice = "12.00"
        next.fields.skus.pop()
        expect(productChangeSummary(before, next)).toEqual([
            "SKU 数量：2 → 1",
            "小礼盒 · 销售价：10.00 → 12.00",
        ])
    })
    it("ignores batch input drafts but detects replacement assets with the same file name", () => {
        const before = baseline()
        before.fields.carouselImages = ["tea.png"]
        before.fields.carouselFileAssetIds = { "tea.png": "old" }
        const next = { ...structuredClone(before), batchSalePrice: "99.00" }
        expect(productChangeSummary(before, next)).toEqual([])
        next.fields.carouselFileAssetIds = { "tea.png": "new" }
        expect(productChangeSummary(before, next)).toEqual(["轮播图"])
    })
})
