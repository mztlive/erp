import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchCustomerSourceDocuments } from "./source-documents"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => vi.resetAllMocks())
it("发票按应收主键读取原销售单，编码地址并去重", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        id: "account",
        sales_order_id: "sale/a",
        sales_order_no: "XS-1",
        entries: [],
    } as never)
    const result = await fetchCustomerSourceDocuments({
        accountIds: ["account/a", "account/a", "account/b"],
    })
    expect(apiGet).toHaveBeenCalledTimes(2)
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/receivable-accounts/account%2Fa",
    )
    expect(result).toEqual({
        documents: [
            { id: "sale/a", label: "XS-1", href: "/sales/orders/sale%2Fa" },
        ],
        unresolved: 0,
    })
})
it("回款分录在后续页时继续读取，找到目标即停止", async () => {
    vi.mocked(apiGet)
        .mockResolvedValueOnce({
            items: [{ id: "other", entries: [{ id: "entry-other" }] }],
            total: 3,
        } as never)
        .mockResolvedValueOnce({
            items: [
                {
                    id: "account",
                    sales_order_id: "sale-1",
                    sales_order_no: "XS-1",
                    entries: [{ id: "entry-1" }],
                },
            ],
            total: 3,
        } as never)
    const result = await fetchCustomerSourceDocuments({
        entryIds: ["entry-1"],
        counterpartyPartyId: "party-1",
    })
    expect(apiGet).toHaveBeenCalledTimes(2)
    expect(apiGet).toHaveBeenLastCalledWith(
        "/admin/receivable-accounts",
        expect.objectContaining({
            page: 2,
            counterparty_party_id: "party-1",
            page_size: 100,
        }),
    )
    expect(result.documents[0]?.href).toBe("/sales/orders/sale-1")
    expect(result.unresolved).toBe(0)
})
it("缺少主体范围时不发全库查询，也不将分录 ID 当作销售单 ID", async () => {
    expect(
        await fetchCustomerSourceDocuments({ entryIds: ["entry-1"] }),
    ).toEqual({ documents: [], unresolved: 1 })
    expect(apiGet).not.toHaveBeenCalled()
})
it("权限或读取错误交给局部查询显示重试", async () => {
    vi.mocked(apiGet).mockRejectedValue(new Error("无权限"))
    await expect(
        fetchCustomerSourceDocuments({ accountIds: ["account-1"] }),
    ).rejects.toThrow("无权限")
})
