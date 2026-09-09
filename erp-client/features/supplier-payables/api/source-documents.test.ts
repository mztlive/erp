import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchSupplierSourceDocuments } from "./source-documents"
vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => vi.resetAllMocks())
const source = {
    payableAccountId: "pa-1",
    sourceDocumentNo: "PO-1",
    sourceType: "PURCHASE_ORDER" as const,
}
const account = {
    id: "pa-1",
    supplier_id: "s-1",
    source_type: "supplier_settlement",
    source_document_id: "statement/a",
    source_document_no: "JS-1",
    status: "open",
    entries: [],
    version: 1,
}
it("已有原单地址不重复读取，多个核销行指向同一原单时去重", async () => {
    const result = await fetchSupplierSourceDocuments({
        allocations: [
            { ...source, sourceHref: "/procurement/orders/po-1" },
            { ...source, sourceHref: "/procurement/orders/po-1" },
        ],
    })
    expect(apiGet).not.toHaveBeenCalled()
    expect(result.documents).toEqual([
        {
            href: "/procurement/orders/po-1",
            label: "PO-1",
            action: "打开采购单",
        },
    ])
})
it("补读应付详情后按真实来源类型打开结算单", async () => {
    vi.mocked(apiGet).mockResolvedValue(account as never)
    const result = await fetchSupplierSourceDocuments({ allocations: [source] })
    expect(apiGet).toHaveBeenCalledWith("/admin/payable-accounts/pa-1")
    expect(result.documents[0]).toEqual({
        href: "/supplier-api/settlements/statement%2Fa",
        label: "JS-1",
        action: "打开结算单",
    })
})
it("一个原单失败时保留其他已知入口，不生成猜测链接", async () => {
    vi.mocked(apiGet).mockRejectedValue(new Error("无权限"))
    const result = await fetchSupplierSourceDocuments({
        allocations: [
            { ...source, sourceHref: "/procurement/orders/po-1" },
            { ...source, payableAccountId: "missing" },
        ],
    })
    expect(result.documents).toHaveLength(1)
    expect(result.unresolved).toBe(1)
})
it("只有原应付分录的退款在供应商范围内分页找来源", async () => {
    vi.mocked(apiGet)
        .mockResolvedValueOnce({
            items: [{ ...account, entries: [] }],
            total: 2,
        } as never)
        .mockResolvedValueOnce({
            items: [
                {
                    ...account,
                    entries: [{ id: "entry-1", direction: "increase" }],
                },
            ],
            total: 2,
        } as never)
    const result = await fetchSupplierSourceDocuments({
        entryId: "entry-1",
        supplierId: "s-1",
    })
    expect(apiGet).toHaveBeenLastCalledWith(
        "/admin/payable-accounts",
        expect.objectContaining({ supplier_id: "s-1", page: 2 }),
    )
    expect(result.documents[0]?.href).toBe(
        "/supplier-api/settlements/statement%2Fa",
    )
})
