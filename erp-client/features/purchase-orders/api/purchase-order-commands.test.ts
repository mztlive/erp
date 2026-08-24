import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

import { apiGet, apiPost } from "@/lib/api"
import { createPurchaseOrderFromBasis } from "@/features/purchase-orders/api/purchase-order-commands"

const mockedApiGet = vi.mocked(apiGet)
const mockedApiPost = vi.mocked(apiPost)

const input = {
    basisId: "basis-1",
    purchaseType: "PHYSICAL" as const,
    paymentTermCode: "POSTPAY_NET30",
    lines: [{ salesOrderLineId: "sales-line-1", quantity: "3" }],
    idempotencyKey: "create-basis:uuid-1",
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("createPurchaseOrderFromBasis", () => {
    it("posts exactly once without fetching the basis first", async () => {
        mockedApiPost.mockResolvedValue({
            purchase_order_id: "po-1",
            purchase_no: "PO-2026-001",
            lock_version: 1,
            reference: "PO-2026-001",
        })

        const result = await createPurchaseOrderFromBasis(input)

        expect(mockedApiGet).not.toHaveBeenCalled()
        expect(mockedApiPost).toHaveBeenCalledTimes(1)
        expect(mockedApiPost).toHaveBeenCalledWith("/admin/purchase-orders", {
            basis_id: "basis-1",
            purchase_type: "PHYSICAL",
            payment_term_code: "POSTPAY_NET30",
            lines: [
                {
                    sales_order_line_id: "sales-line-1",
                    quantity: "3",
                },
            ],
            idempotency_key: "create-basis:uuid-1",
        })
        expect(result).toMatchObject({
            status: "succeeded",
            data: { purchaseOrderId: "po-1" },
        })
    })

    it("returns the stable quantity-conflict copy for HTTP 409", async () => {
        mockedApiPost.mockRejectedValue({
            kind: "Conflict",
            message: "stale remaining quantity",
            status: 409,
        })

        await expect(createPurchaseOrderFromBasis(input)).resolves.toEqual({
            status: "failed",
            message: "可采购数量已更新，请刷新后重试",
            code: "CONFLICT",
        })
    })
})
