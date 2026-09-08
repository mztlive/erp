import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { loadPendingCardReviewCount } from "./loaders"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => {
    vi.resetAllMocks()
    vi.mocked(apiGet).mockImplementation(
        async (_path, query) =>
            ({
                items: [],
                total: query?.review_status === "opening_pending" ? 27 : 13,
            }) as never,
    )
})

it("使用服务端总数汇总两种待复核状态，不受当前页大小影响", async () => {
    expect(
        await loadPendingCardReviewCount({
            view: "receivable",
            page: 3,
            pageSize: 20,
            customerId: "c1",
            q: "SO1",
        }),
    ).toBe(40)
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/receivable-accounts",
        expect.objectContaining({
            page: 1,
            page_size: 1,
            customer_id: "c1",
            q: "SO1",
            review_status: "opening_pending",
        }),
    )
})
it("期初待复核筛选只统计期初，已复核筛选返回零", async () => {
    expect(
        await loadPendingCardReviewCount({
            view: "receivable",
            page: 1,
            pageSize: 20,
            reviewStatus: "pending_opening",
        }),
    ).toBe(27)
    expect(apiGet).toHaveBeenCalledTimes(1)
    expect(
        await loadPendingCardReviewCount({
            view: "receivable",
            page: 1,
            pageSize: 20,
            reviewStatus: "reviewed",
        }),
    ).toBe(0)
    expect(apiGet).toHaveBeenCalledTimes(1)
})
it("发票编号不作为销售单关键字，但保留往来主体范围", async () => {
    await loadPendingCardReviewCount({
        view: "sales_invoice",
        page: 1,
        pageSize: 20,
        q: "INV1",
        counterpartyPartyId: "p1",
    })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/receivable-accounts",
        expect.objectContaining({ q: undefined, counterparty_party_id: "p1" }),
    )
})
