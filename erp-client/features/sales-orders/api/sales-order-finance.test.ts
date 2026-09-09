import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Page } from "@/lib/api"
import type { SalesOrderDetailView } from "./sales-orders"
import {
    collectOrderPages,
    fetchOrderReceivables,
    fetchOrderReceipts,
    fetchOrderInvoices,
} from "./sales-order-finance"
import {
    loadReceivables,
    loadReceipts,
    loadSalesInvoices,
} from "@/features/customer-receivables/api/loaders"
import { projectReceipt } from "@/features/customer-receivables/api/mappers"
import { amountAllocatedToTargets } from "../lib/sales-order-receivable"

vi.mock("@/features/customer-receivables/api/loaders", () => ({
    loadReceivables: vi.fn(),
    loadReceipts: vi.fn(),
    loadSalesInvoices: vi.fn(),
}))
const page = <T>(items: T[], total = items.length, current = 1): Page<T> => ({
    items,
    total,
    page: current,
    page_size: 20,
})
const order = {
    id: "order-1",
    nature: "card_voucher",
    customerName: "客户甲",
    settlementEntity: "主体甲",
} as SalesOrderDetailView
beforeEach(() => vi.resetAllMocks())

describe("本单票款独立读取", () => {
    it("回款和发票各自携带本单范围与页码，不依赖应收列表加载", async () => {
        vi.mocked(loadReceipts).mockResolvedValue(page([], 21, 2))
        vi.mocked(loadSalesInvoices).mockResolvedValue(page([], 43, 3))
        const [receipts, invoices] = await Promise.all([
            fetchOrderReceipts(order, 2),
            fetchOrderInvoices(order, 3),
        ])
        expect(loadReceipts).toHaveBeenCalledWith({
            view: "receipt",
            salesOrderId: "order-1",
            page: 2,
            pageSize: 20,
        })
        expect(loadSalesInvoices).toHaveBeenCalledWith({
            view: "sales_invoice",
            salesOrderId: "order-1",
            page: 3,
            pageSize: 20,
        })
        expect(receipts.total).toBe(21)
        expect(invoices.total).toBe(43)
        expect(loadReceivables).not.toHaveBeenCalled()
    })
    it("读取全部页后才汇总，后续页失败不返回部分余额", async () => {
        const load = vi
            .fn()
            .mockResolvedValueOnce(page(["a"], 2))
            .mockResolvedValueOnce(page(["b"], 2, 2))
        expect(await collectOrderPages(load)).toEqual(["a", "b"])
        expect(load.mock.calls).toEqual([[1], [2]])
        const broken = vi
            .fn()
            .mockResolvedValueOnce(page(["a"], 2))
            .mockRejectedValueOnce(new Error("network"))
        await expect(collectOrderPages(broken)).rejects.toThrow("network")
    })
    it("分页空洞或重复页不能当作完整数据", async () => {
        await expect(
            collectOrderPages(
                vi
                    .fn()
                    .mockResolvedValueOnce(page([1], 2))
                    .mockResolvedValueOnce(page([], 2, 2)),
            ),
        ).rejects.toThrow("记录读取不完整")
        await expect(
            collectOrderPages(vi.fn().mockResolvedValue(page([1], 2))),
        ).rejects.toThrow("记录读取不完整")
        expect(await collectOrderPages(async () => page([]))).toEqual([])
    })
    it("按订单真实性质显示应收，未提供到期状态时保持未知", async () => {
        vi.mocked(loadReceivables).mockResolvedValue(
            page([
                {
                    id: "account-1",
                    sales_order_id: "order-1",
                    sales_order_no: "XS-1",
                    account_seq: 1,
                    customer_id: "c",
                    customer_name: "客户甲",
                    counterparty_party_id: "p",
                    counterparty_party_name: "主体甲",
                    gross_total: "100.00",
                    settled_total: "0.00",
                    open_total: "100.00",
                    invoiceable_total: "100.00",
                    invoiced_total: "0.00",
                    open_invoiceable_total: "100.00",
                    status: "open",
                    version: 1,
                    created_at: 0,
                    entries: [],
                },
            ]),
        )
        const accounts = await fetchOrderReceivables(order)
        expect(accounts[0]).toMatchObject({
            businessType: "card",
            dueState: "unknown",
            openTotal: "100.00",
        })
    })
    it("拟核销独立保留，实际核销只合计本单已过账净额", () => {
        const receipt = projectReceipt({
            id: "r",
            receipt_no: "HK-1",
            status: "in_approval",
            counterparty_party_id: "p",
            received_at: 0,
            amount: "500.00",
            version: 1,
            created_at: 0,
            allocated_total: "80.00",
            unallocated_amount: "420.00",
            pending_allocations: [
                { receivable_entry_id: "entry-1", allocated_amount: "120.00" },
            ],
            allocations: [
                {
                    id: "a",
                    allocation_seq: 1,
                    allocation_action: "apply",
                    receivable_entry_id: "entry-1",
                    allocated_amount: "100.00",
                    allocated_at: 0,
                },
                {
                    id: "b",
                    allocation_seq: 2,
                    allocation_action: "reverse",
                    receivable_entry_id: "entry-1",
                    allocated_amount: "20.00",
                    allocated_at: 0,
                },
                {
                    id: "c",
                    allocation_seq: 3,
                    allocation_action: "apply",
                    receivable_entry_id: "other-order",
                    allocated_amount: "200.00",
                    allocated_at: 0,
                },
            ],
        })
        expect(receipt.pendingAllocations).toEqual([
            { targetId: "entry-1", amountGross: "120.00" },
        ])
        expect(
            amountAllocatedToTargets(receipt.allocations, new Set(["entry-1"])),
        ).toBe("80.00")
        expect(receipt.allocatedTotal).toBe("80.00")
    })
})
