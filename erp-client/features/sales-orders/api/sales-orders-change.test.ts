import { beforeEach, expect, test, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchSalesChangeOrderDetail } from "./sales-orders-change"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn(), apiPost: vi.fn() }))
beforeEach(() => vi.clearAllMocks())

test("精确请求历史变更 ID，并拒绝与当前销售单不一致的返回", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        id: "old-change",
        sales_order_id: "another-sales",
    })
    await expect(
        fetchSalesChangeOrderDetail(
            "old/change",
            "physical_service",
            "sales-1",
        ),
    ).rejects.toThrow("不属于当前销售单")
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/sales-change-orders/old%2Fchange",
    )
})
