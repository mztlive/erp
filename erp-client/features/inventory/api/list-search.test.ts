import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchInventoryList } from "./list"
vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => {
    vi.resetAllMocks()
    vi.mocked(apiGet).mockImplementation(
        async (path) =>
            ({
                items: [],
                total: path === "/admin/stock-balances" ? 101 : 0,
                page: 1,
                page_size: 20,
            }) as never,
    )
})

it("余额关键词与数量条件进入接口，总数不能被当前页过滤覆盖", async () => {
    const result = await fetchInventoryList({
        view: "balance",
        pageSize: 20,
        sort: [],
        q: " 名称.[x] ",
        availability: "positive",
        balanceId: "balance-101",
        warehouseId: "warehouse-1",
    })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/stock-balances",
        expect.objectContaining({
            q: "名称.[x]",
            availability: "positive",
            balance_id: "balance-101",
            warehouse_id: "warehouse-1",
            page_size: 20,
        }),
    )
    expect(result.total).toBe(101)
    expect(result.nextCursor).toBeTruthy()
})

it.each(["movement", "reservation", "adjustment"] as const)(
    "%s 视图消费关键词",
    async (view) => {
        await fetchInventoryList({
            view,
            pageSize: 20,
            sort: [],
            q: "规格",
            skuId: "sku-101",
            adjustmentId: "adjustment-101",
        })
        const endpoint = {
            movement: "/admin/stock-movements",
            reservation: "/admin/stock-reservations",
            adjustment: "/admin/stock-adjustments",
        }[view]
        expect(apiGet).toHaveBeenCalledWith(
            endpoint,
            expect.objectContaining({ q: "规格", sku_id: "sku-101" }),
        )
        if (view === "adjustment")
            expect(apiGet).toHaveBeenCalledWith(
                endpoint,
                expect.objectContaining({ adjustment_id: "adjustment-101" }),
            )
    },
)
