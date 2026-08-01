/**
 * 外部商品供给映射与供给 · 客户端契约类型
 * 对齐 docs/ui-workspaces/w21-external-product-supply.md §5/§7/§8
 */

export type ChangeType = "NEW" | "CHANGED" | "STOPPED" | "ERROR" | "UNCHANGED"

export type DemoRole = "procurement" | "operations" | "admin" | "ops_tech"

export type CostFieldVisibility = "visible" | "masked"

export type ExternalProductRevisionView = Readonly<{
  revisionNo: number
  externalRevisionToken?: string
  sourceUpdatedAt: string
  syncedAt: string
  name: string
  specification: string
  category: string
  /** 定点字符串；无成本权时 API 可返回 null 并由 UI 掩码 */
  supplyPriceGross: string | null
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
  skuId: string
  skuCode: string
  skuName: string
  specification: string
  baseUnit: string
  revisionNo: number
  similarityLabel: string
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
  revisionNo: number
  status: "ACTIVE" | "PAUSED" | "STOPPED" | "PENDING_CONFIRM"
  supplyPriceGross: string | null
  supplyPriceNet: string | null
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

export type ExternalCatalogRegistrationBlocker = Readonly<{
  code: "WORK_ITEM_TYPE_UNREGISTERED"
  message: string
  businessProcess: "MAPPING" | "OFFERING_REVIEW"
}>

export type ExternalCatalogExceptionWorkItem = Readonly<{
  workItemId: string
  workItemType: "BUSINESS_EXCEPTION"
  businessObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
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

export type ExternalCatalogItemBase = {
  queuePosition?: { current: number; total: number; snapshotId: string }
  externalProduct: {
    id: string
    supplier: { id: string; name: string }
    connection: { id: string; code: string }
    externalProductId: string
    externalSkuId?: string
    status: string
    currentRevision: ExternalProductRevisionView
    incomingRevision?: ExternalProductRevisionView
  }
  mapping?: SupplierProductMappingView
  skuCandidates: SkuCandidateView[]
  offering?: {
    stableId: string
    currentRevision?: SupplierOfferingRevisionView
    revisionHistory: readonly SupplierOfferingRevisionView[]
    proposedDefaults?: SafeOfferingDraftView
  }
  publicationImpact: PublicationImpactView
  syncContext: {
    jobId: string
    sourceBatchIdentity: string
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

export type ExternalCatalogItemView =
  | (ExternalCatalogItemBase & {
      changeType: "ERROR" | "STOPPED"
      workItem: ExternalCatalogExceptionWorkItem
      registrationBlocker?: never
    })
  | (ExternalCatalogItemBase & {
      changeType: "NEW" | "CHANGED"
      workItem?: never
      registrationBlocker: ExternalCatalogRegistrationBlocker
    })
  | (ExternalCatalogItemBase & {
      changeType: "UNCHANGED"
      workItem?: never
      registrationBlocker?: never
    })

export type ExternalCatalogQueueQuery = {
  mode?: "queue" | "list"
  supplierId?: string
  connectionId?: string
  changeType?: "actionable" | "NEW" | "CHANGED" | "STOPPED" | "ERROR" | "all"
  mappingStatus?: string
  offeringStatus?: string
  publicationImpact?: string
  freshness?: string
  q?: string
  queueContextId?: string
  currentExternalProductId?: string
  currentWorkItemId?: string
  pageSize?: number
  sort?: string
  /** demo 角色切换 */
  demoRole?: DemoRole
  /** demo：无成本字段权限 */
  maskCost?: boolean
  status?: "pending" | "held"
}

export type ExternalCatalogQueueView = Readonly<{
  preferences: { autoNextDefault: boolean }
  context: {
    queueContextId: string
    position: number
    total: number
    currentExternalProductId?: string
    currentWorkItemId?: string
    previousExternalProductId?: string
    nextExternalProductId?: string
    filterSummary: string
    queueContextUpdatedAt: string
  }
  items: readonly ExternalCatalogItemView[]
  current?: ExternalCatalogItemView
  emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
  role: DemoRole
  costFieldVisibility: CostFieldVisibility
}>

export type ExternalCatalogCenterView = Readonly<{
  item: ExternalCatalogItemView
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

export type ExternalCatalogWorkItemAction =
  | { kind: "HOLD"; reasonCode: string; comment?: string }
  | {
      kind: "RETURN_FOR_DATA_FIX"
      reasonCode: string
      suggestedResponsibleRole?: string
      comment?: string
    }
  | { kind: "QUERY_ORIGINAL_RESULT"; operationId: string }
  | { kind: "SAVE_EVIDENCE"; evidenceReferences: string[]; comment?: string }

export type ExternalCatalogDecision =
  | {
      kind: "CONFIRM_ERROR_RESOLVED"
      expectedExternalRevision: string
      resolutionCode: string
      evidenceReferences?: string[]
      comment?: string
    }
  | {
      kind: "CONFIRM_STOP_SUPPLY"
      expectedExternalRevision: string
      expectedOfferingRevision?: string
      reasonCode: string
      comment?: string
    }

export type ExternalCatalogBusinessResult = Readonly<{
  decisionKind: ExternalCatalogDecision["kind"]
  externalProductId: string
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
      business: ExternalCatalogBusinessResult
    }

export type FormalActionResponse =
  | { status: "succeeded"; outcome: FormalOutcome }
  | { status: "failed"; message: string; code: string }
  | { status: "unknown"; message: string; idempotencyKey: string }

/** 会话内映射/供给草稿（非正式） */
export type SessionCatalogDraft = Readonly<{
  externalProductId: string
  selectedSkuId?: string
  offeringDraft?: SafeOfferingDraftView
  substituteCandidateSkuIds?: string[]
  note?: string
  updatedAt: string
}>

export const CHANGE_TYPE_LABEL: Record<ChangeType, string> = {
  NEW: "新外部商品",
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
  { value: "WAITING_MASTER_DATA", label: "等待主数据" },
  { value: "OTHER", label: "其他" },
] as const

export const RETURN_REASON_OPTIONS = [
  { value: "SOURCE_DATA_ERROR", label: "来源数据错误" },
  { value: "PAYLOAD_INVALID", label: "结构/字段无效" },
  { value: "SYNC_CORRUPT", label: "同步损坏" },
  { value: "OTHER", label: "其他" },
] as const

export const REGISTRATION_BLOCKER_MESSAGE =
  "外部商品映射/供给复核任务类型尚未登记。当前仅可浏览差异、准备会话草稿或进入主数据建档；不得领取或提交确认。安全暂停与不可下单状态不受影响。"

export const RECOVERY_BLOCKER_MESSAGE =
  "替代供给选定人与恢复发布责任链尚未确认（Q3）。服务端固定返回 RECOVERY_RESPONSIBILITY_UNCONFIRMED：可准备会话内候选证据，不得选定替代供给或从外部商品供给发起商品发布恢复。"

export const COST_MASK = "***"
