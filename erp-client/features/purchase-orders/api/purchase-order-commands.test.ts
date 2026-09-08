import { expect, it, vi } from "vitest"
import { apiPost } from "@/lib/api"
import { submitPurchaseChange } from "./purchase-order-commands"
vi.mock("@/lib/api", () => ({ apiPost: vi.fn() }))
it("提交采购变更显式携带目标行集合以沿用基准版本", async () => {
    vi.mocked(apiPost).mockResolvedValue({
        id: "change1",
        purchase_order_id: "po1",
        status: "IN_APPROVAL",
        version: 2,
        created_at: 1,
    })
    const result = await submitPurchaseChange({
        purchaseChangeOrderId: "change1",
        purchaseOrderId: "po1",
        expectedLockVersion: 1,
        idempotencyKey: "key1",
    })
    expect(apiPost).toHaveBeenCalledWith(
        "/admin/purchase-change-orders/change1/submit",
        { expected_lock_version: 1, lines: [], idempotency_key: "key1" },
    )
    expect(result.status).toBe("succeeded")
})
