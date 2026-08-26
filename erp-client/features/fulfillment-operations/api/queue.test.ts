import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiGet } from "@/lib/api"

import { fetchFulfillmentQueue } from "./queue"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

const apiGetMock = vi.mocked(apiGet)

describe("fetchFulfillmentQueue W01 exact object query", () => {
    beforeEach(() => {
        apiGetMock.mockReset()
    })

    it("loads one receipt detail by frozen id without scanning lists", async () => {
        apiGetMock.mockResolvedValue({
            receipt: {
                id: "receipt/1",
                receipt_no: "PR-001",
                purchase_order_id: "po-1",
                warehouse_id: "warehouse-1",
                status: "DRAFT",
                version: 3,
                created_at: 1_700_000_000,
            },
            lines: [
                {
                    id: "receipt-line-1",
                    line_no: 1,
                    purchase_order_revision_line_id: "po-line-1",
                    received_quantity: "2",
                    qualified_quantity: "2",
                    rejected_quantity: "0",
                    quality_result: "QUALIFIED",
                },
            ],
        })

        const view = await fetchFulfillmentQueue({
            role: "warehouse",
            operationTypes: ["RECEIPT"],
            operationId: "receipt/1",
            currentOperationId: "receipt/1",
        })

        expect(apiGetMock).toHaveBeenCalledTimes(1)
        expect(apiGetMock).toHaveBeenCalledWith(
            "/admin/purchase-receipts/receipt%2F1",
        )
        expect(view.operations).toHaveLength(1)
        expect(view.current?.operationId).toBe("receipt/1")
        expect(view.current?.lines).toHaveLength(1)
        expect(view.emptyReason).toBeUndefined()
    })

    it("fails closed when a delivery does not match the frozen operation type", async () => {
        apiGetMock.mockResolvedValue({
            delivery: {
                id: "delivery-1",
                delivery_no: "DV-001",
                delivery_type: "WAREHOUSE_SHIP",
                sales_order_id: "so-1",
                warehouse_id: "warehouse-1",
                status: "DRAFT",
                version: 1,
                created_at: 1_700_000_000,
            },
            lines: [],
        })

        const view = await fetchFulfillmentQueue({
            role: "procurement",
            operationTypes: ["SUPPLIER_DIRECT"],
            operationId: "delivery-1",
        })

        expect(apiGetMock).toHaveBeenCalledTimes(1)
        expect(view.operations).toHaveLength(0)
        expect(view.emptyReason).toBe("FILTER_NO_RESULT")
    })
})
