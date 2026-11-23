/**
 * 审批区用户可见文案。枚举必须映射中文，内部 ID 不得上屏。
 */

import { formatDateTime, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import { displayName } from "@/lib/display-name"
import {
    APPROVAL_INSTANCE_STATUS_META,
    APPROVAL_NODE_STATUS_META,
    DOCUMENT_TYPE_CANONICAL_LABELS,
    getApprovalInstanceStatusMeta,
    getApprovalNodeStatusMeta,
} from "@/lib/task-status-dictionary"

import type { ApprovalSalesSubmission } from "./api/materials"
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
    RUNNING: APPROVAL_INSTANCE_STATUS_META.RUNNING.label,
    APPROVED: APPROVAL_INSTANCE_STATUS_META.APPROVED.label,
    REJECTED: APPROVAL_INSTANCE_STATUS_META.REJECTED.label,
    CANCELLED: APPROVAL_INSTANCE_STATUS_META.CANCELLED.label,
    BLOCKED: APPROVAL_INSTANCE_STATUS_META.BLOCKED.label,
}

export const EXECUTION_STATUS_LABEL: Record<string, string> = {
    ACTIVE: APPROVAL_NODE_STATUS_META.ACTIVE.label,
    APPROVED: APPROVAL_NODE_STATUS_META.APPROVED.label,
    REJECTED: APPROVAL_NODE_STATUS_META.REJECTED.label,
    CANCELLED: APPROVAL_NODE_STATUS_META.CANCELLED.label,
    BLOCKED: APPROVAL_NODE_STATUS_META.BLOCKED.label,
    PENDING: APPROVAL_NODE_STATUS_META.PENDING.label,
    SUPERSEDED: APPROVAL_NODE_STATUS_META.SUPERSEDED.label,
}

export const RECOVERY_ACTION_LABEL: Record<RecoveryOption, string> = {
    RESUME_CURRENT_APPROVER: "继续由原审批人审批",
    CANCEL_BLOCKED: "终止并退回此审批",
}

/**
 * 实例状态中文。权威口径统一来自 `@/lib/task-status-dictionary`。
 */
export const displayInstanceStatus = (status?: string | null): string =>
    getApprovalInstanceStatusMeta(status).label

/**
 * 执行结果中文。权威口径统一来自 `@/lib/task-status-dictionary`。
 */
export const displayExecutionStatus = (status?: string | null): string =>
    getApprovalNodeStatusMeta(status).label

/**
 * 实例状态色调。权威口径统一来自 `@/lib/task-status-dictionary`（`RUNNING -> info`，`BLOCKED -> destructive`）。
 */
export const instanceStatusTone = (
    status?: string | null,
): ApprovalStatusTone => getApprovalInstanceStatusMeta(status).tone

/**
 * 节点执行状态色调。权威口径统一来自 `@/lib/task-status-dictionary`（`ACTIVE -> warning`，`BLOCKED -> destructive`）。
 */
export const executionStatusTone = (
    status?: string | null,
): ApprovalStatusTone => getApprovalNodeStatusMeta(status).tone

/**
 * 审批单据权威中文名（如 `sales_invoice_request -> 销项开票申请`）。
 */
export const displayCanonicalDocumentTypeLabel = (
    documentType?: string | null,
): string | undefined => {
    if (!documentType) return undefined
    const entry =
        DOCUMENT_TYPE_CANONICAL_LABELS[
            documentType as keyof typeof DOCUMENT_TYPE_CANONICAL_LABELS
        ]
    return entry?.label
}

/** 名称仅来自展示字段；已知对象标识和不透明 ID 不作为名称。 */
export const displayReadableName = displayName

/** 人员展示名。缺失时由调用方显示占位，不回退到人员标识。 */
export const displayActorName = displayReadableName

export function approvalSalesUnit(
    line: ApprovalSalesSubmission["lines"][number],
    isCard: boolean,
): string | undefined {
    return (
        displayReadableName(line.unit_name ?? line.unit_snapshot) ||
        (isCard ? "张" : undefined)
    )
}

export function approvalSalesTaxPoint(value: string): string {
    const point = value.trim()
    return /^\d+(?:\.\d+)?$/.test(point) ? `${point}%` : point || "—"
}

/**
 * Unix 秒时间戳转本地时间。无效值不上屏。
 */
export const displayUnixSeconds = (
    secs?: number | null,
): { dateTime: string; label: string } | undefined => {
    const dateTime = unixSecondsToIsoOrEmpty(secs)
    if (!dateTime) return undefined
    const label = formatDateTime(dateTime, "full")
    if (!label || label === "—") return undefined
    return { dateTime, label }
}

const DATE_ONLY_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
})

/**
 * Unix 秒时间戳或 ISO 日期转本地日期（YYYY/MM/DD）。无效值返回「—」。
 */
export const displayUnixDate = (secs?: number | null): string => {
    const dateTime = unixSecondsToIsoOrEmpty(secs)
    if (!dateTime) return "—"
    const parsed = new Date(dateTime)
    if (Number.isNaN(parsed.getTime())) return "—"
    return DATE_ONLY_FORMATTER.format(parsed)
}

/**
 * 日期字符串或 Date 转本地日期（YYYY/MM/DD）。无效值返回「—」。
 */
export const displayDateString = (
    value?: string | number | Date | null,
): string => {
    if (value == null || value === "") return "—"
    const parsed = value instanceof Date ? value : new Date(value)
    if (Number.isNaN(parsed.getTime()))
        return typeof value === "string" ? value : "—"
    return DATE_ONLY_FORMATTER.format(parsed)
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
 * `APPROVED` / `REJECTED` / `CANCELLED` 是终态，未知码不当成进行中。
 */
export const isOpenInstanceStatus = (status?: string | null): boolean =>
    status === "RUNNING" || status === "BLOCKED"
