import { describe, expect, it } from "vitest"

import {
    fulfillmentListNumber,
    fulfillmentObjectTitle,
    fulfillmentTaskTitle,
} from "./fulfillment-title"

describe("fulfillment work item titles", () => {
    it("strips draft status and opaque delivery numbers", () => {
        expect(
            fulfillmentObjectTitle(
                "供应商直发 DLV-4024b6046fd64028984c1f25d52a81c4 · 草稿",
                "履约处理",
            ),
        ).toBe("供应商直发")
        expect(
            fulfillmentListNumber(
                "供应商直发 DLV-4024b6046fd64028984c1f25d52a81c4 · 草稿",
                "履约处理",
            ),
        ).toBe("供应商直发")
    })

    it("keeps a readable sales order number for list scanning", () => {
        expect(
            fulfillmentListNumber(
                "供应商直发 · 销售单 SO20260826-000001",
                "履约处理",
            ),
        ).toBe("SO20260826-000001")
        expect(
            fulfillmentObjectTitle(
                "供应商直发 · 销售单 SO20260826-000001",
                "履约处理",
            ),
        ).toBe("供应商直发 · 销售单 SO20260826-000001")
    })

    it("prefers the hydrated sales order number on the task header", () => {
        expect(
            fulfillmentTaskTitle(
                { objectTitle: "供应商直发", workItemTypeLabel: "履约处理" },
                {
                    operationType: "SUPPLIER_DIRECT",
                    source: {
                        salesOrderId: "so-1",
                        salesOrderNo: "SO20260826-000001",
                        salesRevisionId: "",
                        customerLabel: "演示客户",
                    },
                },
            ),
        ).toBe("供应商直发 · SO20260826-000001")
    })
})
