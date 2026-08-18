/**
 * W10 库存台账 · 台账列表 HTTP 入口。
 * 分页/排序/筛选由服务端完成；本文件将 Page{items,total,page,page_size}
 * 映射为前端 InventoryListView（含游标兼容）。
 */

import { apiGet } from "@/lib/api"
import type {
    InventoryListView,
    InventoryQuery,
    StockAdjustmentRow,
    StockBalanceRow,
    StockMovementRow,
    StockReservationRow,
} from "@/features/inventory/types"
import {
    EXCLUDED_NOTE,
    OPENING_STOCK_NOTE,
    backendMovementTypeFilter,
    filterSummary,
    isApiError,
} from "@/features/inventory/api/display"
import {
    mapAdjustment,
    mapAdjustmentApproval,
    mapBalance,
    mapMovement,
    mapReservation,
} from "@/features/inventory/api/mappers"
import {
    cursorsFromPage,
    dateToUnixEnd,
    dateToUnixStart,
    pageFromCursor,
    sortTokenToBackend,
} from "@/features/inventory/api/pagination"
import type {
    BackendPage,
    BackendStockAdjustment,
    BackendStockAdjustmentDetail,
    BackendStockBalance,
    BackendStockMovement,
    BackendStockReservation,
    BackendWarehouse,
} from "@/features/inventory/api/dto"

async function fetchWarehouses(): Promise<
    { id: string; code: string; name: string }[]
> {
    try {
        const page = await apiGet<BackendPage<BackendWarehouse>>(
            "/admin/warehouses",
            {
                page: 1,
                page_size: 100,
                sort_by: "warehouse_code",
                sort_dir: "asc",
            },
        )
        return page.items.map((w) => ({
            id: w.id,
            code: w.warehouse_code,
            name: w.warehouse_code, // WarehouseView has no display name; use code
        }))
    } catch {
        return []
    }
}

export async function fetchInventoryList(
    query: InventoryQuery,
): Promise<InventoryListView> {
    const pageSize = Math.min(100, Math.max(1, Math.trunc(query.pageSize)))
    const page = pageFromCursor(query.cursor, query.view, pageSize)
    const sort = query.sort.length > 0 ? query.sort : []
    const { sort_by, sort_dir } = sortTokenToBackend(sort, query.view)
    const warehouses = await fetchWarehouses()
    const hasWarehouseScope = warehouses.length > 0

    const emptyBase = (
        emptyReason: InventoryListView["emptyReason"],
        extras: Partial<InventoryListView> = {},
    ): InventoryListView => ({
        view: query.view,
        metrics: {
            balanceDimensionCount: 0,
            reservedDimensionCount: 0,
            zeroAvailableDimensionCount: 0,
            pendingAdjustmentCount: 0,
        },
        balances: [],
        movements: [],
        reservations: [],
        adjustments: [],
        total: 0,
        cursor: "",
        pageSize,
        sort: query.sort,
        filterSummary: extras.filterSummary ?? "",
        permissionVersion: "pv-real",
        dataWatermark: "",
        lastMovementWatermark: "",
        queriedAt: new Date().toISOString(),
        hasWarehouseScope,
        moduleAllowed: true,
        canCreateAdjustment: true,
        canExport: true,
        emptyReason,
        excludedKindsNote: EXCLUDED_NOTE,
        openingStockNote: OPENING_STOCK_NOTE,
        warehouses,
        ...extras,
    })

    if (!hasWarehouseScope) {
        return emptyBase("NO_DATA_SCOPE", {
            filterSummary: "未配置仓库数据范围",
            moduleAllowed: true,
            canCreateAdjustment: false,
            canExport: false,
        })
    }

    // Metrics: use server totals where available (no qty recompute)
    let balanceDimensionCount = 0
    let reservedDimensionCount = 0
    let zeroAvailableDimensionCount = 0
    let pendingAdjustmentCount = 0

    try {
        const [balPage, pendingApproval] = await Promise.all([
            apiGet<BackendPage<BackendStockBalance>>("/admin/stock-balances", {
                page: 1,
                page_size: 1,
                warehouse_id: query.warehouseId,
                sku_id: query.skuId,
            }),
            apiGet<BackendPage<BackendStockAdjustment>>(
                "/admin/stock-adjustments",
                {
                    page: 1,
                    page_size: 1,
                    warehouse_id: query.warehouseId,
                    status: "IN_APPROVAL",
                },
            ),
        ])
        balanceDimensionCount = balPage.total
        // reserved/zero metrics require availability filters the backend lacks
        reservedDimensionCount = 0
        zeroAvailableDimensionCount = 0
        pendingAdjustmentCount = pendingApproval.total
    } catch (error) {
        if (isApiError(error) && error.status === 403) {
            return emptyBase("PERMISSION_REVOKED", {
                filterSummary: "权限已收回",
                moduleAllowed: false,
                canCreateAdjustment: false,
                canExport: false,
                hasWarehouseScope: false,
            })
        }
        throw error
    }

    let balances: StockBalanceRow[] = []
    let movements: StockMovementRow[] = []
    let reservations: StockReservationRow[] = []
    let adjustments: StockAdjustmentRow[] = []
    let total = 0
    let dataWatermark = ""

    if (query.view === "balance") {
        // availability filter not on backend — documented gap; still pass warehouse/sku
        const res = await apiGet<BackendPage<BackendStockBalance>>(
            "/admin/stock-balances",
            {
                page,
                page_size: pageSize,
                warehouse_id: query.warehouseId,
                sku_id: query.skuId,
                sort_by,
                sort_dir,
            },
        )
        balances = res.items.map(mapBalance)
        // client-side availability narrow only when backend can't — mark as gap adaptation
        if (query.availability && query.availability !== "all") {
            balances = balances.filter((b) => {
                if (query.availability === "zero")
                    return b.availableQuantity === "0"
                if (query.availability === "positive")
                    return b.availableQuantity !== "0"
                if (query.availability === "reserved")
                    return b.hasActiveReservation
                return true
            })
        }
        if (query.balanceId) {
            balances = balances.filter((b) => b.balanceId === query.balanceId)
        }
        total = res.total
        if (query.q?.trim()) {
            const q = query.q.trim().toUpperCase()
            balances = balances.filter((b) =>
                [
                    b.skuCode,
                    b.skuName,
                    b.specSummary,
                    b.warehouseName,
                    b.warehouseCode,
                ]
                    .join(" ")
                    .toUpperCase()
                    .includes(q),
            )
            // q filter not on backend — total becomes page-local (gap)
            total = balances.length
        }
    } else if (query.view === "movement") {
        const res = await apiGet<BackendPage<BackendStockMovement>>(
            "/admin/stock-movements",
            {
                page,
                page_size: pageSize,
                warehouse_id: query.warehouseId,
                sku_id: query.skuId,
                movement_type: backendMovementTypeFilter(query.movementType),
                occurred_from: dateToUnixStart(query.occurredFrom),
                occurred_to: dateToUnixEnd(query.occurredTo),
                sort_by: sort_by ?? "occurred_at",
                sort_dir: sort_dir ?? "desc",
            },
        )
        const whMap = new Map(warehouses.map((w) => [w.id, w.name]))
        movements = res.items.map((m) =>
            mapMovement(m, { warehouseName: whMap.get(m.warehouse_id) }),
        )
        total = res.total
        dataWatermark =
            movements
                .map((m) => m.recordedAt)
                .sort()
                .at(-1) ?? ""
    } else if (query.view === "reservation") {
        const res = await apiGet<BackendPage<BackendStockReservation>>(
            "/admin/stock-reservations",
            {
                page,
                page_size: pageSize,
                warehouse_id: query.warehouseId,
                sku_id: query.skuId,
                sales_order_line_id: query.salesOrderLineId,
                sort_by: sort_by ?? "created_at",
                sort_dir: sort_dir ?? "desc",
            },
        )
        reservations = res.items.map(mapReservation)
        total = res.total
    } else {
        // adjustment
        const res = await apiGet<BackendPage<BackendStockAdjustment>>(
            "/admin/stock-adjustments",
            {
                page,
                page_size: pageSize,
                warehouse_id: query.warehouseId,
                sort_by: sort_by ?? "created_at",
                sort_dir: sort_dir ?? "desc",
            },
        )
        // hydrate lines for quantity/sku when possible (N+1 limited to page)
        adjustments = await Promise.all(
            res.items.map(async (a) => {
                try {
                    const detail = await apiGet<BackendStockAdjustmentDetail>(
                        `/admin/stock-adjustments/${encodeURIComponent(a.id)}`,
                    )
                    const line = detail.lines[0]
                    return mapAdjustment(
                        detail.adjustment,
                        line,
                        mapAdjustmentApproval(detail.approval),
                    )
                } catch {
                    return mapAdjustment(a)
                }
            }),
        )
        if (query.adjustmentId) {
            adjustments = adjustments.filter(
                (a) => a.adjustmentId === query.adjustmentId,
            )
        }
        if (query.skuId) {
            adjustments = adjustments.filter((a) => a.skuId === query.skuId)
        }
        total = res.total
    }

    const { cursor, nextCursor, previousCursor } = cursorsFromPage(
        query.view,
        page,
        pageSize,
        total,
    )

    let emptyReason: InventoryListView["emptyReason"]
    if (total === 0) {
        const hasActiveFilters = Boolean(
            query.q?.trim() ||
            query.warehouseId ||
            query.skuId ||
            query.balanceId ||
            query.salesOrderLineId ||
            query.adjustmentId ||
            query.movementType?.length ||
            query.occurredFrom ||
            query.occurredTo ||
            (query.availability && query.availability !== "all"),
        )
        emptyReason =
            hasActiveFilters || query.view !== "balance"
                ? "FILTER_NO_RESULT"
                : "NO_DATA"
    }

    return {
        view: query.view,
        metrics: {
            balanceDimensionCount,
            reservedDimensionCount,
            zeroAvailableDimensionCount,
            pendingAdjustmentCount,
        },
        balances,
        movements,
        reservations,
        adjustments,
        total,
        cursor,
        nextCursor,
        previousCursor,
        pageSize,
        sort: query.sort,
        filterSummary: filterSummary(query, total, warehouses),
        permissionVersion: "pv-real",
        dataWatermark,
        lastMovementWatermark: dataWatermark,
        queriedAt: new Date().toISOString(),
        hasWarehouseScope: true,
        moduleAllowed: true,
        canCreateAdjustment: true,
        canExport: true,
        emptyReason,
        excludedKindsNote: EXCLUDED_NOTE,
        openingStockNote: OPENING_STOCK_NOTE,
        warehouses,
    }
}
