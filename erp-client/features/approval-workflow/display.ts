/**
 * 审批区用户可见文案。枚举必须映射中文，内部 ID 不得上屏。
 */

import { formatDateTime } from "@/lib/datetime"

import type { ApprovalAllowedAction, RecoveryOption } from "./types"

/** 与 `StatusBadge` tone 对齐，审批状态同时用文字、图标和颜色。 */
export type ApprovalStatusTone =
    | "neutral"
    | "info"
    | "success"
    | "warning"
    | "destructive"
    | "void"

/** 32 位十六进制或标准 UUID。服务端人名偶尔回落成用户 id。 */
const OPAQUE_ID =
    /^(?:[0-9a-f]{24,}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i

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
 * 实例状态色调。未知码与「审批中」一致，按进行中处理。
 */
export const instanceStatusTone = (
    status?: string | null,
): ApprovalStatusTone => {
    switch (status) {
        case "APPROVED":
            return "success"
        case "CANCELLED":
            return "void"
        case "BLOCKED":
            return "destructive"
        case "RUNNING":
            return "warning"
        default:
            return "warning"
    }
}

/**
 * 节点执行状态色调。未知码与「办理中」一致。
 */
export const executionStatusTone = (
    status?: string | null,
): ApprovalStatusTone => {
    switch (status) {
        case "APPROVED":
            return "success"
        case "REJECTED":
        case "BLOCKED":
            return "destructive"
        case "CANCELLED":
            return "void"
        case "SUPERSEDED":
            return "neutral"
        case "ACTIVE":
            return "info"
        default:
            return "info"
    }
}

/**
 * 人员展示名。空值或内部 ID 不上屏，由调用方回落到「—」或省略。
 */
export const displayActorName = (value?: string | null): string | undefined => {
    const text = value?.trim()
    if (!text || OPAQUE_ID.test(text)) return undefined
    return text
}

/**
 * Unix 秒时间戳转本地时间。无效值不上屏。
 */
export const displayUnixSeconds = (
    secs?: number | null,
): { dateTime: string; label: string } | undefined => {
    if (secs == null || secs <= 0) return undefined
    const dateTime = new Date(secs * 1000).toISOString()
    const label = formatDateTime(dateTime, "full")
    if (!label || label === "—") return undefined
    return { dateTime, label }
}

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
 * 判断实例是否仍在途。只认服务端 `RUNNING` / `BLOCKED`；
 * `APPROVED` / `CANCELLED` 是终态，未知码不当成进行中。
 */
export const isOpenInstanceStatus = (status?: string | null): boolean =>
    status === "RUNNING" || status === "BLOCKED"

/**
 * 人员失效类 blocker。仅这些类别可显示恢复原审批人。
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
