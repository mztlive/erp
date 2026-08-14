/** W14 基础资料 · 列表查询与结果视图类型。 */

import type { StatusTone } from "@/components/ui/status-badge"
import type {
    ActionBlocker,
    LifecycleStatus,
    MasterDataResource,
    ProductKind,
    ProductListingFilter,
    ProductListingStatus,
    ProductSkuCoverageFilter,
    SelectorEligibility,
    SupplierQualificationHealth,
} from "@/features/master-data/api/types-core"

export type MasterDataListItem = Readonly<{
    objectType: MasterDataResource
    stableId: string
    stableNo: string
    name: string
    lifecycleStatus: LifecycleStatus
    lifecycleStatusLabel: string
    lifecycleTone: StatusTone
    /** 商品列表专用：从当前启用 SKU 继承的上架状态。 */
    listingStatus?: ProductListingStatus
    /** 商品列表专用：当前已上架 SKU 数。 */
    listedSkuCount?: number
    /** 商品列表专用：当前启用 SKU 总数。 */
    skuCount?: number
    /** Only for FUTURE revisions that will change lifecycle. */
    scheduledLifecycleStatus?: LifecycleStatus
    scheduledLifecycleLabel?: string
    revisionTiming: "CURRENT" | "FUTURE"
    revisionTimingLabel: string
    currentRevisionId: string
    displayedRevisionId: string
    revisionNo: number
    effectiveFrom: string
    effectiveTo?: string
    /** Resource-specific key facts for list secondary columns. */
    keyFacts: ReadonlyArray<{
        label: string
        value: string
        sensitive?: boolean
    }>
    /** Primary blocker summary for list (not mixed into lifecycle). */
    primaryBlocker?: string
    selectorEligibility: readonly SelectorEligibility[]
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
    lockVersion: number
    ownerName?: string
    metricTags: readonly string[]
    /**
     * 字典稳定代码（分类代码 / 品牌代码等）。
     * 列表与 Combobox 共用，避免只从 keyFacts 反解析。
     */
    dictionaryCode?: string
    /**
     * 商品分类树：上级分类稳定 ID；根节点为空。
     * 仅 `categories` 资源使用。
     */
    parentStableId?: string
    /** 分类适用商品类型（实物 / 虚拟 / 服务 / 卡券）。 */
    productKind?: string
    /** 公司商品池列表与预览专用的只读销售资格投影。 */
    sellableItem?: Readonly<{
        productId: string
        productNo: string
        specificationAttributes: ReadonlyArray<{
            name: string
            value: string
        }>
        specificationLabel: string
        barcode?: string
        baseUnit: string
        productKindLabel: string
        salesVisiblePriceGross: string
        marketPrice?: string
        supplierCount: number
        supplyRegions: readonly string[]
        eligibilityAsOf: string
    }>
}>

export type MasterDataListQuery = Readonly<{
    resource: MasterDataResource
    q?: string
    lifecycleStatus?: "enabled" | "disabled" | "all"
    revisionTiming?: "current" | "future" | "all"
    metricKey?: string
    productKind?: ProductKind
    /** 公司商品池：资格判定业务日期（YYYY-MM-DD）；空表示服务端今天。日常 UI 不暴露。 */
    eligibilityAsOf?: string
    productCategoryId?: string
    productBrandId?: string
    productSupplierId?: string
    /** 公司商品池：可供区域精确匹配。 */
    supplyRegion?: string
    productListingStatus?: ProductListingFilter
    productSupplyCoverage?: ProductSkuCoverageFilter
    productSalesPriceMin?: string
    productSalesPriceMax?: string
    supplierCapabilityCodes?: readonly string[]
    supplierQualificationTypes?: readonly string[]
    supplierQualificationHealth?: SupplierQualificationHealth
}>

export type MasterDataListResult = Readonly<{
    resource: MasterDataResource
    rows: readonly MasterDataListItem[]
    totalCount: number
    permissionVersion: string
    effectiveAsOf: string
    eligibilityAsOf: string
    queriedAt: string
    metrics: readonly {
        key: string
        label: string
        value: number
        detail?: string
    }[]
}>

/** 商品列表的当前启用 SKU 摘要；销售价来自 SKU 当前修订。 */
export type ProductListSkuSummary = Readonly<{
    productId: string
    skuId: string
    skuNo: string
    skuName: string
    specification: string
    baseUnit: string
    salesVisiblePriceGross?: string
}>

/** 商品 / 公司商品池筛选使用的分类树节点（供 `CategoryCombobox`）。 */
export type ProductFilterCategoryOption = Readonly<{
    categoryId: string
    categoryCode: string
    categoryName: string
    parentId?: string
}>

/** 商品列表筛选使用的归属与有效供给供应商选项。 */
export type ProductFilterOptions = Readonly<{
    categories: readonly ProductFilterCategoryOption[]
    brands: readonly Readonly<{
        value: string
        label: string
        keywords: string
    }>[]
    suppliers: readonly Readonly<{
        value: string
        label: string
        keywords: string
    }>[]
}>
