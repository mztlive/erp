/**
 * W20 · API 供应商连接 · 新建与治理命令。
 * 治理结果、动作和阻塞原因只消费服务端事实，不在客户端伪造成功或递增版本。
 */

import { apiPost, apiPut } from "@/lib/api"
import type {
    CapabilityCode,
    ConnectionEnvironment,
    FormalOutcome,
} from "@/features/supplier-api-connections/types"
import {
    CAPABILITY_LABEL,
    ENVIRONMENT_LABEL,
} from "@/features/supplier-api-connections/types"
import {
    type BackendCapabilityUpdateResult,
    type BackendCommandResult,
    type BackendConnection,
    type BackendHealthCheckType,
    mapCapabilityCode,
    toBackendCapabilityCode,
    toBackendEnvironment,
} from "@/features/supplier-api-connections/api/mapping"

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
            message:
                "操作结果尚未确认，请保留当前页面并查询最新状态后再决定是否重试。",
            operationId: result.operation_id,
            idempotencyKey,
        }
    }
    if (result.outcome === "REJECTED") {
        return {
            status: "rejected",
            code: "COMMAND_REJECTED",
            title: "操作被拒绝",
            message:
                "当前业务条件不允许执行该操作，请核对连接状态和必填配置后重试。",
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
        message: "地址配置引用已绑定，可继续检查连接状态。",
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
        message: "能力配置已更新，请按最新连接状态继续操作。",
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
        message: "连接已启用，采购业务确认、连接健康和关联影响均已重新核对。",
    })
}
