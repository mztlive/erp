import { describe, expect, it } from "vitest"

import { confirmDescription } from "./validation"
import type { FulfillmentDraft } from "@/features/fulfillment-operations/types"

describe("confirmDescription", () => {
    it("keeps supplier direct to one sentence", () => {
        const draft: FulfillmentDraft = {
            type: "SUPPLIER_DIRECT",
            carrier: "顺丰速运",
            trackingNo: "SF1",
            shippedAt: "2026-08-28T09:31:00",
            lines: [
                {
                    salesOrderLineId: "sol_1",
                    purchaseLineSalesAllocationId: "alloc_1",
                    quantity: "1",
                },
            ],
        }
        expect(confirmDescription(draft)).toBe(
            "供应商直发给客户，不走自有仓库。确认后不能改。",
        )
    })

    it("mentions receipt quantities without a second remaining line", () => {
        const draft: FulfillmentDraft = {
            type: "RECEIPT",
            warehouseId: "wh_1",
            warehouseLabel: "中心仓",
            occurredAt: "2026-08-14T09:00:00.000Z",
            lines: [
                {
                    purchaseRevisionLineId: "prl_1",
                    receivedQuantity: "10",
                    qualifiedQuantity: "8",
                    rejectedQuantity: "2",
                    qualityResult: "PARTIAL",
                },
            ],
        }
        expect(confirmDescription(draft)).toBe(
            "合格 8 入库存并留货，不合格 2 不入库。确认后不能改。",
        )
    })
})
