/**
 * 主数据列表的共享查询参数。
 *
 * 列表请求与导出转移任务必须发送同一组筛选条件；此文件是这两条路径的唯一映射，
 * 避免字段增删后一边生效、一边漏传。
 */

import type { MasterDataListQuery } from "../../types"

/** 启停筛选 → 后端 status 参数；「全部」不下发。 */
export function lifecycleStatusParam(
    query: MasterDataListQuery,
): "active" | "disabled" | undefined {
    if (query.lifecycleStatus === "enabled") return "active"
    if (query.lifecycleStatus === "disabled") return "disabled"
    return undefined
}

/** 商品列表查询参数。 */
export function productListParams(query: MasterDataListQuery) {
    return {
        status: lifecycleStatusParam(query),
        keyword: query.q || undefined,
        product_kind: query.productKind,
        category_id: query.productCategoryId,
        brand_id: query.productBrandId,
        supplier_id: query.productSupplierId,
        listing_status: query.productListingStatus,
        supply_coverage: query.productSupplyCoverage,
        source_diff_pending: query.productSourceDiffPending,
        sales_price_min: query.productSalesPriceMin,
        sales_price_max: query.productSalesPriceMax,
        owner_user_ids: query.ownerUserIds || undefined,
        procurement_owner_user_ids: query.procurementOwnerUserIds || undefined,
    }
}

/** 供应商列表查询参数。 */
export function supplierListParams(query: MasterDataListQuery) {
    return {
        status: lifecycleStatusParam(query),
        keyword: query.q || undefined,
        capability_codes: joinFilterCodes(query.supplierCapabilityCodes),
        qualification_types: joinFilterCodes(query.supplierQualificationTypes),
        qualification_health: query.supplierQualificationHealth,
        owner_user_ids: query.owner_user_ids || undefined,
        capability_owner_user_ids: query.capability_owner_user_ids || undefined,
    }
}

/** 规范化多选条件，供后端以逗号分隔的稳定查询参数接收。 */
export function joinFilterCodes(
    values: readonly string[] | undefined,
): string | undefined {
    if (!values?.length) return undefined
    return [...new Set(values.map((value) => value.trim()).filter(Boolean))]
        .sort()
        .join(",")
}
