import { describe, expect, it } from "vitest"

import {
    applySpecsFromDrafts,
    createSpecDraft,
    parseProductSectionId,
    productSectionForValidationError,
} from "@/features/master-data/lib/product-editor-model"
import { emptyProductFields } from "./product-model"

it("preserves matching SKU identities, precise prices and images when adding or reordering values", () => {
    const fields = {
        ...emptyProductFields(),
        specs: [{ name: "颜色", values: ["红色"] }],
    }
    Object.assign(fields.skus[0], {
        skuId: "red",
        skuNo: "SKU-01",
        specificationSignature: "颜色=红色",
        attributeValues: ["红色"],
        specLabel: "颜色：红色",
        salePrice: "9007199254740993.0001",
        barcode: "CUSTOM",
        mainImage: "red.png",
        mainImagePreviewUrl: "blob:red",
        listingStatus: "LISTED",
    })
    const next = applySpecsFromDrafts(
        [createSpecDraft("颜色", ["蓝色", "红色"])],
        fields,
    )
    expect(next.skus[1]).toMatchObject(fields.skus[0])
    expect(next.skus[0].skuNo).not.toBe("SKU-01")
    const reordered = applySpecsFromDrafts(
        [createSpecDraft("颜色", ["红色", "蓝色"])],
        next,
    )
    expect(reordered.skus[0]).toEqual(next.skus[1])
    expect(reordered.skus[1]).toEqual(next.skus[0])
})

describe("parseProductSectionId", () => {
    it("empty hash falls back to basic", () => {
        expect(parseProductSectionId("", false)).toBe("basic")
        expect(parseProductSectionId("#", true)).toBe("basic")
    })

    it("reads product-section hash with or without #", () => {
        expect(parseProductSectionId("#product-section-sku", false)).toBe("sku")
        expect(parseProductSectionId("product-section-media", true)).toBe(
            "media",
        )
    })

    it("drops history on create", () => {
        expect(parseProductSectionId("#product-section-history", true)).toBe(
            "basic",
        )
        expect(parseProductSectionId("#product-section-history", false)).toBe(
            "history",
        )
    })

    it("unknown hash falls back to basic", () => {
        expect(parseProductSectionId("#product-section-nope", false)).toBe(
            "basic",
        )
        expect(parseProductSectionId("#other", false)).toBe("basic")
    })
})

describe("productSectionForValidationError", () => {
    it("routes identity errors to basic", () => {
        expect(productSectionForValidationError("请填写商品名称")).toBe("basic")
        expect(productSectionForValidationError("请选择有效品牌")).toBe("basic")
    })

    it("routes sku and spec errors to sku", () => {
        expect(productSectionForValidationError("请至少生成一个 SKU")).toBe(
            "sku",
        )
        expect(
            productSectionForValidationError("规格「颜色」至少填写一个取值"),
        ).toBe("sku")
        expect(
            productSectionForValidationError("启用中的 SKU「A」必须上传主图"),
        ).toBe("sku")
    })

    it("routes reason errors to effective", () => {
        expect(
            productSectionForValidationError("请填写本次保存的变更原因"),
        ).toBe("effective")
    })
})
