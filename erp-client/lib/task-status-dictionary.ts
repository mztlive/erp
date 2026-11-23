/**
 * 任务与审批状态 / 色调 / 权威中文命名唯一真源。
 *
 * 全站 `StatusBadge` 与队列/审批/抄送文案统一消费本字典，
 * 消除 RUNNING / BLOCKED / ACTIVE 色板冲突及多套业务命名漂移。
 */

import type { StatusTone } from "@/components/ui/status-badge"

export type TaskStatusMeta = Readonly<{
    label: string
    tone: StatusTone
}>

export type CanonicalLabelMeta = Readonly<{
    label: string
    shortLabel: string
}>

export type ApprovalInstanceStatusKey =
    | "RUNNING"
    | "BLOCKED"
    | "APPROVED"
    | "REJECTED"
    | "CANCELLED"

export type ApprovalNodeStatusKey =
    | "ACTIVE"
    | "APPROVED"
    | "REJECTED"
    | "BLOCKED"
    | "CANCELLED"
    | "PENDING"
    | "SUPERSEDED"

export type WorkItemStatusKey =
    | "OPEN"
    | "COMPLETED"
    | "CLOSED"
    | "APPROVAL_BLOCKED"
    | "EXECUTION_BLOCKED"

export const APPROVAL_INSTANCE_STATUS_META: Readonly<
    Record<ApprovalInstanceStatusKey, TaskStatusMeta>
> = {
    RUNNING: { label: "审批中", tone: "info" },
    BLOCKED: { label: "审批受阻", tone: "destructive" },
    APPROVED: { label: "已通过", tone: "success" },
    REJECTED: { label: "已驳回", tone: "destructive" },
    CANCELLED: { label: "已撤回", tone: "void" },
}

export const APPROVAL_NODE_STATUS_META: Readonly<
    Record<ApprovalNodeStatusKey, TaskStatusMeta>
> = {
    ACTIVE: { label: "待审批", tone: "warning" },
    APPROVED: { label: "已通过", tone: "success" },
    REJECTED: { label: "已驳回", tone: "destructive" },
    BLOCKED: { label: "审批受阻", tone: "destructive" },
    CANCELLED: { label: "已撤回", tone: "void" },
    PENDING: { label: "未开始", tone: "neutral" },
    SUPERSEDED: { label: "往轮已替代", tone: "neutral" },
}

export const WORK_ITEM_STATUS_META: Readonly<
    Record<WorkItemStatusKey, TaskStatusMeta>
> = {
    OPEN: { label: "待处理", tone: "info" },
    COMPLETED: { label: "已完成", tone: "success" },
    CLOSED: { label: "已关闭", tone: "neutral" },
    APPROVAL_BLOCKED: { label: "审批受阻", tone: "destructive" },
    EXECUTION_BLOCKED: { label: "执行受阻", tone: "destructive" },
}

export const TASK_TYPE_CANONICAL_LABELS = {
    PROCUREMENT_ORDER_CREATION: {
        label: "采购供给分配",
        shortLabel: "供给分配",
    },
    SALES_INQUIRY_PROCESSING: {
        label: "询价采购处理",
        shortLabel: "询价处理",
    },
    PURCHASE_SOURCE_UPDATE: {
        label: "采购依据变更",
        shortLabel: "变更影响",
    },
    INTEGRATION_RESULT_UNKNOWN: {
        label: "外部同步待核实",
        shortLabel: "同步待核",
    },
} as const satisfies Record<string, CanonicalLabelMeta>

export const DOCUMENT_TYPE_CANONICAL_LABELS = {
    sales_invoice_request: {
        label: "销项开票申请",
        shortLabel: "开票申请",
    },
} as const satisfies Record<string, CanonicalLabelMeta>

export const AUTO_NEXT_LABEL = "处理后自动下一项"

/** 获取审批实例状态元数据；未知码回落到「审批中」，不直接上屏枚举原值。 */
export function getApprovalInstanceStatusMeta(
    status?: string | null,
): TaskStatusMeta {
    if (status && status in APPROVAL_INSTANCE_STATUS_META) {
        return APPROVAL_INSTANCE_STATUS_META[
            status as ApprovalInstanceStatusKey
        ]
    }
    return APPROVAL_INSTANCE_STATUS_META.RUNNING
}

/** 获取审批节点执行状态元数据；未知码回落到「待审批」。 */
export function getApprovalNodeStatusMeta(
    status?: string | null,
): TaskStatusMeta {
    if (status && status in APPROVAL_NODE_STATUS_META) {
        return APPROVAL_NODE_STATUS_META[status as ApprovalNodeStatusKey]
    }
    return APPROVAL_NODE_STATUS_META.ACTIVE
}

/**
 * 获取事项状态元数据。
 *
 * - 支持直接传 `OPEN` / `COMPLETED` / `CLOSED` / `APPROVAL_BLOCKED` / `EXECUTION_BLOCKED`；
 * - 也支持同时传 `(status, processingState)`，当任务处于 `OPEN` 且受阻时返回对应受阻状态。
 */
export function getWorkItemStatusMeta(
    status?: string | null,
    processingState?: string | null,
): TaskStatusMeta {
    if (
        status === "OPEN" &&
        (processingState === "APPROVAL_BLOCKED" ||
            processingState === "EXECUTION_BLOCKED")
    ) {
        return WORK_ITEM_STATUS_META[processingState]
    }
    if (status && status in WORK_ITEM_STATUS_META) {
        return WORK_ITEM_STATUS_META[status as WorkItemStatusKey]
    }
    return WORK_ITEM_STATUS_META.OPEN
}
