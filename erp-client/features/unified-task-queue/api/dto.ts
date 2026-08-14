import {
    mapWorkItemDto as mapStableWorkItemDto,
    type WorkItemDto,
} from "@/features/work-items"

import type { QueueWorkItemView } from "../types"
import {
    getHandlerRegistration,
    isRegisteredHandlerDestination,
} from "../lib/handler-destination"

export { isRegisteredHandlerDestination } from "../lib/handler-destination"

const STATUS_PRESENTATION = {
    OPEN: { label: "待处理", tone: "info" },
    COMPLETED: { label: "已完成", tone: "success" },
    CLOSED: { label: "已关闭", tone: "neutral" },
} as const

const PRIORITY_PRESENTATION: Readonly<
    Record<string, { rank: number; label: string }>
> = {
    urgent: { rank: 1, label: "紧急" },
    high: { rank: 2, label: "高" },
    normal: { rank: 3, label: "普通" },
    low: { rank: 4, label: "低" },
    "1": { rank: 1, label: "紧急" },
    "2": { rank: 2, label: "高" },
    "3": { rank: 3, label: "普通" },
    "4": { rank: 4, label: "低" },
}

export function unixToIso(seconds?: number | null): string {
    if (seconds == null || seconds <= 0) return ""
    return new Date(seconds * 1000).toISOString()
}

function formatDateTime(iso?: string): string {
    if (!iso) return "—"
    return new Intl.DateTimeFormat("zh-CN", {
        month: "numeric",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
    }).format(new Date(iso))
}

function responsibilityLabel(item: ReturnType<typeof mapStableWorkItemDto>) {
    if (item.ownerUser) return `由 ${item.ownerUser.displayName} 处理`
    if (item.assignmentMode === "POOL") return "团队待处理"
    return "处理人待确认"
}

/** 构建 W02 展示模型；allowedActions 保持服务端原值。 */
export function mapQueueWorkItemDto(dto: WorkItemDto): QueueWorkItemView {
    const item = mapStableWorkItemDto(dto)
    const registered = getHandlerRegistration(item.handlerKey)
    const registration = isRegisteredHandlerDestination(
        item.handlerKey,
        item.destinationWorkspaceId,
    )
        ? registered
        : undefined
    const enteredDateTime = unixToIso(item.createdAt)
    const dueDateTime = unixToIso(item.dueAt)
    const priority =
        PRIORITY_PRESENTATION[String(item.priority).toLowerCase()] ??
        PRIORITY_PRESENTATION.normal

    return {
        ...item,
        destinationWorkspaceId: registration?.destinationWorkspaceId,
        allowedActions:
            registration && item.processingState === "READY"
                ? item.allowedActions
                : [],
        actionBlockers: registration
            ? item.actionBlockers
            : registered
              ? ["任务处理入口配置不一致，请刷新或联系系统管理员。"]
              : ["当前任务类型尚未接入，请联系系统管理员。"],
        id: item.workItemId,
        workItemTypeLabel:
            registration?.workItemTypeLabel ?? "暂不可处理的任务",
        family: registration?.family ?? "exception",
        handlerKnown: Boolean(registration),
        businessObject: item.businessObjectLabel,
        counterparty: item.counterpartyLabel,
        enteredAt: formatDateTime(enteredDateTime),
        enteredDateTime,
        dueLabel: dueDateTime ? formatDateTime(dueDateTime) : "未设置",
        dueDateTime: dueDateTime || undefined,
        responsibilityLabel: responsibilityLabel(item),
        reason: item.reasonLabel,
        impact: item.impactSummary,
        statusPresentation:
            item.processingState === "APPROVAL_BLOCKED"
                ? { label: "审批受阻", tone: "warning" }
                : STATUS_PRESENTATION[item.status],
        priorityRank: priority.rank,
        priorityLabel: priority.label,
    }
}

export function countOpenWorkItems(items: readonly QueueWorkItemView[]): {
    mine: number
    team: number
    overdue: number
    total: number
} {
    const now = Date.now()
    return {
        mine: items.filter((item) => Boolean(item.ownerUser)).length,
        team: items.filter(
            (item) =>
                item.assignmentMode === "POOL" && item.ownerUser === undefined,
        ).length,
        overdue: items.filter(
            (item) =>
                item.dueDateTime && new Date(item.dueDateTime).getTime() < now,
        ).length,
        total: items.length,
    }
}
