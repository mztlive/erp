import {
    masterDataCopy,
    lifecycleFilterLabel,
    revisionTimingFilterLabel,
} from "@/features/master-data/lib/copy"
import { resourceLabel } from "@/features/master-data/lib/data"
import {
    PRODUCT_COVERAGE_FILTER_OPTIONS,
    PRODUCT_LISTING_FILTER_OPTIONS,
    qualificationHealthLabel,
    selectedSupplierOptionLabels,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import {
    PRODUCT_KIND_LABELS,
    type MasterDataListItem,
    type MasterDataResource,
    type ProductKind,
    type ProductListingFilter,
    type ProductSkuCoverageFilter,
    type SupplierQualificationHealth,
} from "@/features/master-data/types"

type ListMetric = {
    key: string
    label: string
    value: number
    detail?: string
}

type FilterSnapshotInput = {
    resource: MasterDataResource
    q: string
    isSellableResource: boolean
    isSupplierResource: boolean
    lifecycleStatus: "enabled" | "disabled" | "all"
    revisionTiming: "current" | "future" | "all"
    productKind?: ProductKind
    productListingStatus?: ProductListingFilter
    productSupplyCoverage?: ProductSkuCoverageFilter
    productSalesPriceMin?: string
    productSalesPriceMax?: string
    supplyRegion?: string
    supplierCapabilityCodes: readonly string[]
    supplierQualificationTypes: readonly string[]
    supplierQualificationHealth?: SupplierQualificationHealth
    selectedCategoryLabel?: string
    selectedBrandLabel?: string
    selectedSupplierLabel?: string
}

/** 导出 CSV 头部筛选快照（人读摘要）。 */
export function buildFilterSnapshotLabel(input: FilterSnapshotInput): string {
    const {
        resource,
        q,
        isSellableResource,
        isSupplierResource,
        lifecycleStatus,
        revisionTiming,
        productKind,
        productListingStatus,
        productSupplyCoverage,
        productSalesPriceMin,
        productSalesPriceMax,
        supplyRegion,
        supplierCapabilityCodes,
        supplierQualificationTypes,
        supplierQualificationHealth,
        selectedCategoryLabel,
        selectedBrandLabel,
        selectedSupplierLabel,
    } = input

    if (isSellableResource) {
        const parts = [
            `分类=${resourceLabel(resource)}`,
            ...(productKind
                ? [`商品类型=${PRODUCT_KIND_LABELS[productKind]}`]
                : []),
            ...(selectedCategoryLabel
                ? [`商品分类=${selectedCategoryLabel}`]
                : []),
            ...(selectedBrandLabel ? [`品牌=${selectedBrandLabel}`] : []),
            ...(selectedSupplierLabel
                ? [`供应商=${selectedSupplierLabel}`]
                : []),
            ...(supplyRegion ? [`可供区域=${supplyRegion}`] : []),
            ...(productSalesPriceMin || productSalesPriceMax
                ? [
                      `销售价=${productSalesPriceMin ? `¥${productSalesPriceMin}` : "不限"}–${productSalesPriceMax ? `¥${productSalesPriceMax}` : "不限"}`,
                  ]
                : []),
            q.trim() ? `搜索=${q.trim()}` : "搜索=空",
        ]
        return parts.join(" · ")
    }

    const parts = [
        `分类=${resourceLabel(resource)}`,
        `启用状态=${lifecycleFilterLabel(lifecycleStatus)}`,
        ...(isSupplierResource
            ? [
                  `资质状态=${qualificationHealthLabel(supplierQualificationHealth)}`,
              ]
            : [`版本状态=${revisionTimingFilterLabel(revisionTiming)}`]),
        ...(supplierCapabilityCodes.length > 0
            ? [
                  `供应能力=${selectedSupplierOptionLabels(
                      supplierCapabilityCodes,
                      SUPPLIER_CAPABILITY_OPTIONS,
                  ).join("、")}`,
              ]
            : []),
        ...(supplierQualificationTypes.length > 0
            ? [
                  `资质类型=${selectedSupplierOptionLabels(
                      supplierQualificationTypes,
                      SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
                  ).join("、")}`,
              ]
            : []),
        ...(productKind
            ? [`商品类型=${PRODUCT_KIND_LABELS[productKind]}`]
            : []),
        ...(selectedCategoryLabel ? [`商品分类=${selectedCategoryLabel}`] : []),
        ...(selectedBrandLabel ? [`品牌=${selectedBrandLabel}`] : []),
        ...(selectedSupplierLabel ? [`供应商=${selectedSupplierLabel}`] : []),
        ...(productListingStatus
            ? [
                  `上架状态=${PRODUCT_LISTING_FILTER_OPTIONS.find((option) => option.value === productListingStatus)?.label}`,
              ]
            : []),
        ...(productSupplyCoverage
            ? [
                  `供给覆盖=${PRODUCT_COVERAGE_FILTER_OPTIONS.find((option) => option.value === productSupplyCoverage)?.label}`,
              ]
            : []),
        ...(productSalesPriceMin || productSalesPriceMax
            ? [
                  `销售价=${productSalesPriceMin ? `¥${productSalesPriceMin}` : "不限"}–${productSalesPriceMax ? `¥${productSalesPriceMax}` : "不限"}`,
              ]
            : []),
        q.trim() ? `搜索=${q.trim()}` : "搜索=空",
    ]
    return parts.join(" · ")
}

/** 公司商品池表头说明：有筛选时写人读摘要，否则默认操作说明。 */
export function buildSellableTableDescription(input: {
    q: string
    productKind?: ProductKind
    productSalesPriceMin?: string
    productSalesPriceMax?: string
    supplyRegion?: string
    selectedCategoryLabel?: string
    selectedBrandLabel?: string
    selectedSupplierLabel?: string
    rowCount: number
}): string {
    const active: string[] = []
    if (input.q.trim()) active.push(`搜索「${input.q.trim()}」`)
    if (input.productKind) {
        active.push(`类型 ${PRODUCT_KIND_LABELS[input.productKind]}`)
    }
    if (input.selectedCategoryLabel) {
        active.push(`分类 ${input.selectedCategoryLabel}`)
    }
    if (input.selectedBrandLabel) {
        active.push(`品牌 ${input.selectedBrandLabel}`)
    }
    if (input.selectedSupplierLabel) {
        active.push(`供应商 ${input.selectedSupplierLabel}`)
    }
    if (input.supplyRegion) active.push(`区域 ${input.supplyRegion}`)
    if (input.productSalesPriceMin || input.productSalesPriceMax) {
        active.push(
            `销售价 ${input.productSalesPriceMin ? `¥${input.productSalesPriceMin}` : "不限"}–${input.productSalesPriceMax ? `¥${input.productSalesPriceMax}` : "不限"}`,
        )
    }
    if (active.length === 0) {
        return masterDataCopy.sellableListDescription(input.rowCount)
    }
    return `共 ${input.rowCount} 条 · 当前筛选：${active.join(" · ")}`
}

/** 通用列表表头说明：有筛选时写人读摘要，否则默认操作说明（§2.2）。 */
export function buildListTableDescription(input: {
    q: string
    lifecycleStatus: "enabled" | "disabled" | "all"
    revisionTiming: "current" | "future" | "all"
    rowCount: number
}): string {
    const active: string[] = []
    if (input.q.trim()) active.push(`搜索「${input.q.trim()}」`)
    if (input.lifecycleStatus !== "all") {
        active.push(`启用状态 ${lifecycleFilterLabel(input.lifecycleStatus)}`)
    }
    if (input.revisionTiming !== "all") {
        active.push(
            `版本状态 ${revisionTimingFilterLabel(input.revisionTiming)}`,
        )
    }
    if (active.length === 0) {
        return masterDataCopy.listDescription(input.rowCount)
    }
    return `共 ${input.rowCount} 条 · 当前筛选：${active.join(" · ")}`
}

/**
 * 指标与当前搜索/启停/版本筛选同步，避免「全部 3」与表格行数矛盾。
 */
export function syncListMetrics(
    base: readonly ListMetric[],
    rows: readonly MasterDataListItem[],
): ListMetric[] {
    if (rows.length === 0) return [...base]
    const metricCount = (key: string): number => {
        switch (key) {
            case "enabled":
                return rows.filter((r) => r.lifecycleStatus === "ENABLED")
                    .length
            case "disabled":
                return rows.filter((r) => r.lifecycleStatus === "DISABLED")
                    .length
            case "pending":
                return rows.filter((r) => r.revisionTiming === "FUTURE").length
            case "expiring":
                return rows.filter((r) => r.metricTags.includes("expiring"))
                    .length
            default:
                return rows.length
        }
    }
    return base.map((metric) => ({
        ...metric,
        value: metricCount(metric.key),
    }))
}
