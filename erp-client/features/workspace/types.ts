/**
 * W01 我的工作台 — 列表 + 详情主从。
 *
 * 只有「待我处理」口径，不存在团队分区。指标数量来自服务端，不得对已加载条目求和。
 */

import type { StatusTone } from "@/components/ui/status-badge"
import type { WorkItemStatus } from "@/features/work-items/types"
import type { WorkspaceId } from "@/lib/workspace-registry"

export type WorkspaceDueFilter = "today" | "overdue"
export type WorkspaceFamilyFilter =
    | "approval"
    | "finance"
    | "fulfillment"
    | "exception"
export type WorkspaceViewFilter = "inbox" | "started" | "managed"
export type WorkspaceSort = "priority_due" | "due_asc" | "created_desc"

export type WorkspaceMetricKey = "inbox" | "overdue" | "blocked" | "started"

export type WorkspaceActionCode =
    | "VIEW"
    | "PROCESS"
    | "OPEN_DOCUMENT"
    | "APPROVE"
    | "REJECT"
    | "RESUME_CURRENT_APPROVER"
    | "CANCEL_BLOCKED_APPROVAL"

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
    counterpartyName?: string
    listSummary?: string
    status: WorkItemStatus
    statusLabel: string
    statusTone: StatusTone
    processingState: "READY" | "APPROVAL_BLOCKED"
    priority: number
    createdAt: string
    dueAt?: string
    ownerRoleLabel: string
    ownerOrganizationLabel: string
    ownerUserLabel: string
    reasonLabel: string
    impactSummary: string
    nextActionHint: string
    allowedActions: readonly WorkspaceActionCode[]
    actionBlockers: readonly {
        action: string
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
    approvalProcessInstanceId?: string
    approvalNodeExecutionId?: string
    rootBusinessObjectId?: string
    summarySections?: readonly Readonly<{
        label: string
        value: string
        numeric?: boolean
    }>[]
    briefLines?: readonly Readonly<{
        title: string
        quantity?: string
        dueLabel?: string
    }>[]
    briefMoreCount?: number
    approval?: {
        instanceId: string
        currentRoundNo: number
        currentNodeLabel: string
        currentAssigneeLabel: string
        lastRejectorLabel?: string
        lastRejectReason?: string
        processName: string
        processVersion: string
        status: string
    }
}>

export type WorkspaceMetric = Readonly<{
    key: WorkspaceMetricKey
    label: string
    count: number
    visible: boolean
    tone: StatusTone
    detail?: string
}>

export type WorkspaceWarning = Readonly<{
    warningId: string
    kind: string
    severity: "warning" | "destructive" | "info"
    title: string
    description: string
    occurredAt: string
    destinationWorkspaceId: WorkspaceId
    objectId?: string
}>

export type WorkspaceRecentItem = Readonly<{
    id: string
    label: string
    destinationWorkspaceId: WorkspaceId
    objectId?: string
    href: string
}>

export type TodayWorkspaceQuery = Readonly<{
    view: WorkspaceViewFilter
    due?: WorkspaceDueFilter
    blocked?: boolean
    family?: WorkspaceFamilyFilter
    workItemType?: string
    query?: string
    sort: WorkspaceSort
    cursor?: string
    currentWorkItemId?: string
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
        statsUpdatedAt: string
        statsState: "fresh" | "stale" | "failed"
        projectionUpdatedAt: string
        projectionState: "fresh" | "stale" | "failed" | "rebuilding"
    }
    metrics: readonly WorkspaceMetric[]
    items: readonly WorkspaceWorkItem[]
    nextCursor?: string
    total: number
    warnings: readonly WorkspaceWarning[]
    recent: readonly WorkspaceRecentItem[]
}>
