/**
 * W10 库存台账 · 台账列表 HTTP 入口。
 * 分页/排序/筛选由服务端完成；本文件将 Page{items,total,page,page_size}
 * 映射为前端 InventoryListView（含游标兼容）。
 */

import { apiGet } from "@/lib/api"
import { isApiError } from "@/lib/api/errors"
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

export function inventoryListRequestQuery(
    query: InventoryQuery,
    page: number,
    pageSize: number,
    sort_by: string | undefined,
    sort_dir: string | undefined,
): Record<string, unknown> {
    const people =
        query.view === "balance"
            ? {}
            : {
                  operator_user_ids: query.operatorUserIds || undefined,
                  ...(query.view === "adjustment"
                      ? {
                            applicant_user_ids:
                                query.applicantUserIds || undefined,
                            handler_user_ids: query.handlerUserIds || undefined,
                        }
                      : {}),
              }
    return {
        page,
        page_size: pageSize,
        q: query.q?.trim() || undefined,
        warehouse_id: query.warehouseId,
        sku_id: query.skuId,
        sort_by,
        sort_dir,
        scope_version: query.scopeVersion || undefined,
        ...people,
    }
}
import type {
    BackendPage,
    BackendStockAdjustment,
    BackendStockAdjustmentDetail,
    BackendStockBalance,
    BackendStockMovement,
    BackendStockReservation,
} from "@/features/inventory/api/dto"

async function fetchOptionalMetrics(query: InventoryQuery): Promise<{
    balanceDimensionCount: number
    pendingAdjustmentCount: number
}> {
    const [balanceResult, adjustmentResult] = await Promise.allSettled([
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
    return {
        balanceDimensionCount:
            balanceResult.status === "fulfilled"
                ? balanceResult.value.total
                : 0,
        pendingAdjustmentCount:
            adjustmentResult.status === "fulfilled"
                ? adjustmentResult.value.total
                : 0,
    }
}

async function fetchTargetView<T>(
    path: string,
    query: Record<string, unknown>,
): Promise<T | null> {
    try {
        return await apiGet<T>(path, query)
    } catch (error) {
        if (isApiError(error) && error.status === 403) return null
        throw error
    }
}

export async function fetchInventoryList(
    query: InventoryQuery,
): Promise<InventoryListView> {
    const pageSize = Math.min(100, Math.max(1, Math.trunc(query.pageSize)))
    const page = pageFromCursor(query.cursor, query.view, pageSize)
    const sort = query.sort.length > 0 ? query.sort : []
    const { sort_by, sort_dir } = sortTokenToBackend(sort, query.view)
    const optionalMetrics = fetchOptionalMetrics(query)
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
        hasWarehouseScope: false,
        moduleAllowed: true,
        canExport: true,
        emptyReason,
        excludedKindsNote: EXCLUDED_NOTE,
        openingStockNote: OPENING_STOCK_NOTE,
        ...extras,
    })
    const permissionRevoked = () =>
        emptyBase("PERMISSION_REVOKED", {
            filterSummary: "权限已收回",
            moduleAllowed: false,
            canExport: false,
        })

    let balances: StockBalanceRow[] = []
    let movements: StockMovementRow[] = []
    let reservations: StockReservationRow[] = []
    let adjustments: StockAdjustmentRow[] = []
    let total = 0
    let dataWatermark = ""
    let scopeEmptyReason: string | null | undefined
    let scopeVersion: string | undefined

    const takeScope = (res: BackendPage<unknown>) => {
        scopeEmptyReason = res.empty_reason
        scopeVersion = res.scope_version
    }

    if (query.view === "balance") {
        const res = await fetchTargetView<BackendPage<BackendStockBalance>>(
            "/admin/stock-balances",
            {
                ...inventoryListRequestQuery(
                    query,
                    page,
                    pageSize,
                    sort_by,
                    sort_dir,
                ),
                balance_id: query.balanceId,
                availability: query.availability,
            },
        )
        if (!res) return permissionRevoked()
        takeScope(res)
        balances = res.items.map(mapBalance)
        total = res.total
    } else if (query.view === "movement") {
        const res = await fetchTargetView<BackendPage<BackendStockMovement>>(
            "/admin/stock-movements",
            {
                ...inventoryListRequestQuery(
                    query,
                    page,
                    pageSize,
                    sort_by ?? "occurred_at",
                    sort_dir ?? "desc",
                ),
                movement_type: backendMovementTypeFilter(query.movementType),
                occurred_from: dateToUnixStart(query.occurredFrom),
                occurred_to: dateToUnixEnd(query.occurredTo),
            },
        )
        if (!res) return permissionRevoked()
        takeScope(res)
        movements = res.items.map((m) => mapMovement(m))
        total = res.total
        dataWatermark =
            movements
                .map((m) => m.recordedAt)
                .sort()
                .at(-1) ?? ""
    } else if (query.view === "reservation") {
        const res = await fetchTargetView<BackendPage<BackendStockReservation>>(
            "/admin/stock-reservations",
            {
                page,
                page_size: pageSize,
                q: query.q?.trim() || undefined,
                warehouse_id: query.warehouseId,
                sku_id: query.skuId,
                sales_order_line_id: query.salesOrderLineId,
                sort_by: sort_by ?? "created_at",
                sort_dir: sort_dir ?? "desc",
            },
        )
        if (!res) return permissionRevoked()
        takeScope(res)
        reservations = res.items.map(mapReservation)
        total = res.total
    } else {
        // adjustment
        const res = await fetchTargetView<BackendPage<BackendStockAdjustment>>(
            "/admin/stock-adjustments",
            {
                ...inventoryListRequestQuery(
                    query,
                    page,
                    pageSize,
                    sort_by ?? "created_at",
                    sort_dir ?? "desc",
                ),
                adjustment_id: query.adjustmentId,
            },
        )
        if (!res) return permissionRevoked()
        takeScope(res)
        // hydrate lines for quantity/sku when possible (N+1 limited to page)
        adjustments = await Promise.all(
            res.items.map(async (a) => {
                try {
                    const detail = await apiGet<BackendStockAdjustmentDetail>(
                        `/admin/stock-adjustments/${encodeURIComponent(a.id)}`,
                    )
                    const line =
                        detail.lines.find(
                            (line) => line.sku_id === query.skuId,
                        ) ?? detail.lines[0]
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
        total = res.total
    }

    const { balanceDimensionCount, pendingAdjustmentCount } =
        await optionalMetrics
    // reserved/zero metrics require availability filters the backend lacks
    const reservedDimensionCount = 0
    const zeroAvailableDimensionCount = 0

    const { cursor, nextCursor, previousCursor } = cursorsFromPage(
        query.view,
        page,
        pageSize,
        total,
    )

    let emptyReason: InventoryListView["emptyReason"]
    if (scopeEmptyReason === "no_scope") {
        emptyReason = "NO_DATA_SCOPE"
    } else if (total === 0) {
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
            query.operatorUserIds ||
            query.applicantUserIds ||
            query.handlerUserIds ||
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
        filterSummary: filterSummary(query, total),
        permissionVersion: "pv-real",
        dataWatermark,
        lastMovementWatermark: dataWatermark,
        queriedAt: new Date().toISOString(),
        hasWarehouseScope: scopeEmptyReason !== "no_scope",
        scopeVersion,
        moduleAllowed: true,
        canExport: true,
        emptyReason,
        excludedKindsNote: EXCLUDED_NOTE,
        openingStockNote: OPENING_STOCK_NOTE,
    }
}
