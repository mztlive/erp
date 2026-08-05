/**
 * W22 商品发布 · 客户端契约
 * 对齐 docs/ui-workspaces/w22-product-publication.md §5 / §8
 */

export type SafetyPauseCause =
  | "SUPPLIER_STOPPED"
  | "ZERO_INVENTORY"
  | "SUPPLY_UNAVAILABLE"
  | "AVAILABILITY_STALE"
  | "COST_CHANGE_UNCONFIRMED"
  | "CRITICAL_SUPPLY_CHANGE_UNCONFIRMED"

export type SafetyPauseFollowUpWorkItemRef = {
  workItemId: string
  workItemType: "BUSINESS_EXCEPTION"
  businessObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
  businessObjectId: string
  subjectVersion: string
  subjectHash: string
  handlerKey: string
}

export type SafetyPauseNoTaskBlocker = {
  code: "NO_MANUAL_FOLLOW_UP_TASK_BY_CURRENT_POLICY"
  message: string
  evidenceReference: string
}

export type SafetyPauseReviewRegistrationBlocker = {
  code: "NORMAL_REVIEW_WORK_ITEM_TYPE_UNREGISTERED"
  message: string
  evidenceReference: string
}

export type SafetyPauseFollowUpBlocker =
  | SafetyPauseNoTaskBlocker
  | SafetyPauseReviewRegistrationBlocker

export type SafetyPauseAffectedPublicationView =
  | {
      publicationId: string
      pauseArtifactKind: "REVISION"
      pauseRevisionId: string
      deliveryId: string
      outboxMessageId: string
    }
  | {
      publicationId: string
      pauseArtifactKind: "ACTION"
      pauseActionId: string
      deliveryId: string
      outboxMessageId: string
    }

export type KnownSafetyPauseOperationBase = {
  operationId: string
  resultStatus: "COMMITTED" | "ALREADY_SAFE"
  sourceObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
  sourceObjectId: string
  sourceVersion: string
  subjectHash: string
  availabilityEffect: "PAUSED"
  affectedPublications: [
    SafetyPauseAffectedPublicationView,
    ...SafetyPauseAffectedPublicationView[],
  ]
  committedAt: string
}

export type SystemSafetyPauseOperationView =
  | (KnownSafetyPauseOperationBase & {
      cause: "SUPPLIER_STOPPED"
      followUpWorkItem: SafetyPauseFollowUpWorkItemRef
      followUpBlocker?: never
    })
  | (KnownSafetyPauseOperationBase & {
      cause: "ZERO_INVENTORY" | "SUPPLY_UNAVAILABLE" | "AVAILABILITY_STALE"
      followUpWorkItem?: never
      followUpBlocker: SafetyPauseNoTaskBlocker
    })
  | (KnownSafetyPauseOperationBase & {
      cause:
        | "COST_CHANGE_UNCONFIRMED"
        | "CRITICAL_SUPPLY_CHANGE_UNCONFIRMED"
      followUpWorkItem?: never
      followUpBlocker: SafetyPauseReviewRegistrationBlocker
    })
  | {
      operationId: string
      resultStatus: "UNKNOWN"
      cause: SafetyPauseCause
      sourceObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
      sourceObjectId: string
      sourceVersion: string
      subjectHash: string
      originalIdempotencyKey: string
      availabilityEffect: "FAIL_CLOSED_PENDING_RESULT"
      affectedPublications?: never
      followUpWorkItem?: never
      followUpBlocker?: never
      committedAt?: never
    }

export type PublicationCreationBlocker = {
  code: "PUBLICATION_IDENTITY_POLICY_UNCONFIRMED"
  message: string
}

export type PublicationPublishGate =
  | {
      kind: "READY"
      gateVersion: string
      submissionKind: "NORMAL"
      priceOrTaxChanged: boolean
      policyVersion: string
      reviewDisposition: "NOT_REQUIRED" | "SATISFIED"
      reviewEvidenceReference?: string
    }
  | {
      kind: "REVIEW_POLICY_UNCONFIGURED"
      gateVersion: string
      submissionKind: "NORMAL"
      priceOrTaxChanged: true
      blocker: { code: "REVIEW_POLICY_UNCONFIGURED"; message: string }
    }
  | {
      kind: "REVIEW_BLOCKED"
      gateVersion: string
      submissionKind: "NORMAL"
      priceOrTaxChanged: boolean
      policyVersion: string
      blocker: {
        code: "REVIEW_REQUIRED" | "REVIEW_PENDING" | "REVIEW_REJECTED"
        message: string
      }
    }
  | {
      kind: "RECOVERY_RESPONSIBILITY_UNCONFIRMED"
      gateVersion: string
      submissionKind: "RECOVERY"
      blocker: {
        code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED"
        message: string
      }
    }

export type PublicationStatus =
  | "DRAFT"
  | "PENDING_PUBLISH"
  | "MALL_LIVE"
  | "PAUSED"
  | "SAFETY_PAUSED"
  | "INVALID"

export type DeliveryStatus =
  | "PENDING_SEND"
  | "SENDING"
  | "RETRYING"
  | "ACKED"
  | "FAILED"
  | "HANDOFF"

export type SaleStatus = "ON_SALE" | "OFF_SALE" | "PAUSED"

export type ProductPublicationListQuery = {
  q?: string
  skuId?: string
  supplierOfferingRevisionId?: string
  mallId?: string
  /** 发布状态：默认有效对象 */
  publicationStatus?: string
  /** 投递状态快捷筛选：pending_confirm / failed / handoff / all */
  deliveryStatus?: string
  /** 指标快捷：pending_confirm | failed_handoff | mall_live | paused | all */
  metric?: string
  categoryId?: string
  page?: number
  pageSize?: number
}

export type FixedOfferingSummary = {
  offeringRevisionId: string
  supplierName: string
  availability: string
  availabilityLabel: string
  /** 供货价是否可见（掩码时不返回金额） */
  supplyPriceVisible: boolean
  supplyPriceGross?: string
  /** 供应商 MOQ — 仅展示，不得自动写入最小购买量 */
  supplierMoq?: string
}

export type ProductPublicationRow = {
  publicationId: string
  publicationCode: string
  skuId: string
  skuCode: string
  productName: string
  specification: string
  targetMallId: string
  targetMallName: string
  publicationStatus: PublicationStatus
  publicationStatusLabel: string
  publicationStatusTone:
    | "success"
    | "warning"
    | "destructive"
    | "info"
    | "neutral"
  currentAckedRevisionId?: string
  currentAckedRevisionNo?: number
  latestRevisionId?: string
  latestRevisionNo?: number
  /** 最新修订与商城生效版不同时为 true */
  hasPendingConfirmation: boolean
  salesPriceGross?: string
  salesTaxRate?: string
  fixedOffering: FixedOfferingSummary
  safetyPause?: SystemSafetyPauseOperationView
  latestDelivery?: {
    deliveryId: string
    status: DeliveryStatus
    statusLabel: string
    statusTone:
      | "success"
      | "warning"
      | "destructive"
      | "info"
      | "neutral"
    attemptCount: number
    mallAckAt?: string
    errorSummary?: string
  }
  ownerLabel: string
  updatedAt: string
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

export type ProductPublicationListResult = {
  items: ProductPublicationRow[]
  total: number
  page: number
  pageSize: number
  metrics: {
    pendingPublish: number
    pendingConfirm: number
    failedOrHandoff: number
    mallLive: number
    paused: number
  }
  permissionVersion: string
  dataScopeVersion: string
  queriedAt: string
  creationBlocker: PublicationCreationBlocker
  filterSummary: string
  emptyReason?: "NO_DATA" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
  /** 深链筛选的业务显示名（SKU 编号 / 供给供应商名），界面不得展示原始 ID */
  resolvedFilters: {
    skuCode?: string
    supplierName?: string
  }
}

export type PublicationMediaItem = {
  fileAssetId: string
  mediaRole: "MAIN" | "CAROUSEL" | "DETAIL"
  sortNo: number
  altText: string
  thumbnailUrl: string
  securityScanStatus: "PASSED" | "PENDING" | "FAILED"
}

export type ProductPublicationRevisionView = {
  revisionId: string
  revisionNo: number
  skuRevisionId: string
  supplierOfferingRevisionId: string
  fixedOffering: FixedOfferingSummary
  categoryId: string
  categoryLabel: string
  name: string
  specification: string
  salesDescription: string
  minimumPurchaseQuantity: string
  salesPriceGross: string
  salesTaxRate: string
  baseUnitCode: string
  /** 新修订使用结构化区域；旧种子可仅保留 label 兼容展示。 */
  salesRegion?: string[]
  salesRegionLabel: string
  saleStatus: SaleStatus
  saleStatusLabel: string
  productCapabilities: string[]
  validFrom: string
  validTo?: string
  contentHash: string
  media: PublicationMediaItem[]
  createdAt: string
  createdBy: string
}

export type PublicationDeliveryView = {
  deliveryId: string
  revisionId: string
  revisionNo: number
  targetMallId: string
  status: DeliveryStatus
  statusLabel: string
  statusTone:
    | "success"
    | "warning"
    | "destructive"
    | "info"
    | "neutral"
  attemptCount: number
  lastAttemptAt?: string
  mallAckAt?: string
  mallVersion?: string
  errorCode?: string
  errorSummary?: string
}

export type ProductPublicationView = {
  identity: {
    publicationId: string
    publicationCode: string
    skuId: string
    skuCode: string
    targetMallId: string
    targetMallName: string
  }
  status: PublicationStatus
  statusLabel: string
  statusTone:
    | "success"
    | "warning"
    | "destructive"
    | "info"
    | "neutral"
  currentAckedRevisionId?: string
  currentAckedRevisionNo?: number
  latestRevisionId?: string
  latestRevisionNo?: number
  selectedRevision: ProductPublicationRevisionView
  revisions: Array<{
    revisionId: string
    revisionNo: number
    saleStatus: SaleStatus
    saleStatusLabel: string
    createdAt: string
    createdBy: string
    contentHash: string
    deliverySummary: string
    isMallAcked: boolean
    isLatest: boolean
  }>
  deliveries: PublicationDeliveryView[]
  safetyPause?: SystemSafetyPauseOperationView
  publishGate: PublicationPublishGate
  freshness: { queriedAt: string; integrationUpdatedAt: string }
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  fieldPermissions: Record<string, "full" | "masked" | "hidden">
  objectVersion: string
  ownerLabel: string
}

export type PublishRevisionCommand = {
  publicationId: string
  expectedObjectVersion: string
  expectedPublishGateVersion: string
  requestId: string
  content: {
    skuRevisionId: string
    supplierOfferingRevisionId: string
    categoryId: string
    name: string
    specification: string
    salesDescription: string
    minimumPurchaseQuantity: string
    salesPriceGross: string
    salesTaxRate: string
    baseUnitCode: string
    salesRegion: string[]
    saleStatus: SaleStatus
    productCapabilities: string[]
    validFrom: string
    validTo?: string
    media: Array<{
      fileAssetId: string
      mediaRole: "MAIN" | "CAROUSEL" | "DETAIL"
      sortNo: number
      altText: string
    }>
  }
  /** 演示：强制 UNKNOWN 一次 */
  forceUnknown?: boolean
}

export type PublishRevisionResult =
  | {
      status: "succeeded"
      operationId: string
      publicationId: string
      revisionId: string
      revisionNo: number
      deliveryId: string
      deliveryStatus: "PENDING_SEND"
      committedAt: string
    }
  | {
      status: "blocked"
      code:
        | "REVIEW_POLICY_UNCONFIGURED"
        | "RECOVERY_RESPONSIBILITY_UNCONFIRMED"
        | "REVIEW_BLOCKED"
        | "GATE_VERSION_MISMATCH"
        | "OBJECT_VERSION_CONFLICT"
        | "VALIDATION_FAILED"
      message: string
      publishGate?: PublicationPublishGate
    }
  | {
      status: "unknown"
      requestId: string
      message: string
    }

export type ManualPauseCommand = {
  publicationId: string
  expectedObjectVersion: string
  requestId: string
  reason: string
}

export type ManualPauseResult =
  | {
      status: "succeeded"
      revisionId: string
      revisionNo: number
      deliveryId: string
      committedAt: string
    }
  | { status: "blocked"; code: string; message: string }
  | { status: "unknown"; requestId: string; message: string }

export type RetryDeliveryCommand = {
  publicationId: string
  deliveryId: string
  requestId: string
}

export type RetryDeliveryResult =
  | {
      status: "succeeded"
      deliveryId: string
      attemptCount: number
      deliveryStatus: DeliveryStatus
    }
  | { status: "blocked"; code: string; message: string }
  | { status: "unknown"; requestId: string; message: string }

export type ResolvePublishUnknownCommand = {
  requestId: string
  settle?: boolean
}

/** 仅供 mock 领域事件处理器调用；浏览器页面不得构造。 */
export type SystemSafetyPauseTrigger = {
  cause: SafetyPauseCause
  sourceObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
  sourceObjectId: string
  sourceVersion: string
  subjectHash: string
  affectedPublicationIds: string[]
  occurredAt: string
  idempotencyKey: string
}

export const PUBLICATION_STATUS_LABEL: Record<PublicationStatus, string> = {
  DRAFT: "草稿",
  PENDING_PUBLISH: "待发布",
  MALL_LIVE: "商城已生效",
  PAUSED: "已暂停",
  SAFETY_PAUSED: "安全暂停",
  INVALID: "已失效",
}

export const PUBLICATION_STATUS_TONE: Record<
  PublicationStatus,
  "success" | "warning" | "destructive" | "info" | "neutral"
> = {
  DRAFT: "neutral",
  PENDING_PUBLISH: "info",
  MALL_LIVE: "success",
  PAUSED: "warning",
  SAFETY_PAUSED: "destructive",
  INVALID: "neutral",
}

export const DELIVERY_STATUS_LABEL: Record<DeliveryStatus, string> = {
  PENDING_SEND: "待发送",
  SENDING: "发送中",
  RETRYING: "重试中",
  ACKED: "已确认",
  FAILED: "失败",
  HANDOFF: "转人工",
}

export const DELIVERY_STATUS_TONE: Record<
  DeliveryStatus,
  "success" | "warning" | "destructive" | "info" | "neutral"
> = {
  PENDING_SEND: "info",
  SENDING: "info",
  RETRYING: "warning",
  ACKED: "success",
  FAILED: "destructive",
  HANDOFF: "warning",
}

export const SALE_STATUS_LABEL: Record<SaleStatus, string> = {
  ON_SALE: "上架",
  OFF_SALE: "下架",
  PAUSED: "暂停下单",
}

export const SAFETY_PAUSE_CAUSE_LABEL: Record<SafetyPauseCause, string> = {
  SUPPLIER_STOPPED: "供应商停供",
  ZERO_INVENTORY: "零库存",
  SUPPLY_UNAVAILABLE: "明确不可供",
  AVAILABILITY_STALE: "可供数据过期",
  COST_CHANGE_UNCONFIRMED: "成本变化未确认",
  CRITICAL_SUPPLY_CHANGE_UNCONFIRMED: "关键供给变化未确认",
}

export const MEDIA_ROLE_LABEL: Record<
  PublicationMediaItem["mediaRole"],
  string
> = {
  MAIN: "主图",
  CAROUSEL: "轮播图",
  DETAIL: "详情图",
}

export const MEDIA_SCAN_STATUS_LABEL: Record<
  PublicationMediaItem["securityScanStatus"],
  string
> = {
  PASSED: "已通过",
  PENDING: "检查中",
  FAILED: "未通过",
}

/** 安全暂停后续任务类型中文名（禁止枚举原值上屏）。 */
export const WORK_ITEM_TYPE_LABEL: Record<string, string> = {
  BUSINESS_EXCEPTION: "业务异常",
}
