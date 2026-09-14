import { beforeEach, describe, expect, it, vi } from "vitest"

const apiGet = vi.fn()
vi.mock("@/lib/api", () => ({
    apiGet: (...args: unknown[]) => apiGet(...args),
}))

import { fetchPurchaseReturnOrders } from "./purchase-return-orders"

describe("采购退货列表范围版本", () => {
    beforeEach(() => {
        apiGet.mockReset()
    })

    it("把 scope_version 送进查询并回传 empty_reason", async () => {
        apiGet.mockResolvedValueOnce({
            items: [],
            total: 0,
            page: 1,
            page_size: 100,
            empty_reason: "no_scope",
            scope_version: "v2",
            policy_version: 3,
            organization_version: 4,
            scope_summary: "采购退货单沿来源采购单当前负责人及单据业务组织范围",
        })
        const result = await fetchPurchaseReturnOrders("po-1", "v2")
        expect(apiGet).toHaveBeenCalledWith("/admin/purchase-return-orders", {
            purchase_order_id: "po-1",
            scope_version: "v2",
            page: 1,
            page_size: 100,
            sort_by: "created_at",
            sort_dir: "desc",
        })
        expect(result.emptyReason).toBe("no_scope")
        expect(result.scopeVersion).toBe("v2")
        expect(result.rows).toEqual([])
    })
})
