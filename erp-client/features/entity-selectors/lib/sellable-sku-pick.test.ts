import { describe, expect, it } from "vitest"

import { sellableItemToPick } from "./sellable-sku-pick"
import type { MasterDataListItem } from "@/features/master-data/types"

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
            baseUnit: "件",
            productKindLabel: "实物",
            salesVisiblePriceGross: "12.00",
            supplierCount: 2,
            supplyRegions: ["全国"],
            eligibilityAsOf: "2026-08-25",
            mainImageAssetId: "asset-1",
        },
        ...overrides,
    }
}

describe("sellableItemToPick", () => {
    it("maps the stable SKU identity and sales fields", () => {
        expect(sellableItemToPick(row())).toEqual({
            skuId: "sku-1",
            skuRevisionId: "rev-1",
            skuNo: "SKU-1",
            name: "礼盒",
            specificationLabel: "颜色：红",
            baseUnit: "件",
            salesVisiblePriceGross: "12.00",
            mainImageAssetId: "asset-1",
            productKind: "PHYSICAL",
        })
    })
})
