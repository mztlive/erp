import { beforeEach, describe, expect, it, test, vi } from "vitest"
import { apiGet } from "@/lib/api"
import {
    fetchActiveSalesChangeOrder,
    fetchSalesChangeOrderDetail,
} from "./sales-orders-change"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn(), apiPost: vi.fn() }))
beforeEach(() => vi.clearAllMocks())

test("精确请求历史变更 ID，并拒绝与当前销售单不一致的返回", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        id: "old-change",
        sales_order_id: "another-sales",
    })
    await expect(
        fetchSalesChangeOrderDetail(
            "old/change",
            "physical_service",
            "sales-1",
        ),
    ).rejects.toThrow("不属于当前销售单")
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/sales-change-orders/old%2Fchange",
    )
})

describe("销售变更范围版本", () => {
    const listRow = {
        id: "change-1",
        sales_order_id: "so-1",
        base_revision_id: "rev-1",
        change_type: "AMOUNT",
        reason: "列表行不得当作详情",
        status: "DRAFT",
        version: 1,
        created_at: 1,
    }

    it("把 scope_version 送进查询并用 empty_reason 区分无范围", async () => {
        vi.mocked(apiGet).mockResolvedValueOnce({
            items: [listRow],
            total: 1,
            page: 1,
            page_size: 10,
            empty_reason: "no_scope",
            scope_version: "v2",
        })
        const result = await fetchActiveSalesChangeOrder(
            "so-1",
            "physical_service",
            "v2",
        )
        expect(apiGet).toHaveBeenCalledWith("/admin/sales-change-orders", {
            sales_order_id: "so-1",
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

    it("列表 403 不得伪装成无改单", async () => {
        vi.mocked(apiGet).mockRejectedValueOnce(
            Object.assign(new Error("禁止访问"), {
                kind: "Http",
                status: 403,
                message: "禁止访问",
            }),
        )
        await expect(
            fetchActiveSalesChangeOrder("so-1", "physical_service"),
        ).rejects.toMatchObject({
            status: 403,
        })
    })

    it("DATA_SCOPE_CHANGED 必须抛出以触发刷新", async () => {
        vi.mocked(apiGet).mockRejectedValueOnce(
            Object.assign(
                new Error("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新"),
                {
                    status: 409,
                    code: "DATA_SCOPE_CHANGED",
                },
            ),
        )
        await expect(
            fetchActiveSalesChangeOrder("so-1", "physical_service", "v1"),
        ).rejects.toMatchObject({
            status: 409,
            code: "DATA_SCOPE_CHANGED",
        })
    })

    it("详情失败不得用列表行顶替，也不得把 403 伪装成无改单", async () => {
        vi.mocked(apiGet)
            .mockResolvedValueOnce({
                items: [listRow],
                total: 1,
                page: 1,
                page_size: 10,
                empty_reason: null,
                scope_version: "v3",
            })
            .mockRejectedValueOnce(
                Object.assign(new Error("禁止访问"), {
                    kind: "Http",
                    status: 403,
                    message: "禁止访问",
                }),
            )
        await expect(
            fetchActiveSalesChangeOrder("so-1", "physical_service"),
        ).rejects.toMatchObject({
            status: 403,
        })
        expect(apiGet).toHaveBeenNthCalledWith(
            2,
            "/admin/sales-change-orders/change-1",
        )
    })

    it("详情 404 按空集处理并回传范围版本", async () => {
        vi.mocked(apiGet)
            .mockResolvedValueOnce({
                items: [listRow],
                total: 1,
                page: 1,
                page_size: 10,
                empty_reason: null,
                scope_version: "v3",
            })
            .mockRejectedValueOnce(
                Object.assign(new Error("销售变更单不存在或无权查看"), {
                    kind: "NotFound",
                    status: 404,
                    message: "销售变更单不存在或无权查看",
                }),
            )
        await expect(
            fetchActiveSalesChangeOrder("so-1", "physical_service"),
        ).resolves.toEqual({
            order: null,
            emptyReason: null,
            scopeVersion: "v3",
        })
    })

    it("详情成功时回传范围版本且不把列表行当详情", async () => {
        vi.mocked(apiGet)
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
        const result = await fetchActiveSalesChangeOrder(
            "so-1",
            "physical_service",
            "v4",
        )
        expect(apiGet).toHaveBeenNthCalledWith(
            1,
            "/admin/sales-change-orders",
            expect.objectContaining({
                sales_order_id: "so-1",
                scope_version: "v4",
            }),
        )
        expect(result.scopeVersion).toBe("v4")
        expect(result.emptyReason).toBeNull()
        expect(result.order?.id).toBe("change-1")
        expect(result.order?.statusCode).toBe("DRAFT")
    })
})
