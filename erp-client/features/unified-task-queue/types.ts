import type { StatusTone } from "@/components/ui/status-badge"
import type { WorkItemProjection, WorkItemScope } from "@/features/work-items"

export type WorkItemFamily =
    | "approval"
    | "finance"
    | "fulfillment"
    | "exception"

export type QueueScopeSlug = WorkItemScope

export type UnifiedQueueFilters = Readonly<{
    scope: QueueScopeSlug
    family?: WorkItemFamily
    workItemType?: string
    historyStatus?: "COMPLETED" | "CLOSED"
    due?: "today" | "overdue"
    priorities?: readonly number[]
    query?: string
    sort?: "priority_due" | "due_asc" | "created_desc"
    queueContextId?: string
    currentWorkItemId?: string
    viewerKey?: string
    viewerUserId?: string
}>

export type QueueWorkItemView = WorkItemProjection &
    Readonly<{
        id: string
        workItemTypeLabel: string
        family: WorkItemFamily
        handlerKnown: boolean
        businessObject: string
        counterparty?: string
        enteredAt: string
        enteredDateTime: string
        dueLabel: string
        dueDateTime?: string
        responsibilityLabel: string
        reason: string
        impact: string
        statusPresentation: { label: string; tone: StatusTone }
        priorityRank: number
        priorityLabel: string
    }>

export type UnifiedTaskQueueView = Readonly<{
    queueContextId?: string
    total: number
    items: readonly QueueWorkItemView[]
}>

export const FAMILY_LABELS: Record<WorkItemFamily, string> = {
    approval: "审批与确认",
    finance: "票款与结算",
    fulfillment: "履约与库存",
    exception: "数据治理与异常",
}
