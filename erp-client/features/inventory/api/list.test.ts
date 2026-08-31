import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiGet } from "@/lib/api"
import type { InventoryQuery } from "@/features/inventory/types"

import { fetchInventoryList } from "./list"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

const apiGetMock = vi.mocked(apiGet)

const query: InventoryQuery = {
    view: "balance",
    warehouseId: "warehouse-1",
    pageSize: 20,
    sort: [],
}

const balance = {
    id: "balance-1",
    warehouse_id: "warehouse-1",
    warehouse_code: "WH-1",
    warehouse_name: "一号仓",
    sku_id: "sku-1",
    sku_code: "SKU-1",
    sku_name: "商品一",
    on_hand_quantity: "10",
    reserved_quantity: "2",
    available_quantity: "8",
    version: "7",
    has_active_reservation: true,
    allowed_actions: ["CREATE_ADJUSTMENT"],
}

const forbidden = {
    kind: "Http",
    status: 403,
    code: "PERMISSION_DENIED",
    message: "无权访问该资源",
}

function page<T>(items: T[], total = items.length) {
    return { items, total, page: 1, page_size: 20 }
}

function isMetricRequest(params: unknown): boolean {
    return (
        typeof params === "object" &&
        params !== null &&
        "page_size" in params &&
        params.page_size === 1
    )
}

describe("inventory list optional enhancements", () => {
    beforeEach(() => {
        apiGetMock.mockReset()
    })

    it("keeps an authorized balance view when warehouse options return 403", async () => {
        apiGetMock.mockImplementation(async (path, params) => {
            if (path === "/admin/warehouses") throw forbidden
            if (path === "/admin/stock-adjustments") return page([], 3)
            if (path === "/admin/stock-balances") {
                return isMetricRequest(params) ? page([], 1) : page([balance])
            }
            throw new Error(`unexpected path: ${path}`)
        })

        const result = await fetchInventoryList(query)

        expect(result.balances).toHaveLength(1)
        expect(result.balances[0]?.allowedActions).toEqual([
            "CREATE_ADJUSTMENT",
        ])
        expect(result.total).toBe(1)
        expect(result.warehouses).toEqual([])
        expect(result.filterSummary).toContain("已选仓库")
        expect(result.filterSummary).not.toContain("warehouse-1")
        expect(result.hasWarehouseScope).toBe(true)
        expect(result.moduleAllowed).toBe(true)
        expect(result.emptyReason).toBeUndefined()
    })

    it("does not infer missing data scope from an empty warehouse option list", async () => {
        apiGetMock.mockImplementation(async (path, params) => {
            if (path === "/admin/warehouses") return page([])
            if (path === "/admin/stock-adjustments") return page([])
            if (path === "/admin/stock-balances") {
                return isMetricRequest(params) ? page([], 1) : page([balance])
            }
            throw new Error(`unexpected path: ${path}`)
        })

        const result = await fetchInventoryList(query)

        expect(result.balances).toHaveLength(1)
        expect(result.warehouses).toEqual([])
        expect(result.hasWarehouseScope).toBe(true)
        expect(result.emptyReason).toBeUndefined()
    })

    it("keeps an authorized balance view when the adjustment metric returns 403", async () => {
        apiGetMock.mockImplementation(async (path, params) => {
            if (path === "/admin/warehouses") {
                return page([
                    {
                        id: "warehouse-1",
                        warehouse_code: "WH-1",
                    },
                ])
            }
            if (path === "/admin/stock-adjustments") throw forbidden
            if (path === "/admin/stock-balances") {
                return isMetricRequest(params)
                    ? page([], 1)
                    : page([{ ...balance, allowed_actions: [] }])
            }
            throw new Error(`unexpected path: ${path}`)
        })

        const result = await fetchInventoryList(query)

        expect(result.balances).toHaveLength(1)
        expect(result.balances[0]?.allowedActions).toEqual([])
        expect(result.metrics.balanceDimensionCount).toBe(1)
        expect(result.metrics.pendingAdjustmentCount).toBe(0)
        expect(result.moduleAllowed).toBe(true)
        expect(result.emptyReason).toBeUndefined()
    })

    it("propagates a target 500 instead of projecting permission revoked", async () => {
        const targetFailure = {
            kind: "Http",
            status: 500,
            code: "INTERNAL_ERROR",
            message: "库存读取失败",
        }
        apiGetMock.mockImplementation(async (path, params) => {
            if (path === "/admin/warehouses") return page([])
            if (path === "/admin/stock-adjustments") return page([])
            if (path === "/admin/stock-balances") {
                if (isMetricRequest(params)) return page([], 1)
                throw targetFailure
            }
            throw new Error(`unexpected path: ${path}`)
        })

        await expect(fetchInventoryList(query)).rejects.toEqual(targetFailure)
    })

    it.each([
        ["balance", "/admin/stock-balances"],
        ["movement", "/admin/stock-movements"],
        ["reservation", "/admin/stock-reservations"],
        ["adjustment", "/admin/stock-adjustments"],
    ] as const)(
        "projects permission revoked only when the %s target request returns 403",
        async (view, targetPath) => {
            apiGetMock.mockImplementation(async (path, params) => {
                if (path === "/admin/warehouses") return page([])
                if (
                    path === "/admin/stock-balances" &&
                    isMetricRequest(params)
                ) {
                    return page([], 1)
                }
                if (
                    path === "/admin/stock-adjustments" &&
                    typeof params === "object" &&
                    params !== null &&
                    "status" in params &&
                    params.status === "IN_APPROVAL"
                ) {
                    return page([])
                }
                if (path === targetPath) throw forbidden
                throw new Error(`unexpected path: ${path}`)
            })

            const result = await fetchInventoryList({ ...query, view })

            expect(result.hasWarehouseScope).toBe(false)
            expect(result.moduleAllowed).toBe(false)
            expect(result.canExport).toBe(false)
            expect(result.emptyReason).toBe("PERMISSION_REVOKED")
        },
    )
})
