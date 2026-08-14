/**
 * W01 今日工作台 — 真实 HTTP（D03 work_item + /account/profile）。
 * 指标与列表复用同一授权规则；更新时间使用统计接口的 as_of。
 */

import type { AccountProfile } from "@/features/auth/api"
import { isRegisteredHandlerDestination } from "@/features/unified-task-queue/lib/handler-destination"
import {
    getWorkItemStats,
    listWorkItems,
    type WorkItemStats,
} from "@/features/work-items/api"
import { mapWorkItemDto, type WorkItemDto } from "@/features/work-items/types"
import { WORKSPACE_ROUTES, type WorkspaceId } from "@/lib/workspace-registry"
import { sequentialText } from "@/lib/ui-text"

import type {
    TodayWorkspaceQuery,
    TodayWorkspaceView,
    WorkspaceFamilyFilter,
    WorkspaceMetric,
    WorkspaceTaskGroup,
    WorkspaceWorkItem,
} from "../types"

const TEMPORARY_PREVIEW_LIMIT = 5

const FAMILY_META: Record<
    WorkspaceFamilyFilter,
    { label: string; defaultExpanded: boolean }
> = {
    approval: { label: "审批与确认", defaultExpanded: true },
    finance: { label: "票款与结算", defaultExpanded: true },
    fulfillment: { label: "履约与库存", defaultExpanded: false },
    exception: { label: "数据治理与异常", defaultExpanded: false },
}

const TYPE_META: Record<
    string,
    {
        label: string
        family: WorkspaceFamilyFilter
    }
> = {
    PROCUREMENT_CONFIRMATION: {
        label: "采购二次确认",
        family: "fulfillment",
    },
    LOW_MARGIN_MANAGER_CONFIRMATION: {
        label: "低毛利销售审批",
        family: "approval",
    },
    PURCHASE_ORDER_REVIEW: {
        label: "采购单财务审核",
        family: "finance",
    },
    SALES_CHANGE_IMPACT_REVIEW: {
        label: "销售变更履约影响复核",
        family: "fulfillment",
    },
    SALES_CHANGE_FINANCE_REVIEW: {
        label: "销售变更财务复核",
        family: "finance",
    },
    CARD_FUNDS_REVIEW: {
        label: "卡券票款复核",
        family: "finance",
    },
    CARD_FUNDS_DELTA_REVIEW: {
        label: "卡券票款差异复核",
        family: "finance",
    },
    CARD_SALES_MANAGER_APPROVAL: {
        label: "卡券销售领导审批",
        family: "approval",
    },
    CARD_SALES_OPERATION_APPROVAL: {
        label: "卡券运营审批",
        family: "approval",
    },
    OWNERSHIP_MIGRATION_SALES_CONFIRMATION: {
        label: "归属迁移销售确认",
        family: "approval",
    },
    OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION: {
        label: "归属迁移财务确认",
        family: "finance",
    },
    INVENTORY_ADJUSTMENT_REVIEW: {
        label: "库存调整复核",
        family: "fulfillment",
    },
    FINANCE_CORRECTION_REVIEW: {
        label: "财务纠错复核",
        family: "finance",
    },
    SUPPLIER_SETTLEMENT_REVIEW: {
        label: "供应商结算复核",
        family: "finance",
    },
    IMPORT_BUSINESS_CONFIRMATION: {
        label: "导入业务确认",
        family: "exception",
    },
    INTEGRATION_RESULT_UNKNOWN: {
        label: "集成结果未知",
        family: "exception",
    },
    BUSINESS_EXCEPTION: {
        label: "业务异常",
        family: "exception",
    },
}

const PRIORITY_RANK: Record<string, number> = {
    urgent: 1,
    high: 2,
    normal: 3,
    low: 4,
}

const STATUS_LABEL: Record<
    WorkspaceWorkItem["status"],
    { label: string; tone: WorkspaceWorkItem["statusTone"] }
> = {
    OPEN: { label: "待处理", tone: "info" },
    COMPLETED: { label: "已完成", tone: "success" },
    CLOSED: { label: "已关闭", tone: "neutral" },
}

const WORKSPACE_IDS = new Set<string>(
    WORKSPACE_ROUTES.map((workspace) => workspace.id),
)

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

    if (dto.processing_blocker) {
        messages.push(dto.processing_blocker)
    }

    return messages.map((blocker) => ({
        action: "PROCESS",
        code: blocker.code,
        message: blocker.message,
    }))
}

function allowedActions(dto: WorkItemDto): WorkspaceWorkItem["allowedActions"] {
    if (dto.processing_state === "APPROVAL_BLOCKED") return []

    return (dto.allowed_actions ?? []).filter(
        (action): action is "VIEW" | "PROCESS" | "START_PROCESSING" =>
            action === "VIEW" ||
            action === "PROCESS" ||
            action === "START_PROCESSING",
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

function dueBucket(
    dueAtIso: string,
    timezone: string,
): WorkspaceWorkItem["dueBucket"] {
    if (!dueAtIso) return "later"
    const due = new Date(dueAtIso).getTime()
    // 比较用服务端 due_at；无 as_of 时以 due 与本地日历日粗分桶仅供展示分组，
    // 不作为 projection as_of。
    const now = Date.now()
    if (due < now) return "overdue"
    try {
        const fmt = new Intl.DateTimeFormat("en-CA", {
            timeZone: timezone,
            year: "numeric",
            month: "2-digit",
            day: "2-digit",
        })
        const dueDay = fmt.format(new Date(dueAtIso))
        const todayDay = fmt.format(new Date(now))
        if (dueDay === todayDay) return "today"
    } catch {
        /* ignore */
    }
    return "later"
}

function mapWorkItem(dto: WorkItemDto, timezone: string): WorkspaceWorkItem {
    const task = mapWorkItemDto(dto)
    const meta = TYPE_META[dto.work_item_type] ?? {
        label: dto.work_item_type,
        family: "exception" as WorkspaceFamilyFilter,
    }
    const statusMeta = STATUS_LABEL[task.status]
    const createdAt = unixToIso(task.createdAt)
    const dueAt = unixToIso(task.dueAt)
    const bucket = dueBucket(dueAt, timezone)
    const configuredDestination = workspaceId(task.destinationWorkspaceId)
    const destinationWorkspaceId = configuredDestination ?? "W02"
    const queueContextId = task.queueContextId
    const routeContext = task.routeContext
    const hasRequiredRouting =
        Boolean(configuredDestination) &&
        isRegisteredHandlerDestination(
            task.handlerKey,
            task.destinationWorkspaceId,
        ) &&
        Boolean(queueContextId) &&
        (destinationWorkspaceId !== "W18" ||
            Boolean(routeContext?.confirmationScope))
    const serverAllowedActions = allowedActions(dto)
    const mappedActionBlockers = actionBlockers(dto)
    const routeBlocker = hasRequiredRouting
        ? []
        : [
              {
                  action: "PROCESS" as const,
                  code: "HANDLER_CONTEXT_MISSING",
                  message: "任务处理入口尚未配置完整，请刷新或联系管理员。",
              },
          ]

    return {
        workItemId: task.workItemId,
        taskVersion: task.taskVersion,
        workItemType: task.workItemType,
        workItemTypeLabel: meta.label,
        businessObjectType: task.businessObjectType,
        businessObjectId: task.businessObjectId,
        subjectVersion: task.subjectVersion,
        stableNumber: task.businessObjectId,
        objectTitle:
            dto.business_object_label ??
            `${meta.label} · ${task.businessObjectId}`,
        counterpartyName: task.counterpartyLabel ?? task.businessObjectType,
        status: task.status,
        statusLabel: statusMeta.label,
        statusTone: statusMeta.tone,
        processingState: task.processingState,
        assignmentMode: task.assignmentMode,
        priority:
            typeof task.priority === "number"
                ? task.priority
                : (PRIORITY_RANK[task.priority] ?? 3),
        createdAt,
        dueAt,
        ownerRoleLabel: task.ownerRoleLabel,
        ownerOrganizationLabel: task.ownerOrganization.displayName,
        ownerUserLabel: task.ownerUser?.displayName,
        reasonLabel: task.reasonLabel,
        impactSummary: task.impactSummary,
        allowedActions: hasRequiredRouting
            ? serverAllowedActions
            : serverAllowedActions.filter((action) => action === "VIEW"),
        actionBlockers: [...mappedActionBlockers, ...routeBlocker],
        destinationWorkspaceId,
        queueContextId,
        handlerKey: task.handlerKey,
        routeContext,
        enteredAtLabel: formatRelativeLabel(createdAt, timezone),
        dueAtLabel:
            bucket === "overdue"
                ? "已超期"
                : formatRelativeLabel(dueAt, timezone),
        dueBucket: bucket,
        family: meta.family,
    }
}

function buildMetrics(
    stats: WorkItemStats,
    scope: TodayWorkspaceQuery["scope"],
): WorkspaceMetric[] {
    const detail = "当前授权范围"
    return [
        {
            key: "mine",
            label:
                scope === "mine"
                    ? sequentialText.minePending
                    : sequentialText.teamPending,
            count: scope === "mine" ? stats.assigned : stats.team,
            visible: true,
            tone: "info",
            detail,
        },
        {
            key: "due_today",
            label: "今日到期",
            count: stats.due_today,
            visible: true,
            tone: "warning",
            detail,
        },
        {
            key: "overdue",
            label: "已超期",
            count: stats.overdue,
            visible: true,
            tone: "destructive",
            detail,
        },
        {
            key: "exception",
            label: "异常待处理",
            count: stats.exception,
            visible: true,
            tone: "warning",
            detail,
        },
    ]
}

function buildGroups(
    items: readonly WorkspaceWorkItem[],
    familyFilter?: WorkspaceFamilyFilter,
): WorkspaceTaskGroup[] {
    const families = (
        Object.keys(FAMILY_META) as WorkspaceFamilyFilter[]
    ).filter((f) => !familyFilter || f === familyFilter)

    return families
        .map((family) => {
            const familyItems = items.filter((i) => i.family === family)
            const meta = FAMILY_META[family]
            return {
                family,
                label: meta.label,
                total: familyItems.length,
                pagePreviewLimit: TEMPORARY_PREVIEW_LIMIT,
                previewLimitSource: "TEMPORARY_FALLBACK" as const,
                defaultExpanded: meta.defaultExpanded,
                items: familyItems.slice(0, TEMPORARY_PREVIEW_LIMIT),
            }
        })
        .filter((g) => g.total > 0 || Boolean(familyFilter))
}

/**
 * 拉取今日工作台视图：任务列表来自 `/admin/work-items`，观众身份来自 `/account/profile`。
 */
export async function fetchWorkspaceDashboard(
    query: TodayWorkspaceQuery,
    profile: AccountProfile,
): Promise<TodayWorkspaceView> {
    const [page, stats] = await Promise.all([
        listWorkItems({
            scope: query.scope,
            family: query.family,
            due: query.due,
            sort: "priority_due",
            timezone: query.timezone,
            page: 1,
            pageSize: 100,
        }),
        getWorkItemStats({
            scope: query.scope,
            family: query.family,
            due: query.due,
            timezone: query.timezone,
        }),
    ])

    const items = page.items.map((dto) => mapWorkItem(dto, query.timezone))

    const updatedAt = unixToIso(stats.as_of)

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
            projectionUpdatedAt: updatedAt,
            projectionState: updatedAt ? "fresh" : "stale",
        },
        metrics: buildMetrics(stats, query.scope),
        groups: buildGroups(items, query.family),
        warnings: [],
        recent: [],
        canOpenTaskQueue: true,
        temporaryPreviewLimitFallback: TEMPORARY_PREVIEW_LIMIT,
    }
}
