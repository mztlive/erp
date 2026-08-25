import { describe, expect, it } from "vitest"

import type { SellableSkuDto } from "@/features/master-data/api/contracts"
import { mapSkuAsSellable } from "@/features/master-data/api/list-mappers"

function sellableDto(overrides: Partial<SellableSkuDto> = {}): SellableSkuDto {
    return {
        sku_id: "sku-1",
        sku_version: 1,
        sku_revision_id: "rev-1",
        sku_revision_no: 1,
        sku_no: "SKU-1",
        product_id: "p-1",
        product_no: "P-1",
        product_kind: "PHYSICAL",
        name: "礼盒",
        specification_attributes: [{ name: "颜色", value: "红" }],
        specification: "颜色：红",
        barcode: null,
        base_unit_id: "u-1",
        base_unit_code: "PCS",
        base_unit_name: "件",
        sales_visible_price_gross: "12.00",
        market_price: null,
        main_image_asset_id: "asset-1",
        effective_from: "2026-01-01",
        effective_to: null,
        supplier_count: 2,
        supply_regions: ["全国"],
        eligibility_as_of: "2026-08-25",
        ...overrides,
    }
}

describe("mapSkuAsSellable", () => {
    it("keeps the main image asset id for picker thumbnails", () => {
        const row = mapSkuAsSellable(sellableDto())
        expect(row.sellableItem?.mainImageAssetId).toBe("asset-1")
        expect(row.stableId).toBe("sku-1")
        expect(row.currentRevisionId).toBe("rev-1")
    })

    it("drops a blank main image asset id", () => {
        const row = mapSkuAsSellable(sellableDto({ main_image_asset_id: "  " }))
        expect(row.sellableItem?.mainImageAssetId).toBeUndefined()
    })
})
