/**
 * W20 · API 供应商连接 · 新建与治理命令。
 * 治理结果、动作和阻塞原因只消费服务端事实，不在客户端伪造成功或递增版本。
 */

import { apiPost } from "@/lib/api"
import type { FormalOutcome } from "@/features/supplier-api-connections/types"
import {
    type BackendCommandResult,
    type BackendConnection,
    type BackendHealthCheckType,
} from "@/features/supplier-api-connections/api/mapping"

export async function createConnection(input: {
    connectionCode: string
    supplierId: string
    supplierName: string
    connectorType: string
    idempotencyKey: string
}): Promise<FormalOutcome> {
    const code = input.connectionCode.trim().toUpperCase()
    if (!code) {
        return {
            status: "failed",
            code: "CODE_REQUIRED",
            title: "连接代码必填",
            message: "请填写唯一的连接代码",
        }
    }
    const created = await apiPost<BackendConnection>(
        "/admin/supplier-api-connections",
        {
            supplier_id: input.supplierId,
            connection_code: code,
            connector_type: input.connectorType,
            rate_limit_policy: null,
            status: "disabled",
            capabilities: [],
        },
    )
    return {
        status: "succeeded",
        title: "连接身份已创建",
        message: `已创建 ${code}。服务端自动提供接口配置，打开连接即可开始同步商品。`,
        reference: code,
        connectionId: created.id,
        connectionVersion: String(created.version),
        facts: [
            { label: "连接代码", value: code },
            { label: "供应商", value: input.supplierName },
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
            message: success.message,
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
        title: "连接测试已提交",
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
        title: "商品同步任务已创建",
        message: "商品在后台同步，本页会自动更新进度。",
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
        title: "自动下单已关闭",
        message: "不再新增发单；商品同步、选品和已有订单跟踪继续运行。",
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
        title: "自动下单已开启",
        message:
            "连接检查已通过，符合现有采购审批与执行条件的订单可以自动发单。",
    })
}
