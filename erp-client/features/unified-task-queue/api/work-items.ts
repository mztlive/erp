/**
 * W02 统一待办队列 — 真实 HTTP（D03 `/admin/work-items`）。
 * 保持 queries.ts / 页面消费的函数签名与返回形状稳定。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { ApiError, Page } from "@/lib/api"

import { buildFilterSummary } from "../lib/filter-work-items"
import type {
    CloseSessionResult,
    CompleteSessionResult,
    InTaskActionKind,
    SessionLease,
    TransferSessionResult,
    UnifiedQueueFilters,
    UnifiedTaskQueueView,
    WorkItemActionRecord,
} from "../types"
import { FAMILY_LABELS } from "../types"
import {
    computeQueueCounts,
    enrichProcurementPresentation,
    mapWorkItemDto,
    unixToIso,
} from "./dto"
import type { AccountProfileDto, WorkItemDto } from "./dto"

/**
 * 真实后端错误映射类型；页面 `instanceof` 与 code 分支保持不变。
 * 由 ApiError（409/403 等）映射而来。
 */
export class WorkItemApiError extends Error {
    code:
        | "LEASE_LOST"
        | "VERSION_CONFLICT"
        | "PERMISSION_REVOKED"
        | "ACTION_NOT_ALLOWED"
        | "ALREADY_TERMINAL"
        | "NOT_FOUND"

    constructor(code: WorkItemApiError["code"], message: string) {
        super(message)
        this.name = "WorkItemApiError"
        this.code = code
    }
}

function isApiError(err: unknown): err is ApiError {
    return (
        typeof err === "object" &&
        err !== null &&
        "kind" in err &&
        "message" in err
    )
}

function mapApiError(err: unknown): never {
    if (isApiError(err)) {
        if (err.status === 403) {
            throw new WorkItemApiError(
                "PERMISSION_REVOKED",
                err.message || "当前权限已收回，不能执行该操作。",
            )
        }
        if (err.status === 404) {
            throw new WorkItemApiError(
                "NOT_FOUND",
                err.message || "任务不存在。",
            )
        }
        if (err.status === 409) {
            throw new WorkItemApiError(
                "VERSION_CONFLICT",
                err.message || "数据已变更，请刷新后重试",
            )
        }
        if (err.kind === "Validation") {
            throw new WorkItemApiError(
                "ACTION_NOT_ALLOWED",
                err.message || "操作不被允许。",
            )
        }
        throw new WorkItemApiError(
            "ACTION_NOT_ALLOWED",
            err.message || "操作失败。",
        )
    }
    throw err
}

function parseLockVersion(subjectVersion?: string): number {
    const n = Number(subjectVersion)
    if (!Number.isFinite(n) || n < 1) {
        throw new WorkItemApiError(
            "VERSION_CONFLICT",
            "缺少有效的数据版本，请刷新后重试。",
        )
    }
    return Math.trunc(n)
}

async function fetchProfile(): Promise<AccountProfileDto> {
    return apiGet<AccountProfileDto>("/account/profile")
}

async function listWorkItems(
    query: Record<string, unknown>,
): Promise<Page<WorkItemDto>> {
    return apiGet<Page<WorkItemDto>>("/admin/work-items", query)
}

/**
 * 拉取队列视图。scope 映射到后端 status / owner_user_id 筛选。
 * hold 范围：后端暂挂回到 UNCLAIMED，无独立 hold 状态 → 返回空列表并登记缺口。
 */
export async function fetchUnifiedTaskQueue(
    filters: UnifiedQueueFilters,
): Promise<UnifiedTaskQueueView> {
    const profile = await fetchProfile()

    if (filters.scope === "hold" || filters.scope === "team") {
        // backend_gap：无 hold/team 专用筛选；暂挂后状态回 UNCLAIMED，团队范围需数据范围服务
        const empty: UnifiedTaskQueueView = {
            queueContextId: `queue:W02:${filters.scope}`,
            permissionVersion: 1,
            permissionRevoked: false,
            freshness: { updatedAt: "", state: "stale" },
            filterSummary: buildFilterSummary(
                filters,
                0,
                FAMILY_LABELS[filters.family!] ?? "全部类型",
            ),
            total: 0,
            counts: { mine: 0, rolePool: 0, team: 0, hold: 0, overdue: 0 },
            items: [],
        }
        // 仍拉全量计数用列表
        const open = await listWorkItems({
            page: 1,
            page_size: 100,
            sort_by: "due_at",
            sort_dir: "asc",
        })
        const allItems = open.items
            .filter(
                (r) => r.status === "UNCLAIMED" || r.status === "IN_PROGRESS",
            )
            .map((row) => mapWorkItemDto(row, profile))
        empty.counts = computeQueueCounts(allItems)
        empty.filterSummary = buildFilterSummary(
            filters,
            0,
            filters.family ? FAMILY_LABELS[filters.family] : "全部类型",
        )
        return empty
    }

    const listQuery: Record<string, unknown> = {
        page: 1,
        page_size: 100,
        sort_by: "due_at",
        sort_dir: "asc",
    }

    if (filters.scope === "mine") {
        listQuery.owner_user_id = profile.userid
        listQuery.status = "IN_PROGRESS"
    } else if (filters.scope === "role_pool") {
        listQuery.status = "UNCLAIMED"
    }

    if (filters.workItemType) {
        listQuery.work_item_type = filters.workItemType
    }

    const page = await listWorkItems(listQuery)
    let items = await Promise.all(
        page.items.map(async (row) =>
            enrichProcurementPresentation(mapWorkItemDto(row, profile), row),
        ),
    )

    if (filters.family) {
        items = items.filter((i) => i.family === filters.family)
    }
    if (filters.due === "overdue") {
        items = items.filter(
            (i) => i.status.tone === "destructive" || i.dueAt.includes("超期"),
        )
    } else if (filters.due === "today") {
        items = items.filter(
            (i) => i.dueAt.includes("今天") || i.dueAt.includes("今日"),
        )
    }
    if (filters.query?.trim()) {
        const q = filters.query.trim().toLowerCase()
        items = items.filter(
            (i) =>
                i.id.toLowerCase().includes(q) ||
                i.businessObject.toLowerCase().includes(q) ||
                i.counterparty.toLowerCase().includes(q) ||
                i.workItemTypeLabel.toLowerCase().includes(q),
        )
    }

    // 角标：再取一份开放任务（不分 scope）做 counts
    const openPage = await listWorkItems({
        page: 1,
        page_size: 100,
        sort_by: "due_at",
        sort_dir: "asc",
    })
    const openItems = openPage.items
        .filter((r) => r.status === "UNCLAIMED" || r.status === "IN_PROGRESS")
        .map((row) => mapWorkItemDto(row, profile))

    const maxCreated = page.items.reduce(
        (m, r) => Math.max(m, r.created_at ?? 0),
        0,
    )

    return {
        queueContextId: `queue:W02:${filters.scope}`,
        permissionVersion: 1,
        permissionRevoked: false,
        freshness: {
            updatedAt: maxCreated > 0 ? unixToIso(maxCreated) : "",
            state: maxCreated > 0 ? "fresh" : "stale",
        },
        filterSummary: buildFilterSummary(
            filters,
            items.length,
            filters.family ? FAMILY_LABELS[filters.family] : "全部类型",
        ),
        total: items.length,
        counts: computeQueueCounts(openItems),
        items,
    }
}

export async function fetchUnifiedTaskQueueCounts(): Promise<
    ReturnType<typeof computeQueueCounts> & { total: number }
> {
    const profile = await fetchProfile()
    const page = await listWorkItems({
        page: 1,
        page_size: 100,
        sort_by: "due_at",
        sort_dir: "asc",
    })
    const items = page.items
        .filter((r) => r.status === "UNCLAIMED" || r.status === "IN_PROGRESS")
        .map((row) => mapWorkItemDto(row, profile))
    return { ...computeQueueCounts(items), total: items.length }
}

export async function claimWorkItem(input: {
    workItemId: string
    subjectVersion?: string
}): Promise<SessionLease> {
    try {
        const version = parseLockVersion(input.subjectVersion)
        const view = await apiPost<WorkItemDto>(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}/claim`,
            { version },
        )
        return {
            workItemId: view.id,
            ownerUserId: view.owner_user_id ?? "",
            subjectVersion: String(view.version),
        }
    } catch (err) {
        mapApiError(err)
    }
}

export async function batchClaimWorkItems(input: {
    workItemIds: readonly string[]
    subjectVersions?: Readonly<Record<string, string>>
}): Promise<SessionLease[]> {
    const claimed: SessionLease[] = []
    for (const workItemId of input.workItemIds) {
        try {
            claimed.push(
                await claimWorkItem({
                    workItemId,
                    subjectVersion: input.subjectVersions?.[workItemId],
                }),
            )
        } catch {
            // 单条领取失败不阻断其余条目
        }
    }
    return claimed
}

export async function applyWorkItemAction(input: {
    workItemId: string
    expectedSubjectVersion?: string
    ownerUserId?: string
    action: { kind: InTaskActionKind; note?: string }
}): Promise<WorkItemActionRecord> {
    try {
        const version = parseLockVersion(input.expectedSubjectVersion)
        if (input.action.kind === "DEFER") {
            const view = await apiPost<WorkItemDto>(
                `/admin/work-items/${encodeURIComponent(input.workItemId)}/defer`,
                { version, comment: input.action.note },
            )
            return {
                actionRecordId: `${view.id}:defer:${view.version}`,
                actionKind: "DEFER",
                workItemStatus: "IN_PROGRESS",
                evidenceNote: input.action.note,
                recordedAt: unixToIso(view.created_at) || "",
            }
        }
        // SAVE_EVIDENCE / QUERY_RESULT：后端无独立任务内动作接口（backend_gap）
        throw new WorkItemApiError(
            "ACTION_NOT_ALLOWED",
            "当前后端未提供该任务内动作接口，请在专业工作台完成。",
        )
    } catch (err) {
        if (err instanceof WorkItemApiError) throw err
        mapApiError(err)
    }
}

export async function completeWorkItem(input: {
    workItemId: string
    expectedSubjectVersion?: string
    ownerUserId?: string
    decision: { kind: string; note?: string; summary?: string }
}): Promise<CompleteSessionResult> {
    try {
        const version = parseLockVersion(input.expectedSubjectVersion)
        const view = await apiPost<WorkItemDto>(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}/complete`,
            { version },
        )
        return {
            workItemId: view.id,
            workItemStatus: "COMPLETED",
            completionRecordId: `${view.id}:complete:${view.version}`,
            businessResult: {
                kind: input.decision.kind,
                reference: view.id,
                summary:
                    input.decision.summary ??
                    input.decision.note ??
                    `业务结论「${input.decision.kind}」与任务完成同一事务生效`,
            },
            subjectVersion: String(view.version),
        }
    } catch (err) {
        mapApiError(err)
    }
}

export async function closeWorkItem(input: {
    workItemId: string
    expectedSubjectVersion?: string
    ownerUserId?: string
    closeAllowed: boolean
    closure: {
        kind: "CLOSE_DUPLICATE" | "CLOSE_MISROUTED" | "CLOSE_WITH_REPLACEMENT"
        reasonCode: string
        replacementWorkItemId?: string
        comment?: string
    }
}): Promise<CloseSessionResult> {
    if (!input.closeAllowed) {
        throw new WorkItemApiError(
            "ACTION_NOT_ALLOWED",
            "审批、确认、结果未知和补偿任务不允许人工关闭。",
        )
    }
    try {
        const version = parseLockVersion(input.expectedSubjectVersion)
        const view = await apiPost<WorkItemDto>(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}/close`,
            {
                version,
                close_reason_code: input.closure.reasonCode,
                close_reason_text: input.closure.comment,
            },
        )
        return {
            workItemId: view.id,
            workItemStatus: "CLOSED",
            closureRecordId: `${view.id}:close:${view.version}`,
            reasonCode: view.close_reason_code ?? input.closure.reasonCode,
            replacementWorkItemId: input.closure.replacementWorkItemId,
        }
    } catch (err) {
        mapApiError(err)
    }
}

export async function transferWorkItem(input: {
    workItemId: string
    expectedSubjectVersion?: string
    transfer: { targetUserId: string; reason: string }
}): Promise<TransferSessionResult> {
    try {
        const version = parseLockVersion(input.expectedSubjectVersion)
        // 后端要求 owner_role + owner_user_id；targetUserId 即账号 userId，角色占位
        const view = await apiPost<WorkItemDto>(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}/transfer`,
            {
                version,
                owner_role: "assignee",
                owner_user_id: input.transfer.targetUserId,
                comment: input.transfer.reason,
            },
        )
        return {
            workItemId: view.id,
            transferRecordId: `${view.id}:transfer:${view.version}`,
            targetUserId: input.transfer.targetUserId,
            subjectVersion: String(view.version),
        }
    } catch (err) {
        mapApiError(err)
    }
}
