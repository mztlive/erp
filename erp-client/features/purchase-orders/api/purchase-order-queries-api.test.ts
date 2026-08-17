import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

import { apiGet } from "@/lib/api"
import {
    fetchCreationBases,
    fetchPurchaseOrders,
} from "./purchase-order-queries-api"
import type { BackendListItem } from "./purchase-order-wire-types"

const mockedApiGet = vi.mocked(apiGet)

function makeBackendListItem(
    overrides: Partial<BackendListItem> = {},
): BackendListItem {
    return {
        id: "po_1",
        purchase_no: "PO-1",
        sales_order_id: "so_1",
        supplier_id: "sup_1",
        supplier_name: "供应商A",
        purchase_type: "PHYSICAL",
        status: "DRAFT",
        review_status: "NONE",
        gross_amount: "10",
        net_amount: "9",
        tax_amount: "1",
        payment_progress: "NONE",
        invoice_progress: "NONE",
        fulfillment_progress: "NONE",
        version: 1,
        created_at: 1_700_000_000,
        ...overrides,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("fetchPurchaseOrders", () => {
    it("只请求一次采购单列表，不再附带统计或建单依据", async () => {
        mockedApiGet.mockResolvedValue({
            items: [makeBackendListItem()],
            total: 1,
            page: 1,
            page_size: 20,
        })

        const result = await fetchPurchaseOrders({
            q: "钢",
            status: "DRAFT",
            metric: "all",
            page: 1,
            pageSize: 20,
        })

        expect(mockedApiGet).toHaveBeenCalledTimes(1)
        expect(mockedApiGet).toHaveBeenCalledWith("/admin/purchase-orders", {
            q: "钢",
            status: "DRAFT",
            page: 1,
            page_size: 20,
            sort_by: undefined,
            sort_dir: undefined,
        })
        expect(result.rows).toHaveLength(1)
        expect(result.rows[0]?.purchaseOrderId).toBe("po_1")
        expect(result.rows[0]?.paymentTermLabel).toBe("—")
        expect(result.rows[0]?.ownerName).toBe("—")
        expect(result.total).toBe(1)
        expect(result.metrics).toEqual([])
    })

    it("把付款条件与负责人映射到列表行", async () => {
        mockedApiGet.mockResolvedValue({
            items: [
                makeBackendListItem({
                    payment_term_code: "POSTPAY_NET30",
                    owner_name: "张三",
                }),
            ],
            total: 1,
            page: 1,
            page_size: 20,
        })

        const result = await fetchPurchaseOrders({
            page: 1,
            pageSize: 20,
        })

        expect(result.rows[0]?.paymentTermCode).toBe("POSTPAY_NET30")
        expect(result.rows[0]?.paymentTermLabel).toBe("货到 30 天")
        expect(result.rows[0]?.ownerName).toBe("张三")
    })
})

describe("fetchCreationBases", () => {
    it("按独立接口拉取创建依据", async () => {
        mockedApiGet.mockResolvedValue([
            {
                basis_id: "bas_1",
                sales_order_id: "so_1",
                submission_id: "sub_1",
                supplier_id: "sup_1",
                supplier_name: "供应商A",
                payment_term_code: "POSTPAY_NET30",
                lines: [],
                estimated_gross: "100",
            },
        ])

        const result = await fetchCreationBases()
        expect(mockedApiGet).toHaveBeenCalledTimes(1)
        expect(mockedApiGet).toHaveBeenCalledWith(
            "/admin/purchase-creation-bases",
        )
        expect(result).toHaveLength(1)
        expect(result[0]?.basisId).toBe("bas_1")
    })
})
