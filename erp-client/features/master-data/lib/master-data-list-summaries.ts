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

export function buildDictionaryFilterSnapshotLabel(input: {
    categoryLabel: string
    q: string
    lifecycleStatus: "enabled" | "disabled" | "all"
    revisionTiming: "current" | "future" | "all"
}): string {
    return [
        `分类=${input.categoryLabel}`,
        `启用状态=${lifecycleFilterLabel(input.lifecycleStatus)}`,
        `版本状态=${revisionTimingFilterLabel(input.revisionTiming)}`,
        input.q.trim() ? `搜索=${input.q.trim()}` : "搜索=空",
    ].join(" · ")
}

export function buildSellableFilterSnapshotLabel(input: {
    q: string
    productKind?: ProductKind
    productSalesPriceMin?: string
    productSalesPriceMax?: string
    supplyRegion?: string
    selectedCategoryLabel?: string
    selectedBrandLabel?: string
    selectedSupplierLabel?: string
    supplyPresetLabel?: string
}): string {
    return [
        `分类=${resourceLabel("sellable-items")}`,
        ...(input.productKind
            ? [`商品类型=${PRODUCT_KIND_LABELS[input.productKind]}`]
            : []),
        ...(input.selectedCategoryLabel
            ? [`商品分类=${input.selectedCategoryLabel}`]
            : []),
        ...(input.selectedBrandLabel
            ? [`品牌=${input.selectedBrandLabel}`]
            : []),
        ...(input.selectedSupplierLabel
            ? [`供应商=${input.selectedSupplierLabel}`]
            : []),
        ...(input.supplyRegion ? [`可供区域=${input.supplyRegion}`] : []),
        ...(input.supplyPresetLabel
            ? [`快捷视图=${input.supplyPresetLabel}`]
            : []),
        ...(input.productSalesPriceMin || input.productSalesPriceMax
            ? [
                  `销售价=${input.productSalesPriceMin ? `¥${input.productSalesPriceMin}` : "不限"}–${input.productSalesPriceMax ? `¥${input.productSalesPriceMax}` : "不限"}`,
              ]
            : []),
        input.q.trim() ? `搜索=${input.q.trim()}` : "搜索=空",
    ].join(" · ")
}

export function buildProductFilterSnapshotLabel(input: {
    q: string
    lifecycleStatus: "enabled" | "disabled" | "all"
    revisionTiming: "current" | "future" | "all"
    productKind?: ProductKind
    productListingStatus?: ProductListingFilter
    productSupplyCoverage?: ProductSkuCoverageFilter
    productSalesPriceMin?: string
    productSalesPriceMax?: string
    selectedCategoryLabel?: string
    selectedBrandLabel?: string
    selectedSupplierLabel?: string
}): string {
    return [
        `分类=${resourceLabel("products")}`,
        `启用状态=${lifecycleFilterLabel(input.lifecycleStatus)}`,
        `版本状态=${revisionTimingFilterLabel(input.revisionTiming)}`,
        ...(input.productKind
            ? [`商品类型=${PRODUCT_KIND_LABELS[input.productKind]}`]
            : []),
        ...(input.selectedCategoryLabel
            ? [`商品分类=${input.selectedCategoryLabel}`]
            : []),
        ...(input.selectedBrandLabel
            ? [`品牌=${input.selectedBrandLabel}`]
            : []),
        ...(input.selectedSupplierLabel
            ? [`供应商=${input.selectedSupplierLabel}`]
            : []),
        ...(input.productListingStatus
            ? [
                  `上架状态=${PRODUCT_LISTING_FILTER_OPTIONS.find((option) => option.value === input.productListingStatus)?.label}`,
              ]
            : []),
        ...(input.productSupplyCoverage
            ? [
                  `供给覆盖=${PRODUCT_COVERAGE_FILTER_OPTIONS.find((option) => option.value === input.productSupplyCoverage)?.label}`,
              ]
            : []),
        ...(input.productSalesPriceMin || input.productSalesPriceMax
            ? [
                  `销售价=${input.productSalesPriceMin ? `¥${input.productSalesPriceMin}` : "不限"}–${input.productSalesPriceMax ? `¥${input.productSalesPriceMax}` : "不限"}`,
              ]
            : []),
        input.q.trim() ? `搜索=${input.q.trim()}` : "搜索=空",
    ].join(" · ")
}

export function buildSupplierFilterSnapshotLabel(input: {
    q: string
    lifecycleStatus: "enabled" | "disabled" | "all"
    supplierCapabilityCodes: readonly string[]
    supplierQualificationTypes: readonly string[]
    supplierQualificationHealth?: SupplierQualificationHealth
}): string {
    return [
        `分类=${resourceLabel("suppliers")}`,
        `启用状态=${lifecycleFilterLabel(input.lifecycleStatus)}`,
        `资质状态=${qualificationHealthLabel(input.supplierQualificationHealth)}`,
        ...(input.supplierCapabilityCodes.length > 0
            ? [
                  `供应能力=${selectedSupplierOptionLabels(
                      input.supplierCapabilityCodes,
                      SUPPLIER_CAPABILITY_OPTIONS,
                  ).join("、")}`,
              ]
            : []),
        ...(input.supplierQualificationTypes.length > 0
            ? [
                  `资质类型=${selectedSupplierOptionLabels(
                      input.supplierQualificationTypes,
                      SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
                  ).join("、")}`,
              ]
            : []),
        input.q.trim() ? `搜索=${input.q.trim()}` : "搜索=空",
    ].join(" · ")
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
    supplyPresetLabel?: string
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
    if (input.supplyPresetLabel) {
        active.push(`快捷视图 ${input.supplyPresetLabel}`)
    }
    if (input.productSalesPriceMin || input.productSalesPriceMax) {
        active.push(
            `销售价 ${input.productSalesPriceMin ? `¥${input.productSalesPriceMin}` : "不限"}–${input.productSalesPriceMax ? `¥${input.productSalesPriceMax}` : "不限"}`,
        )
    }
    if (active.length === 0) {
        return masterDataCopy.sellableListDescription(input.rowCount)
    }
    return `当前筛选：${active.join(" · ")} · 点击任一行查看价格、可供区域和供应保障。`
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
