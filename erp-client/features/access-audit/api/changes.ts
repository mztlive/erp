// 权限变更写路径：影响预览 + 提交。仅紧急撤权映射到后端 HTTP，其余命令如实阻断。

import { apiGet, apiPost } from "@/lib/api"
import type {
    AccessChangeCommand,
    AccessChangeOutcome,
    AccessImpactPreview,
} from "@/features/access-audit/types"
import type { BackendUserRole } from "./backend-types"
import { instantToIso } from "./mappers"

function submissionBlockerFor(command: AccessChangeCommand) {
    if (command.action === "EMERGENCY_REVOKE_USER_ROLE") return undefined

    if (command.subjectType === "USER") {
        return {
            action: command.action,
            code: "USER_ROLE_TIME_POLICY_MISSING",
            message:
                "用户角色时间策略尚未配置；当前只允许立即紧急撤权，其它角色变更已阻断。",
        } as const
    }

    if (command.subjectType === "FIELD_POLICY") {
        return {
            action: command.action,
            code: "FIELD_POLICY_GRANULARITY_MISSING",
            message: "字段粒度策略尚未配置，字段访问策略保持只读。",
        } as const
    }

    return {
        action: command.action,
        code: "REVIEW_POLICY_UNCONFIGURED",
        message:
            "本次权限变更尚未取得可直接生效的结论，且双人复核规则未配置；当前变更已阻断。",
    } as const
}

export async function previewAccessChange(
    command: AccessChangeCommand,
): Promise<AccessImpactPreview> {
    const subjectLabel = "subjectId" in command ? command.subjectId : "—"
    const submissionBlocker = submissionBlockerFor(command)
    return {
        subjectLabel,
        actionLabel: command.action,
        changeSummary: `预览：${command.action} → ${subjectLabel}`,
        affectedSubjectCount: 1,
        affectedWorkSurfaceSummary: submissionBlocker
            ? "影响预览尚未可用"
            : "权限与审计 / 相关工作台（按处理结果生效）",
        riskLevel: submissionBlocker ? "high" : "medium",
        riskSummary: submissionBlocker
            ? submissionBlocker.message
            : "紧急撤权将立即影响该授权主体的可访问范围。",
        riskFlags: [],
        diffs: [
            {
                id: "d1",
                field: "action",
                before: "—",
                after: command.action,
            },
        ],
        submissionBlocker,
    }
}

/** 读取当前角色绑定版本；不存在时返回空，禁止用猜测版本提交撤权。 */
async function roleAssignmentVersion(
    subjectId: string,
    roleAssignmentId: string,
): Promise<number | null> {
    const list = await apiGet<BackendUserRole[]>("/admin/user-roles", {
        user_id: subjectId,
    })
    return (
        list.find((binding) => binding.id === roleAssignmentId)?.version ?? null
    )
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
                message: "紧急撤权需要 USER + roleAssignmentId",
            }
        }
        const version = await roleAssignmentVersion(
            command.subjectId,
            command.roleAssignmentId,
        )
        if (version == null) {
            return {
                outcome: "REJECTED",
                code: "ROLE_ASSIGNMENT_NOT_FOUND",
                message: "当前角色绑定不存在或已被其他操作撤销，请刷新后核对。",
            }
        }
        await apiPost(`/admin/user-roles/${command.roleAssignmentId}/revoke`, {
            version,
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
        message:
            "当前操作暂不可用，请刷新后重试；如仍不可用，请联系管理员。",
    }
}
