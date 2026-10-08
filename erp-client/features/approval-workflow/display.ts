/**
 * 审批区用户可见文案。枚举必须映射中文，内部 ID 不得上屏。
 */

import { formatDateTime } from "@/lib/datetime"
import { displayName } from "@/lib/display-name"

import type { RecoveryOption } from "./types"

/** 与 `StatusBadge` tone 对齐，审批状态同时用文字、图标和颜色。 */
export type ApprovalStatusTone =
    | "neutral"
    | "info"
    | "success"
    | "warning"
    | "destructive"
    | "void"

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

/** 名称仅来自展示字段；已知对象标识和不透明 ID 不作为名称。 */
export const displayReadableName = displayName

/** 人员展示名。缺失时由调用方显示占位，不回退到人员标识。 */
export const displayActorName = displayReadableName

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
    id?: string | null
}): string => {
    const name = displayReadableName(input.name, input.id) || "审批流程"
    const version = input.version == null ? "" : String(input.version).trim()
    return version ? `${name} v${version}` : name
}

/**
 * 有序节点路线，如「张三 → 李四 → 王五」。
 */
export const displayRoute = (
    nodes: readonly { key?: string; name: string; assigneeName?: string }[],
): string =>
    nodes
        .map(
            (node) =>
                displayActorName(node.assigneeName) ||
                displayReadableName(node.name, node.key) ||
                "审批节点未标注",
        )
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
