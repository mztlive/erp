import type { StatusTone } from "@/components/ui/status-badge"

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

export type MasterDataResource =
  (typeof MASTER_DATA_RESOURCES)[number]["key"]

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
export type ProductListingStatus =
  | "LISTED"
  | "PARTIALLY_LISTED"
  | "UNLISTED"
export type ProductListingFilter = "listed" | "partially_listed" | "unlisted"
export type ProductSkuCoverageFilter = "complete" | "partial" | "none"
export type SupplierQualificationHealth =
  | "valid"
  | "expiring_30"
  | "expired"
  | "not_registered"
type RevisionTiming = "CURRENT" | "FUTURE" | "HISTORICAL"

export type MasterDataSectionId =
  | "overview"
  | "versions"
  | "relations"
  | "audit"

type FieldVisibility = "full" | "masked" | "hidden"

type SelectorEligibility = Readonly<{
  context: string
  contextLabel: string
  eligible: boolean
  blockerCodes: readonly string[]
  reason?: string
}>

type ActionBlocker = Readonly<{
  action: string
  code: string
  message: string
}>

type SensitiveFieldView = Readonly<{
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
  productCategoryId?: string
  productBrandId?: string
  productSupplierId?: string
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

/** 商品列表筛选使用的归属与有效供给供应商选项。 */
export type ProductFilterOptions = Readonly<{
  categories: readonly Readonly<{
    value: string
    label: string
    keywords: string
  }>[]
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
  /** 供应商关联 Party 的独立乐观锁版本。 */
  partyLockVersion?: number
  /** `资质类型::证书编号` → 当前适用能力代码，供原样修订。 */
  supplierQualificationCapabilityCodes?: Readonly<
    Record<string, readonly string[]>
  >
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
  /** 公司商品类型（`product.product_kind`）；SPU 稳定身份，创建后不可变。 */
  productKind?: ProductKind
  /**
   * 媒体字段的已登记资产回显：字段 key（`logo`/`qualification`/`contractFile`…）
   * → 文件清单（文件名 + asset id + 可访问 URL）。用于编辑回填与展示链接。
   */
  mediaAssets?: Readonly<
    Record<
      string,
      ReadonlyArray<{
        fileName: string
        assetId: string
        url: string
      }>
    >
  >
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
  /** 当前 SKU 修订 ID；编辑既有行时作为乐观并发条件提交。 */
  skuRevisionId?: string
  /** 该行来自历史停用 SKU；再次启用时必须提交明确意图。 */
  requiresExplicitReenable?: boolean
  /**
   * 规范化规格签名（`specification_signature`）：系统按规格属性组合派生，
   * 创建后不可变；用于判断某行能否延续原 `sku_id`。业务 UI 不展示、不手填。
   */
  specificationSignature?: string
  /**
   * 产品编码 = `sku_no`。
   * 系统按规格组合默认生成，允许手动覆盖；仅为全局唯一业务编码，
   * 不能作为身份恢复或重绑键。
   */
  skuNo: string
  /** 与 specs 顺序对齐的属性取值。 */
  attributeValues: readonly string[]
  /** 展示用规格文案，如「颜色：红 / 规格：大」。 */
  specLabel: string
  barcode?: string
  /** SKU 主图（单张，展示名）。 */
  mainImage: string
  /** SKU 主图可访问地址（仅回显用）。 */
  mainImagePreviewUrl?: string
  /** SKU 主图已登记文件资产（D05）。 */
  mainImageAssetId?: string
  /** 公司商品池销售可见价；销售可见，采购成本不可见。 */
  salePrice?: string
  /** 市场价（参考展示，非正式发布价）。 */
  marketPrice?: string
  baseUnit?: string
  /** 独立于启停状态的 SKU 上架状态。 */
  listingStatus?: "LISTED" | "UNLISTED"
  lifecycleStatus: LifecycleStatus
}>

/** 商品（SPU）可写字段：规格组合出 SKU；无「规格标识」手填字段。 */
export type ProductFields = Readonly<{
  /** 商品稳定身份当前启停状态；编辑不得隐式改为启用。 */
  lifecycleStatus: LifecycleStatus
  /** 商品编号（product_no）；创建必填，创建后不可变。 */
  productNo: string
  /** 公司审核后的商品描述；可由供应商来源资料预填，但保存后独立维护。 */
  description?: string
  /** 公司审核后的 SPU 规格/服务内容；未展示编辑时仍须原样保留。 */
  specification?: string
  /** `unit_of_measure` 稳定身份与代码；`baseUnit` 仅为显示快照。 */
  baseUnitId: string
  baseUnitCode: string
  baseUnit: string
  categoryId: string
  category: string
  brandId: string
  brand: string
  /**
   * 公司商品类型（`product.product_kind`）：独立必填稳定属性，
   * W14 正向创建必须显式提交，写入后不可变；分类仅校验兼容性。
   * 空字符串表示草稿未选择，提交前 fail-closed。
   */
  productKind: ProductKind | ""
  /** SPU 轮播图（多张，可空）。 */
  carouselImages: readonly string[]
  /** SPU 详情图（多张，可空）。 */
  detailImages: readonly string[]
  /** 轮播图 fileName → 可访问预览/来源 URL（回显与再次保存用） */
  carouselPreviewUrls: Readonly<Record<string, string>>
  detailPreviewUrls: Readonly<Record<string, string>>
  /** 轮播图 fileName → 已登记文件资产 id */
  carouselFileAssetIds: Readonly<Record<string, string>>
  detailFileAssetIds: Readonly<Record<string, string>>
  specs: readonly ProductSpecDimension[]
  skus: readonly ProductSkuFields[]
}>

/** 对象中心展示用的商品详情投影。 */
export type ProductDetailView = Readonly<{
  lifecycleStatus: LifecycleStatus
  productNo: string
  description?: string
  specification?: string
  baseUnitId: string
  baseUnitCode: string
  baseUnit: string
  categoryId: string
  category: string
  brandId: string
  brand: string
  carouselImages: readonly string[]
  detailImages: readonly string[]
  carouselPreviewUrls: Readonly<Record<string, string>>
  detailPreviewUrls: Readonly<Record<string, string>>
  carouselFileAssetIds: Readonly<Record<string, string>>
  detailFileAssetIds: Readonly<Record<string, string>>
  specs: readonly ProductSpecDimension[]
  skus: readonly ProductSkuFields[]
}>

export type SellableItemFields = Readonly<{
  sku: string
  /** 采购发布给销售的公司商品池价格；不是任何一家供应商的成本。 */
  salesVisiblePriceGross: string
  /** 从 supplier_offering 聚合的可用供应商数量，只读投影。 */
  supplierCount?: string
  region?: string
  leadTime?: string
  fulfillmentModes?: string
}>

/**
 * 卡券类目可写字段：业务上一个卡券类目即一个 VOUCHER 类型的 SKU，
 * `voucherNo` 同时作为 `product_no` 与 `sku_no`（无需分别填写）。
 *
 * 分类 / 品牌 / 基础单位由后端默认补齐（共用卡券根分类、品牌「福尚云」、单位「张」），
 * 前端新建 Dialog 只提交编号与描述；可选字段保留给显式覆盖或兼容旧调用。
 */
export type VoucherCategoryFields = Readonly<{
  voucherNo: string
  description: string
  specification?: string
  /** 显式分类 ID；省略则用共用卡券根分类。 */
  categoryId?: string
  category?: string
  newCategoryCode?: string
  newCategoryName?: string
  newCategoryParentId?: string
  /** 显式品牌 ID；省略则用「福尚云」。 */
  brandId?: string
  brand?: string
  /** 显式基础单位 ID；省略则用「张」。 */
  baseUnitId?: string
  baseUnitCode?: string
  baseUnit?: string
  barcode?: string
  salesVisiblePriceGross?: string
  marketPrice?: string
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
  /** 品牌 Logo 展示文件名。 */
  logo?: string
  /** 品牌 Logo 已登记文件资产（D05）。 */
  logoAssetId?: string
  /** 品牌 Logo 可访问 URL（仅回显用）。 */
  logoPreviewUrl?: string
}>

/**
 * 计量单位字典：稳定代码 + 名称（name 在通用字段）+ 符号 + 数量小数位。
 * `unit_code` 创建后不可改；更新只改 name / symbol / quantity_scale / status。
 */
export type UnitOfMeasureFields = Readonly<{
  code: string
  symbol: string
  /** 允许数量小数位（0–6），表单以字符串承载。 */
  quantityScale: string
}>

export type SupplierFields = Readonly<{
  company: string
  creditCode?: string
  contactName?: string
  contactPhone?: string
  clearContact?: boolean
  address?: string
  clearAddress?: boolean
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
  /** 资质字段 fileName → 已登记文件资产 id（D05）。 */
  qualificationFileAssetIds?: Readonly<Record<string, string>>
  contractFileAssetIds?: Readonly<Record<string, string>>
  authorizationFileAssetIds?: Readonly<Record<string, string>>
  foodLicenseFileAssetIds?: Readonly<Record<string, string>>
  legalPersonIdCardFileAssetIds?: Readonly<Record<string, string>>
  /** `资质类型::证书编号` → 适用能力代码；新资质默认适用当前勾选能力。 */
  qualificationCapabilityCodes?: Readonly<Record<string, readonly string[]>>
  taxNo?: string
  clearTaxProfile?: boolean
  bankName?: string
  bankAccount?: string
  clearBankAccount?: boolean
  invoiceType?: string
  invoiceTaxRate?: string
  initialScore?: string
  supplierRating?: string
  currentScore?: string
}>

type WarehouseFields = Readonly<Record<string, never>>

/**
 * 按资源强类型化的专属字段（对齐 W14 §4.3 / §5.2 / §8.2）。
 * 正式提交不得退回通用 `Record<string, string>` 契约。
 */
export type MasterDataResourceFields = {
  "sellable-items": SellableItemFields
  products: ProductFields
  categories: CategoryFields
  brands: BrandFields
  "unit-of-measures": UnitOfMeasureFields
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
}>

export type CreateRevisionInput = Readonly<{
  resource: MasterDataResource
  stableId: string
  baseRevisionId: string
  expectedLockVersion: number
  /** 聚合跨 Party 修订时的独立乐观锁版本。 */
  expectedPartyVersion?: number
  name: string
  effectiveFrom: string
  effectiveTo?: string
  changeReason: string
  fields: MasterDataResourceFields[MasterDataResource]
  idempotencyKey: string
}>

export type DisableMasterDataInput = Readonly<{
  resource: MasterDataResource
  stableId: string
  baseRevisionId: string
  expectedLockVersion: number
  changeReason: string
  effectiveFrom: string
  idempotencyKey: string
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
