/**
 * W20 · API 供应商连接 · 客户端契约
 * 对齐 docs/ui-workspaces/w20-supplier-api-connections.md §5/§7/§8
 */

import type { StatusTone } from "@/components/ui/status-badge"

export type DemoRole = "procurement" | "ops" | "admin"

export type ConnectionEnvironment = "DEVELOPMENT" | "STAGING" | "PRODUCTION"

export type ConnectionStatus = "ENABLED" | "DISABLED" | "FAULTED" | "PENDING_CONFIG"

export type HealthResult =
  | "SUCCESS"
  | "FAILED"
  | "PARTIAL"
  | "UNCHECKED"
  | "STALE"
  | "AUTH_FAILED"
  | "UNKNOWN"

export type CatalogFreshnessState =
  | "FRESH"
  | "STALE"
  | "RUNNING"
  | "FAILED"
  | "NEVER"

export type ReferenceState = "MISSING" | "BOUND" | "ROTATION_DUE"

export type CapabilityCode =
  | "CATALOG"
  | "PRICE"
  | "STOCK"
  | "ORDER"
  | "QUERY"
  | "CANCEL"
  | "REFUND"
  | "LOGISTICS"
  | "CALLBACK"
  | "SETTLEMENT"

export type CapabilityStatus = "ENABLED" | "DISABLED"

export type CapabilityVerification =
  | "SUCCESS"
  | "FAILED"
  | "UNVERIFIED"
  | "STALE"

export type ConnectionSection =
  | "overview"
  | "capabilities"
  | "security"
  | "health"
  | "catalog"
  | "related"
  | "audit"

export type EmptyReason =
  | "NO_PERMISSION"
  | "NO_SCOPE"
  | "NO_CONNECTIONS"
  | "FILTER_NO_RESULT"

export type ActionBlocker = {
  action: string
  code: string
  message: string
  destinationWorkspaceId?: string
}

export type SafeReferenceView = {
  state: ReferenceState
  /** 仅技术角色可见；采购只看就绪文案 */
  alias?: string
  version?: string
  visible: boolean
}

export type CapabilityView = {
  capabilityCode: CapabilityCode
  capabilityLabel: string
  status: CapabilityStatus
  statusLabel: string
  constraintSummary?: string
  verification: CapabilityVerification
  verificationLabel: string
  verifiedAt?: string
  businessRequirement?: "REQUIRED" | "NOT_REQUIRED" | "UNCONFIRMED"
  businessRequirementLabel: string
  version: string
  /** 连接级能力 ≠ 商品级可用 */
  productLevelNote: string
}

export type HealthRecordView = {
  recordId: string
  at: string
  checkType: string
  result: HealthResult
  resultLabel: string
  resultTone: StatusTone
  latencyMs?: number
  errorClass?: string
  errorSummary?: string
  autoRetryStopped?: boolean
  traceId?: string
  jobId?: string
  jobNo?: string
}

export type CatalogSyncView = {
  state: CatalogFreshnessState
  stateLabel: string
  lastSuccessfulAt?: string
  activeJobId?: string
  activeJobNo?: string
  progress?: {
    status: "queued" | "running" | "succeeded" | "partial" | "failed"
    total: number
    completed: number
    succeeded: number
    failed: number
  }
}

export type RelatedImpactView = {
  activeOfferings: number
  activePublications: number
  openSupplierOrders: number
  activeSyncJobs: number
}

export type AuditEventView = {
  eventId: string
  at: string
  actor: string
  action: string
  summary: string
  auditNo?: string
}

export type ConnectionListItem = {
  connectionId: string
  connectionCode: string
  supplier: { id: string; name: string }
  environment: ConnectionEnvironment
  environmentLabel: string
  status: ConnectionStatus
  statusLabel: string
  statusTone: StatusTone
  capabilitySummary: string
  healthResult: HealthResult
  healthLabel: string
  healthTone: StatusTone
  lastHealthAt?: string
  catalogState: CatalogFreshnessState
  catalogLabel: string
  nextStep: string
  businessOwner?: string
  technicalOwner?: string
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
}

export type ConnectionCenterView = {
  connectionId: string
  connectionCode: string
  supplier: { id: string; name: string }
  environment: ConnectionEnvironment
  environmentLabel: string
  status: ConnectionStatus
  statusLabel: string
  statusTone: StatusTone
  businessOwner?: { id: string; label: string }
  technicalOwner?: { id: string; label: string }
  adapter?: { code: string; version: string; visible: boolean }
  version: string
  updatedAt: string
  safeReferences: {
    endpoint: SafeReferenceView
    credential: SafeReferenceView
  }
  capabilities: CapabilityView[]
  lastHealth?: {
    at: string
    result: HealthResult
    resultLabel: string
    latencyMs?: number
    traceId?: string
    autoRetryStopped?: boolean
    errorClass?: string
    errorSummary?: string
  }
  healthRecords: HealthRecordView[]
  catalog: CatalogSyncView
  relatedImpact: RelatedImpactView
  auditEvents: AuditEventView[]
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  /** 鉴权/签名失败等高风险告警 */
  alerts: Array<{
    id: string
    severity: "destructive" | "warning" | "info"
    title: string
    description: string
  }>
  nextStep: string
}

export type ConnectionListMetrics = {
  enabled: number
  faulted: number
  pendingConfig: number
  healthAbnormal: number
  catalogStale: number
}

export type ConnectionListView = {
  metrics: ConnectionListMetrics
  items: ConnectionListItem[]
  total: number
  page: number
  pageSize: number
  emptyReason?: EmptyReason
  viewerRole: DemoRole
  viewerRoleLabel: string
  hasModulePermission: boolean
  hasDataScope: boolean
  projectedAt: string
  credentialOpaqueOptions: Array<{
    referenceId: string
    alias: string
    version: string
  }>
}

export type FormalOutcome =
  | {
      status: "succeeded"
      title: string
      message: string
      reference?: string
      facts?: Array<{ label: string; value: string }>
      jobId?: string
      jobNo?: string
      connectionVersion?: string
      auditEventId?: string
    }
  | {
      status: "failed" | "blocked" | "rejected"
      code: string
      title: string
      message: string
      reference?: string
    }
  | {
      status: "unknown"
      title: string
      message: string
      operationId: string
      idempotencyKey: string
    }
  | {
      status: "processing"
      title: string
      message: string
      jobId: string
      jobNo: string
    }

export const DEMO_ROLE_LABEL: Record<DemoRole, string> = {
  procurement: "采购",
  ops: "研发运维",
  admin: "系统管理员",
}

export const ENVIRONMENT_LABEL: Record<ConnectionEnvironment, string> = {
  DEVELOPMENT: "开发",
  STAGING: "测试",
  PRODUCTION: "生产",
}

export const STATUS_LABEL: Record<ConnectionStatus, string> = {
  ENABLED: "启用",
  DISABLED: "停用",
  FAULTED: "故障",
  PENDING_CONFIG: "待配置",
}

export const STATUS_TONE: Record<ConnectionStatus, StatusTone> = {
  ENABLED: "success",
  DISABLED: "neutral",
  FAULTED: "destructive",
  PENDING_CONFIG: "warning",
}

export const HEALTH_LABEL: Record<HealthResult, string> = {
  SUCCESS: "成功",
  FAILED: "失败",
  PARTIAL: "部分能力失败",
  UNCHECKED: "未检查",
  STALE: "检查陈旧",
  AUTH_FAILED: "鉴权/签名失败",
  UNKNOWN: "结果未知",
}

export const HEALTH_TONE: Record<HealthResult, StatusTone> = {
  SUCCESS: "success",
  FAILED: "destructive",
  PARTIAL: "warning",
  UNCHECKED: "neutral",
  STALE: "warning",
  AUTH_FAILED: "destructive",
  UNKNOWN: "warning",
}

export const CATALOG_LABEL: Record<CatalogFreshnessState, string> = {
  FRESH: "目录新鲜",
  STALE: "目录陈旧",
  RUNNING: "同步中",
  FAILED: "同步失败",
  NEVER: "从未同步",
}

export const CAPABILITY_LABEL: Record<CapabilityCode, string> = {
  CATALOG: "商品目录",
  PRICE: "价格",
  STOCK: "库存",
  ORDER: "下单",
  QUERY: "查询",
  CANCEL: "取消",
  REFUND: "退款",
  LOGISTICS: "物流",
  CALLBACK: "通知",
  SETTLEMENT: "结算",
}

export const REFERENCE_STATE_LABEL: Record<ReferenceState, string> = {
  MISSING: "未绑定",
  BOUND: "已绑定",
  ROTATION_DUE: "需轮换",
}

export const SECTION_LABEL: Record<ConnectionSection, string> = {
  overview: "概览",
  capabilities: "能力",
  security: "安全配置引用",
  health: "健康与尝试",
  catalog: "目录同步",
  related: "关联业务",
  audit: "审计",
}

export const SECTIONS: ConnectionSection[] = [
  "overview",
  "capabilities",
  "security",
  "health",
  "catalog",
  "related",
  "audit",
]
