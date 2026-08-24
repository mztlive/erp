import { describe, expect, it } from "vitest"

import {
    canExecuteFulfillmentOperation,
    canListFulfillmentOperation,
} from "./fulfillment-permissions"

describe("fulfillment operation permissions", () => {
    it("requires every detail, update and post permission for receipt execution", () => {
        const readOnly = ["purchase_receipt:list"]
        expect(canListFulfillmentOperation(readOnly, "RECEIPT")).toBe(true)
        expect(canExecuteFulfillmentOperation(readOnly, "RECEIPT")).toBe(false)

        expect(
            canExecuteFulfillmentOperation(
                [
                    "purchase_receipt:list",
                    "purchase_receipt:detail",
                    "purchase_receipt:update",
                    "purchase_receipt:post",
                ],
                "RECEIPT",
            ),
        ).toBe(true)
    })

    it("supports wildcard permissions for every operation type", () => {
        expect(canExecuteFulfillmentOperation(["*:*"], "WAREHOUSE_SHIP")).toBe(
            true,
        )
        expect(canExecuteFulfillmentOperation(["*:*"], "ELECTRONIC")).toBe(true)
        expect(canExecuteFulfillmentOperation(["*:*"], "SERVICE")).toBe(true)
    })
})
