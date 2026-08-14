/**
 * W20 · API 供应商连接 · HTTP 适配层。
 * 治理结果、动作和阻塞原因只消费服务端事实，不在客户端伪造成功或递增版本。
 */

import { apiGet, apiPost, apiPut, type Page } from "@/lib/api"
import type {
    CapabilityCode,
    ConnectionCenterView,
    ConnectionEnvironment,
    ConnectionListItem,
    ConnectionListView,
    ConnectionStatus,
    FormalOutcome,
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

type BackendActionBlocker = {
    action: string
    code: string
    message: string
    destination_workspace_id?: string | null
}

type BackendSafeReference = {
    state: string
    alias?: string | null
    version?: string | null
    visible: boolean
}

type BackendConnection = {
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

type BackendCapability = {
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

type BackendHealthCheckType =
    | "CONNECTIVITY"
    | "AUTHENTICATION"
    | "CAPABILITY_METADATA"

type BackendHealthRun = {
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

type BackendConnectionDetail = BackendConnection & {
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

type BackendCommandResult = {
    outcome: "SUCCEEDED" | "PROCESSING" | "REJECTED" | "UNKNOWN"
    action: string
    operation_id: string
    connection_version: number
    job_id?: string | null
    job_no?: string | null
    audit_event_id: string
}

type BackendCapabilityUpdateResult = {
    outcome: "SUCCEEDED" | "PROCESSING" | "REJECTED" | "UNKNOWN"
    operation_id: string
    connection_version: number
    capabilities: BackendCapability[]
    audit_event_id: string
}

function secsToIso(secs?: number | null): string | undefined {
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

function toBackendEnvironment(
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

function mapCapabilityCode(raw: string): CapabilityCode {
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

function toBackendCapabilityCode(code: CapabilityCode): string {
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

function toListItem(
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
                : "等待服务端开放下一治理动作"),
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

function toCenter(
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
                : "等待服务端开放下一治理动作"),
    }
}

async function resolveSupplierName(supplierId: string): Promise<string> {
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

export type ListQueryInput = {
    environment: string
    status?: string
    health?: string
    capability?: string
    catalogFreshness?: string
    supplierId?: string
    q?: string
    page: number
    pageSize?: number
}

export async function fetchConnectionList(
    input: ListQueryInput,
): Promise<ConnectionListView> {
    const page = Math.max(1, input.page)
    const pageSize = input.pageSize ?? 20
    const environment = input.environment.toUpperCase()
    const query: Record<string, unknown> = {
        page,
        page_size: pageSize,
        sort_by: "updated_at",
        sort_dir: "desc",
    }
    if (input.supplierId) query.supplier_id = input.supplierId
    if (input.q?.trim()) query.connection_code = input.q.trim()
    if (environment !== "ALL")
        query.environment =
            environment === "PRODUCTION" ? "production" : "testing"
    if (input.status) {
        const status = input.status.split(",")[0]?.trim().toUpperCase()
        if (status === "ENABLED") query.status = "active"
        if (status === "DISABLED") query.status = "disabled"
        if (status === "FAULTED") query.status = "fault"
    }

    const pageResult = await apiGet<Page<BackendConnection>>(
        "/admin/supplier-api-connections",
        query,
    )
    const capPage = await apiGet<Page<BackendCapability>>(
        "/admin/supplier-api-capabilities",
        { page: 1, page_size: 100 },
    )
    const capsByConnection = new Map<string, BackendCapability[]>()
    for (const capability of capPage.items) {
        const items = capsByConnection.get(capability.connection_id) ?? []
        items.push(capability)
        capsByConnection.set(capability.connection_id, items)
    }
    const supplierNames = new Map<string, string>()
    await Promise.all(
        pageResult.items.map(async (connection) => {
            if (!supplierNames.has(connection.supplier_id)) {
                supplierNames.set(
                    connection.supplier_id,
                    await resolveSupplierName(connection.supplier_id),
                )
            }
        }),
    )
    const items = pageResult.items.map((connection) =>
        toListItem(
            connection,
            capsByConnection.get(connection.id) ?? [],
            supplierNames.get(connection.supplier_id),
        ),
    )
    return {
        metrics: {
            enabled: items.filter((item) => item.status === "ENABLED").length,
            faulted: items.filter((item) => item.status === "FAULTED").length,
            pendingConfig: items.filter(
                (item) => item.status === "PENDING_CONFIG",
            ).length,
            healthAbnormal: items.filter((item) =>
                ["FAILED", "AUTH_FAILED", "PARTIAL", "UNKNOWN"].includes(
                    item.healthResult,
                ),
            ).length,
            catalogStale: 0,
        },
        items,
        total: pageResult.total,
        page: pageResult.page,
        pageSize: pageResult.page_size,
        emptyReason: items.length === 0 ? "NO_CONNECTIONS" : undefined,
        hasModulePermission: true,
        hasDataScope: true,
        projectedAt:
            secsToIso(
                Math.max(0, ...pageResult.items.map((item) => item.created_at)),
            ) ?? new Date(0).toISOString(),
        credentialOpaqueOptions: [],
        endpointOpaqueOptions: [],
    }
}

export async function fetchOpaqueReferenceOptions(
    kind: "credential" | "endpoint",
) {
    const view = await fetchConnectionList({
        environment: "all",
        page: 1,
        pageSize: 1,
    })
    return kind === "credential"
        ? view.credentialOpaqueOptions
        : view.endpointOpaqueOptions
}

export async function fetchConnectionCenter(input: {
    connectionId: string
}): Promise<ConnectionCenterView | null> {
    try {
        const detail = await apiGet<BackendConnectionDetail>(
            `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}`,
        )
        return toCenter(detail, await resolveSupplierName(detail.supplier_id))
    } catch (error) {
        const apiError = error as { kind?: string; status?: number }
        if (apiError.kind === "Http" && apiError.status === 404) return null
        throw error
    }
}

export async function createConnection(input: {
    connectionCode: string
    supplierId: string
    supplierName: string
    environment: ConnectionEnvironment
    idempotencyKey: string
}): Promise<FormalOutcome> {
    const code = input.connectionCode.trim().toUpperCase()
    if (!code) {
        return {
            status: "failed",
            code: "CODE_REQUIRED",
            title: "连接代码必填",
            message: "请填写全局唯一的连接代码，不可与环境组合复用",
        }
    }
    const created = await apiPost<BackendConnection>(
        "/admin/supplier-api-connections",
        {
            supplier_id: input.supplierId,
            connection_code: code,
            environment: toBackendEnvironment(input.environment),
            rate_limit_policy: null,
            status: "disabled",
            capabilities: [],
        },
    )
    return {
        status: "succeeded",
        title: "连接身份已创建",
        message: `已创建 ${code}。下一步完成技术引用与能力配置。`,
        reference: code,
        connectionId: created.id,
        connectionVersion: String(created.version),
        facts: [
            { label: "连接代码", value: code },
            { label: "供应商", value: input.supplierName },
            { label: "环境", value: ENVIRONMENT_LABEL[input.environment] },
        ],
    }
}

async function runCommand(input: {
    connectionId: string
    action: string
    expectedVersion: string
    idempotencyKey: string
    payloadReference?: string
    reasonCode?: string
    checkType?: BackendHealthCheckType
}): Promise<BackendCommandResult> {
    return apiPost<BackendCommandResult>(
        `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}/commands`,
        {
            action: input.action,
            expected_version: Number(input.expectedVersion),
            payload_reference: input.payloadReference,
            reason_code: input.reasonCode,
            check_type: input.checkType,
            idempotency_key: input.idempotencyKey,
        },
    )
}

function commandOutcome(
    result: BackendCommandResult,
    idempotencyKey: string,
    success: { title: string; message: string },
): FormalOutcome {
    if (result.outcome === "PROCESSING" && result.job_id && result.job_no) {
        return {
            status: "processing",
            title: success.title,
            message: "后台任务已创建；请按任务号查询进度与终态。",
            jobId: result.job_id,
            jobNo: result.job_no,
        }
    }
    if (result.outcome === "UNKNOWN") {
        return {
            status: "unknown",
            title: "处理结果待确认",
            message: "不得乐观改变连接状态；请使用原操作身份查询最终结果。",
            operationId: result.operation_id,
            idempotencyKey,
        }
    }
    if (result.outcome === "REJECTED") {
        return {
            status: "rejected",
            code: "COMMAND_REJECTED",
            title: "操作被拒绝",
            message: "服务端业务前置条件未满足。",
            reference: result.operation_id,
        }
    }
    return {
        status: "succeeded",
        ...success,
        reference: result.operation_id,
        connectionVersion: String(result.connection_version),
        auditEventId: result.audit_event_id,
    }
}

export async function bindCredentialReference(input: {
    connectionId: string
    opaqueReferenceId: string
    expectedVersion: string
    idempotencyKey: string
}): Promise<FormalOutcome> {
    const result = await runCommand({
        connectionId: input.connectionId,
        action: "BIND_CREDENTIAL_REFERENCE",
        expectedVersion: input.expectedVersion,
        idempotencyKey: input.idempotencyKey,
        payloadReference: input.opaqueReferenceId,
    })
    return commandOutcome(result, input.idempotencyKey, {
        title: "密钥引用已绑定",
        message: "已绑定密钥管理系统引用；响应不包含密钥正文。",
    })
}

export async function bindEndpointReference(input: {
    connectionId: string
    opaqueReferenceId: string
    expectedVersion: string
    idempotencyKey: string
}): Promise<FormalOutcome> {
    const result = await runCommand({
        connectionId: input.connectionId,
        action: "BIND_ENDPOINT_REFERENCE",
        expectedVersion: input.expectedVersion,
        idempotencyKey: input.idempotencyKey,
        payloadReference: input.opaqueReferenceId,
    })
    return commandOutcome(result, input.idempotencyKey, {
        title: "地址引用已绑定",
        message: "服务端已确认并绑定地址配置引用。",
    })
}

export async function updateCapabilities(input: {
    connectionId: string
    changes: Array<{ code: CapabilityCode; enabled: boolean }>
    expectedConnectionVersion: string
    expectedCapabilityVersions: Record<string, string>
    reasonCode: string
    operationId: string
    idempotencyKey: string
}): Promise<FormalOutcome> {
    const expectedVersions: Record<string, number> = {}
    for (const change of input.changes) {
        expectedVersions[toBackendCapabilityCode(change.code)] = Number(
            input.expectedCapabilityVersions[change.code] ?? 0,
        )
    }
    const result = await apiPut<BackendCapabilityUpdateResult>(
        `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}/capabilities`,
        {
            capability_changes: input.changes.map((change) => ({
                code: toBackendCapabilityCode(change.code),
                enabled: change.enabled,
                constraint_snapshot: null,
            })),
            expected_connection_version: Number(
                input.expectedConnectionVersion,
            ),
            expected_capability_versions: expectedVersions,
            reason_code: input.reasonCode,
            operation_id: input.operationId,
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        title: "能力配置已更新",
        message: "服务端已提交能力版本并返回权威连接版本。",
        reference: result.operation_id,
        connectionVersion: String(result.connection_version),
        auditEventId: result.audit_event_id,
        facts: result.capabilities.map((capability) => ({
            label: CAPABILITY_LABEL[
                mapCapabilityCode(capability.capability_code)
            ],
            value: capability.status === "active" ? "启用" : "停用",
        })),
    }
}

export async function runHealthCheck(input: {
    connectionId: string
    expectedVersion: string
    idempotencyKey: string
    checkType: BackendHealthCheckType
}): Promise<FormalOutcome> {
    const result = await runCommand({
        connectionId: input.connectionId,
        action: "RUN_HEALTH_CHECK",
        expectedVersion: input.expectedVersion,
        idempotencyKey: input.idempotencyKey,
        checkType: input.checkType,
    })
    return commandOutcome(result, input.idempotencyKey, {
        title: "健康检查任务已创建",
        message: "检查在后台执行；HTTP 完成不代表技术健康成功。",
    })
}

export async function startCatalogSync(input: {
    connectionId: string
    expectedVersion: string
    idempotencyKey: string
}): Promise<FormalOutcome> {
    const result = await runCommand({
        connectionId: input.connectionId,
        action: "START_CATALOG_SYNC",
        expectedVersion: input.expectedVersion,
        idempotencyKey: input.idempotencyKey,
    })
    return commandOutcome(result, input.idempotencyKey, {
        title: "目录同步任务已创建",
        message: "目录同步在后台执行。",
    })
}

export async function disableConnection(input: {
    connectionId: string
    expectedVersion: string
    reasonCode: string
    idempotencyKey: string
}): Promise<FormalOutcome> {
    const result = await runCommand({
        connectionId: input.connectionId,
        action: "DISABLE",
        expectedVersion: input.expectedVersion,
        idempotencyKey: input.idempotencyKey,
        reasonCode: input.reasonCode,
    })
    return commandOutcome(result, input.idempotencyKey, {
        title: "连接已停用",
        message: "连接状态已停用；历史版本与业务事实保持不变。",
    })
}

export async function enableConnection(input: {
    connectionId: string
    expectedVersion: string
    idempotencyKey: string
}): Promise<FormalOutcome> {
    const result = await runCommand({
        connectionId: input.connectionId,
        action: "ENABLE",
        expectedVersion: input.expectedVersion,
        idempotencyKey: input.idempotencyKey,
    })
    return commandOutcome(result, input.idempotencyKey, {
        title: "连接已启用",
        message: "服务端已在同一事务重验采购业务确认、技术健康与关联影响。",
    })
}
