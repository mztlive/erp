/**
 * 供应商商品库、公司商品映射与供给版本 · 客户端契约类型。
 * Excel、API 与手工录入只代表来源渠道，统一形成供应商 SPU/SKU。
 */

import type { ProductKind } from "@/features/master-data/types"

export type ChangeType = "NEW" | "CHANGED" | "STOPPED" | "ERROR" | "UNCHANGED"

export type SupplierCatalogSourceType = "EXCEL" | "API" | "MANUAL"

export type SupplierCatalogMediaUsage =
  | "SPU_CAROUSEL"
  | "SPU_DETAIL"
  | "SKU_MAIN"

/**
 * 供应商来源媒体。来源 URL 只用于取回，正式复用前必须归档为 fileAssetId；
 * 公司商品媒体会形成自己的修订引用，不能长期依赖供应商 URL。
 */
export type SupplierCatalogMediaView = Readonly<{
  id: string
  usage: SupplierCatalogMediaUsage
  fileName: string
  sortOrder: number
  fileAssetId?: string
  sourceUrl?: string
  archiveStatus: "ARCHIVED" | "PENDING_IMPORT" | "FAILED"
}>

export type SupplierCatalogAttributeView = Readonly<{
  name: string
  value: string
}>

export type SupplierCatalogSourceView = Readonly<{
  type: SupplierCatalogSourceType
  label: string
  /** 只有 API 来源才存在连接；Excel 和手工来源不得伪造连接。 */
  connection?: { id: string; code: string }
  fileName?: string
  batchNo?: string
  recordedBy?: string
}>

export type SupplierProductRevisionView = Readonly<{
  revisionNo: number
  sourceRevisionToken?: string
  sourceUpdatedAt: string
  syncedAt: string
  name: string
  description?: string
  /** 来源商品自身声明的类型；手工来源必填。 */
  sourceProductKind?: ProductKind
  specification: string
  category: string
  brand?: string
  baseUnit?: string
  barcode?: string
  attributes?: readonly SupplierCatalogAttributeView[]
  media?: readonly SupplierCatalogMediaView[]
  /** 一件代发底价（含税运） */
  dropshipFloorPriceGross: string | null
  /** 集采底价（含税） */
  bulkFloorPriceGross: string | null
  /** 集采起订量 */
  bulkMinimumOrderQuantity: string | null
  availableQuantity: string
  availabilityStatus: "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE"
  /** 仅技术/审计短指纹，非原始报文 */
  contentFingerprintShort?: string
}>

export type SkuCandidateView = Readonly<{
  productId?: string
  skuId: string
  skuCode: string
  skuName: string
  specification: string
  baseUnit: string
  barcode?: string
  brand?: string
  category?: string
  revisionNo: number
  similarityLabel: string
  matchSignals?: readonly string[]
  /** 当前已生效供给的供应商数量。 */
  activeSupplierCount?: number
  /** 当前公司 SKU 的销售资格投影；不产生独立商品池身份。 */
  sellableSku?: {
    skuId: string
    skuRevisionId: string
    status: "ACTIVE" | "PAUSED" | "DISABLED"
    salesVisiblePriceGross: string
  }
}>

export type SupplierProductMappingView = Readonly<{
  mappingStatus: "PENDING" | "ACTIVE" | "CONFLICT" | "DISABLED"
  skuId?: string
  skuCode?: string
  skuName?: string
  skuRevisionId?: string
  specification?: string
  baseUnit?: string
  approvedBy?: string
  approvedAt?: string
  reason?: string
  mappingVersion?: string
  history: readonly {
    id: string
    skuCode: string
    status: string
    at: string
    note: string
  }[]
}>

export type SupplierOfferingRevisionView = Readonly<{
  offeringId: string
  offeringRevisionId: string
  revisionNo: number
  status: "ACTIVE" | "PAUSED" | "STOPPED" | "PENDING_CONFIRM"
  supplyPriceGross: string | null
  supplyPriceNet: string | null
  floorPriceGross: string | null
  /** 一件代发供给价（含税）；与集采供给价同版保存，不折叠成单一确认成本。 */
  dropshipSupplyPriceGross: string | null
  /** 集采供给价（含税） */
  bulkSupplyPriceGross: string | null
  dropshipExpress?: string
  inputTaxRate: string | null
  freightAmount: string | null
  serviceFeeAmount: string | null
  minimumOrderQuantity: string
  supplyRegion: string[]
  availabilityStatus: string
  availableQuantity: string
  productCapabilities: string[]
  validFrom: string
  validTo?: string
  createdAt: string
  immutable: true
}>

export type SafeOfferingDraftView = Readonly<{
  dropshipSupplyPriceGross: string
  bulkSupplyPriceGross: string
  dropshipExpress?: string
  inputTaxRate: string
  freightAmount: string
  serviceFeeAmount: string
  minimumOrderQuantity: string
  supplyRegion: string[]
  productCapabilities: string[]
  validFrom: string
  validTo?: string
  /** 会话草稿标记，非正式修订 */
  sessionDraftOnly: true
}>

/** 保存既有供给的新不可变修订。 */
export type ReviseSupplierOfferingInput = Readonly<{
  offeringId: string
  expectedRevisionNo: number
  dropshipSupplyPriceGross: string
  bulkSupplyPriceGross: string
  inputTaxRate: string
  bulkMinimumOrderQuantity: string
  supplyRegion: string[]
  productCapabilities: string[]
  validFrom: string
  validTo?: string
  dropshipExpress?: string
  freightAmount?: string
  serviceFeeAmount?: string
  availableQuantity?: string
  status: "ACTIVE" | "PAUSED" | "STOPPED"
  changeReason: string
  idempotencyKey: string
}>

export type PublicationImpactView = Readonly<{
  activePublicationCount: number | null
  pausedPublicationCount: number | null
  historicalPaidOrderCount: number | null
  safetyPauseTriggered: boolean
  safetyPauseReasons: readonly string[]
  pauseSubResults: readonly {
    id: string
    publicationId: string
    reason: string
    outboxId: string
    status: string
  }[]
  /** 供货价变化不自动改商城销售价 */
  mallSalePriceAutoUpdate: false
  /** MOQ 不自动复制为商城最小购买量 */
  moqCopiedToMallMinPurchase: false
  recoveryBlocker?: {
    code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED"
    message: string
  }
  note: string
}>

export type SupplierCatalogRegistrationBlocker = Readonly<{
  code: "WORK_ITEM_TYPE_UNREGISTERED"
  message: string
  businessProcess: "MAPPING" | "OFFERING_REVIEW"
}>

export type SupplierCatalogExceptionWorkItem = Readonly<{
  workItemId: string
  workItemType: "BUSINESS_EXCEPTION"
  businessObjectType: "SUPPLIER_CATALOG_SKU" | "SUPPLIER_OFFERING"
  subjectVersion: string
  subjectHash: string
  workItemStatus: "PENDING" | "IN_PROGRESS" | "COMPLETED"
  dueAt?: string
  claimedBy?: { userId: string; displayName: string }
  allowedActions: readonly (
    | "CLAIM"
    | "HOLD"
    | "RETURN_FOR_DATA_FIX"
    | "QUERY_ORIGINAL_RESULT"
    | "SAVE_EVIDENCE"
    | "CONFIRM_ERROR_RESOLVED"
    | "CONFIRM_STOP_SUPPLY"
  )[]
  actionBlockers: readonly {
    action: string
    code: string
    message: string
    destinationWorkspaceId?: string
  }[]
  held?: boolean
  reason: string
  impact: string
  priority: number
  handlerKey: string
}>

export type DiffChange = Readonly<{
  id: string
  field: string
  before: string
  after: string
  note?: string
  /** 成本相关字段：掩码时 before/after 已是 *** */
  costSensitive?: boolean
}>

/** 供应商 SPU 下的一条 SKU 及当前来源修订（供给字段在 SKU 修订上）。 */
export type SupplierCatalogSkuView = Readonly<{
  id: string
  supplierSkuCode: string
  currentRevision: SupplierProductRevisionView
}>

/** 供应商 SKU 相对公司商品池状态。 */
export type PoolMatchStatus = "MAPPED" | "HAS_CANDIDATES" | "UNMATCHED"

export type CompanySkuMatchCandidate = Readonly<{
  skuId: string
  skuNo: string
  productId: string
  productNo: string
  name: string
  specification?: string
  barcode?: string
  baseUnitId: string
  salesVisiblePriceGross?: string
  activeSupplierCount: number
  matchSignals: readonly string[]
  score: number
}>

export type SupplierSkuPoolMatch = Readonly<{
  supplierCatalogSkuId: string
  supplierSkuCode: string
  specification?: string
  barcode?: string
  poolStatus: PoolMatchStatus
  mappedCompanySkuId?: string
  mappedCompanySkuNo?: string
  candidates: readonly CompanySkuMatchCandidate[]
}>

export type SupplierProductPoolMatchView = Readonly<{
  supplierProductId: string
  sourceRevisionNo: number
  items: readonly SupplierSkuPoolMatch[]
}>

export type LinkPromoteSkuItemInput = Readonly<{
  supplierCatalogSkuId: string
  companySkuId: string
  dropshipSupplyPriceGross?: string
  bulkSupplyPriceGross?: string
}>

export type LinkPromoteToCompanyPoolInput = Readonly<{
  supplierProductId: string
  expectedSourceRevisionNo: number
  inputTaxRate: string
  supplyRegion: string[]
  items: readonly LinkPromoteSkuItemInput[]
  idempotencyKey: string
}>

export type SupplierCatalogItemBase = {
  queuePosition?: { current: number; total: number; snapshotId: string }
  supplierProduct: {
    id: string
    supplier: { id: string; name: string }
    source: SupplierCatalogSourceView
    /** 供应商自己的 SPU/SKU 身份；无供应商编码时由 ERP 生成内部来源身份。 */
    supplierSpuCode?: string
    /** 主展示 / 入池默认：首个或当前选中的供应商 SKU 编码 */
    supplierSkuCode: string
    status: string
    /** SPU 内容 + 主 SKU 供给投影（兼容列表）；完整多 SKU 见 catalogSkus */
    currentRevision: SupplierProductRevisionView
    incomingRevision?: SupplierProductRevisionView
    /** 规格组合后的多供应商 SKU；缺省时等价于仅 currentRevision 一条 */
    catalogSkus?: readonly SupplierCatalogSkuView[]
  }
  mapping?: SupplierProductMappingView
  skuCandidates: SkuCandidateView[]
  offering?: {
    stableId: string
    currentRevision?: SupplierOfferingRevisionView
    revisionHistory: readonly SupplierOfferingRevisionView[]
    proposedDefaults?: SafeOfferingDraftView
  }
  /** 公司 SKU 的销售资格投影；销售只消费该投影，不读取采购成本。 */
  sellableSku?: {
    skuId: string
    skuRevisionId: string
    status: "ACTIVE" | "PAUSED" | "DISABLED"
    salesVisiblePriceGross: string
    validFrom: string
    validTo?: string
  }
  publicationImpact: PublicationImpactView
  sourceContext: {
    intakeId: string
    sourceReference: string
    receivedAt: string
  }
  sourceDiff: readonly DiffChange[]
  allowedActions: string[]
  actionBlockers: Array<{
    action: string
    code: string
    message: string
    destinationWorkspaceId?: string
  }>
}

export type SupplierCatalogItemView =
  | (SupplierCatalogItemBase &
      (
        | {
            changeType: "ERROR" | "STOPPED"
            workItem: SupplierCatalogExceptionWorkItem
            registrationBlocker?: never
          }
        | {
            changeType: "ERROR" | "STOPPED"
            workItem?: never
            registrationBlocker: SupplierCatalogRegistrationBlocker
          }
      ))
  | (SupplierCatalogItemBase & {
      changeType: "NEW" | "CHANGED"
      workItem?: never
      registrationBlocker: SupplierCatalogRegistrationBlocker
    })
  | (SupplierCatalogItemBase & {
      changeType: "UNCHANGED"
      workItem?: never
      registrationBlocker?: never
    })

export type SupplierCatalogQueueQuery = {
  mode?: "queue" | "list"
  sourceType?: SupplierCatalogSourceType | "all"
  supplierId?: string
  connectionId?: string
  /** 从 W14 进入时，限定到与该稳定 SKU 已映射或候选关联的供给 */
  skuId?: string
  changeType?: "actionable" | "NEW" | "CHANGED" | "STOPPED" | "ERROR" | "all"
  mappingStatus?: string
  offeringStatus?: string
  publicationImpact?: string
  freshness?: string
  q?: string
  queueContextId?: string
  currentSupplierProductId?: string
  currentWorkItemId?: string
  pageSize?: number
  sort?: string
  status?: "pending" | "held"
}

export type SupplierCatalogQueueView = Readonly<{
  preferences: { autoNextDefault: boolean }
  /**
   * 从商品中心进入时的 SKU 上下文。
   * 它来自商品主档，不能从供给关系结果反推，否则无关系时会丢失商品身份。
   * 公司侧资料用于「添加供应商并登记成本」时反向复用为供应商商品快照。
   */
  skuContext?: {
    productId: string
    productName: string
    productKind: ProductKind
    skuId: string
    skuCode: string
    specification: string
    baseUnit: string
    category?: string
    brand?: string
    barcode?: string
    description?: string
    carouselImages?: readonly string[]
    detailImages?: readonly string[]
    mainImage?: string
    sellableSku?: {
      skuId: string
      skuRevisionId: string
      status: "ACTIVE" | "PAUSED" | "DISABLED"
      salesVisiblePriceGross: string
    }
  }
  context: {
    queueContextId: string
    position: number
    total: number
    currentSupplierProductId?: string
    currentWorkItemId?: string
    previousSupplierProductId?: string
    nextSupplierProductId?: string
    filterSummary: string
    queueContextUpdatedAt: string
  }
  items: readonly SupplierCatalogItemView[]
  current?: SupplierCatalogItemView
  emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
}>

export type SupplierCatalogCenterView = Readonly<{
  item: SupplierCatalogItemView
  section: string
  related: {
    publications: readonly {
      id: string
      label: string
      status: string
      href: string
    }[]
    historyOrders: readonly {
      id: string
      label: string
      note: string
    }[]
    techExceptions: readonly {
      id: string
      label: string
      href: string
    }[]
  }
}>

export type WorkItemLease = Readonly<{
  workItemId: string
  claimedByLabel: string
}>

export type SupplierCatalogWorkItemAction =
  | { kind: "HOLD"; reasonCode: string; comment?: string }
  | {
      kind: "RETURN_FOR_DATA_FIX"
      reasonCode: string
      suggestedResponsibleRole?: string
      comment?: string
    }
  | { kind: "QUERY_ORIGINAL_RESULT"; operationId: string }
  | { kind: "SAVE_EVIDENCE"; evidenceReferences: string[]; comment?: string }

export type SupplierCatalogDecision =
  | {
      kind: "CONFIRM_ERROR_RESOLVED"
      expectedSourceRevision: string
      resolutionCode: string
      evidenceReferences?: string[]
      comment?: string
    }
  | {
      kind: "CONFIRM_STOP_SUPPLY"
      expectedSourceRevision: string
      expectedOfferingRevision?: string
      reasonCode: string
      comment?: string
    }

export type SupplierCatalogBusinessResult = Readonly<{
  decisionKind: SupplierCatalogDecision["kind"]
  supplierProductId: string
  /** 精确供应商 SKU；错误中心/审计以此为准。 */
  supplierCatalogSkuId: string
  auditEventId: string
  offeringRevision?: string
  publicationImpact: PublicationImpactView
  reference: string
  completedAt: string
  subjectHash: string
}>

export type FormalOutcome =
  | {
      kind: "ACTION"
      workItemId: string
      workItemStatus: "PENDING" | "IN_PROGRESS"
      actionKind: string
      heldAt?: string
      resumeHint: string
      reference: string
    }
  | {
      kind: "COMPLETED"
      business: SupplierCatalogBusinessResult
    }

export type FormalActionResponse =
  | { status: "succeeded"; outcome: FormalOutcome }
  | { status: "failed"; message: string; code: string }

/** 会话内映射/供给草稿（非正式）；按供应商 SKU 为粒度。 */
export type SessionCatalogDraft = Readonly<{
  supplierCatalogSkuId: string
  selectedSkuId?: string
  offeringDraft?: SafeOfferingDraftView
  substituteCandidateSkuIds?: string[]
  note?: string
  updatedAt: string
}>

/** SPU 级内容（名称/类目/图文等）。 */
export type SupplierCatalogSpuContentFields = Readonly<{
  name: string
  description?: string
  /** 来源商品类型；MANUAL 来源必须显式填写。 */
  sourceProductKind?: ProductKind
  specification: string
  category: string
  brand?: string
  sourceBaseUnit?: string
  attributes: readonly SupplierCatalogAttributeView[]
  /** SPU 轮播/详情；SKU 主图写在 skus[].media */
  media: readonly Omit<SupplierCatalogMediaView, "id">[]
}>

/** 单条供应商 SKU 及来源供给（对应 sku_revision）。 */
export type SupplierCatalogSkuWriteFields = Readonly<{
  id?: string
  supplierSkuCode: string
  barcode?: string
  specification?: string
  attributes?: readonly SupplierCatalogAttributeView[]
  media?: readonly Omit<SupplierCatalogMediaView, "id">[]
  dropshipFloorPriceGross: string
  bulkFloorPriceGross: string
  bulkMinimumOrderQuantity: string
  availableQuantity?: string
  availabilityStatus?: "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE"
}>

/**
 * 兼容旧扁平字段：单 SKU 时可用顶层 barcode/价格等；
 * 多 SKU 时必须传 skus[]，顶层价格字段取 skus[0] 投影。
 */
export type SupplierCatalogContentFields = SupplierCatalogSpuContentFields &
  Partial<
    Omit<SupplierCatalogSkuWriteFields, "id" | "specification" | "attributes" | "media">
  > &
  Readonly<{
    barcode?: string
    media: readonly Omit<SupplierCatalogMediaView, "id">[]
    dropshipFloorPriceGross?: string
    bulkFloorPriceGross?: string
    bulkMinimumOrderQuantity?: string
  }>

/** Excel/API/手工三种来源共用的供应商商品录入命令。 */
export type CreateSupplierCatalogItemInput = SupplierCatalogSpuContentFields &
  Readonly<{
    sourceType: SupplierCatalogSourceType
    supplierId: string
    supplierName: string
    supplierSpuCode?: string
    /**
     * 多规格多 SKU；至少一行。
     * 未传时退化为 supplierSkuCode + 顶层价格字段单 SKU。
     */
    skus?: readonly SupplierCatalogSkuWriteFields[]
    /** 单 SKU 兼容字段 */
    supplierSkuCode?: string
    barcode?: string
    dropshipFloorPriceGross?: string
    bulkFloorPriceGross?: string
    bulkMinimumOrderQuantity?: string
    availableQuantity?: string
    availabilityStatus?: "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE"
    /** 固定公司 SKU 入口使用；形成供给修订，销售侧不可见。 */
    confirmedCostGross?: string
    sourceReference?: string
    targetSkuId?: string
    targetSkuCode?: string
    targetSkuName?: string
    targetSpecification?: string
    baseUnit?: string
    minimumOrderQuantity: string
    /** 仅固定 SKU 入池/供给路径使用；非供应商商品目录字段 */
    supplyRegion?: string[]
    inputTaxRate?: string
    validFrom: string
    idempotencyKey: string
  }>

/**
 * 在供应商商品中心保存内容：形成新的来源修订（不可变），
 * 不写公司 SKU / 商品池 / 供给确认成本。
 */
export type ReviseSupplierCatalogProductInput = SupplierCatalogSpuContentFields &
  Readonly<{
    supplierProductId: string
    expectedSourceRevisionNo: number
    supplierSpuCode?: string
    skus: readonly SupplierCatalogSkuWriteFields[]
    changeReason: string
    idempotencyKey: string
  }>

/** 采购把供应商 SKU 选入公司商品池，并同时确认供给成本。 */
export type PromoteSupplierProductInput = Readonly<{
  /** 唯一正式入池键：精确供应商 SKU（supplier_catalog_sku_id）。 */
  supplierCatalogSkuId: string
  targetSkuId: string
  targetSkuCode: string
  targetSkuName: string
  specification: string
  baseUnit: string
  /** 公司商品类型（ProductKind 编码）；无可靠来源时空白必填，来源只可预填。 */
  productKind: ProductKind
  /** 采购私密成本；与来源报价和销售可见价均为不同事实。 */
  confirmedCostGross: string
  inputTaxRate: string
  minimumOrderQuantity: string
  supplyRegion: string[]
  validFrom: string
  expectedSourceRevisionNo: number
  idempotencyKey: string
}>

export type SupplierCatalogWriteResult = Readonly<{
  supplierProductId: string
  /** 本次写入的精确供应商 SKU。 */
  supplierCatalogSkuId?: string
  /** 反向创建分支：新公司商品/SKU。 */
  companyProductId?: string
  companySkuId?: string
  /** 入池/反向创建确认的商品类型。 */
  productKind?: string
  supplierOfferingRevisionId?: string
  companySkuChange: "NONE" | "CREATED" | "REVISED" | "UNCHANGED"
  activeSupplierCount?: number
  reference: string
  recordedAt: string
}>

/** 反向入池：单行供应商 SKU → 新建公司 SKU + 映射 + 双价供给。 */
export type ReversePromoteSkuItemInput = Readonly<{
  supplierCatalogSkuId: string
  skuNo?: string
  /** 可空：空则后端回退目录代发底价。 */
  dropshipSupplyPriceGross?: string
  /** 可空：空则后端回退目录集采底价。 */
  bulkSupplyPriceGross?: string
  salesVisiblePriceGross: string
  marketPrice: string
}>

/**
 * 反向入池：以供应商 SPU 为上下文，同构新建公司商品 + 勾选 SKU，
 * 原子写入映射与双价供给；确认即生效，无生效日期字段。
 */
export type ReversePromoteToCompanyPoolInput = Readonly<{
  supplierProductId: string
  expectedSourceRevisionNo: number
  productKind: ProductKind
  productNo?: string
  categoryId: string
  brandId: string
  baseUnitId: string
  inputTaxRate: string
  supplyRegion: string[]
  items: readonly ReversePromoteSkuItemInput[]
  idempotencyKey: string
}>

/** @deprecated 使用 ReversePromoteToCompanyPoolInput */
export type CreateCompanyProductFromSupplierSkuInput = ReversePromoteToCompanyPoolInput

export const CHANGE_TYPE_LABEL: Record<ChangeType, string> = {
  NEW: "新供应商商品",
  CHANGED: "关键变化",
  STOPPED: "停止供应",
  ERROR: "异常数据",
  UNCHANGED: "无变化",
}

export const HOLD_REASON_OPTIONS = [
  { value: "NEED_CLARIFICATION", label: "需澄清规格/映射" },
  { value: "WAITING_SOURCE", label: "等待来源修复" },
  { value: "WAITING_MASTER_DATA", label: "等待基础资料" },
  { value: "OTHER", label: "其他" },
] as const

export const RETURN_REASON_OPTIONS = [
  { value: "SOURCE_DATA_ERROR", label: "来源数据错误" },
  { value: "PAYLOAD_INVALID", label: "结构/字段无效" },
  { value: "SYNC_CORRUPT", label: "同步损坏" },
  { value: "OTHER", label: "其他" },
] as const

export const REGISTRATION_BLOCKER_MESSAGE =
  "来源商品变化仍需采购复核；采购可以直接把新供应商商品加入公司商品池，异常数据必须先修复。"

export const RECOVERY_BLOCKER_MESSAGE =
  "替代供给选定人与恢复发布责任链尚未确认。系统按保守策略处理：可准备候选证据，但不得选定替代供给或从供应商商品库发起商品发布恢复。"
