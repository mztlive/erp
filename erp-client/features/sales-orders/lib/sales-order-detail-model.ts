import type { SalesOrderListItem } from "@/features/sales-orders/types"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/contracts"
import {
    isPendingReviewStage,
    PROCUREMENT_REJECT_REASON_LABEL,
    stageOwnerDisplay,
} from "@/features/sales-orders/lib/labels"

/** 详情页动作（作废/低毛利/发起改单等）的统一结果状态。 */
export type SalesOrderDetailActionResult = {
    status: "succeeded" | "blocked" | "rejected" | "unknown"
    title: string
    description: string
    reference: string
    nextResponsible?: string
}

export type NavSectionId =
    | "overview"
    | "fulfillment"
    | "receivable"
    | "collaboration"
    | "versions"

export type WorkSectionId =
    | "procurement-rejection"
    | "approval"
    | "acceptance"
    | "change-review"

export type FocusTaskId = WorkSectionId | "versions"

export type FocusTask = {
    id: FocusTaskId
    title: string
    description: string
    actionLabel: string
    tone: "warning" | "info"
}

export type LifecycleStepState = "done" | "current" | "todo"

export type LifecycleStepId =
    | "submit"
    | "review"
    | "effective"
    | "fulfill"
    | "collect"
    | "closed"

export type LifecycleStep = {
    id: LifecycleStepId
    label: string
    state: LifecycleStepState
    hint?: string
}

const REVIEW_CODES = new Set([
    "awaiting_confirm",
    "awaiting_sales",
    "awaiting_sales_lead",
    "awaiting_ops",
    "in_approval",
])

const FROM_FULFILLMENT = new Set(["W07", "W08", "W09"])
const FROM_RECEIVABLE = new Set(["W11", "W13"])

const LIFECYCLE_ORDER: LifecycleStepId[] = [
    "submit",
    "review",
    "effective",
    "fulfill",
    "collect",
    "closed",
]

export function isOpenProcurementRejection(order: SalesOrderListItem) {
    const rejection = order.procurementRejection
    if (!rejection) return false
    return (
        rejection.reviewStatus !== "RESOLVED" &&
        rejection.reviewStatus !== "VOIDED"
    )
}

/** 普通草稿：还没有正式提交，也不是采购驳回后的待处理单。 */
export function isSalesOrderDraft(order: SalesOrderListItem) {
    return (
        order.primaryStatus.code === "draft" &&
        !isOpenProcurementRejection(order)
    )
}

/** 服务端已允许「改完再报」时，才可以进入整单编辑。 */
export function rejectionAllowsResubmit(order: SalesOrderListItem) {
    return Boolean(
        isOpenProcurementRejection(order) &&
        order.procurementRejection?.allowedActions.includes(
            "RESUBMIT_CHANGED_TERMS",
        ),
    )
}

/** 服务端已允许「驳回后作废」时，才展示作废入口。 */
export function rejectionAllowsVoid(order: SalesOrderListItem) {
    return Boolean(
        isOpenProcurementRejection(order) &&
        order.procurementRejection?.allowedActions.includes(
            "VOID_AFTER_REJECTION",
        ),
    )
}

/**
 * 是否用建单表单替换对象中心。
 * 草稿直接进入编辑；采购驳回必须先看详情，再显式带 `mode=edit`。
 */
export function shouldOpenSalesOrderEditor(options: {
    order: SalesOrderListItem
    mode?: string | null
    canResubmit: boolean
}) {
    if (isSalesOrderDraft(options.order)) return true
    return options.mode === "edit" && options.canResubmit
}

export function isWorkSection(section?: string): section is WorkSectionId {
    return (
        section === "procurement-rejection" ||
        section === "approval" ||
        section === "acceptance" ||
        section === "change-review"
    )
}

export function resolveNavSection(
    section: string | undefined,
    options: {
        from?: string | null
        isCard: boolean
    },
): NavSectionId {
    const { from, isCard } = options

    switch (section) {
        case "fulfillment":
        case "acceptance":
            return "fulfillment"
        case "procurement-rejection":
        case "change-review":
            return "overview"
        case "receivable":
            return "receivable"
        case "collaboration":
            return isCard ? "collaboration" : "overview"
        case "versions":
            return "versions"
        case "approval":
        case "overview":
        case "commercial":
        case "close":
            return "overview"
        default:
            break
    }

    if (from && FROM_FULFILLMENT.has(from)) return "fulfillment"
    if (from && FROM_RECEIVABLE.has(from)) return "receivable"
    return "overview"
}

export function resolveFocusTask(
    order: SalesOrderListItem,
    canAccept: boolean,
): FocusTask | null {
    if (
        order.nature !== "physical_service" &&
        isOpenProcurementRejection(order) &&
        order.procurementRejection
    ) {
        const rejection = order.procurementRejection
        const reason =
            PROCUREMENT_REJECT_REASON_LABEL[rejection.rejectReasonCode] ??
            rejection.rejectReasonCode
        const comment = rejection.rejectComment.trim()
        return {
            id: "procurement-rejection",
            title: "采购未通过，需要你处理",
            description: comment
                ? `${reason}：${comment}`
                : `${reason}。可以改完再报采购，或作废本单。`,
            actionLabel: "去处理",
            tone: "warning",
        }
    }
    if (order.nature === "physical_service" && order.approval?.instance) {
        return {
            id: "approval",
            title: "销售单等审批",
            description:
                "审批通过后本单才会生效。采购确认是流程中的普通审批节点。",
            actionLabel: "去审批",
            tone: "info",
        }
    }
    if (order.nature === "card_voucher" && order.approval?.instance) {
        return {
            id: "approval",
            title: "卡券销售等审批",
            description:
                "审批通过后本单才会生效。卡券运营是流程中的普通审批节点。",
            actionLabel: "去审批",
            tone: "info",
        }
    }
    if (canAccept) {
        return {
            id: "acceptance",
            title: "可以做客户验收",
            description: "客户确认完成后，本单才算交付完毕。",
            actionLabel: "去验收",
            tone: "info",
        }
    }
    if (order.activeChangeOrder) {
        return {
            id: "versions",
            title: "有一笔改单还在走",
            description: `状态：${order.activeChangeOrder.statusLabel}（基于 v${order.activeChangeOrder.baseRevisionNo}）。改单生效前，客户仍按当前版本执行。`,
            actionLabel: "看历史版本",
            tone: "warning",
        }
    }
    return null
}

export function isActionableFocusTask(
    task: FocusTask | null,
): task is FocusTask & { id: WorkSectionId } {
    return (
        task != null &&
        (task.id === "procurement-rejection" ||
            task.id === "approval" ||
            task.id === "acceptance")
    )
}

function currentLifecycleId(order: SalesOrderListItem): LifecycleStepId {
    const code = order.primaryStatus.code
    if (code === "draft") return "submit"
    if (REVIEW_CODES.has(code)) return "review"
    if (code === "closed") return "closed"
    if (!order.closeEligibility.fulfillmentComplete) return "fulfill"
    if (!order.closeEligibility.receivableSettled) return "collect"
    return "closed"
}

export function lifecycleSteps(order: SalesOrderListItem): {
    voided: boolean
    steps: LifecycleStep[]
} {
    if (order.primaryStatus.code === "voided") {
        return { voided: true, steps: [] }
    }

    const current = currentLifecycleId(order)
    const currentIndex = LIFECYCLE_ORDER.indexOf(current)
    const isCard = order.nature === "card_voucher"

    const hints: Partial<Record<LifecycleStepId, string>> = {
        fulfill: isCard
            ? "卡券到履约期限即算交付完成，消费多少不影响。"
            : "客户验收完成后才算交付完成。",
        collect: "回款收齐后系统自动结案。",
        closed: "开票进度单独看，不挡结案。",
    }

    return {
        voided: false,
        steps: LIFECYCLE_ORDER.map((id, index) => ({
            id,
            label:
                id === "submit"
                    ? "提交"
                    : id === "review"
                      ? "确认/审批"
                      : id === "effective"
                        ? "生效"
                        : id === "fulfill"
                          ? "履约"
                          : id === "collect"
                            ? "应收结清"
                            : "已关闭",
            state:
                index < currentIndex
                    ? "done"
                    : index === currentIndex
                      ? "current"
                      : "todo",
            hint: hints[id],
        })),
    }
}

export function nextStepOwner(order: SalesOrderListItem): string {
    if (
        order.primaryStatus.ownerRole ||
        isPendingReviewStage(order.primaryStatus.code)
    ) {
        return stageOwnerDisplay(order)
    }
    return order.ownerName
}

export function buildSelfHref(
    salesOrderId: string,
    section: string,
    extras?: { returnTo?: string | null; from?: string | null },
) {
    const params = new URLSearchParams()
    if (section) params.set("section", section)
    if (extras?.returnTo) params.set("returnTo", extras.returnTo)
    if (extras?.from) params.set("from", extras.from)
    const qs = params.toString()
    return qs
        ? `/sales/orders/${salesOrderId}?${qs}`
        : `/sales/orders/${salesOrderId}`
}

export function receivableWorkspaceHref(
    order: SalesOrderListItem,
    selfReturn: string,
    mode: "receipt" | "invoice" = "receipt",
) {
    const params = new URLSearchParams({
        view: "receivable",
        salesOrderId: order.id,
        q: order.documentNumber,
        from: "W05",
        returnTo: selfReturn,
        register: mode,
    })
    return `/finance/customer-accounts?${params.toString()}`
}

export function fulfillmentWorkspaceHref(
    order: SalesOrderListItem,
    selfReturn: string,
) {
    return `/fulfillment?scope=mine&salesOrderId=${encodeURIComponent(order.id)}&from=W05&returnTo=${encodeURIComponent(selfReturn)}`
}

export function canCreatePurchaseFromSalesOrder(
    order: SalesOrderListItem,
): boolean {
    if (order.nature !== "physical_service") return false
    if (order.related.purchaseOrders > 0) return false
    if (
        order.primaryStatus.code === "draft" ||
        order.primaryStatus.code === "voided"
    ) {
        return false
    }
    if (
        isPendingReviewStage(order.primaryStatus.code) ||
        order.primaryStatus.code === "in_approval"
    ) {
        return false
    }
    return true
}

export function purchaseOrdersWorkspaceHref(
    order: SalesOrderListItem,
    selfReturn: string,
) {
    const params = new URLSearchParams()
    if (canCreatePurchaseFromSalesOrder(order)) {
        params.set("salesOrderId", order.id)
    } else {
        params.set("q", order.documentNumber)
    }
    params.set("from", "W05")
    params.set("returnTo", selfReturn)
    return `/procurement/orders?${params.toString()}`
}

export function navItemsFor(order: SalesOrderDetailView): Array<{
    id: NavSectionId
    label: string
    hint: string
    show: boolean
}> {
    return [
        {
            id: "overview",
            label: "概览",
            hint: "约定、明细和下一步",
            show: true,
        },
        {
            id: "fulfillment",
            label: "履约",
            hint: "采购、发货和验收",
            show: true,
        },
        {
            id: "receivable",
            label: "票款",
            hint: "回款和开票",
            show: true,
        },
        {
            id: "collaboration",
            label: "协同",
            hint: "商城同步与执行投影",
            show: order.nature === "card_voucher",
        },
        {
            id: "versions",
            label: "版本",
            hint: "改单与历史版本",
            show: true,
        },
    ]
}
