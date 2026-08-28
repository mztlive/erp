/**
 * W01 工作台 — 任务列表与指标。
 * 指标与列表使用同一授权口径；不得对已加载条目求和。
 */

import type { AccountProfile } from "@/features/auth/api"
import { listApprovalInstances } from "@/features/approval-workflow/api"
import type {
    ApprovalInstanceListItem,
    ApprovalInstanceListPage,
} from "@/features/approval-workflow/types"
import {
    getWorkItemStats,
    listWorkItems,
    type WorkItemStats,
} from "@/features/work-items/api"
import { mapWorkItemDto, type WorkItemDto } from "@/features/work-items/types"
import { hasPermission } from "@/lib/permissions"
import { WORKSPACE_ROUTES, type WorkspaceId } from "@/lib/workspace-registry"

import type {
    TodayWorkspaceQuery,
    TodayWorkspaceView,
    WorkspaceActionCode,
    WorkspaceFamilyCounts,
    WorkspaceFamilyFilter,
    WorkspaceMetric,
    WorkspaceWorkItem,
} from "../types"
import {
    fulfillmentListNumber,
    fulfillmentObjectTitle,
} from "../lib/fulfillment-title"
import {
    PRIORITY_RANK,
    STATUS_LABEL,
    TYPE_META,
    workspaceTypeLabel,
} from "./work-item-meta"

const WORKSPACE_IDS = new Set<string>(
    WORKSPACE_ROUTES.map((workspace) => workspace.id),
)

const WORKSPACE_ACTIONS = new Set<WorkspaceActionCode>([
    "VIEW",
    "PROCESS",
    "REASSIGN",
    "OPEN_DOCUMENT",
    "APPROVE",
    "REJECT",
    "RESUME_CURRENT_APPROVER",
    "CANCEL_BLOCKED_APPROVAL",
])

function workspaceId(value?: string): WorkspaceId | undefined {
    return value && WORKSPACE_IDS.has(value)
        ? (value as WorkspaceId)
        : undefined
}

function actionBlockers(dto: WorkItemDto): WorkspaceWorkItem["actionBlockers"] {
    const messages = (dto.action_blockers ?? []).map((blocker) =>
        typeof blocker === "string"
            ? { code: "ACTION_BLOCKED", message: blocker }
            : blocker,
    )
    if (dto.processing_blocker) messages.push(dto.processing_blocker)
    return messages.map((blocker) => ({
        action:
            dto.processing_state === "APPROVAL_BLOCKED" ? "APPROVE" : "PROCESS",
        code: blocker.code,
        message: blocker.message,
    }))
}

function allowedActions(dto: WorkItemDto): WorkspaceWorkItem["allowedActions"] {
    return (dto.allowed_actions ?? []).filter(
        (action): action is WorkspaceActionCode =>
            WORKSPACE_ACTIONS.has(action as WorkspaceActionCode),
    )
}

function unixToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return ""
    return new Date(secs * 1000).toISOString()
}

function formatRelativeLabel(iso: string, timezone: string): string {
    if (!iso) return "—"
    try {
        return new Intl.DateTimeFormat("zh-CN", {
            timeZone: timezone,
            month: "numeric",
            day: "numeric",
            hour: "2-digit",
            minute: "2-digit",
            hour12: false,
        }).format(new Date(iso))
    } catch {
        return iso
    }
}

function fulfillmentStableNumber(
    workItemType: string,
    label: string | null | undefined,
    typeLabel: string,
): string {
    if (workItemType === "FULFILLMENT_OPERATION") {
        return fulfillmentListNumber(label ?? undefined, typeLabel)
    }
    return label?.trim() || typeLabel || "业务单号待补全"
}

function fulfillmentTitle(
    workItemType: string,
    label: string | null | undefined,
    typeLabel: string,
): string {
    if (workItemType === "FULFILLMENT_OPERATION") {
        return fulfillmentObjectTitle(label ?? undefined, typeLabel)
    }
    return label?.trim() || typeLabel || "业务对象名称待补全"
}

function dueBucket(
    dueAtIso: string,
    timezone: string,
): WorkspaceWorkItem["dueBucket"] {
    if (!dueAtIso) return "later"
    const due = new Date(dueAtIso).getTime()
    const now = Date.now()
    if (due < now) return "overdue"
    try {
        const fmt = new Intl.DateTimeFormat("en-CA", {
            timeZone: timezone,
            year: "numeric",
            month: "2-digit",
            day: "2-digit",
        })
        if (fmt.format(new Date(dueAtIso)) === fmt.format(new Date(now))) {
            return "today"
        }
    } catch {
        /* ignore */
    }
    return "later"
}

/**
 * 把任务 DTO 转成工作台行。责任与动作只读服务端字段。
 */
export function mapWorkspaceWorkItem(
    dto: WorkItemDto,
    timezone: string,
): WorkspaceWorkItem {
    const task = mapWorkItemDto(dto)
    const meta = TYPE_META[dto.work_item_type] ?? {
        label: workspaceTypeLabel(dto.work_item_type, dto.business_object_type),
        family: "exception" as WorkspaceFamilyFilter,
    }
    const statusMeta = STATUS_LABEL[task.status]
    const createdAt = unixToIso(task.createdAt)
    const dueAt = unixToIso(task.dueAt)
    const bucket = dueBucket(dueAt, timezone)
    const configuredDestination = workspaceId(task.destinationWorkspaceId)
    const destinationWorkspaceId = configuredDestination ?? "W01"
    const ownerUserLabel = task.ownerUser?.displayName || "处理人待确认"

    return {
        workItemId: task.workItemId,
        taskVersion: task.taskVersion,
        workItemType: task.workItemType,
        workItemTypeLabel: workspaceTypeLabel(
            dto.work_item_type,
            dto.business_object_type,
        ),
        businessObjectType: task.businessObjectType,
        businessObjectId: task.businessObjectId,
        subjectVersion: task.subjectVersion,
        stableNumber: fulfillmentStableNumber(
            dto.work_item_type,
            dto.business_object_label,
            workspaceTypeLabel(dto.work_item_type, dto.business_object_type),
        ),
        objectTitle: fulfillmentTitle(
            dto.work_item_type,
            dto.business_object_label,
            workspaceTypeLabel(dto.work_item_type, dto.business_object_type),
        ),
        counterpartyName: task.counterpartyLabel,
        listSummary: task.listSummary,
        status: task.status,
        statusLabel: statusMeta.label,
        statusTone: statusMeta.tone,
        processingState: task.processingState,
        priority:
            typeof task.priority === "number"
                ? task.priority
                : (PRIORITY_RANK[String(task.priority).toLowerCase()] ?? 3),
        createdAt,
        dueAt: dueAt || undefined,
        ownerRole: task.ownerRole,
        ownerRoleLabel: task.ownerRoleLabel,
        ownerOrganizationLabel: task.ownerOrganization.displayName,
        ownerUserLabel,
        reasonLabel: task.reasonLabel,
        reasonCode: task.reasonCode,
        impactSummary: task.impactSummary,
        nextActionHint: task.nextActionHint,
        allowedActions: allowedActions(dto),
        actionBlockers: actionBlockers(dto),
        destinationWorkspaceId,
        queueContextId: task.queueContextId,
        handlerKey: task.handlerKey,
        routeContext: task.routeContext,
        enteredAtLabel: formatRelativeLabel(createdAt, timezone),
        dueAtLabel:
            bucket === "overdue"
                ? "已超期"
                : formatRelativeLabel(dueAt, timezone),
        dueBucket: bucket,
        family: meta.family,
        approvalProcessInstanceId: task.approvalProcessInstanceId,
        approvalNodeExecutionId: task.approvalNodeExecutionId,
        approval: task.approvalContext
            ? {
                  instanceId: task.approvalContext.instanceId,
                  currentRoundNo: task.approvalContext.currentRoundNo,
                  currentNodeLabel: task.approvalContext.currentNodeLabel,
                  currentAssigneeLabel:
                      task.approvalContext.currentAssigneeLabel ??
                      "审批人待补全",
                  lastRejectReason: task.approvalContext.latestRejectionReason,
                  processVersion: task.approvalContext.processVersion,
                  status: task.approvalContext.status,
              }
            : undefined,
        rootBusinessObjectId: task.rootBusinessObjectId,
        summarySections: task.summarySections,
        briefLines: task.briefLines,
        briefMoreCount: task.briefMoreCount,
    }
}

function buildMetrics(
    stats: WorkItemStats,
    startedCount: number,
    canReadStarted: boolean,
): WorkspaceMetric[] {
    return [
        {
            key: "inbox",
            label: "待我处理",
            count: stats.inbox ?? stats.assigned,
            visible: true,
            tone: "info",
        },
        {
            key: "overdue",
            label: "已超期",
            count: stats.overdue,
            visible: true,
            tone: "destructive",
        },
        {
            key: "blocked",
            label: "受阻",
            count: stats.blocked ?? 0,
            visible: stats.blocked != null,
            tone: "warning",
        },
        {
            key: "started",
            label: "我发起的",
            count: startedCount,
            visible: canReadStarted,
            tone: "info",
        },
    ]
}

/** 映射服务端任务族统计；旧服务缺少字段时保持不可见，不能回退当前页求和。 */
function buildFamilyCounts(
    stats: WorkItemStats,
): WorkspaceFamilyCounts | undefined {
    const counts = stats.family_counts
    if (!counts) return undefined
    return {
        approval: counts.approval,
        procurement: counts.procurement,
        fulfillment: counts.fulfillment,
        finance: counts.finance,
        exception: counts.exception,
    }
}

/**
 * 拉取工作台视图：任务列表与指标分别请求，口径一致。
 */
export async function fetchWorkspaceDashboard(
    query: TodayWorkspaceQuery,
    profile: AccountProfile,
): Promise<TodayWorkspaceView> {
    const canManage = [
        "approval_instance:resume",
        "approval_instance:cancel_blocked",
    ].some((permission) => hasPermission(profile.permissions, permission))
    const canReadStarted = hasPermission(
        profile.permissions,
        "approval_instance:read",
    )
    const view =
        (query.view === "managed" && !canManage) ||
        (query.view === "started" && !canReadStarted)
            ? "inbox"
            : query.view

    const startedPagePromise: Promise<ApprovalInstanceListPage> = canReadStarted
        ? listApprovalInstances({
              view: "started",
              cursor: view === "started" ? query.cursor : undefined,
              limit: view === "started" ? 20 : 1,
          })
        : Promise.resolve({ items: [], total: 0 })

    const [page, stats, startedPage] = await Promise.all([
        view === "started"
            ? startedPagePromise.then((result) => ({
                  items: [] as WorkItemDto[],
                  startedItems: result.items,
                  total: result.total ?? result.items.length,
                  nextCursor: result.nextCursor,
              }))
            : listWorkItems({
                  scope: view === "managed" ? "managed" : "mine",
                  family: query.family,
                  workItemType: query.workItemType,
                  due: query.due,
                  blocked: query.blocked,
                  query: query.query,
                  sort: query.sort,
                  cursor: query.cursor,
                  timezone: query.timezone,
                  page: 1,
                  pageSize: 50,
              }).then((result) => ({
                  items: result.items,
                  startedItems: [],
                  total: result.total,
                  nextCursor: undefined as string | undefined,
              })),
        getWorkItemStats({
            scope: "mine",
            family: query.family,
            workItemType: query.workItemType,
            due: query.due,
            blocked: query.blocked,
            timezone: query.timezone,
        }),
        startedPagePromise,
    ])

    const items =
        view === "started"
            ? page.startedItems.map((item) =>
                  startedInstanceToWorkItem(item, query.timezone),
              )
            : page.items.map((dto) => mapWorkspaceWorkItem(dto, query.timezone))

    const updatedAt = unixToIso(stats.as_of)
    const metrics = buildMetrics(
        stats,
        startedPage.total ?? startedPage.items.length,
        canReadStarted,
    )

    return {
        access: "allowed",
        viewer: {
            userId: profile.userid,
            displayName: profile.name || profile.account,
            activeRoleLabel:
                profile.role_ids.length > 0
                    ? profile.role_ids.join("、")
                    : "已登录",
            timezone: query.timezone,
        },
        freshness: {
            workItemsUpdatedAt: updatedAt,
            statsUpdatedAt: updatedAt,
            statsState: updatedAt ? "fresh" : "stale",
            projectionUpdatedAt: updatedAt,
            projectionState: updatedAt ? "fresh" : "stale",
        },
        metrics,
        familyCounts: view === "started" ? undefined : buildFamilyCounts(stats),
        items,
        nextCursor: page.nextCursor,
        total: page.total,
        warnings: [],
        recent: [],
    }
}

/**
 * 顶栏待办角标：本人开放任务数，来自服务端统计。
 */
export async function fetchWorkspaceInboxCount(): Promise<{ mine: number }> {
    const stats = await getWorkItemStats({
        scope: "mine",
        timezone:
            Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
    })
    return { mine: stats.inbox ?? stats.assigned }
}

function startedInstanceToWorkItem(
    item: ApprovalInstanceListItem,
    timezone: string,
): WorkspaceWorkItem {
    const businessObjectType = item.documentType ?? ""
    const typeLabel = workspaceTypeLabel(
        "APPROVAL_INSTANCE",
        businessObjectType,
    )
    const destinationWorkspaceId = approvalDestination(businessObjectType)
    const status = startedStatus(item.status)
    const createdAt = unixToIso(item.startedAt)
    const currentNodeLabel = item.currentNodeName ?? item.currentNodeKey
    const currentAssigneeLabel = item.currentAssigneeName
    const listSummary = [currentNodeLabel, currentAssigneeLabel]
        .map((part) => part?.trim())
        .filter(Boolean)
        .join(" · ")
    const paymentReversal = businessObjectType === "payment_reversal"
    return {
        workItemId: item.instanceId,
        taskVersion: "",
        workItemType: "APPROVAL_INSTANCE",
        workItemTypeLabel: item.processName ?? typeLabel,
        businessObjectType,
        businessObjectId: item.documentId ?? item.instanceId,
        subjectVersion: "",
        stableNumber: item.documentLabel ?? "单号待补全",
        objectTitle: item.documentLabel
            ? `${typeLabel} ${item.documentLabel}`
            : typeLabel,
        status: status.workItemStatus,
        statusLabel: status.statusLabel,
        statusTone: status.statusTone,
        processingState:
            item.status === "BLOCKED" ? "APPROVAL_BLOCKED" : "READY",
        priority: 3,
        createdAt,
        ownerRole: "approval_initiator",
        ownerRoleLabel: "审批",
        ownerOrganizationLabel: "",
        ownerUserLabel: item.currentAssigneeName ?? "处理人待确认",
        reasonLabel: "我发起的审批",
        listSummary: listSummary || undefined,
        impactSummary: paymentReversal
            ? "审批通过前原付款保持不变；通过后系统追加冲正记录并回冲原付款。"
            : "审批完成前，业务单据保持当前状态。",
        nextActionHint: paymentReversal
            ? "可打开冲正详情查看完整审批进度与原付款。"
            : "打开单据查看完整审批进度。",
        allowedActions: ["VIEW"],
        actionBlockers: [],
        destinationWorkspaceId,
        handlerKey: destinationWorkspaceId === "W01" ? "" : "document_approval",
        enteredAtLabel: formatRelativeLabel(createdAt, timezone),
        dueAtLabel: "—",
        dueBucket: "later",
        family: "approval",
        approvalProcessInstanceId: item.instanceId,
        approval: {
            instanceId: item.instanceId,
            currentRoundNo: item.currentRoundNo,
            currentNodeLabel:
                item.currentNodeName ?? item.currentNodeKey ?? "—",
            currentAssigneeLabel: item.currentAssigneeName ?? "—",
            processName: item.processName ?? "审批流程",
            processVersion: item.processVersion ?? "",
            lastRejectReason: item.latestRejectionSummary,
            status: item.status,
        },
    }
}

/** 按单据类型解析“我发起的”记录应回到的正式工作面。 */
function approvalDestination(documentType: string): WorkspaceId {
    if (
        ["sales_order", "voucher_sales_order", "sales_change_order"].includes(
            documentType,
        )
    ) {
        return "W05"
    }
    if (["purchase_order", "purchase_change_order"].includes(documentType)) {
        return "W08"
    }
    if (documentType === "stock_adjustment") return "W10"
    if (
        ["customer_receipt", "customer_refund", "receipt_reversal"].includes(
            documentType,
        )
    ) {
        return "W11"
    }
    if (
        ["supplier_payment", "supplier_refund", "payment_reversal"].includes(
            documentType,
        )
    ) {
        return "W12"
    }
    return "W01"
}

/** 把审批实例状态映射为只读跟踪行的展示状态。 */
function startedStatus(status: string): Pick<
    WorkspaceWorkItem,
    "statusLabel" | "statusTone"
> & {
    workItemStatus: WorkspaceWorkItem["status"]
} {
    switch (status) {
        case "APPROVED":
            return {
                workItemStatus: "COMPLETED",
                statusLabel: "已通过",
                statusTone: "success",
            }
        case "CANCELLED":
            return {
                workItemStatus: "CLOSED",
                statusLabel: "已取消",
                statusTone: "neutral",
            }
        case "BLOCKED":
            return {
                workItemStatus: "OPEN",
                statusLabel: "受阻",
                statusTone: "destructive",
            }
        default:
            return {
                workItemStatus: "OPEN",
                statusLabel: "审批中",
                statusTone: "info",
            }
    }
}

export { FAMILY_META } from "./work-item-meta"
