import { describe, expect, it } from "vitest"

import { clientValidation } from "@/features/fulfillment-operations/lib/validation"
import { makeOperation } from "@/features/fulfillment-operations/pages/hooks/test-data"
import type { FulfillmentDraft } from "@/features/fulfillment-operations/types"

import {
    EMPTY_FULFILLMENT_DRAFT,
    activeFulfillmentDraft,
    fulfillmentDraftFormSeed,
} from "./draft-form-seed"

const DIRECT_DRAFT: FulfillmentDraft = {
    type: "SUPPLIER_DIRECT",
    carrier: "",
    trackingNo: "",
    shippedAt: "2026-08-31T11:00",
    lines: [
        {
            salesOrderLineId: "sol_1",
            purchaseLineSalesAllocationId: "alloc_1",
            quantity: "1",
        },
    ],
}

describe("fulfillmentDraftFormSeed", () => {
    it("does not seed a receipt placeholder for supplier-direct jobs", () => {
        const operation = {
            ...makeOperation({
                operationId: "dlv_direct_1",
                operationType: "SUPPLIER_DIRECT",
            }),
            operationType: "SUPPLIER_DIRECT" as const,
            draft: DIRECT_DRAFT,
        }
        expect(fulfillmentDraftFormSeed(operation)).toEqual({
            formId: "fulfillment-draft-dlv_direct_1",
            draft: DIRECT_DRAFT,
        })
        expect(fulfillmentDraftFormSeed(undefined).draft).toEqual(
            EMPTY_FULFILLMENT_DRAFT,
        )
    })
})

describe("activeFulfillmentDraft", () => {
    it("replaces a leftover receipt store with the supplier-direct job draft", () => {
        const operation = {
            ...makeOperation({
                operationId: "dlv_direct_1",
                operationType: "SUPPLIER_DIRECT",
            }),
            operationType: "SUPPLIER_DIRECT" as const,
            draft: DIRECT_DRAFT,
        }
        expect(
            activeFulfillmentDraft(operation, EMPTY_FULFILLMENT_DRAFT)?.type,
        ).toBe("SUPPLIER_DIRECT")
        expect(activeFulfillmentDraft(operation, DIRECT_DRAFT)).toBe(
            DIRECT_DRAFT,
        )
        expect(
            activeFulfillmentDraft(undefined, EMPTY_FULFILLMENT_DRAFT),
        ).toBeNull()
    })

    it("asks for carrier and tracking instead of a type mismatch", () => {
        const operation = {
            ...makeOperation({
                operationId: "dlv_direct_1",
                operationType: "SUPPLIER_DIRECT",
            }),
            operationType: "SUPPLIER_DIRECT" as const,
            draft: DIRECT_DRAFT,
        }
        const aligned = activeFulfillmentDraft(
            operation,
            EMPTY_FULFILLMENT_DRAFT,
        )
        expect(aligned).not.toBeNull()
        const issues = clientValidation(operation, aligned!)
        expect(issues.map((issue) => issue.id)).toEqual([
            "d-carrier",
            "d-tracking",
        ])
    })
})
