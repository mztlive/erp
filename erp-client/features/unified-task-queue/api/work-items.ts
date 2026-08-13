/**
 * W02 统一待办队列 — 真实 HTTP（D03 `/admin/work-items`）。
 * 保持 queries.ts / 页面消费的函数签名与返回形状稳定。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { ApiError, Page } from "@/lib/api"

import { buildFilterSummary } from "./filter-work-items"
import { fetchProcurementWorkItemPresentation } from "@/features/procurement-confirmation/api"
import type {
    CloseSessionResult,
    CompleteSessionResult,
    InTaskActionKind,
    QueueWorkItemView,
    SessionLease,
    TransferSessionResult,
    UnifiedQueueFilters,
    UnifiedTaskQueueView,
    WorkItemActionCode,
    WorkItemActionRecord,
    WorkItemFamily,
    WorkItemStatusCode,
} from "./types"
import { FAMILY_LABELS } from "./types"

/** 后端 work_item 列表/详情 DTO。 */
type WorkItemDto = {
    id: string
    work_item_type: string
    business_object_type: string
    business_object_id: string
    subject_version?: string | null
    status: string
    owner_role?: string | null
    owner_user_id?: string | null
    priority: string
    due_at?: number | null
    reason_code?: string | null
    impact_summary?: string | null
    completion_action: string
    completed_at?: number | null
    completed_by?: string | null
    close_reason_code?: string | null
    close_reason_text?: string | null
    version: number
    created_at: number
}

type AccountProfileDto = {
    userid: string
    account: string
    name: string
    role_ids: string[]
}

const TYPE_META: Record<
    string,
    {
        label: string
        family: WorkItemFamily
        handlerKey: string
        handlerHref?: string
        processorGroup: string
        closeAllowed: boolean
    }
> = {
    PROCUREMENT_CONFIRMATION: {
        label: "采购二次确认",
        family: "procurement",
        handlerKey: "procurement_confirmation",
        handlerHref: "/procurement/confirm",
        processorGroup: "procurement",
        closeAllowed: false,
    },
    PURCHASE_ORDER_REVIEW: {
        label: "采购单财务审核",
        family: "finance",
        handlerKey: "po_review",
        handlerHref: "/procurement/orders",
        processorGroup: "finance_review",
        closeAllowed: false,
    },
    CARD_FUNDS_REVIEW: {
        label: "卡券票款复核",
        family: "finance",
        handlerKey: "card_funds",
        handlerHref: "/finance/card-funds",
        processorGroup: "finance_review",
        closeAllowed: false,
    },
    CARD_FUNDS_DELTA_REVIEW: {
        label: "卡券票款差异复核",
        family: "finance",
        handlerKey: "card_funds_delta",
        handlerHref: "/finance/card-funds",
        processorGroup: "finance_review",
        closeAllowed: false,
    },
    CARD_SALES_MANAGER_APPROVAL: {
        label: "卡券销售领导审批",
        family: "approval",
        handlerKey: "card_sales_manager",
        handlerHref: "/sales/orders",
        processorGroup: "sales_approval",
        closeAllowed: false,
    },
    CARD_SALES_OPERATION_APPROVAL: {
        label: "卡券运营审批",
        family: "approval",
        handlerKey: "card_sales_ops",
        handlerHref: "/sales/orders",
        processorGroup: "sales_approval",
        closeAllowed: false,
    },
    OWNERSHIP_MIGRATION_SALES_CONFIRMATION: {
        label: "归属迁移销售确认",
        family: "approval",
        handlerKey: "ownership_sales",
        handlerHref: "/sales/customers",
        processorGroup: "ownership",
        closeAllowed: false,
    },
    OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION: {
        label: "归属迁移财务确认",
        family: "finance",
        handlerKey: "ownership_finance",
        handlerHref: "/finance/receivables",
        processorGroup: "ownership",
        closeAllowed: false,
    },
    INVENTORY_ADJUSTMENT_REVIEW: {
        label: "库存调整复核",
        family: "fulfillment",
        handlerKey: "inventory_adj",
        handlerHref: "/inventory",
        processorGroup: "fulfillment",
        closeAllowed: false,
    },
    FINANCE_CORRECTION_REVIEW: {
        label: "财务纠错复核",
        family: "finance",
        handlerKey: "finance_correction",
        handlerHref: "/finance/receivables",
        processorGroup: "finance_review",
        closeAllowed: false,
    },
    SUPPLIER_SETTLEMENT_REVIEW: {
        label: "供应商结算复核",
        family: "finance",
        handlerKey: "supplier_settlement",
        handlerHref: "/supplier-api/settlements",
        processorGroup: "finance_review",
        closeAllowed: false,
    },
    IMPORT_BUSINESS_CONFIRMATION: {
        label: "导入业务确认",
        family: "exception",
        handlerKey: "import_confirm",
        handlerHref: "/governance/import-opening",
        processorGroup: "import",
        closeAllowed: false,
    },
    INTEGRATION_RESULT_UNKNOWN: {
        label: "集成结果未知",
        family: "exception",
        handlerKey: "integration_unknown",
        handlerHref: "/governance/integration-errors",
        processorGroup: "integration",
        closeAllowed: false,
    },
    BUSINESS_EXCEPTION: {
        label: "业务异常",
        family: "exception",
        handlerKey: "business_exception",
        handlerHref: "/governance/integration-errors",
        processorGroup: "integration",
        closeAllowed: false,
    },
}

const OWNER_ROLE_LABEL: Record<string, string> = {
    procurement: "采购组",
    sales: "销售组",
    sales_approval: "销售审批组",
    finance: "财务组",
    finance_review: "财务复核组",
    operations: "运营组",
    warehouse: "仓储组",
}

const PRIORITY_META: Record<string, { rank: number; label: string }> = {
    urgent: { rank: 1, label: "紧急" },
    high: { rank: 2, label: "高" },
    normal: { rank: 3, label: "普通" },
    low: { rank: 4, label: "低" },
}

const STATUS_UI: Record<
    string,
    {
        code: WorkItemStatusCode
        label: string
        tone: QueueWorkItemView["status"]["tone"]
    }
> = {
    UNCLAIMED: { code: "UNCLAIMED", label: "待领取", tone: "warning" },
    IN_PROGRESS: { code: "IN_PROGRESS", label: "处理中", tone: "info" },
    COMPLETED: { code: "COMPLETED", label: "已完成", tone: "success" },
    CLOSED: { code: "CLOSED", label: "已关闭", tone: "neutral" },
}

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

function unixToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return ""
    return new Date(secs * 1000).toISOString()
}

function formatDueLabel(dueIso: string): { label: string; overdue: boolean } {
    if (!dueIso) return { label: "—", overdue: false }
    const due = new Date(dueIso).getTime()
    const now = Date.now()
    if (due < now) return { label: "已超期", overdue: true }
    try {
        const label = new Intl.DateTimeFormat("zh-CN", {
            month: "numeric",
            day: "numeric",
            hour: "2-digit",
            minute: "2-digit",
            hour12: false,
        }).format(new Date(dueIso))
        return { label, overdue: false }
    } catch {
        return { label: dueIso, overdue: false }
    }
}

function formatEnteredLabel(iso: string): string {
    if (!iso) return "—"
    try {
        return new Intl.DateTimeFormat("zh-CN", {
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

function allowedActionsFor(
    status: string,
    hasHandler: boolean,
    closeAllowed: boolean,
): WorkItemActionCode[] {
    if (status === "UNCLAIMED") return ["CLAIM"]
    if (status === "IN_PROGRESS") {
        if (hasHandler) return ["DEFER"]
        return [
            "DEFER",
            "TRANSFER",
            "COMPLETE",
            ...(closeAllowed ? (["CLOSE"] as const) : []),
        ]
    }
    return []
}

function mapWorkItemDto(
    dto: WorkItemDto,
    profile?: AccountProfileDto,
): QueueWorkItemView {
    const meta = TYPE_META[dto.work_item_type] ?? {
        label: dto.work_item_type,
        family: "exception" as WorkItemFamily,
        handlerKey: "generic",
        processorGroup: "generic",
        closeAllowed: false,
    }
    const statusUi = STATUS_UI[dto.status] ?? {
        code: dto.status as WorkItemStatusCode,
        label: dto.status,
        tone: "neutral" as const,
    }
    const priority = PRIORITY_META[dto.priority] ?? { rank: 3, label: "普通" }
    const createdIso = unixToIso(dto.created_at)
    const dueIso = unixToIso(dto.due_at)
    const dueMeta = formatDueLabel(dueIso)
    const actions = allowedActionsFor(
        dto.status,
        Boolean(meta.handlerHref),
        meta.closeAllowed,
    )
    const closeAllowed = meta.closeAllowed && actions.includes("CLOSE")
    const ownerRoleLabel = dto.owner_role
        ? (OWNER_ROLE_LABEL[dto.owner_role] ?? "责任组")
        : "责任组"
    const handlerHref =
        dto.work_item_type === "PROCUREMENT_CONFIRMATION"
            ? `/procurement/confirm?${new URLSearchParams({
                  scope: dto.status === "UNCLAIMED" ? "role_pool" : "mine",
                  currentWorkItemId: dto.id,
                  from: "W02",
                  queueContextId: "queue:W02",
              }).toString()}`
            : meta.handlerHref

    const scopeTags: string[] = []
    if (dto.status === "UNCLAIMED") scopeTags.push("待领取")
    if (dto.status === "IN_PROGRESS") scopeTags.push("我的待办")
    if (dueMeta.overdue) scopeTags.push("已超期")

    return {
        id: dto.id,
        workItemType: dto.work_item_type,
        workItemTypeLabel: meta.label,
        family: meta.family,
        handlerKey: meta.handlerKey,
        handlerHref,
        completionAction: dto.completion_action,
        businessObject:
            dto.work_item_type === "PROCUREMENT_CONFIRMATION"
                ? "待确认销售单"
                : meta.label,
        counterparty: ownerRoleLabel,
        enteredAt: formatEnteredLabel(createdIso),
        enteredDateTime: createdIso,
        dueAt: dueMeta.label,
        dueDateTime: dueIso,
        responsibleParty: dto.owner_user_id
            ? dto.owner_user_id === profile?.userid
                ? profile.name
                : `${ownerRoleLabel}其他处理人`
            : `${ownerRoleLabel}待领取`,
        reason: dto.reason_code ?? "—",
        impact: dto.impact_summary ?? "—",
        statusCode: statusUi.code,
        status: {
            label:
                dueMeta.overdue &&
                statusUi.code !== "COMPLETED" &&
                statusUi.code !== "CLOSED"
                    ? `${statusUi.label} · 超期`
                    : statusUi.label,
            tone: dueMeta.overdue ? "destructive" : statusUi.tone,
        },
        priority: priority.rank,
        priorityLabel: priority.label,
        subjectVersion: String(dto.version),
        allowedActions: actions,
        closeAllowed,
        scopeTags,
        summaryFields: [
            { label: "类型", value: meta.label },
            ...(dto.impact_summary
                ? [{ label: "影响", value: dto.impact_summary }]
                : []),
        ],
        processorGroup: meta.processorGroup,
        effectiveStatusCode: statusUi.code,
        claimedByLabel: dto.status === "IN_PROGRESS" ? "我" : undefined,
        permissionRevoked: false,
        showClose: closeAllowed,
    }
}

/** 为采购确认待办补充销售单号、客户和合同等业务名称。 */
const enrichProcurementPresentation = async (
    item: QueueWorkItemView,
    dto: WorkItemDto,
): Promise<QueueWorkItemView> => {
    if (dto.work_item_type !== "PROCUREMENT_CONFIRMATION") return item
    const presentation = await fetchProcurementWorkItemPresentation(
        dto.business_object_id,
    )
    if (!presentation) return item
    return {
        ...item,
        businessObject: `销售单 ${presentation.salesOrderNo}`,
        counterparty: presentation.customerName,
        reason: "销售单待采购确认",
        impact: `销售含税 ${presentation.grossAmount}`,
        summaryFields: [
            { label: "销售单", value: presentation.salesOrderNo },
            { label: "客户", value: presentation.customerName },
            { label: "合同", value: presentation.contractNo ?? "未关联" },
            { label: "付款条件", value: presentation.paymentTermName },
        ],
    }
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

export function computeQueueCounts(items: readonly QueueWorkItemView[]): {
    mine: number
    rolePool: number
    team: number
    hold: number
    overdue: number
} {
    const mine = items.filter(
        (i) =>
            i.effectiveStatusCode === "IN_PROGRESS" ||
            i.scopeTags.includes("我的待办"),
    ).length
    const rolePool = items.filter(
        (i) => i.effectiveStatusCode === "UNCLAIMED",
    ).length
    const team = items.filter((i) => i.scopeTags.includes("团队")).length
    const hold = items.filter(
        (i) => i.status.label === "已跳过" || i.scopeTags.includes("已跳过"),
    ).length
    const overdue = items.filter(
        (i) => i.status.tone === "destructive" || i.dueAt.includes("超期"),
    ).length
    return { mine, rolePool, team, hold, overdue }
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
