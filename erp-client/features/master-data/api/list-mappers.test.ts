import { describe, expect, it } from "vitest"

import type {
    SellableSkuDto,
    SupplierDto,
} from "@/features/master-data/api/contracts"
import {
    mapSkuAsSellable,
    mapSupplierRow,
} from "@/features/master-data/api/list-mappers"

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

function supplierDto(overrides: Partial<SupplierDto> = {}): SupplierDto {
    return {
        id: "sup-1",
        party_id: "party-1",
        party_no: "PTY-1",
        legal_name: "华东福利供应商",
        short_name: "华东福利",
        party_version: 1,
        supplier_no: "SUP-000123",
        default_payment_term_id: null,
        current_commercial_profile_revision_id: "rev-1",
        status: "active",
        version: 3,
        created_at: 1_700_000_000,
        current_profile: {
            id: "rev-1",
            supplier_id: "sup-1",
            revision_no: 2,
            settlement_mode: "monthly",
            reconciliation_cycle: "monthly",
            payment_term_snapshot: "PERIOD_MONTH_15",
            business_category: "礼盒",
            invoice_type: "vat_special",
            invoice_tax_rate: "0.13",
            invoice_tax_rates: ["0.13", "0.09"],
            signing_entity_party_id: "party-sign",
            signing_entity_name: "上海某某公司",
            payment_entity_party_id: "party-pay",
            payment_entity_name: "上海某某公司",
            change_reason: "初始",
            version: 1,
            created_at: 1_700_000_000,
        },
        capability_codes: ["physical", "printing"],
        qualification_health: "expiring_30",
        qualification_types: ["contract", "authorization"],
        ...overrides,
    }
}

describe("mapSupplierRow", () => {
    it("projects capabilities, qualification and commercial facts for the list", () => {
        const row = mapSupplierRow(supplierDto())
        expect(row.stableNo).toBe("SUP-000123")
        expect(row.name).toBe("华东福利供应商")
        expect(row.supplierList).toEqual({
            capabilityCodes: ["physical", "printing"],
            qualificationHealth: "expiring_30",
            qualificationTypes: ["contract", "authorization"],
            settlementLabel: "月结",
            paymentTermLabel: "月结，期末后 15 天付款",
            signingEntityName: "上海某某公司",
            paymentEntityName: "上海某某公司",
            invoiceTypeLabel: "增值税专用发票",
            invoiceTaxRatesLabel: "13%、9%",
            businessCategory: "礼盒",
            maintainerUserId: undefined,
            maintainerUserName: undefined,
            businessOrgUnitId: undefined,
        })
        expect(row.keyFacts.map((fact) => fact.label)).toEqual(
            expect.arrayContaining(["供应能力", "资质状态", "签约主体"]),
        )
        expect(
            row.keyFacts.find((fact) => fact.label === "资质状态")?.value,
        ).toBe("30 天内到期")
    })

    it("keeps missing list projection fields empty instead of inventing health", () => {
        const row = mapSupplierRow(
            supplierDto({
                capability_codes: undefined,
                qualification_health: undefined,
                qualification_types: undefined,
                current_profile: null,
            }),
        )
        expect(row.supplierList?.capabilityCodes).toEqual([])
        expect(row.supplierList?.qualificationHealth).toBeUndefined()
        expect(row.supplierList?.settlementLabel).toBe("—")
        expect(
            row.keyFacts.find((fact) => fact.label === "资质状态")?.value,
        ).toBe("—")
    })
})
