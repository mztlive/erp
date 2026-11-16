// 权限变更写路径：影响预览 + 提交。仅紧急撤权映射到后端 HTTP，其余命令如实阻断。

import { apiPost } from "@/lib/api"
import type {
    AccessChangeCommand,
    AccessChangeOutcome,
    AccessImpactPreview,
} from "@/features/access-audit/types"
import { instantToIso } from "./mappers"

function submissionBlockerFor(command: AccessChangeCommand) {
    if (command.action === "EMERGENCY_REVOKE_USER_ROLE") return undefined

    if (command.subjectType === "USER") {
        return {
            action: command.action,
            code: "USER_ROLE_TIME_POLICY_MISSING",
            message: "暂不支持设置角色生效或到期时间；当前仅支持立即紧急撤权。",
        } as const
    }

    if (command.subjectType === "FIELD_POLICY") {
        return {
            action: command.action,
            code: "FIELD_POLICY_GRANULARITY_MISSING",
            message: "暂不支持按字段单独配置权限，字段访问规则保持只读。",
        } as const
    }

    return {
        action: command.action,
        code: "REVIEW_POLICY_UNCONFIGURED",
        message: "尚未配置权限变更复核规则，当前无法直接提交该变更。",
    } as const
}

export async function previewAccessChange(
    command: AccessChangeCommand,
): Promise<AccessImpactPreview> {
    const subjectLabel = "subjectId" in command ? command.subjectId : "—"
    const submissionBlocker = submissionBlockerFor(command)
    const actionLabel =
        command.action === "EMERGENCY_REVOKE_USER_ROLE"
            ? "紧急撤权"
            : command.action
    return {
        subjectLabel,
        actionLabel,
        changeSummary: `预览：${actionLabel} → ${subjectLabel}`,
        affectedSubjectCount: 1,
        affectedWorkSurfaceSummary: submissionBlocker
            ? "影响预览尚未可用"
            : "权限与审计 / 相关工作台（按处理结果生效）",
        riskLevel: submissionBlocker ? "high" : "medium",
        riskSummary: submissionBlocker
            ? submissionBlocker.message
            : "紧急撤权后，该人员将立即失去对应角色的操作权限。",
        riskFlags: [],
        diffs: [
            {
                id: "d1",
                field: "操作",
                before: "—",
                after: actionLabel,
            },
        ],
        submissionBlocker,
    }
}

export async function submitAccessChange(
    command: AccessChangeCommand,
): Promise<AccessChangeOutcome> {
    const submissionBlocker = submissionBlockerFor(command)
    if (submissionBlocker) {
        return {
            outcome: "REJECTED",
            code: submissionBlocker.code,
            message: submissionBlocker.message,
            actionBlockers: [submissionBlocker],
        }
    }

    if (command.action === "EMERGENCY_REVOKE_USER_ROLE") {
        if (
            command.subjectType !== "USER" ||
            !("roleAssignmentId" in command)
        ) {
            return {
                outcome: "REJECTED",
                code: "INVALID_COMMAND",
                message: "请选择需要撤权的人员及对应角色后再操作。",
            }
        }
        await apiPost(`/admin/user-roles/${command.roleAssignmentId}/revoke`, {
            revoke_reason_code: command.reasonCode,
            revoke_reason_text: command.comment,
        })
        const effectiveAt = instantToIso(Math.floor(Date.now() / 1000)) ?? ""
        return {
            outcome: "CONFIRMED",
            permissionVersion: "pv-live",
            auditEventId: `ae_${command.idempotencyKey}`,
            affectedSubjectCount: 1,
            effectiveAt,
            reference: command.idempotencyKey,
            nextSteps: ["刷新用户授权列表", "核对有效权限解释"],
            message: "已提交紧急撤权。",
        }
    }

    return {
        outcome: "REJECTED",
        code: "UNSUPPORTED_COMMAND",
        message: "当前操作暂不可用，请刷新后重试；如仍不可用，请联系管理员。",
    }
}
