import { describe, expect, it } from "vitest"

import {
    mapAcceptanceHistory,
    type BackendAcceptanceHeader,
} from "@/features/sales-orders/lib/acceptance-mappers"

const header = (
    overrides: Partial<BackendAcceptanceHeader>,
): BackendAcceptanceHeader => ({
    id: "acceptance-1",
    acceptance_no: "CA20260831-000001",
    sales_order_id: "sales-order-1",
    accepted_at: 1_788_112_800,
    result: "PASSED",
    status: "POSTED",
    version: 1,
    created_at: 1_788_112_800,
    ...overrides,
})

describe("mapAcceptanceHistory", () => {
    it("按原记录上的反向引用建立双向冲正关系", () => {
        const history = mapAcceptanceHistory([
            header({
                id: "acceptance-original",
                status: "REVERSED",
                reversal_of_acceptance_id: "acceptance-reverse",
            }),
            header({
                id: "acceptance-reverse",
                acceptance_no: "REV-CA20260831-000001",
            }),
        ])

        expect(history).toEqual([
            expect.objectContaining({
                acceptanceId: "acceptance-original",
                reversedByAcceptanceId: "acceptance-reverse",
            }),
            expect.objectContaining({
                acceptanceId: "acceptance-reverse",
                reversalOfAcceptanceId: "acceptance-original",
            }),
        ])
        expect(history[0]?.reversalOfAcceptanceId).toBeUndefined()
    })
})
