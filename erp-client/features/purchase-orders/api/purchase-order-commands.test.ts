import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

import { apiGet, apiPost } from "@/lib/api"
import {
    createPurchaseOrderFromBasis,
    savePurchaseOrderDraft,
} from "@/features/purchase-orders/api/purchase-order-commands"

const mockedApiGet = vi.mocked(apiGet)
const mockedApiPost = vi.mocked(apiPost)

const input = {
    basisId: "basis-1",
    workItemId: "work-item-1",
    purchaseType: "PHYSICAL" as const,
    paymentTermCode: "POSTPAY_NET30",
    lines: [{ salesOrderLineId: "sales-line-1", quantity: "3" }],
    idempotencyKey: "create-basis:uuid-1",
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("savePurchaseOrderDraft", () => {
    it("preserves frozen source links and keeps allocation equal to edited quantity", async () => {
        mockedApiGet.mockResolvedValue({
            lines: [
                {
                    line_id: "line-1",
                    line_type: "ITEM_SERVICE",
                    procurement_confirmation_line_id: "confirmation-1",
                    sku_id: "sku-1",
                    sku_revision_id: "sku-revision-1",
                    product_name: "商品甲",
                    specification: "标准",
                    quantity: "4",
                    base_unit_code: "EA",
                    unit_cost_gross: "20",
                    input_tax_rate: "0.13",
                    expected_delivery_date: "2026-08-25",
                    sales_order_line_id: "sales-line-1",
                    sales_order_revision_line_id: "sales-revision-line-1",
                    allocated_quantity: "4",
                    gross_amount: "80",
                },
            ],
        })
        mockedApiPost.mockResolvedValue({
            lock_version: 2,
            totals: { gross: "120", net: "106.19", tax: "13.81" },
            reference: "SAVED-V2",
        })

        await savePurchaseOrderDraft({
            purchaseOrderId: "po-1",
            expectedLockVersion: 1,
            draftEditToken: "token-1",
            paymentTermCode: "POSTPAY_NET30",
            paymentTermLabel: "货到 30 天",
            lines: [
                {
                    lineId: "line-1",
                    lineType: "ITEM_SERVICE",
                    quantity: "6",
                    unitCostGross: "20",
                    inputTaxRate: "0.13",
                },
            ],
            idempotencyKey: "save-1",
        })

        expect(mockedApiPost).toHaveBeenCalledWith(
            "/admin/purchase-orders/po-1/draft",
            expect.objectContaining({
                lines: [
                    expect.objectContaining({
                        sales_order_line_id: "sales-line-1",
                        sales_order_revision_line_id: "sales-revision-line-1",
                        quantity: "6",
                        allocated_quantity: "6",
                    }),
                ],
            }),
        )
    })
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
            work_item_id: "work-item-1",
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
