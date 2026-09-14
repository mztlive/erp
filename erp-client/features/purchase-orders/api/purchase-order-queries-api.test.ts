import { beforeEach, describe, expect, it, vi } from "vitest"

const apiGet = vi.fn()
vi.mock("@/lib/api", () => ({
    apiGet: (...args: unknown[]) => apiGet(...args),
}))

import {
    fetchPurchaseOrderExportData,
    fetchPurchaseOrders,
} from "./purchase-order-queries-api"

describe("采购单列表范围版本", () => {
    beforeEach(() => {
        apiGet.mockReset()
    })

    it("把 scope_version 送进查询并回传 empty_reason", async () => {
        apiGet.mockResolvedValueOnce({
            items: [],
            total: 0,
            page: 2,
            page_size: 20,
            owner_options: [{ value: "buyer-1", label: "采购" }],
            empty_reason: "no_scope",
            scope_version: "v2",
            policy_version: 3,
            organization_version: 4,
            scope_summary: "采购单当前负责人及单据业务组织范围",
        })
        const result = await fetchPurchaseOrders({
            page: 2,
            pageSize: 20,
            scopeVersion: "v2",
            ownerUserIds: "buyer-1",
        })
        expect(apiGet).toHaveBeenCalledWith("/admin/purchase-orders", {
            scope_version: "v2",
            q: undefined,
            sales_order_id: undefined,
            owner_user_ids: "buyer-1",
            status: undefined,
            page: 2,
            page_size: 20,
            sort_by: undefined,
            sort_dir: undefined,
        })
        expect(result.emptyReason).toBe("no_scope")
        expect(result.scopeVersion).toBe("v2")
        expect(result.policyVersion).toBe(3)
        expect(result.organizationVersion).toBe(4)
    })

    it("导出跨页携带范围版本并在生成前重验", async () => {
        const page = {
            items: [
                {
                    id: "po-1",
                    purchase_no: "PO-1",
                    sales_order_id: "so-1",
                    sales_order_no: "SO-1",
                    supplier_id: "sup-1",
                    supplier_name: "供应商",
                    purchase_type: "PHYSICAL",
                    fulfillment_responsibility: "WAREHOUSE",
                    owner_user_id: "buyer-1",
                    owner_name: "采购",
                    status: "DRAFT",
                    review_status: "NONE",
                    gross_amount: "1.00",
                    net_amount: "1.00",
                    tax_amount: "0.00",
                    payment_progress: "NONE",
                    invoice_progress: "NONE",
                    fulfillment_progress: "NONE",
                    version: 1,
                    created_at: 1,
                },
            ],
            total: 1,
            page: 1,
            page_size: 100,
            owner_options: [],
            empty_reason: null,
            scope_version: "export-v1",
            policy_version: 1,
            organization_version: 1,
            scope_summary: "采购单当前负责人及单据业务组织范围",
        }
        apiGet.mockResolvedValueOnce(page).mockResolvedValueOnce({
            ...page,
            page_size: 1,
        })
        const rows = await fetchPurchaseOrderExportData({ page: 1, pageSize: 20 })
        expect(rows).toHaveLength(1)
        expect(apiGet).toHaveBeenNthCalledWith(
            1,
            "/admin/purchase-orders",
            expect.objectContaining({
                page: 1,
                page_size: 100,
                scope_version: undefined,
            }),
        )
        expect(apiGet).toHaveBeenNthCalledWith(
            2,
            "/admin/purchase-orders",
            expect.objectContaining({
                page: 1,
                page_size: 1,
                scope_version: "export-v1",
            }),
        )
    })
})
