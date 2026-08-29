import { describe, expect, it } from "vitest"

import {
    buildDraftLines,
    buildOrderProgress,
    collectValidationIssues,
    defaultBatchDraft,
    deriveOverall,
    passQuantity,
    type AcceptanceBatchDraft,
} from "@/features/sales-orders/lib/acceptance-model"
import type { AcceptanceEligibleFact } from "@/features/sales-orders/lib/acceptance-types"

function fact(
    overrides: Partial<AcceptanceEligibleFact> &
        Pick<AcceptanceEligibleFact, "fulfillmentLineId" | "salesOrderLineId">,
): AcceptanceEligibleFact {
    return {
        fulfillmentFactType: "SUPPLIER_DIRECT",
        fulfillmentNo: "FH-1",
        lineNo: 1,
        itemSnapshot: "礼盒 A",
        unitCode: "盒",
        occurredAt: "2026-08-01T00:00:00.000Z",
        netSuccessfulQuantity: "10",
        netAcceptedAllocatedQuantity: "0",
        eligibleQuantity: "10",
        ...overrides,
    }
}

function draft(
    source: AcceptanceEligibleFact,
    patch: Partial<AcceptanceBatchDraft> = {},
): AcceptanceBatchDraft {
    return { ...defaultBatchDraft(source), ...patch }
}

describe("buildOrderProgress", () => {
    it("sums delivered, accepted and pending from fulfillment facts", () => {
        const progress = buildOrderProgress([
            {
                salesOrderLineId: "sol-1",
                lineNo: 1,
                itemSnapshot: "礼盒 A",
                unitCode: "盒",
                requiredQuantity: "20",
                netAcceptedQuantity: "0",
                fulfillmentFacts: [
                    fact({
                        fulfillmentLineId: "f-1",
                        salesOrderLineId: "sol-1",
                        netSuccessfulQuantity: "20",
                        netAcceptedAllocatedQuantity: "8",
                        eligibleQuantity: "12",
                    }),
                ],
            },
        ])
        expect(progress.acceptedQuantity).toBe("8")
        expect(progress.deliveredQuantity).toBe("20")
        expect(progress.pendingQuantity).toBe("12")
        expect(progress.lines[0]?.stuckKind).toBe("accept")
        expect(progress.lines[0]?.stuckLabel).toContain("待验")
    })

    it("marks undelivered remainder as waiting for shipment", () => {
        const progress = buildOrderProgress([
            {
                salesOrderLineId: "sol-1",
                lineNo: 1,
                itemSnapshot: "礼盒 B",
                unitCode: "盒",
                requiredQuantity: "50",
                netAcceptedQuantity: "0",
                fulfillmentFacts: [],
            },
        ])
        expect(progress.lines[0]?.stuckKind).toBe("deliver")
        expect(progress.lines[0]?.stuckLabel).toBe("还差 50 未交付")
    })
})

describe("buildDraftLines", () => {
    it("allocates only pass quantity so backend conservation holds", () => {
        const source = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
        })
        const selected = new Map([
            [
                source.fulfillmentLineId,
                draft(source, {
                    result: "SHORT",
                    exceptionQty: "2",
                    reason: "少两件",
                }),
            ],
        ])
        expect(passQuantity(selected.get("f-1")!)).toBe(8)
        const lines = buildDraftLines(selected)
        expect(lines).toEqual([
            {
                salesOrderLineId: "sol-1",
                acceptedQuantity: "8",
                shortQuantity: "2",
                rejectedQuantity: "0",
                reason: "少两件",
                serviceFail: false,
                allocations: [
                    {
                        fulfillmentLineId: "f-1",
                        fulfillmentFactType: "SUPPLIER_DIRECT",
                        allocatedQuantity: "8",
                    },
                ],
            },
        ])
        expect(deriveOverall(selected.values())).toBe("SHORT")
    })
})

describe("collectValidationIssues", () => {
    it("rejects a full-batch short because pass quantity would be zero", () => {
        const source = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
        })
        const selected = new Map([
            [
                source.fulfillmentLineId,
                draft(source, {
                    result: "SHORT",
                    exceptionQty: "10",
                    reason: "整批短少",
                }),
            ],
        ])
        const issues = collectValidationIssues(selected)
        expect(issues.some((issue) => issue.id.startsWith("line-pass-"))).toBe(
            true,
        )
    })
})
