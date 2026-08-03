/**
 * 供应商商品库、公司商品映射与供给版本 · 客户端契约类型。
 * Excel、API 与手工录入只代表来源渠道，统一形成供应商 SPU/SKU。
 */

export type ChangeType = "NEW" | "CHANGED" | "STOPPED" | "ERROR" | "UNCHANGED"

export type DemoRole = "procurement" | "operations" | "admin" | "ops_tech"

export type CostFieldVisibility = "visible" | "masked"

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
  specification: string
  category: string
  brand?: string
  baseUnit?: string
  barcode?: string
  attributes?: readonly SupplierCatalogAttributeView[]
  media?: readonly SupplierCatalogMediaView[]
  /** 供应商来源报价快照；无成本权时 API 可返回 null 并由 UI 掩码。 */
  sourceQuotedPriceGross: string | null
  inputTaxRate: string | null
  freightAmount: string | null
  otherFeeAmount: string | null
  supplyRegion: string[]
  availableQuantity: string
  availabilityStatus: "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE"
  expectedShipTime?: string
  afterSalesNote?: string
  capabilitySnapshot: string[]
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
  /** 一个公司 SKU 最多一个商品池条目；多家供应商共享该条目。 */
  poolEntry?: {
    poolEntryId: string
    poolEntryRevisionId: string
    status: "ACTIVE" | "PAUSED" | "DISABLED"
    salesVisiblePrice: string
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
  supplyMode: "DROPSHIP" | "BULK"
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
  supplyPriceGross: string
  floorPriceGross: string
  supplyMode: "DROPSHIP" | "BULK"
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

export type PublicationImpactView = Readonly<{
  activePublicationCount: number
  pausedPublicationCount: number
  historicalPaidOrderCount: number
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
  leaseVersion?: number
  leaseExpiresAt?: string
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

export type SupplierCatalogItemBase = {
  queuePosition?: { current: number; total: number; snapshotId: string }
  supplierProduct: {
    id: string
    supplier: { id: string; name: string }
    source: SupplierCatalogSourceView
    /** 供应商自己的 SPU/SKU 身份；无供应商编码时由 ERP 生成内部来源身份。 */
    supplierSpuCode?: string
    supplierSkuCode: string
    status: string
    currentRevision: SupplierProductRevisionView
    incomingRevision?: SupplierProductRevisionView
  }
  mapping?: SupplierProductMappingView
  skuCandidates: SkuCandidateView[]
  offering?: {
    stableId: string
    currentRevision?: SupplierOfferingRevisionView
    revisionHistory: readonly SupplierOfferingRevisionView[]
    proposedDefaults?: SafeOfferingDraftView
  }
  /** 被采购选入公司商品池后形成；销售只消费该投影，不读取采购成本。 */
  poolEntry?: {
    poolEntryId: string
    poolEntryRevisionId: string
    status: "ACTIVE" | "PAUSED" | "DISABLED"
    salesVisiblePrice: string
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
  costFieldVisibility: CostFieldVisibility
}

export type SupplierCatalogItemView =
  | (SupplierCatalogItemBase & {
      changeType: "ERROR" | "STOPPED"
      workItem: SupplierCatalogExceptionWorkItem
      registrationBlocker?: never
    })
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
  /** demo 角色切换 */
  demoRole?: DemoRole
  /** demo：无成本字段权限 */
  maskCost?: boolean
  status?: "pending" | "held"
}

export type SupplierCatalogQueueView = Readonly<{
  preferences: { autoNextDefault: boolean }
  /**
   * 从商品中心进入时的 SKU 上下文。
   * 它来自商品主档，不能从供给关系结果反推，否则无关系时会丢失商品身份。
   */
  skuContext?: {
    productId: string
    productName: string
    skuId: string
    skuCode: string
    specification: string
    baseUnit: string
    poolEntry?: {
      poolEntryId: string
      poolEntryRevisionId: string
      status: "ACTIVE" | "PAUSED" | "DISABLED"
      salesVisiblePrice: string
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
  role: DemoRole
  costFieldVisibility: CostFieldVisibility
}>

export type SupplierCatalogCenterView = Readonly<{
  item: SupplierCatalogItemView
  section: string
  role: DemoRole
  costFieldVisibility: CostFieldVisibility
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
  expiresAt: string
  leaseVersion: number
  claimToken: string
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
  | { status: "unknown"; message: string; idempotencyKey: string }

/** 会话内映射/供给草稿（非正式） */
export type SessionCatalogDraft = Readonly<{
  supplierProductId: string
  selectedSkuId?: string
  offeringDraft?: SafeOfferingDraftView
  substituteCandidateSkuIds?: string[]
  note?: string
  updatedAt: string
}>

/** 与公司商品同构的内容字段 + 供应商独有供给观察字段。 */
export type SupplierCatalogContentFields = Readonly<{
  name: string
  description?: string
  specification: string
  category: string
  brand?: string
  sourceBaseUnit?: string
  barcode?: string
  attributes: readonly SupplierCatalogAttributeView[]
  media: readonly Omit<SupplierCatalogMediaView, "id">[]
  /** 供应商原始报价快照；不因采购确认而被覆盖。 */
  sourceQuotedPriceGross: string
  inputTaxRate: string
  freightAmount?: string
  otherFeeAmount?: string
  supplyRegion: string[]
  availableQuantity?: string
  availabilityStatus?: "AVAILABLE" | "UNAVAILABLE" | "STOPPED" | "STALE"
  expectedShipTime?: string
  afterSalesNote?: string
  capabilitySnapshot?: readonly string[]
}>

/** Excel/API/手工三种来源共用的供应商商品录入命令。 */
export type CreateSupplierCatalogItemInput = SupplierCatalogContentFields &
  Readonly<{
    sourceType: SupplierCatalogSourceType
    supplierId: string
    supplierName: string
    supplierSpuCode?: string
    supplierSkuCode: string
    /** 固定公司 SKU 入口使用；形成供给修订，销售侧不可见。 */
    confirmedCostGross?: string
    sourceReference?: string
    targetSkuId?: string
    targetSkuCode?: string
    targetSkuName?: string
    targetSpecification?: string
    baseUnit?: string
    salesVisiblePrice?: string
    /** 已有商品池时默认沿用；只有显式 SET_PRICE 才允许形成新修订。 */
    poolPriceAction?: "KEEP_EXISTING" | "SET_PRICE"
    minimumOrderQuantity: string
    supplyMode: "DROPSHIP" | "BULK"
    validFrom: string
    idempotencyKey: string
  }>

/**
 * 在供应商商品中心保存内容：形成新的来源修订（不可变），
 * 不写公司 SKU / 商品池 / 供给确认成本。
 */
export type ReviseSupplierCatalogProductInput = SupplierCatalogContentFields &
  Readonly<{
    supplierProductId: string
    expectedSourceRevisionNo: number
    supplierSpuCode?: string
    supplierSkuCode: string
    changeReason: string
    idempotencyKey: string
  }>

/** 采购把供应商 SKU 选入公司商品池，并同时确认供给成本。 */
export type PromoteSupplierProductInput = Readonly<{
  supplierProductId: string
  targetSkuId: string
  targetSkuCode: string
  targetSkuName: string
  specification: string
  baseUnit: string
  /** 采购私密成本；与来源报价和销售可见价均为不同事实。 */
  confirmedCostGross: string
  inputTaxRate: string
  minimumOrderQuantity: string
  supplyMode: "DROPSHIP" | "BULK"
  supplyRegion: string[]
  validFrom: string
  salesVisiblePrice?: string
  poolPriceAction: "KEEP_EXISTING" | "SET_PRICE"
  expectedSourceRevisionNo: number
  expectedPoolEntryRevisionId?: string
  idempotencyKey: string
}>

export type SupplierCatalogWriteResult = Readonly<{
  supplierProductId: string
  supplierOfferingRevisionId?: string
  poolEntryRevisionId?: string
  poolEntryChange: "NONE" | "CREATED" | "REVISED" | "UNCHANGED"
  activeSupplierCount?: number
  reference: string
  recordedAt: string
}>

export const CHANGE_TYPE_LABEL: Record<ChangeType, string> = {
  NEW: "新供应商商品",
  CHANGED: "关键变化",
  STOPPED: "停止供应",
  ERROR: "异常数据",
  UNCHANGED: "无变化",
}

export const DEMO_ROLE_LABEL: Record<DemoRole, string> = {
  procurement: "采购",
  operations: "运营",
  admin: "系统管理员",
  ops_tech: "研发运维",
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
  "替代供给选定人与恢复发布责任链尚未确认（Q3）。服务端固定返回 RECOVERY_RESPONSIBILITY_UNCONFIRMED：可准备会话内候选证据，不得选定替代供给或从供应商商品库发起商品发布恢复。"

export const COST_MASK = "***"
