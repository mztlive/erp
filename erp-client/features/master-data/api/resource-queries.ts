/**
 * W14 基础资料 · 真实 HTTP 适配层。
 *
 * 保持 queries.ts 对外契约（函数签名 / 返回类型）稳定；后端 Page{items,total,page,page_size}
 * 与域 DTO 在本文件内映射为 MasterData* 视图类型。
 *
 * 后端域：catalog / warehouse / supplier / party（路径均在 /admin/...）
 */

import { computeMetrics } from "@/features/master-data/lib/data"
import { filterBySellableSupplyPreset } from "@/features/master-data/lib/sellable-supply-preset"
import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataListQuery,
    MasterDataListResult,
    MasterDataResource,
} from "@/features/master-data/types"
import {
    centerBrand,
    centerCategory,
    centerProduct,
    centerSellable,
    centerSupplier,
    centerUnitOfMeasure,
    centerVoucher,
    centerWarehouse,
} from "@/features/master-data/api/centers"
import {
    listBrands,
    listCategories,
    listProducts,
    listSellableItems,
    listSuppliers,
    listUnitOfMeasures,
    listVoucherCategories,
    listWarehouses,
} from "@/features/master-data/api/lists"
import { isoNow } from "@/features/master-data/api/presentation"

function wrapListResult(
    resource: MasterDataResource,
    rows: MasterDataListItem[],
    extra: Pick<MasterDataListResult, "emptyReason"> = {},
): MasterDataListResult {
    const now = isoNow()
    return {
        resource,
        rows,
        totalCount: rows.length,
        permissionVersion: "pv-w14-http-1",
        effectiveAsOf: now,
        eligibilityAsOf: now,
        queriedAt: now,
        metrics: [...computeMetrics(rows)],
        ...extra,
    }
}

// Public API (stable signatures for queries.ts)
// ---------------------------------------------------------------------------

export async function fetchMasterDataList(
    query: MasterDataListQuery,
): Promise<MasterDataListResult> {
    let rows: MasterDataListItem[]
    let extra: Pick<MasterDataListResult, "emptyReason"> = {}
    switch (query.resource) {
        case "categories":
            rows = await listCategories(query)
            break
        case "brands":
            rows = await listBrands(query)
            break
        case "unit-of-measures":
            rows = await listUnitOfMeasures(query)
            break
        case "products": {
            const page = await listProducts(query)
            rows = page.rows
            extra = {
                emptyReason: page.emptyReason,
            }
            break
        }
        case "sellable-items":
            rows = await listSellableItems(query)
            break
        case "voucher-categories":
            rows = await listVoucherCategories(query)
            break
        case "warehouses":
            rows = await listWarehouses(query)
            break
        case "suppliers": {
            const listed = await listSuppliers(query)
            rows = [...listed.rows]
            extra = {
                emptyReason: listed.emptyReason,
            }
            break
        }
        default:
            rows = []
    }

    // Client-side residual filters the server cannot express (revisionTiming / metricKey)
    if (query.revisionTiming && query.revisionTiming !== "all") {
        rows = rows.filter((r) =>
            query.revisionTiming === "future"
                ? r.revisionTiming === "FUTURE"
                : r.revisionTiming === "CURRENT",
        )
    }
    if (query.metricKey && query.metricKey !== "all") {
        const key = query.metricKey
        rows = rows.filter((r) => {
            if (key === "enabled") return r.lifecycleStatus === "ENABLED"
            if (key === "disabled") return r.lifecycleStatus === "DISABLED"
            if (key === "pending") return r.revisionTiming === "FUTURE"
            if (key === "expiring") return r.metricTags.includes("expiring")
            return true
        })
    }
    if (query.resource === "sellable-items" && query.sellableSupplyPreset) {
        rows = filterBySellableSupplyPreset(rows, query.sellableSupplyPreset)
    }

    return wrapListResult(query.resource, rows, extra)
}

export async function fetchMasterDataCenter(
    resource: MasterDataResource,
    stableId: string,
): Promise<MasterDataCenterView | null> {
    switch (resource) {
        case "categories":
            return centerCategory(stableId)
        case "brands":
            return centerBrand(stableId)
        case "unit-of-measures":
            return centerUnitOfMeasure(stableId)
        case "products":
            return centerProduct(stableId)
        case "sellable-items":
            return centerSellable(stableId)
        case "voucher-categories":
            return centerVoucher(stableId)
        case "warehouses":
            return centerWarehouse(stableId)
        case "suppliers":
            return centerSupplier(stableId)
        default:
            return null
    }
}
