// 权限变更写路径：影响预览 + 提交。角色绑定通过「调整角色」执行，其余命令如实阻断。

import type {
    AccessChangeCommand,
    AccessChangeOutcome,
    AccessImpactPreview,
} from "@/features/access-audit/types"

function submissionBlockerFor(command: AccessChangeCommand) {
    if (command.subjectType === "USER") {
        return {
            action: command.action,
            code: "USER_ROLE_TIME_POLICY_MISSING",
            message: "暂不支持设置角色生效或到期时间；请使用「调整角色」立即变更角色。",
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
    const actionLabel = command.action
    return {
        subjectLabel,
        actionLabel,
        changeSummary: `预览：${actionLabel} → ${subjectLabel}`,
        affectedSubjectCount: 1,
        affectedWorkSurfaceSummary: "影响预览尚未可用",
        riskLevel: "high",
        riskSummary: submissionBlocker.message,
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
    return {
        outcome: "REJECTED",
        code: submissionBlocker.code,
        message: submissionBlocker.message,
        actionBlockers: [submissionBlocker],
    }
}
