import { describe, it, expect } from "vitest"

import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import { findOffering, singleCapabilityForMode } from "./offering"

function makeOption(
    overrides: Partial<ProcurementSupplyOption> = {},
): ProcurementSupplyOption {
    return {
        skuId: "sku_1",
        supplierId: "sup_1",
        offeringRevisionId: "off_1",
        offeringRevisionNo: 1,
        costGross: "10",
        bulkCostGross: "8",
        dropshipCostGross: "10",
        bulkMinimumOrderQuantity: "5",
        inputTaxRate: "0.13",
        freightAmount: "0",
        serviceFeeAmount: "0",
        capabilities: [
            {
                revisionId: "cap_p",
                label: "实物商品",
                capabilityCode: "physical",
            },
            {
                revisionId: "cap_v",
                label: "虚拟商品",
                capabilityCode: "virtual",
            },
        ],
        ...overrides,
    }
}

describe("findOffering", () => {
    it("returns the offering matching the revision id", () => {
        const target = makeOption()
        expect(
            findOffering(
                [makeOption({ offeringRevisionId: "off_2" }), target],
                "off_1",
            ),
        ).toBe(target)
    })

    it("returns undefined when no offering matches", () => {
        expect(findOffering([makeOption()], "off_missing")).toBeUndefined()
    })
})

describe("singleCapabilityForMode", () => {
    it("returns the only capability matching the fulfillment mode", () => {
        const option = makeOption()
        expect(singleCapabilityForMode(option, "ELECTRONIC")).toEqual({
            revisionId: "cap_v",
            label: "虚拟商品",
            capabilityCode: "virtual",
        })
    })

    it("returns undefined when several capabilities match (requires manual choice)", () => {
        const option = makeOption({
            capabilities: [
                {
                    revisionId: "cap_1",
                    label: "实物商品",
                    capabilityCode: "physical",
                },
                {
                    revisionId: "cap_2",
                    label: "实物商品",
                    capabilityCode: "physical",
                },
            ],
        })
        expect(singleCapabilityForMode(option, "WAREHOUSE")).toBeUndefined()
    })

    it("returns undefined when no capability matches or the offering is missing", () => {
        expect(singleCapabilityForMode(makeOption(), "SERVICE")).toBeUndefined()
        expect(singleCapabilityForMode(undefined, "WAREHOUSE")).toBeUndefined()
    })
})
