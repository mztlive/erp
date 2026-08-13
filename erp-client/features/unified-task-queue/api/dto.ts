/**
 * W02 统一待办队列 — `/admin/work-items` DTO 映射与队列视图构建。
 * 纯函数，不发起请求；请求函数见 api/work-items。
 */

import { fetchProcurementWorkItemPresentation } from "@/features/procurement-confirmation/api"

import type {
    QueueWorkItemView,
    WorkItemActionCode,
    WorkItemFamily,
    WorkItemStatusCode,
} from "../types"

/** 后端 work_item 列表/详情 DTO。 */
export type WorkItemDto = {
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

export type AccountProfileDto = {
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

export function unixToIso(secs?: number | null): string {
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
export function mapWorkItemDto(
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
export const enrichProcurementPresentation = async (
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
