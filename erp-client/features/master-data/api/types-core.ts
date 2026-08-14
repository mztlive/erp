/** W14 基础资料 · 核心枚举、资源身份与共享视图类型。 */

export const MASTER_DATA_RESOURCES = [
    { key: "sellable-items", label: "公司商品池" },
    { key: "products", label: "商品与 SKU" },
    { key: "categories", label: "商品分类" },
    { key: "brands", label: "品牌" },
    { key: "unit-of-measures", label: "计量单位" },
    { key: "voucher-categories", label: "卡券类目" },
    { key: "suppliers", label: "供应商与资质" },
    { key: "warehouses", label: "仓库" },
] as const

export type MasterDataResource = (typeof MASTER_DATA_RESOURCES)[number]["key"]

/**
 * 公司商品类型（`product.product_kind`）：决定商品业务作用的独立必填稳定属性，
 * 创建后不可变；分类的适用类型只做兼容性校验，不得派生或覆盖它。
 */
export const PRODUCT_KIND_VALUES = [
    "PHYSICAL",
    "VIRTUAL",
    "OFFLINE_SERVICE",
    "VOUCHER",
] as const

export type ProductKind = (typeof PRODUCT_KIND_VALUES)[number]

export const PRODUCT_KIND_LABELS: Readonly<Record<ProductKind, string>> = {
    PHYSICAL: "实物",
    VIRTUAL: "虚拟",
    OFFLINE_SERVICE: "服务",
    VOUCHER: "卡券",
}

export type LifecycleStatus = "ENABLED" | "DISABLED"
export type ProductListingStatus = "LISTED" | "PARTIALLY_LISTED" | "UNLISTED"
export type ProductListingFilter = "listed" | "partially_listed" | "unlisted"
export type ProductSkuCoverageFilter = "complete" | "partial" | "none"
export type SupplierQualificationHealth =
    | "valid"
    | "expiring_30"
    | "expired"
    | "not_registered"

export type MasterDataSectionId =
    | "overview"
    | "versions"
    | "relations"
    | "audit"

type FieldVisibility = "full" | "masked" | "hidden"

export type SelectorEligibility = Readonly<{
    context: string
    contextLabel: string
    eligible: boolean
    blockerCodes: readonly string[]
    reason?: string
}>

export type ActionBlocker = Readonly<{
    action: string
    code: string
    message: string
}>

export type SensitiveFieldView = Readonly<{
    label: string
    maskedValue: string
    revealToken?: string
    visibility: FieldVisibility
}>
