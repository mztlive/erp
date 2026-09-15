import { beforeEach, describe, expect, it, vi } from "vitest"

const apiGet = vi.fn()
vi.mock("@/lib/api", () => ({
    apiGet: (...args: unknown[]) => apiGet(...args),
}))

import {
    fetchActivePurchaseChangeOrder,
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
            as_of: "2026-09-14T08:00:00Z",
            ownership_basis: "current_procurement_owner",
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
        expect(result.asOf).toBe("2026-09-14T08:00:00Z")
        expect(result.ownershipBasis).toBe("current_procurement_owner")
        expect(result.freshness.updatedAt).toBe("2026-09-14T08:00:00Z")
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
        const rows = await fetchPurchaseOrderExportData({
            page: 1,
            pageSize: 20,
        })
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

describe("采购变更范围版本", () => {
    const listRow = {
        id: "change-1",
        purchase_order_id: "po-1",
        base_revision_id: "rev-1",
        reason: "列表行不得当作详情",
        status: "DRAFT",
        version: 1,
        created_at: 1,
    }

    beforeEach(() => {
        apiGet.mockReset()
    })

    it("把 scope_version 送进查询并用 empty_reason 区分无范围", async () => {
        apiGet.mockResolvedValueOnce({
            items: [listRow],
            total: 1,
            page: 1,
            page_size: 10,
            empty_reason: "no_scope",
            scope_version: "v2",
        })
        const result = await fetchActivePurchaseChangeOrder(
            "po-1",
            undefined,
            "v2",
        )
        expect(apiGet).toHaveBeenCalledWith("/admin/purchase-change-orders", {
            purchase_order_id: "po-1",
            scope_version: "v2",
            page: 1,
            page_size: 10,
        })
        expect(result).toEqual({
            order: null,
            emptyReason: "no_scope",
            scopeVersion: "v2",
        })
        expect(apiGet).toHaveBeenCalledTimes(1)
    })

    it("DATA_SCOPE_CHANGED 必须抛出以触发刷新", async () => {
        apiGet.mockRejectedValueOnce(
            Object.assign(
                new Error("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新"),
                {
                    status: 409,
                    code: "DATA_SCOPE_CHANGED",
                },
            ),
        )
        await expect(
            fetchActivePurchaseChangeOrder("po-1", undefined, "v1"),
        ).rejects.toMatchObject({
            status: 409,
            code: "DATA_SCOPE_CHANGED",
        })
    })

    it("详情失败不得用列表行顶替", async () => {
        apiGet
            .mockResolvedValueOnce({
                items: [listRow],
                total: 1,
                page: 1,
                page_size: 10,
                empty_reason: null,
                scope_version: "v3",
            })
            .mockRejectedValueOnce(
                Object.assign(new Error("采购变更单不存在或无权查看"), {
                    kind: "NotFound",
                    status: 500,
                    message: "采购变更单不存在或无权查看",
                }),
            )
        await expect(
            fetchActivePurchaseChangeOrder("po-1"),
        ).rejects.toMatchObject({
            status: 500,
        })
        expect(apiGet).toHaveBeenNthCalledWith(
            2,
            "/admin/purchase-change-orders/change-1",
        )
    })

    it("详情 404 按空集处理并回传范围版本", async () => {
        apiGet
            .mockResolvedValueOnce({
                items: [listRow],
                total: 1,
                page: 1,
                page_size: 10,
                empty_reason: null,
                scope_version: "v3",
            })
            .mockRejectedValueOnce(
                Object.assign(new Error("采购变更单不存在或无权查看"), {
                    kind: "NotFound",
                    status: 404,
                    message: "采购变更单不存在或无权查看",
                }),
            )
        await expect(fetchActivePurchaseChangeOrder("po-1")).resolves.toEqual({
            order: null,
            emptyReason: null,
            scopeVersion: "v3",
        })
    })

    it("详情成功时回传范围版本且不把列表行当详情", async () => {
        apiGet
            .mockResolvedValueOnce({
                items: [listRow],
                total: 1,
                page: 1,
                page_size: 10,
                empty_reason: null,
                scope_version: "v4",
            })
            .mockResolvedValueOnce({
                ...listRow,
                reason: "详情审批投影",
                approval: null,
            })
        const result = await fetchActivePurchaseChangeOrder(
            "po-1",
            undefined,
            "v4",
        )
        expect(apiGet).toHaveBeenNthCalledWith(
            1,
            "/admin/purchase-change-orders",
            expect.objectContaining({
                purchase_order_id: "po-1",
                scope_version: "v4",
            }),
        )
        expect(result.scopeVersion).toBe("v4")
        expect(result.emptyReason).toBeNull()
        expect(result.order?.reason).toBe("详情审批投影")
        expect(result.order?.id).toBe("change-1")
    })
})
