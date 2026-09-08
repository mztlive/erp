import { describe, expect, it } from "vitest"

import {
    PAUSE_OFFERING_INTENT,
    RESUME_OFFERING_INTENT,
    STOP_OFFERING_INTENT,
    statusIntentsFor,
    statusRevisionBlocker,
} from "./offering-status"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"

function offering(
    overrides: Partial<SupplierOfferingView> = {},
): SupplierOfferingView {
    return {
        id: "offering-1",
        sku_id: "sku-1",
        supplier_id: "supplier-1",
        supplier_sku_code: "SUP-SKU-1",
        source_type: "MANUAL",
        status: "ACTIVE",
        current_revision_no: 1,
        dropship_supply_price_gross: "10",
        bulk_supply_price_gross: "9",
        input_tax_rate: "0.13",
        bulk_minimum_order_quantity: "1",
        supply_region: ["全国"],
        product_capabilities: [],
        valid_from: "2026-08-08",
        version: 1,
        created_at: 1,
        ...overrides,
    }
}

describe("statusIntentsFor", () => {
    it("启用中可暂停或停止，暂停中可启用或停止，已停止只能启用", () => {
        expect(statusIntentsFor("ACTIVE")).toEqual([
            PAUSE_OFFERING_INTENT,
            STOP_OFFERING_INTENT,
        ])
        expect(statusIntentsFor("PAUSED")).toEqual([
            RESUME_OFFERING_INTENT,
            STOP_OFFERING_INTENT,
        ])
        expect(statusIntentsFor("STOPPED")).toEqual([RESUME_OFFERING_INTENT])
    })
})

describe("statusRevisionBlocker", () => {
    it("当前条款完整时允许状态修订", () => {
        expect(statusRevisionBlocker(offering())).toBeNull()
    })

    it("看不到价格时要求改走修订条款", () => {
        expect(
            statusRevisionBlocker(
                offering({ dropship_supply_price_gross: null }),
            ),
        ).toBe("当前条款不完整或看不到价格，请先修订条款。")
    })
})
