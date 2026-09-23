import { afterEach, expect, test, vi } from "vitest"

import { fetchSupplierOrders } from "@/features/supplier-orders/api/list"

const apiGet = vi.fn()

vi.mock("@/lib/api", () => ({
    apiGet: (...args: unknown[]) => apiGet(...args),
}))

afterEach(() => {
    apiGet.mockReset()
})

test("把视图、取消退款和人员筛选交给服务端并保留三分空态元信息", async () => {
    apiGet.mockResolvedValue({
        items: [
            {
                id: "o-1",
                fulfillment_order_no: "FO-1",
                supplier_id: "s-1",
                connection_id: "c-1",
                split_no: 1,
                fulfillment_status: "EXCEPTION",
                cancel_status: "NONE",
                refund_status: "NONE",
                follow_up_user_id: "buyer-1",
                follow_up_user_name: "采购员（buyer）",
                handler_user_id: "handler-1",
                handler_user_name: "处理人（handler）",
                business_org_unit_id: "org-a",
                version: 1,
                created_at: 1_700_000_000,
            },
        ],
        total: 1,
        page: 1,
        page_size: 50,
        empty_reason: null,
        scope_version: "v1:abc",
        ownership_basis: "fulfillment_follow_up",
    })

    const result = await fetchSupplierOrders({
        view: "actionable",
        cancelStatuses: ["FAILED"],
        refundStatuses: ["MANUAL"],
        ownerUserIds: "buyer-1",
        handlerUserIds: "handler-1",
        orgUnitIds: "org-a",
        includeDescendants: true,
        page: 2,
        pageSize: 50,
        scopeVersion: "v1:abc",
    })

    expect(apiGet).toHaveBeenCalledWith(
        "/admin/supplier-fulfillment-orders",
        expect.objectContaining({
            view: "actionable",
            cancel_status: "FAILED",
            refund_status: "MANUAL",
            owner_user_ids: "buyer-1",
            handler_user_ids: "handler-1",
            org_unit_ids: "org-a",
            include_descendants: true,
            scope_version: "v1:abc",
            page: 2,
        }),
    )
    expect(result.rows).toHaveLength(1)
    expect(result.rows[0]?.followUpUserName).toBe("采购员（buyer）")
    expect(result.rows[0]?.handlerUserName).toBe("处理人（handler）")
    expect(result.emptyReason).toBeNull()
    expect(result.scopeVersion).toBe("v1:abc")
    expect(result.ownershipBasis).toBe("fulfillment_follow_up")
})

test("无范围空态不在客户端按视图截断分页", async () => {
    apiGet.mockResolvedValue({
        items: [],
        total: 0,
        page: 1,
        page_size: 50,
        empty_reason: "no_scope",
        scope_version: "v0",
    })
    const result = await fetchSupplierOrders({
        view: "actionable",
        page: 1,
        pageSize: 50,
    })
    expect(result.rows).toEqual([])
    expect(result.pageInfo.total).toBe(0)
    expect(result.emptyReason).toBe("no_scope")
})
