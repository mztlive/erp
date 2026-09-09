import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchSalesOrders } from "@/features/sales-orders/api/sales-orders-list"
import { searchContracts } from "@/features/entity-selectors/api/contracts"
import {
    searchWarehouses,
    fetchWarehouseOption,
} from "@/features/entity-selectors/api/warehouses"
import {
    loadReceipts,
    loadSalesInvoices,
} from "@/features/customer-receivables/api/loaders"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => {
    vi.resetAllMocks()
    vi.mocked(apiGet).mockResolvedValue({
        items: [],
        total: 0,
        page: 1,
        page_size: 20,
    })
})

it("销售关键词与客户、合同、责任视图同时提交，保留服务端总数", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        items: [],
        total: 101,
        page: 2,
        page_size: 20,
    })
    const result = await fetchSalesOrders({
        page: 2,
        pageSize: 20,
        search: " 客户.[x] ",
        customerId: "c1",
        contractId: "ct1",
        summary: "mine",
        currentUserId: "u1",
    })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/sales-orders",
        expect.objectContaining({
            q: "客户.[x]",
            customer_id: "c1",
            contract_id: "ct1",
            my_todo: true,
            created_by: "u1",
            page: 2,
        }),
    )
    expect(result.total).toBe(101)
    expect(vi.mocked(apiGet).mock.calls[0][1]).not.toHaveProperty("order_no")
})

it("合同选择器使用完整关键词路径并保留归属限制", async () => {
    await searchContracts({
        query: " 客户 ",
        purpose: "form",
        scope: "assigned",
        customerId: "c1",
        selectableOnly: true,
    })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/contracts",
        expect.objectContaining({
            q: "客户",
            scope: "assigned",
            customer_id: "c1",
            status: "EFFECTIVE",
        }),
    )
    expect(vi.mocked(apiGet).mock.calls[0][1]).not.toHaveProperty("contract_no")
})

it("仓库候选在分页前限制有效收货负责人，回显按稳定 ID 定位", async () => {
    await searchWarehouses({ query: " 上海 ", purpose: "purchase-receipt" })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/warehouses",
        expect.objectContaining({
            q: "上海",
            status: "active",
            require_inbound_handler: true,
        }),
    )
    await fetchWarehouseOption("warehouse-101", "purchase-receipt")
    expect(apiGet).toHaveBeenLastCalledWith(
        "/admin/warehouses",
        expect.objectContaining({
            warehouse_id: "warehouse-101",
            page_size: 1,
            require_inbound_handler: true,
        }),
    )
})

it("客户回款和发票关键词保留来源单据高级筛选", async () => {
    const query = {
        view: "receipt" as const,
        page: 2,
        pageSize: 20,
        q: " SO.01 ",
        salesOrderId: "s1",
        receivableAccountId: "r1",
        counterpartyPartyId: "p1",
    }
    await loadReceipts(query)
    await loadSalesInvoices(query)
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/customer-receipts",
        expect.objectContaining({
            q: "SO.01",
            sales_order_id: "s1",
            receivable_account_id: "r1",
            counterparty_party_id: "p1",
            page: 2,
        }),
    )
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/invoices",
        expect.objectContaining({
            q: "SO.01",
            sales_order_id: "s1",
            receivable_account_id: "r1",
            party_id: "p1",
            invoice_direction: "sales",
            page: 2,
        }),
    )
})
