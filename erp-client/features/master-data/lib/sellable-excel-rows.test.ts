import { describe, expect, it } from "vitest"

import type { MasterDataListItem } from "@/features/master-data/types"
import {
    excelImageExtension,
    mapWithConcurrency,
    selectSellableRows,
    sellableSupplierLabel,
    toSellableExcelRow,
} from "./sellable-excel-rows"

function row(overrides: Partial<MasterDataListItem> = {}): MasterDataListItem {
    return {
        objectType: "sellable-items",
        stableId: "sku-1",
        stableNo: "SKU-1",
        name: "礼盒",
        lifecycleStatus: "ENABLED",
        lifecycleStatusLabel: "当前启用",
        lifecycleTone: "success",
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: "rev-1",
        displayedRevisionId: "rev-1",
        revisionNo: 1,
        effectiveFrom: "2026-01-01",
        keyFacts: [],
        selectorEligibility: [],
        allowedActions: [],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: ["enabled"],
        productKind: "PHYSICAL",
        sellableItem: {
            productId: "p-1",
            productNo: "P-1",
            specificationAttributes: [{ name: "颜色", value: "红" }],
            specificationLabel: "颜色：红",
            barcode: "123456",
            baseUnit: "件",
            productKindLabel: "实物",
            salesVisiblePriceGross: "12.00",
            marketPrice: "15.00",
            supplierCount: 2,
            supplyRegions: ["全国", "上海"],
            eligibilityAsOf: "2026-08-25",
            mainImageAssetId: "asset-1",
        },
        ...overrides,
    }
}

describe("toSellableExcelRow", () => {
    it("covers every table column plus the extra identity fields", () => {
        expect(toSellableExcelRow(row())).toEqual({
            name: "礼盒",
            specification: "颜色：红",
            skuNo: "SKU-1",
            productNo: "P-1",
            salesPrice: "12.00",
            marketPrice: "15.00",
            supplyRegions: "全国、上海",
            supplierLabel: "2 家可供",
            productKind: "实物",
            baseUnit: "件",
            barcode: "123456",
            imageAssetId: "asset-1",
        })
    })

    it("writes placeholders when optional fields are empty", () => {
        expect(
            toSellableExcelRow(
                row({
                    sellableItem: {
                        ...row().sellableItem!,
                        specificationLabel: "无规格",
                        marketPrice: undefined,
                        barcode: undefined,
                        supplyRegions: [],
                        supplierCount: 1,
                        mainImageAssetId: undefined,
                    },
                }),
            ),
        ).toMatchObject({
            specification: "—",
            marketPrice: "—",
            barcode: "—",
            supplyRegions: "未标注",
            supplierLabel: "单一供应商",
            imageAssetId: undefined,
        })
    })
})

describe("selectSellableRows", () => {
    it("keeps the current result order and drops unselected rows", () => {
        const first = row({ stableId: "sku-1" })
        const second = row({ stableId: "sku-2", name: "茶杯" })
        const third = row({ stableId: "sku-3", name: "茶盘" })
        expect(
            selectSellableRows(
                [first, second, third],
                new Set(["sku-3", "sku-1"]),
            ),
        ).toEqual([first, third])
    })
})

describe("excelImageExtension", () => {
    it("reads jpeg/png/gif from content type or file name", () => {
        expect(excelImageExtension("image/jpeg")).toBe("jpeg")
        expect(excelImageExtension("image/png; charset=binary")).toBe("png")
        expect(excelImageExtension("application/octet-stream", "a.GIF")).toBe(
            "gif",
        )
        expect(excelImageExtension("image/webp")).toBeUndefined()
    })
})

describe("sellableSupplierLabel", () => {
    it("matches the table supply-risk copy", () => {
        expect(sellableSupplierLabel(0)).toBe("单一供应商")
        expect(sellableSupplierLabel(1)).toBe("单一供应商")
        expect(sellableSupplierLabel(3)).toBe("3 家可供")
    })
})

describe("mapWithConcurrency", () => {
    it("preserves order with a worker limit", async () => {
        const values = await mapWithConcurrency([3, 2, 1], 2, async (item) => {
            await Promise.resolve()
            return item * 10
        })
        expect(values).toEqual([30, 20, 10])
    })
})
