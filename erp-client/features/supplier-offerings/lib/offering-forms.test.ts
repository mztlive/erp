import { describe, expect, it } from "vitest"

import { buildStatusRevisionInput } from "./offering-forms"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"

function offering(
    overrides: Partial<SupplierOfferingView> = {},
): SupplierOfferingView {
    return {
        id: "offering-1",
        sku_id: "sku-1",
        sku_no: "SKU-1",
        product_no: "P-1",
        sku_name: "测试 SKU",
        specification: "默认规格",
        supplier_id: "supplier-1",
        supplier_no: "S-1",
        supplier_name: "测试供应商",
        supplier_product_code: "SPU-1",
        supplier_sku_code: "SUP-SKU-1",
        source_type: "MANUAL",
        source_connection_id: null,
        status: "ACTIVE",
        current_revision_id: "rev-1",
        current_revision_no: 3,
        dropship_supply_price_gross: "11.30",
        dropship_supply_price_net: "9.83",
        bulk_supply_price_gross: "9.04",
        bulk_supply_price_net: "7.86",
        input_tax_rate: "0.13",
        bulk_minimum_order_quantity: "10",
        supply_region: ["全国"],
        product_capabilities: ["REFUND"],
        dropship_express: "顺丰",
        freight_amount: "1.00",
        service_fee_amount: null,
        valid_from: "2026-08-08",
        valid_to: null,
        availability_status: "AVAILABLE",
        available_quantity: "100",
        availability_source_updated_at: null,
        availability_version: 1,
        version: 4,
        created_at: 1,
        ...overrides,
    }
}

describe("buildStatusRevisionInput", () => {
    it("复制当前条款并带上新状态，沿用当前修订号作为期望版本", () => {
        const input = buildStatusRevisionInput(
            offering(),
            "PAUSED",
            " 暂停供给关系 ",
            "idem-1",
        )

        expect(input).toEqual({
            offeringId: "offering-1",
            expected_revision_no: 3,
            terms: {
                dropship_supply_price_gross: "11.30",
                bulk_supply_price_gross: "9.04",
                input_tax_rate: "0.13",
                bulk_minimum_order_quantity: "10",
                supply_region: ["全国"],
                product_capabilities: ["REFUND"],
                valid_from: "2026-08-08",
                valid_to: null,
                dropship_express: "顺丰",
                freight_amount: "1.00",
                service_fee_amount: null,
            },
            status: "PAUSED",
            change_reason: "暂停供给关系",
            idempotency_key: "idem-1",
        })
    })

    it("条款不完整时拒绝构造，避免写出残缺新版本", () => {
        expect(
            buildStatusRevisionInput(
                offering({ dropship_supply_price_gross: null }),
                "STOPPED",
                "停止供给关系",
                "idem-2",
            ),
        ).toBeNull()
        expect(
            buildStatusRevisionInput(
                offering({ current_revision_no: null }),
                "ACTIVE",
                "启用供给关系",
                "idem-3",
            ),
        ).toBeNull()
    })
})
