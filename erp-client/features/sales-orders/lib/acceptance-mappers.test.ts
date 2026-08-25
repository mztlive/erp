import { describe, expect, it } from "vitest"

import {
    hasRemainingEligibleAcceptance,
    type BackendEligibilityView,
} from "./acceptance-mappers"

function eligibility(
    facts: Array<{ eligible_quantity: string }>,
): BackendEligibilityView {
    return {
        sales_order_id: "so-1",
        sales_lines: [
            {
                sales_order_line_id: "line-1",
                line_no: 1,
                item_snapshot: "样品",
                required_quantity: "10",
                net_accepted_quantity: "0",
                fulfillment_facts: facts.map((fact, index) => ({
                    fulfillment_line_id: `fact-${index}`,
                    fulfillment_fact_type: "DELIVERY",
                    fulfillment_no: `FH-${index}`,
                    sales_order_line_id: "line-1",
                    line_no: 1,
                    item_snapshot: "样品",
                    occurred_at: 0,
                    net_successful_quantity: fact.eligible_quantity,
                    net_accepted_allocated_quantity: "0",
                    eligible_quantity: fact.eligible_quantity,
                })),
            },
        ],
        history: [],
    }
}

describe("hasRemainingEligibleAcceptance", () => {
    it("is false when there are no fulfillment facts yet", () => {
        expect(hasRemainingEligibleAcceptance(eligibility([]))).toBe(false)
        expect(
            hasRemainingEligibleAcceptance({
                sales_order_id: "so-1",
                sales_lines: [],
                history: [],
            }),
        ).toBe(false)
    })

    it("is true when a fact still has remaining eligible quantity", () => {
        expect(
            hasRemainingEligibleAcceptance(
                eligibility([{ eligible_quantity: "2" }]),
            ),
        ).toBe(true)
    })

    it("is false when remaining eligible quantity is already zero", () => {
        expect(
            hasRemainingEligibleAcceptance(
                eligibility([{ eligible_quantity: "0" }]),
            ),
        ).toBe(false)
    })
})
