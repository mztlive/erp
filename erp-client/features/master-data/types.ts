import type { StatusTone } from "@/components/ui/status-badge"

export const MASTER_DATA_RESOURCES = [
  { key: "sellable-items", label: "可销售项目" },
  { key: "products", label: "商品与 SKU" },
  { key: "categories", label: "商品分类" },
  { key: "brands", label: "品牌" },
  { key: "voucher-categories", label: "卡券类目" },
  { key: "suppliers", label: "供应商与资质" },
  { key: "warehouses", label: "仓库" },
] as const

/** W14 角色默认落地资源（对齐 docs/ui-workspaces/w14-basic-data.md §2）。 */
export const MASTER_DATA_ROLE_DEFAULT: Readonly<
  Record<string, MasterDataResource>
> = {
  procurement: "sellable-items",
  operations: "voucher-categories",
  warehouse: "warehouses",
  finance: "suppliers",
  sales: "sellable-items",
}

export type MasterDataResource =
  (typeof MASTER_DATA_RESOURCES)[number]["key"]

export type LifecycleStatus = "ENABLED" | "DISABLED"
export type RevisionTiming = "CURRENT" | "FUTURE" | "HISTORICAL"

export type MasterDataSectionId =
  | "overview"
  | "versions"
  | "relations"
  | "audit"

export type FieldVisibility = "full" | "masked" | "hidden"

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

export type MasterDataListItem = Readonly<{
  objectType: MasterDataResource
  stableId: string
  stableNo: string
  name: string
  lifecycleStatus: LifecycleStatus
  lifecycleStatusLabel: string
  lifecycleTone: StatusTone
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
}>

export type MasterDataListQuery = Readonly<{
  resource: MasterDataResource
  q?: string
  lifecycleStatus?: "enabled" | "disabled" | "all"
  revisionTiming?: "current" | "future" | "all"
  metricKey?: string
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
  /** Demo: module/resource/field/action permission snapshot. */
  permissionDemo: PermissionDemoSnapshot
}>

export type PermissionDemoSnapshot = Readonly<{
  hasModuleAccess: boolean
  resourceAccess: Record<MasterDataResource, boolean>
  canExport: boolean
  /** Role label for demos (采购 / 运营 / 仓储 / 销售 / 财务). */
  roleLabel: string
  /** Field-level: whether sensitive fields may be revealed. */
  canRevealSensitive: boolean
}>

export type RevisionTimelineEntry = Readonly<{
  id: string
  revisionNo: number
  revisionTiming: RevisionTiming
  timingLabel: string
  nameSnapshot: string
  actor: string
  effectiveFrom: string
  effectiveTo?: string
  changeReason: string
  isCurrent: boolean
  lifecycleAtRevision: LifecycleStatus
}>

export type MasterDataCenterView = Readonly<{
  resource: MasterDataResource
  stableId: string
  stableNo: string
  name: string
  lifecycleStatus: LifecycleStatus
  lifecycleStatusLabel: string
  lifecycleTone: StatusTone
  scheduledLifecycleStatus?: LifecycleStatus
  scheduledLifecycleLabel?: string
  revisionTiming: "CURRENT" | "FUTURE"
  revisionTimingLabel: string
  lockVersion: number
  currentRevision: {
    revisionId: string
    revisionNo: number
    name: string
    effectiveFrom: string
    effectiveTo?: string
    changeReason: string
    actor: string
    fields: ReadonlyArray<{ label: string; value: string }>
  }
  revisionTimeline: readonly RevisionTimelineEntry[]
  selectorEligibility: readonly SelectorEligibility[]
  usageSummary: {
    historicalReferenceCount: number
    note: string
  }
  sensitiveFields: readonly SensitiveFieldView[]
  /** Resource-specific overview facts. */
  resourceFacts: ReadonlyArray<{ label: string; value: string }>
  /** Warehouse only: policy is alert-only, stock summary links W10. */
  warehouseStockSummary?: {
    onHandQty: string
    reservedQty: string
    hasBlockingStock: boolean
    w10Href: string
    policyNote: string
  }
  /** Product: signature & base unit constraints. */
  productConstraints?: {
    specificationSignature: string
    baseUnit: string
    hasFormalReferences: boolean
  }
  allowedActions: readonly string[]
  actionBlockers: readonly ActionBlocker[]
  auditEvents: readonly {
    id: string
    at: string
    actor: string
    action: string
    detail: string
  }[]
  sections: readonly MasterDataSectionId[]
}>

/** 商品与 SKU：规格身份变化必须新建 SKU，不允许通过修订改变规格签名。 */
export type ProductFields = Readonly<{
  spu: string
  specSignature: string
  baseUnit: string
  category?: string
  brand?: string
  /** 默认/主供应商，下拉选自供应商主数据启用项。 */
  supplier?: string
  barcode?: string
  /** 主图文件名/资产标识，必填。 */
  mainImage: string
  /** 轮播图，逗号分隔文件名；允许空。 */
  carouselImages?: string
  /** 详情图，逗号分隔文件名；允许空。 */
  detailImages?: string
  /** 参考成本价（资料维护用，非正式供给成本）。 */
  costPrice?: string
  /** 参考销售价（资料维护用，非正式商城发布价）。 */
  salePrice?: string
}>

export type SellableItemFields = Readonly<{
  sku: string
  refSupplier?: string
  refCost?: string
  region?: string
  leadTime?: string
  fulfillmentModes?: string
  taxRate?: string
}>

export type VoucherCategoryFields = Readonly<{
  sku: string
  description?: string
}>

/** 商品分类字典：稳定代码 + 名称（name 在通用字段）。 */
export type CategoryFields = Readonly<{
  code: string
  parent?: string
  productKind?: string
}>

/** 品牌字典：稳定代码 + 名称（name 在通用字段）。 */
export type BrandFields = Readonly<{
  code: string
}>

export type SupplierFields = Readonly<{
  company: string
  role: string
  settlement?: string
  capability?: string
  qualification?: string
}>

export type WarehouseFields = Readonly<Record<string, never>>

/**
 * 按资源强类型化的专属字段（对齐 W14 §4.3 / §5.2 / §8.2）。
 * 正式提交不得退回通用 `Record<string, string>` 契约。
 */
export type MasterDataResourceFields = {
  "sellable-items": SellableItemFields
  products: ProductFields
  categories: CategoryFields
  brands: BrandFields
  "voucher-categories": VoucherCategoryFields
  suppliers: SupplierFields
  warehouses: WarehouseFields
}

export type CreateMasterDataInput = Readonly<{
  resource: MasterDataResource
  name: string
  effectiveFrom: string
  effectiveTo?: string
  changeReason: string
  fields: MasterDataResourceFields[MasterDataResource]
  idempotencyKey: string
  simulate?: "ok" | "overlap" | "sku_signature" | "base_unit" | "warehouse_stock"
}>

export type CreateRevisionInput = Readonly<{
  resource: MasterDataResource
  stableId: string
  baseRevisionId: string
  expectedLockVersion: number
  name: string
  effectiveFrom: string
  effectiveTo?: string
  changeReason: string
  fields: MasterDataResourceFields[MasterDataResource]
  idempotencyKey: string
  simulate?: "ok" | "overlap" | "sku_signature" | "base_unit" | "conflict"
}>

export type DisableMasterDataInput = Readonly<{
  resource: MasterDataResource
  stableId: string
  baseRevisionId: string
  expectedLockVersion: number
  changeReason: string
  effectiveFrom: string
  idempotencyKey: string
  simulate?: "ok" | "warehouse_stock" | "conflict"
}>

export type MasterDataMutationResult =
  | {
      outcome: "succeeded"
      stableId: string
      stableNo: string
      revisionId: string
      revisionNo: number
      revisionState: "CURRENT" | "FUTURE"
      effectiveFrom: string
      recordedAt: string
      actor: string
      changeReason: string
      reference: string
      nextActions: readonly string[]
    }
  | {
      outcome: "blocked"
      code: string
      message: string
      detail?: string
      drillHref?: string
    }
  | {
      outcome: "conflict"
      message: string
      serverLockVersion: number
      serverRevisionNo: number
    }
  | {
      outcome: "unknown"
      message: string
      idempotencyKey: string
    }

