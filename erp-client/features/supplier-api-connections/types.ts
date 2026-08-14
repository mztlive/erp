/**
 * W20 · API 供应商连接 · 客户端契约
 * 对齐 docs/ui-workspaces/w20-supplier-api-connections.md §5/§7/§8
 */

import type { StatusTone } from "@/components/ui/status-badge"

export type ConnectionEnvironment = "DEVELOPMENT" | "STAGING" | "PRODUCTION"

export type ConnectionStatus =
    | "ENABLED"
    | "DISABLED"
    | "FAULTED"
    | "PENDING_CONFIG"

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

type CapabilityStatus = "ENABLED" | "DISABLED"

type CapabilityVerification = "SUCCESS" | "FAILED" | "UNVERIFIED" | "STALE"

export type ConnectionSection =
    | "overview"
    | "capabilities"
    | "security"
    | "health"
    | "catalog"
    | "related"
    | "audit"

type EmptyReason =
    | "NO_PERMISSION"
    | "NO_SCOPE"
    | "NO_CONNECTIONS"
    | "FILTER_NO_RESULT"

type ActionBlocker = {
    action: string
    code: string
    message: string
    destinationWorkspaceId?: string
}

type SafeReferenceView = {
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
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
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

type CatalogSyncView = {
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

type RelatedImpactView = {
    activeOfferings: number
    activePublications: number
    openSupplierOrders: number
    activeSyncJobs: number
}

type AuditEventView = {
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
    /** 服务端固定下发的只读健康检查类型白名单。 */
    healthCheckTypes: Array<
        "CONNECTIVITY" | "AUTHENTICATION" | "CAPABILITY_METADATA"
    >
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

type ConnectionListMetrics = {
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
    hasModulePermission: boolean
    hasDataScope: boolean
    projectedAt: string
    credentialOpaqueOptions: Array<{
        referenceId: string
        alias: string
        version: string
    }>
    endpointOpaqueOptions: Array<{
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
          /** 新建成功后的连接内部 id，仅用于页面跳转，不展示 */
          connectionId?: string
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
    health: "健康检查",
    catalog: "目录同步",
    related: "关联业务",
    audit: "审计",
}

/** 审计动作中文映射（动作码只在代码与审计数据结构中使用） */
export const AUDIT_ACTION_LABEL: Record<string, string> = {
    CREATE_CONNECTION: "创建连接",
    BIND_ENDPOINT_REFERENCE: "绑定地址引用",
    BIND_CREDENTIAL_REFERENCE: "绑定/轮换密钥引用",
    CONFIRM_CAPABILITY_REQUIREMENT: "确认业务能力需求",
    UPDATE_CAPABILITIES: "配置能力",
    RUN_HEALTH_CHECK: "执行健康检查",
    START_CATALOG_SYNC: "触发目录同步",
    DISABLE: "停用连接",
    ENABLE: "启用连接",
    HEALTH_AUTH_FAILED: "鉴权/签名失败",
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
