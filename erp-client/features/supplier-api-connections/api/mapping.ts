/**
 * W20 · API 供应商连接 · 后端载荷 → 视图映射；只消费服务端事实，不在客户端伪造成功或递增版本。
 */

import { apiGet } from "@/lib/api"
import type {
    CapabilityCode,
    ConnectionCenterView,
    ConnectionListItem,
    ConnectionStatus,
    HealthResult,
    ReferenceState,
} from "@/features/supplier-api-connections/types"
import {
    CAPABILITY_LABEL,
    CATALOG_LABEL,
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
    connector_type: string | null
    connector_type_label: string | null
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

export type BackendConnectionListItem = BackendConnection & {
    supplier_name?: string | null
    capabilities: Array<Pick<BackendCapability, "capability_code" | "status">>
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
    deployment_ready: boolean
    deployment_message: string
    declared_capabilities: string[]
    health_records: BackendHealthRun[]
    health_check_types: BackendHealthCheckType[]
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

export function secsToIso(secs?: number | null): string | undefined {
    if (secs == null || secs <= 0) return undefined
    return new Date(secs * 1000).toISOString()
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
        service_areas: "SERVICE_AREAS",
        delivery: "DELIVERY",
        payment_confirmation: "PAYMENT_CONFIRMATION",
        order: "ORDER",
        query: "QUERY",
        cancel: "CANCEL",
        refund: "REFUND",
        logistics: "LOGISTICS",
        callback: "CALLBACK",
        settlement: "SETTLEMENT",
    }
    const code = table[raw.toLowerCase()]
    if (!code) throw new Error("接口能力类型已变化，请刷新页面后重试。")
    return code
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

export function toListItem(
    connection: BackendConnection,
    capabilities: Array<Pick<BackendCapability, "capability_code" | "status">>,
    supplierName?: string,
): ConnectionListItem {
    const status = mapStatus(connection.status)
    const health = mapHealth(connection.last_health_result)
    const enabledCapabilities = capabilities.filter(
        (item) => item.status === "active",
    )
    return {
        connectionId: connection.id,
        connectionCode: connection.connection_code,
        connectorType: connection.connector_type ?? undefined,
        connectorTypeLabel: connection.connector_type_label ?? "未指定接口类型",
        supplier: {
            id: connection.supplier_id,
            name: supplierName ?? connection.supplier_id,
        },
        status,
        statusLabel: STATUS_LABEL[status],
        statusTone: STATUS_TONE[status],
        capabilitySummary:
            capabilities.length === 0
                ? "接口未配置"
                : enabledCapabilities.length === 0
                  ? "暂无可用能力"
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
        catalogLabel: "打开连接查看",
        nextStep:
            connection.action_blockers[0]?.message ??
            (status === "ENABLED"
                ? "自动下单已开启"
                : "打开连接，开始同步商品"),
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
    const records = detail.health_records.map((run) => {
        const result = healthResult(run)
        return {
            recordId: run.id,
            at:
                secsToIso(run.finished_at ?? run.started_at) ??
                secsToIso(detail.created_at) ??
                new Date(0).toISOString(),
            checkType: {
                CONNECTIVITY: "连通性检查",
                AUTHENTICATION: "身份验证",
                CAPABILITY_METADATA: "能力配置检查",
            }[run.check_type],
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
    return {
        deploymentReady: detail.deployment_ready,
        deploymentMessage: detail.deployment_message,
        declaredCapabilities: Array.from(
            new Set(
                (detail.declared_capabilities ?? []).map(mapCapabilityCode),
            ),
        ),
        connectionId: detail.id,
        connectionCode: detail.connection_code,
        connectorType: detail.connector_type ?? undefined,
        connectorTypeLabel: detail.connector_type_label ?? "未指定接口类型",
        supplier: {
            id: detail.supplier_id,
            name: supplierName ?? detail.supplier_id,
        },
        status,
        statusLabel: STATUS_LABEL[status],
        statusTone: STATUS_TONE[status],
        version: String(detail.version),
        updatedAt: secsToIso(detail.created_at) ?? new Date(0).toISOString(),
        safeReferences: {
            endpoint: mapReference(detail.safe_references.endpoint),
            credential: mapReference(detail.safe_references.credential),
        },
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
                ? "自动下单已开启"
                : "打开连接，开始同步商品"),
    }
}

export async function resolveSupplierName(supplierId: string): Promise<string> {
    try {
        const row = await apiGet<{
            id: string
            name?: string
            supplier_name?: string
            short_name?: string | null
            legal_name?: string | null
        }>(`/admin/suppliers/${encodeURIComponent(supplierId)}`)
        return (
            row.short_name?.trim() ||
            row.legal_name?.trim() ||
            row.name ||
            row.supplier_name ||
            "未命名供应商"
        )
    } catch {
        return "供应商名称暂不可用"
    }
}
