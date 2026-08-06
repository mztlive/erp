/**
 * W01 今日工作台 — 客户端契约类型（对齐 docs/ui-workspaces/w01 §8）。
 * 任务行与指标由 `/admin/work-items` 投影；工作台汇总 as_of 依赖 E3（见 p4-evidence/F7.md）。
 */

import type { StatusTone } from "@/components/ui/status-badge"
import type { WorkspaceId } from "@/lib/workspace-registry"

export type WorkspaceDueFilter = "today" | "overdue"
export type WorkspaceFamilyFilter =
  | "approval"
  | "finance"
  | "fulfillment"
  | "exception"
  | "procurement"

export type WorkspaceMetricKey =
  | "mine"
  | "due_today"
  | "overdue"
  | "exception"

export type WorkspaceActionCode = "VIEW" | "PROCESS"

export type WorkspaceWorkItem = Readonly<{
  workItemId: string
  workItemType: string
  workItemTypeLabel: string
  businessObjectType: string
  businessObjectId: string
  stableNumber: string
  objectTitle: string
  counterpartyName: string
  status: string
  statusLabel: string
  statusTone: StatusTone
  priority: number
  createdAt: string
  dueAt: string
  ownerRoleLabel: string
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
  queueContextId: string
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
  previewLimitSource?: "SERVER" | "TEMPORARY_FALLBACK"
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
  scope: "mine" | "role_pool"
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
