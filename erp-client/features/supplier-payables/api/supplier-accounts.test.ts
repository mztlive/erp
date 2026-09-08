import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchSupplierAccounts } from "./supplier-accounts"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn(), apiPost: vi.fn() }))

beforeEach(() => {
    vi.resetAllMocks()
    vi.mocked(apiGet).mockImplementation(async (path) => {
        if (path === "/admin/suppliers/supplier-account-1") {
            return { id: "supplier-account-1", party_id: "party-2" } as never
        }
        return { items: [], total: 0 } as never
    })
})

it("供应商筛选对进项发票使用往来单位 ID，付款保留供应商账户 ID", async () => {
    await fetchSupplierAccounts({
        view: "purchase_invoice",
        supplierId: "supplier-account-1",
        q: " INV-1 ",
    })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/invoices",
        expect.objectContaining({ party_id: "party-2", invoice_no: "INV-1" }),
    )
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/supplier-payments",
        expect.objectContaining({
            supplier_id: "supplier-account-1",
            q: "INV-1",
        }),
    )
})

it("未选择供应商时不附加往来单位筛选", async () => {
    await fetchSupplierAccounts({ view: "purchase_invoice" })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/invoices",
        expect.objectContaining({ party_id: undefined }),
    )
})

it("供应商身份缺失时拒绝查询，避免扩大筛选范围", async () => {
    vi.mocked(apiGet).mockResolvedValue({ id: "supplier-account-1" } as never)
    await expect(
        fetchSupplierAccounts({
            view: "purchase_invoice",
            supplierId: "supplier-account-1",
        }),
    ).rejects.toThrow("供应商缺少往来单位标识")
    expect(apiGet).toHaveBeenCalledTimes(1)
})
