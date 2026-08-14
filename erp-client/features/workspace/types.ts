/**
 * W01 今日工作台 — 客户端契约类型（对齐 docs/ui-workspaces/w01 §8）。
 * 任务行由 `/admin/work-items` 提供，指标由同一授权规则的 `/admin/work-items/stats` 聚合。
 */

import type { StatusTone } from "@/components/ui/status-badge"
import type {
    AssignmentMode,
    WorkItemStatus,
} from "@/features/work-items/types"
import type { WorkspaceId } from "@/lib/workspace-registry"

export type WorkspaceDueFilter = "today" | "overdue"
export type WorkspaceFamilyFilter =
    | "approval"
    | "finance"
    | "fulfillment"
    | "exception"

export type WorkspaceMetricKey = "mine" | "due_today" | "overdue" | "exception"

export type WorkspaceActionCode = "VIEW" | "PROCESS" | "START_PROCESSING"

export type WorkspaceWorkItem = Readonly<{
    workItemId: string
    taskVersion: string
    workItemType: string
    workItemTypeLabel: string
    businessObjectType: string
    businessObjectId: string
    subjectVersion: string
    stableNumber: string
    objectTitle: string
    counterpartyName: string
    status: WorkItemStatus
    statusLabel: string
    statusTone: StatusTone
    processingState: "READY" | "APPROVAL_BLOCKED"
    assignmentMode: AssignmentMode
    priority: number
    createdAt: string
    dueAt: string
    ownerRoleLabel: string
    ownerOrganizationLabel: string
    ownerUserLabel?: string
    reasonLabel: string
    impactSummary: string
    allowedActions: readonly WorkspaceActionCode[]
    actionBlockers: readonly {
        action: WorkspaceActionCode
        code: string
        message: string
    }[]
    destinationWorkspaceId: WorkspaceId
    queueContextId?: string
    handlerKey: string
    routeContext?: {
        confirmationScope?: string
    }
    enteredAtLabel: string
    dueAtLabel: string
    dueBucket: "today" | "overdue" | "later"
    family: WorkspaceFamilyFilter
}>

export type WorkspaceTaskGroup = Readonly<{
    family: WorkspaceFamilyFilter
    label: string
    total: number
    pagePreviewLimit?: number
    previewLimitSource?: "CONFIGURED" | "TEMPORARY_FALLBACK"
    defaultExpanded: boolean
    items: readonly WorkspaceWorkItem[]
}>

export type WorkspaceMetric = Readonly<{
    key: WorkspaceMetricKey
    label: string
    count: number
    visible: boolean
    tone: StatusTone
    detail?: string
}>

type WorkspaceWarning = Readonly<{
    warningId: string
    kind: string
    severity: "warning" | "destructive" | "info"
    title: string
    description: string
    occurredAt: string
    destinationWorkspaceId: WorkspaceId
    objectId?: string
}>

type WorkspaceRecentItem = Readonly<{
    id: string
    label: string
    destinationWorkspaceId: WorkspaceId
    objectId?: string
    href: string
}>

export type TodayWorkspaceQuery = Readonly<{
    scope: "mine" | "team"
    due?: WorkspaceDueFilter
    family?: WorkspaceFamilyFilter
    timezone: string
}>

export type TodayWorkspaceView = Readonly<{
    access: "allowed" | "forbidden" | "no_data_scope"
    viewer: {
        userId: string
        displayName: string
        activeRoleLabel: string
        timezone: string
    }
    freshness: {
        workItemsUpdatedAt: string
        projectionUpdatedAt: string
        projectionState: "fresh" | "stale" | "failed" | "rebuilding"
    }
    metrics: readonly WorkspaceMetric[]
    groups: readonly WorkspaceTaskGroup[]
    warnings: readonly WorkspaceWarning[]
    recent: readonly WorkspaceRecentItem[]
    canOpenTaskQueue: boolean
    temporaryPreviewLimitFallback: number
}>
