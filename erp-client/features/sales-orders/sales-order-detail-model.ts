import type { SalesOrderListItem } from "@/features/sales-orders/types"
import {
    isPendingReviewStage,
    stageOwnerDisplay,
} from "@/features/sales-orders/labels"

export type NavSectionId =
    | "overview"
    | "fulfillment"
    | "receivable"
    | "collaboration"
    | "versions"

export type WorkSectionId = "procurement-rejection" | "approval" | "acceptance"

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

export function isWorkSection(section?: string): section is WorkSectionId {
    return (
        section === "procurement-rejection" ||
        section === "approval" ||
        section === "acceptance"
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
        case "procurement-rejection":
        case "acceptance":
            return "fulfillment"
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
    if (isOpenProcurementRejection(order) && order.procurementRejection) {
        return {
            id: "procurement-rejection",
            title: "采购未通过，需要你处理",
            description:
                order.procurementRejection.rejectComment ||
                "可以改价后再报采购，或作废本单。",
            actionLabel: "去处理",
            tone: "warning",
        }
    }
    if (order.activeCardSalesApproval) {
        return {
            id: "approval",
            title: "卡券销售等审批",
            description: "审批通过后本单才会生效。",
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
) {
    return `/finance/customer-accounts?view=receivable&salesOrderId=${encodeURIComponent(order.id)}&q=${encodeURIComponent(order.documentNumber)}&from=W05&returnTo=${encodeURIComponent(selfReturn)}`
}

export function fulfillmentWorkspaceHref(
    order: SalesOrderListItem,
    selfReturn: string,
) {
    return `/fulfillment?scope=mine&salesOrderId=${encodeURIComponent(order.id)}&from=W05&returnTo=${encodeURIComponent(selfReturn)}`
}

export function purchaseOrdersWorkspaceHref(
    order: SalesOrderListItem,
    selfReturn: string,
) {
    return `/procurement/orders?q=${encodeURIComponent(order.documentNumber)}&from=W05&returnTo=${encodeURIComponent(selfReturn)}`
}
