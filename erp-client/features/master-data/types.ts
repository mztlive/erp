import type { StatusTone } from "@/components/ui/status-badge"

export const MASTER_DATA_RESOURCES = [
  { key: "sellable-items", label: "公司商品池" },
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
  /** 商品修订的完整 SPU/SKU/价格快照；历史查看不得回填当前主档。 */
  productSnapshot?: ProductDetailView
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
  /**
   * 商品 SPU 约束摘要（不含规格标识：签名由属性组合系统派生，UI 不展示）。
   */
  productConstraints?: {
    baseUnit: string
    hasFormalReferences: boolean
    skuCount: number
  }
  /**
   * 商品 SPU 详情：规格维度 + 由规格组合生成的 SKU 行。
   * 主图在 SKU；轮播图 / 详情图在 SPU。
   */
  productDetail?: ProductDetailView
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

/** 规格维度（如 颜色 / 规格），取值组合生成 SKU。 */
export type ProductSpecDimension = Readonly<{
  name: string
  values: readonly string[]
}>

/**
 * SKU 行：保存公司商品身份与媒体；`salePrice` 是商品池销售可见价的
 * 编辑投影，命令处理器必须写入商品池修订，不得把它当作供应商成本。
 * 规格取值由 SPU 规格维度组合得出。
 *
 * 供应商、供给模式、供货价、底价、进项税、费用、MOQ、区域、能力与
 * 有效期全部归 W21 `supplier_offering` / `supplier_offering_revision`；
 * 不得嵌入 SKU 修订形成第二份供给事实或草稿。
 */
export type ProductSkuFields = Readonly<{
  skuId?: string
  /**
   * 产品编码 = `sku_no`。
   * 系统按规格组合默认生成，允许手动覆盖。
   */
  skuNo: string
  /** 与 specs 顺序对齐的属性取值。 */
  attributeValues: readonly string[]
  /** 展示用规格文案，如「颜色：红 / 规格：大」。 */
  specLabel: string
  barcode?: string
  /** SKU 主图（单张）。 */
  mainImage: string
  /** 公司商品池销售可见价；销售可见，采购成本不可见。 */
  salePrice?: string
  /** 市场价（参考展示，非正式发布价）。 */
  marketPrice?: string
  baseUnit?: string
  lifecycleStatus: LifecycleStatus
}>

/** 商品（SPU）可写字段：规格组合出 SKU；无「规格标识」手填字段。 */
export type ProductFields = Readonly<{
  /** 公司审核后的商品描述；可由供应商来源资料预填，但保存后独立维护。 */
  description?: string
  /** `unit_of_measure` 稳定身份与代码；`baseUnit` 仅为显示快照。 */
  baseUnitId: string
  baseUnitCode: string
  baseUnit: string
  categoryId: string
  category: string
  brandId: string
  brand: string
  /** SPU 轮播图（多张，可空）。 */
  carouselImages: readonly string[]
  /** SPU 详情图（多张，可空）。 */
  detailImages: readonly string[]
  specs: readonly ProductSpecDimension[]
  skus: readonly ProductSkuFields[]
}>

/** 对象中心展示用的商品详情投影。 */
export type ProductDetailView = Readonly<{
  description?: string
  baseUnitId: string
  baseUnitCode: string
  baseUnit: string
  categoryId: string
  category: string
  brandId: string
  brand: string
  carouselImages: readonly string[]
  detailImages: readonly string[]
  specs: readonly ProductSpecDimension[]
  skus: readonly ProductSkuFields[]
}>

export type SellableItemFields = Readonly<{
  sku: string
  /** 采购发布给销售的公司商品池价格；不是任何一家供应商的成本。 */
  salesVisiblePrice: string
  /** 从 supplier_offering 聚合的可用供应商数量，只读投影。 */
  supplierCount?: string
  region?: string
  leadTime?: string
  fulfillmentModes?: string
}>

export type VoucherCategoryFields = Readonly<{
  sku: string
  description?: string
}>

/** 商品分类字典：稳定代码 + 名称（name 在通用字段）+ 可选上级。 */
export type CategoryFields = Readonly<{
  code: string
  /** 上级分类稳定 ID；根分类为空。 */
  parentId?: string
  /** 兼容旧表单/展示：上级分类名称。 */
  parent?: string
  productKind?: string
}>

/** 品牌字典：稳定代码 + 名称（name 在通用字段）+ 可选 Logo。 */
export type BrandFields = Readonly<{
  code: string
  /** 品牌 Logo 文件名（演示）；正式为 file_asset 引用。 */
  logo?: string
}>

export type SupplierFields = Readonly<{
  company: string
  contactName?: string
  contactPhone?: string
  address?: string
  settlement?: string
  capability?: string
  businessCategory?: string
  signingEntity?: string
  paymentEntity?: string
  qualification?: string
  contractNo?: string
  contractValidFrom?: string
  contractValidTo?: string
  contractFile?: string
  authorizationFile?: string
  authorizationValidFrom?: string
  authorizationValidTo?: string
  foodLicense?: string
  legalPersonIdCard?: string
  taxNo?: string
  bankName?: string
  bankAccount?: string
  invoiceType?: string
  invoiceTaxRate?: string
  initialScore?: string
  supplierRating?: string
  currentScore?: string
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
  simulate?: "ok" | "overlap" | "base_unit" | "warehouse_stock"
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
  simulate?: "ok" | "overlap" | "base_unit" | "conflict"
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
