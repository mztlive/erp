/**
 * W20 · API 供应商连接 · 后端载荷 → 视图映射；只消费服务端事实，不在客户端伪造成功或递增版本。
 */

import { apiGet } from "@/lib/api"
import type {
    CapabilityCode,
    ConnectionCenterView,
    ConnectionEnvironment,
    ConnectionListItem,
    ConnectionStatus,
    HealthResult,
    ReferenceState,
} from "@/features/supplier-api-connections/types"
import {
    CAPABILITY_LABEL,
    CATALOG_LABEL,
    ENVIRONMENT_LABEL,
    HEALTH_LABEL,
    HEALTH_TONE,
    STATUS_LABEL,
    STATUS_TONE,
} from "@/features/supplier-api-connections/types"

export type BackendActionBlocker = {
    action: string
    code: string
    message: string
    destination_workspace_id?: string | null
}

export type BackendSafeReference = {
    state: string
    alias?: string | null
    version?: string | null
    visible: boolean
}

export type BackendConnection = {
    id: string
    supplier_id: string
    connection_code: string
    environment: "production" | "testing" | string
    status: "active" | "disabled" | "fault" | string
    rate_limit_policy?: { max_requests: number; window_secs: number } | null
    last_health_at?: number | null
    last_health_result?: "healthy" | "failed" | string | null
    safe_references: {
        endpoint: BackendSafeReference
        credential: BackendSafeReference
    }
    technical_config_version: number
    allowed_actions: string[]
    action_blockers: BackendActionBlocker[]
    version: number
    created_at: number
}

export type BackendCapability = {
    id: string
    connection_id: string
    capability_code: string
    status: "active" | "disabled" | string
    version: number
    created_at: number
    constraint_summary?: string | null
    business_requirement?: "REQUIRED" | "NOT_REQUIRED" | null
    business_confirmation_version?: number | null
    technically_verified?: boolean
    verified_at?: number | null
    allowed_actions?: string[]
    action_blockers?: BackendActionBlocker[]
}

export type BackendHealthCheckType =
    | "CONNECTIVITY"
    | "AUTHENTICATION"
    | "CAPABILITY_METADATA"

export type BackendHealthRun = {
    id: string
    job_id: string
    check_type: BackendHealthCheckType
    status: "PENDING" | "RUNNING" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
    technical_config_version: number
    requested_by: string
    started_at?: number | null
    finished_at?: number | null
    latency_ms?: number | null
    error_code?: string | null
    error_summary?: string | null
}

export type BackendConnectionDetail = BackendConnection & {
    capabilities: BackendCapability[]
    health_records: BackendHealthRun[]
    health_check_types: BackendHealthCheckType[]
    related_impact: {
        active_offerings: number
        active_publications: number
        open_supplier_orders: number
        active_sync_jobs: number
    }
}

export type BackendCommandResult = {
    outcome: "SUCCEEDED" | "PROCESSING" | "REJECTED" | "UNKNOWN"
    action: string
    operation_id: string
    connection_version: number
    job_id?: string | null
    job_no?: string | null
    audit_event_id: string
}

export type BackendCapabilityUpdateResult = {
    outcome: "SUCCEEDED" | "PROCESSING" | "REJECTED" | "UNKNOWN"
    operation_id: string
    connection_version: number
    capabilities: BackendCapability[]
    audit_event_id: string
}

export function secsToIso(secs?: number | null): string | undefined {
    if (secs == null || secs <= 0) return undefined
    return new Date(secs * 1000).toISOString()
}

function mapEnvironment(raw: string): ConnectionEnvironment {
    if (raw === "production") return "PRODUCTION"
    if (raw === "testing") return "STAGING"
    const upper = raw.toUpperCase()
    return upper === "DEVELOPMENT" ||
        upper === "STAGING" ||
        upper === "PRODUCTION"
        ? upper
        : "STAGING"
}

export function toBackendEnvironment(
    env: ConnectionEnvironment,
): "production" | "testing" {
    return env === "PRODUCTION" ? "production" : "testing"
}

function mapStatus(raw: string): ConnectionStatus {
    if (raw === "active") return "ENABLED"
    if (raw === "disabled") return "DISABLED"
    if (raw === "fault") return "FAULTED"
    return "PENDING_CONFIG"
}

function mapHealth(raw?: string | null): HealthResult {
    if (!raw) return "UNCHECKED"
    if (raw === "healthy") return "SUCCESS"
    if (raw === "failed") return "FAILED"
    return "UNKNOWN"
}

export function mapCapabilityCode(raw: string): CapabilityCode {
    const table: Record<string, CapabilityCode> = {
        product: "CATALOG",
        catalog: "CATALOG",
        price: "PRICE",
        stock: "STOCK",
        order: "ORDER",
        query: "QUERY",
        cancel: "CANCEL",
        refund: "REFUND",
        logistics: "LOGISTICS",
        callback: "CALLBACK",
        settlement: "SETTLEMENT",
    }
    return table[raw.toLowerCase()] ?? "CATALOG"
}

export function toBackendCapabilityCode(code: CapabilityCode): string {
    return code === "CATALOG" ? "product" : code.toLowerCase()
}

function mapBlocker(blocker: BackendActionBlocker) {
    return {
        action: blocker.action,
        code: blocker.code,
        message: blocker.message,
        destinationWorkspaceId: blocker.destination_workspace_id ?? undefined,
    }
}

function mapReference(reference: BackendSafeReference) {
    const state: ReferenceState =
        reference.state === "BOUND" || reference.state === "ROTATION_DUE"
            ? reference.state
            : "MISSING"
    return {
        state,
        alias: reference.alias ?? undefined,
        version: reference.version ?? undefined,
        visible: reference.visible,
    }
}

function mapCapability(capability: BackendCapability) {
    const code = mapCapabilityCode(capability.capability_code)
    const enabled = capability.status === "active"
    const requirement: "REQUIRED" | "NOT_REQUIRED" | "UNCONFIRMED" =
        capability.business_requirement ?? "UNCONFIRMED"
    const verified = capability.technically_verified === true
    return {
        capabilityCode: code,
        capabilityLabel: CAPABILITY_LABEL[code],
        status: (enabled ? "ENABLED" : "DISABLED") as "ENABLED" | "DISABLED",
        statusLabel: enabled ? "启用" : "停用",
        constraintSummary: capability.constraint_summary ?? undefined,
        verification: (verified ? "SUCCESS" : "UNVERIFIED") as
            | "SUCCESS"
            | "UNVERIFIED",
        verificationLabel: verified ? "已验证" : "未验证",
        verifiedAt: secsToIso(capability.verified_at),
        businessRequirement: requirement,
        businessRequirementLabel:
            requirement === "REQUIRED"
                ? "业务必需"
                : requirement === "NOT_REQUIRED"
                  ? "业务不需要"
                  : "未确认",
        version: String(capability.version),
        allowedActions: capability.allowed_actions ?? [],
        actionBlockers: (capability.action_blockers ?? []).map(mapBlocker),
        productLevelNote:
            "连接级能力声明 ≠ 每条供给可用；供给/发布级能力见供应商供给 / 商品发布",
    }
}

export function toListItem(
    connection: BackendConnection,
    capabilities: BackendCapability[],
    supplierName?: string,
): ConnectionListItem {
    const status = mapStatus(connection.status)
    const environment = mapEnvironment(connection.environment)
    const health = mapHealth(connection.last_health_result)
    const enabledCapabilities = capabilities.filter(
        (item) => item.status === "active",
    )
    return {
        connectionId: connection.id,
        connectionCode: connection.connection_code,
        supplier: {
            id: connection.supplier_id,
            name: supplierName ?? connection.supplier_id,
        },
        environment,
        environmentLabel: ENVIRONMENT_LABEL[environment],
        status,
        statusLabel: STATUS_LABEL[status],
        statusTone: STATUS_TONE[status],
        capabilitySummary:
            enabledCapabilities.length === 0
                ? "未配置能力"
                : enabledCapabilities
                      .map(
                          (item) =>
                              CAPABILITY_LABEL[
                                  mapCapabilityCode(item.capability_code)
                              ],
                      )
                      .join("、"),
        healthResult: health,
        healthLabel: HEALTH_LABEL[health],
        healthTone: HEALTH_TONE[health],
        lastHealthAt: secsToIso(connection.last_health_at),
        catalogState: "NEVER",
        catalogLabel: CATALOG_LABEL.NEVER,
        nextStep:
            connection.action_blockers[0]?.message ??
            (status === "ENABLED"
                ? "连接已启用"
                : "当前暂无可执行操作，请联系管理员确认连接配置"),
        allowedActions: connection.allowed_actions,
        actionBlockers: connection.action_blockers.map(mapBlocker),
    }
}

function healthResult(run: BackendHealthRun): HealthResult {
    if (run.status === "SUCCEEDED") return "SUCCESS"
    if (
        run.status === "UNKNOWN" ||
        run.status === "PENDING" ||
        run.status === "RUNNING"
    ) {
        return "UNKNOWN"
    }
    return run.error_code?.toLowerCase().includes("auth")
        ? "AUTH_FAILED"
        : "FAILED"
}

export function toCenter(
    detail: BackendConnectionDetail,
    supplierName?: string,
): ConnectionCenterView {
    const status = mapStatus(detail.status)
    const environment = mapEnvironment(detail.environment)
    const records = detail.health_records.map((run) => {
        const result = healthResult(run)
        return {
            recordId: run.id,
            at:
                secsToIso(run.finished_at ?? run.started_at) ??
                secsToIso(detail.created_at) ??
                new Date(0).toISOString(),
            checkType: run.check_type,
            result,
            resultLabel: HEALTH_LABEL[result],
            resultTone: HEALTH_TONE[result],
            latencyMs: run.latency_ms ?? undefined,
            errorClass: run.error_code ?? undefined,
            errorSummary: run.error_summary ?? undefined,
            autoRetryStopped: result === "AUTH_FAILED",
            jobId: run.job_id,
        }
    })
    const latest = records[0]
    const relatedImpact = {
        activeOfferings: detail.related_impact.active_offerings,
        activePublications: detail.related_impact.active_publications,
        openSupplierOrders: detail.related_impact.open_supplier_orders,
        activeSyncJobs: detail.related_impact.active_sync_jobs,
    }
    return {
        connectionId: detail.id,
        connectionCode: detail.connection_code,
        supplier: {
            id: detail.supplier_id,
            name: supplierName ?? detail.supplier_id,
        },
        environment,
        environmentLabel: ENVIRONMENT_LABEL[environment],
        status,
        statusLabel: STATUS_LABEL[status],
        statusTone: STATUS_TONE[status],
        version: String(detail.version),
        updatedAt: secsToIso(detail.created_at) ?? new Date(0).toISOString(),
        safeReferences: {
            endpoint: mapReference(detail.safe_references.endpoint),
            credential: mapReference(detail.safe_references.credential),
        },
        capabilities: detail.capabilities.map(mapCapability),
        lastHealth: latest
            ? {
                  at: latest.at,
                  result: latest.result,
                  resultLabel: latest.resultLabel,
                  latencyMs: latest.latencyMs,
                  autoRetryStopped: latest.autoRetryStopped,
                  errorClass: latest.errorClass,
                  errorSummary: latest.errorSummary,
              }
            : undefined,
        healthRecords: records,
        healthCheckTypes: detail.health_check_types,
        catalog: {
            state: "NEVER",
            stateLabel: CATALOG_LABEL.NEVER,
        },
        relatedImpact,
        auditEvents: [],
        allowedActions: detail.allowed_actions,
        actionBlockers: detail.action_blockers.map(mapBlocker),
        alerts: records
            .filter((record) => record.result === "AUTH_FAILED")
            .slice(0, 1)
            .map((record) => ({
                id: `health-${record.recordId}`,
                severity: "destructive" as const,
                title: "鉴权/签名失败",
                description:
                    record.errorSummary ?? "请检查安全引用与适配器配置。",
            })),
        nextStep:
            detail.action_blockers[0]?.message ??
            (status === "ENABLED"
                ? "连接已启用"
                : "当前暂无可执行操作，请联系管理员确认连接配置"),
    }
}

export async function resolveSupplierName(supplierId: string): Promise<string> {
    try {
        const row = await apiGet<{
            id: string
            name?: string
            supplier_name?: string
        }>(`/admin/suppliers/${encodeURIComponent(supplierId)}`)
        return row.name ?? row.supplier_name ?? supplierId
    } catch {
        return supplierId
    }
}
