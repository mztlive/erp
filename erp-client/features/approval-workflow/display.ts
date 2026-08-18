/**
 * 审批区用户可见文案。枚举必须映射中文，内部 ID 不得上屏。
 */

import type { ApprovalAllowedAction, RecoveryOption } from "./types"

export const INSTANCE_STATUS_LABEL: Record<string, string> = {
    RUNNING: "审批中",
    APPROVED: "已通过",
    CANCELLED: "已撤回",
    BLOCKED: "受阻",
}

export const EXECUTION_STATUS_LABEL: Record<string, string> = {
    ACTIVE: "办理中",
    APPROVED: "已通过",
    REJECTED: "已驳回",
    CANCELLED: "已撤回",
    BLOCKED: "受阻",
    SUPERSEDED: "已由后续轮次替代",
}

export const RECOVERY_ACTION_LABEL: Record<RecoveryOption, string> = {
    RESUME_CURRENT_APPROVER: "恢复当前审批人",
    REASSIGN_CURRENT_APPROVER: "改派当前审批人",
    CANCEL_BLOCKED: "取消受阻审批",
}

export const ALLOWED_ACTION_LABEL: Partial<
    Record<ApprovalAllowedAction, string>
> = {
    APPROVE: "通过",
    REJECT: "驳回",
    OPEN_DOCUMENT: "打开单据",
    VIEW: "查看",
    SUBMIT: "提交",
    CANCEL: "撤回审批",
    CANCEL_APPROVAL: "撤回审批",
    UPGRADE_BINDING: "更新审批流程版本",
    RESUME_CURRENT_APPROVER: "恢复当前审批人",
    REASSIGN_CURRENT_APPROVER: "改派当前审批人",
    CANCEL_BLOCKED_APPROVAL: "取消受阻审批",
}

/**
 * 实例状态中文。未知码回落到「审批中」，不上屏原值。
 */
export const displayInstanceStatus = (status?: string | null): string =>
    INSTANCE_STATUS_LABEL[status ?? ""] ?? "审批中"

/**
 * 执行结果中文。未知码回落到「办理中」。
 */
export const displayExecutionStatus = (status?: string | null): string =>
    EXECUTION_STATUS_LABEL[status ?? ""] ?? "办理中"

/**
 * 当前轮次文案。
 */
export const displayRound = (roundNo?: number | null): string =>
    `第 ${roundNo && roundNo > 0 ? roundNo : 1} 轮`

/**
 * 流程名与版本。缺名称时只显示版本。
 */
export const displayProcessVersion = (input: {
    name?: string | null
    version?: string | number | null
}): string => {
    const name = input.name?.trim() || "审批流程"
    const version = input.version == null ? "" : String(input.version).trim()
    return version ? `${name} v${version}` : name
}

/**
 * 有序节点路线，如「张三 → 李四 → 王五」。
 */
export const displayRoute = (
    nodes: readonly { name: string; assigneeName?: string }[],
): string =>
    nodes
        .map((node) => node.assigneeName?.trim() || node.name)
        .filter(Boolean)
        .join(" → ")

/**
 * 判断实例是否受阻。只读服务端状态，不做本地推断。
 */
export const isBlockedStatus = (status?: string | null): boolean =>
    status === "BLOCKED"

/**
 * 人员失效类 blocker。仅这些类别可显示恢复/改派。
 */
export const PERSONNEL_BLOCKER_CODES = new Set([
    "APPROVER_ACCOUNT_INACTIVE",
    "APPROVER_EMPLOYMENT_INVALID",
    "APPROVER_NOT_ELIGIBLE",
    "APPROVER_OUT_OF_DATA_SCOPE",
    "APPROVER_CANNOT_READ_SUBJECT",
    "SEPARATION_OF_DUTIES_VIOLATION",
])

/**
 * 判断 blocker 是否属于人员失效类别。页面仍以 `recovery_options` 为准。
 */
export const isPersonnelBlocker = (code?: string | null): boolean =>
    Boolean(code && PERSONNEL_BLOCKER_CODES.has(code))
