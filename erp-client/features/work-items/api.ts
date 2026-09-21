import { apiGet, apiPost, type Page } from "@/lib/api"
import { isApiError } from "@/lib/api/errors"

import type {
    WorkItemDto,
    WorkItemConflict,
    WorkItemConflictCode,
    WorkItemConflictDataDto,
    WorkItemResponsibilityCommand,
    WorkItemReassignCandidate,
    WorkItemScope,
} from "./types"

export type WorkItemListParams = Readonly<{
    scope: WorkItemScope
    family?: string
    workItemType?: string
    status?: "COMPLETED" | "CLOSED"
    due?: "today" | "overdue"
    blocked?: boolean
    priorities?: readonly number[]
    query?: string
    /** 逗号分隔的当前处理人稳定 ID；只收窄授权结果。 */
    handlerUserIds?: string
    /** 逗号分隔的来源销售单稳定 ID；只收窄授权结果。 */
    salesOrderIds?: string
    /** 逗号分隔的来源采购单稳定 ID；只收窄授权结果。 */
    purchaseOrderIds?: string
    sort?: "priority_due" | "due_asc" | "created_desc"
    cursor?: string
    queueContextId?: string
    scopeVersion?: string
    currentWorkItemId?: string
    timezone: string
    page?: number
    pageSize?: number
}>

export type WorkItemPage = Page<WorkItemDto> & {
    queue_context_id?: string | null
    scope_version: string
}

export type WorkItemStatsParams = Readonly<{
    scope: WorkItemScope
    family?: string
    workItemType?: string
    due?: "today" | "overdue"
    blocked?: boolean
    /** 逗号分隔的当前处理人稳定 ID；只收窄授权结果。 */
    handlerUserIds?: string
    /** 逗号分隔的来源销售单稳定 ID；只收窄授权结果。 */
    salesOrderIds?: string
    /** 逗号分隔的来源采购单稳定 ID；只收窄授权结果。 */
    purchaseOrderIds?: string
    timezone: string
}>

export type WorkItemStats = Readonly<{
    assigned: number
    due_today: number
    overdue: number
    exception: number
    blocked?: number
    started?: number
    inbox?: number
    /** 新旧服务滚动发布期间允许缺省；缺省时前端不得用当前页条目推算。 */
    family_counts?: Readonly<{
        approval: number
        procurement: number
        fulfillment: number
        finance: number
        exception: number
    }>
    as_of: number
}>

const WORK_ITEM_CONFLICT_CODES = new Set<WorkItemConflictCode>([
    "WORK_ITEM_VERSION_CONFLICT",
    "WORK_ITEM_RESPONSIBILITY_CONFLICT",
])

/** 判断未知值是否为可安全读取的对象。 */
const isRecord = (value: unknown): value is Record<string, unknown> =>
    typeof value === "object" && value !== null

/** 从统一 API 异常中读取责任命令的结构化 409 数据。 */
export const parseWorkItemConflict = (
    error: unknown,
): WorkItemConflict | undefined => {
    if (!isApiError(error) || error.status !== 409) return undefined
    const envelope = isRecord(error.responseData)
        ? error.responseData
        : undefined
    const rawCode =
        error.code ??
        (typeof envelope?.code === "string" ? envelope.code : undefined)
    if (
        typeof rawCode !== "string" ||
        !WORK_ITEM_CONFLICT_CODES.has(rawCode as WorkItemConflictCode)
    ) {
        return undefined
    }
    const data = isRecord(envelope?.data)
        ? (envelope.data as WorkItemConflictDataDto)
        : undefined
    const current = data?.current_work_item
    return {
        code: rawCode as WorkItemConflictCode,
        currentWorkItem: isRecord(current) ? (current as WorkItemDto) : null,
    }
}

/** 查询经服务端责任范围过滤的任务；前端不得先取全量再隐藏。 */
export function listWorkItems(
    params: WorkItemListParams,
): Promise<WorkItemPage> {
    return apiGet<WorkItemPage>("/admin/work-items", {
        scope: params.scope,
        family: params.family,
        work_item_type: params.workItemType,
        status: params.status,
        due: params.due,
        blocked: params.blocked ? "1" : undefined,
        priorities: params.priorities?.join(","),
        q: params.query,
        handler_user_ids: params.handlerUserIds,
        sales_order_ids: params.salesOrderIds,
        purchase_order_ids: params.purchaseOrderIds,
        sort: params.sort ?? "priority_due",
        cursor: params.cursor,
        queue_context_id: params.queueContextId,
        scope_version: params.scopeVersion,
        current_work_item_id: params.currentWorkItemId,
        timezone: params.timezone,
        page: params.page ?? 1,
        page_size: params.pageSize ?? 100,
    })
}

/** 查询与待办列表复用同一授权范围的统计快照。 */
export function getWorkItemStats(
    params: WorkItemStatsParams,
): Promise<WorkItemStats> {
    return apiGet<WorkItemStats>("/admin/work-items/stats", {
        scope: params.scope,
        family: params.family,
        work_item_type: params.workItemType,
        due: params.due,
        blocked: params.blocked ? "1" : undefined,
        handler_user_ids: params.handlerUserIds,
        sales_order_ids: params.salesOrderIds,
        purchase_order_ids: params.purchaseOrderIds,
        timezone: params.timezone,
    })
}

/** 查询单条权限安全的最新任务投影。 */
export const getWorkItem = (workItemId: string): Promise<WorkItemDto> =>
    apiGet<WorkItemDto>(`/admin/work-items/${encodeURIComponent(workItemId)}`)

/** 查询当前管理范围内、可接收该任务及其采购级联任务的具体账号。 */
export const getWorkItemReassignCandidates = (
    workItemId: string,
): Promise<WorkItemReassignCandidate[]> =>
    apiGet<WorkItemReassignCandidate[]>(
        `/admin/work-items/${encodeURIComponent(workItemId)}/reassign-candidates`,
    )

/** 发送一条责任命令；任务版本只能来自最近一次服务端查询。 */
export function submitWorkItemResponsibility(
    command: WorkItemResponsibilityCommand,
): Promise<WorkItemDto> {
    const id = encodeURIComponent(command.workItemId)
    const common = {
        expected_task_version: command.expectedTaskVersion,
        idempotency_key: command.idempotencyKey,
    }

    switch (command.kind) {
        case "REASSIGN":
            return apiPost<WorkItemDto>(`/admin/work-items/${id}/reassign`, {
                ...common,
                target_user_id: command.targetUserId,
                reason: command.reason,
            })
        case "CLOSE":
            return apiPost<WorkItemDto>(`/admin/work-items/${id}/close`, {
                ...common,
                reason_code: command.reasonCode,
                replacement_work_item_id: command.replacementWorkItemId,
                comment: command.comment,
            })
    }
}
