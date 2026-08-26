import { isOpenInstanceStatus } from "@/features/approval-workflow/display"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/contracts"
import {
    isPendingReviewStage,
    stageOwnerDisplay,
} from "@/features/sales-orders/lib/labels"
import { fulfillmentTasksHref } from "@/features/workspace/lib/fulfillment-destination"

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
    | "approval"
    | "fulfillment"
    | "acceptance"
    | "receivable"
    | "collaboration"
    | "versions"

export type WorkSectionId = "approval" | "change-review"

export type FocusTaskId = "approval" | "acceptance" | "versions"

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

const FROM_FULFILLMENT = new Set(["W01", "W07", "W08", "W09"])
const FROM_RECEIVABLE = new Set(["W11", "W13"])

const LIFECYCLE_ORDER: LifecycleStepId[] = [
    "submit",
    "review",
    "effective",
    "fulfill",
    "collect",
    "closed",
]

/** 尚未正式提交的销售单草稿。 */
export function isSalesOrderDraft(order: SalesOrderListItem) {
    return order.primaryStatus.code === "draft"
}

/** 草稿直接使用建单表单，已提交单据进入只读对象中心。 */
export function shouldOpenSalesOrderEditor(order: SalesOrderListItem) {
    return isSalesOrderDraft(order)
}

export function isWorkSection(section?: string): section is WorkSectionId {
    return section === "approval" || section === "change-review"
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
        case "purchase":
            return "fulfillment"
        case "acceptance":
            return isCard ? "fulfillment" : "acceptance"
        case "change-review":
            return "overview"
        case "receivable":
            return "receivable"
        case "collaboration":
            return isCard ? "collaboration" : "overview"
        case "versions":
            return "versions"
        case "approval":
            return "approval"
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

/**
 * 销售单审批是否仍需处理。已通过/已撤回的实例只是历史投影，不得再占焦点。
 * 尚无 instance 时，仅在单据仍处审核轨才当作待审批（投影尚未回写）。
 */
export function isSalesOrderApprovalInProgress(
    order: Pick<SalesOrderListItem, "approval" | "primaryStatus">,
): boolean {
    if (!order.approval) return false
    if (isOpenInstanceStatus(order.approval.instance?.status)) return true
    if (order.approval.instance) return false
    return REVIEW_CODES.has(order.primaryStatus.code)
}

/**
 * 现在能否做客户验收。实物单且服务端放出登记动作还不够：
 * 必须已有尚未验收完的履约事实（发货/电子交付/服务履约）。
 * 已生效但交付尚未发生时不得当作当前待办。
 */
export function canRegisterCustomerAcceptance(
    order: Pick<SalesOrderListItem, "nature" | "allowedActions">,
    hasEligibleAcceptance: boolean,
): boolean {
    return (
        order.nature === "physical_service" &&
        order.allowedActions.includes("REGISTER_ACCEPTANCE") &&
        hasEligibleAcceptance
    )
}

export function resolveFocusTask(
    order: SalesOrderListItem,
    canAccept: boolean,
): FocusTask | null {
    // 审批仍在途时优先引导去审批，不得被验收等履约动作抢焦点。
    // 已通过的实例不得仅因「有 instance」就继续提示等审批。
    if (
        order.nature === "physical_service" &&
        isSalesOrderApprovalInProgress(order)
    ) {
        return {
            id: "approval",
            title: "销售单等审批",
            description: "",
            actionLabel: "去审批",
            tone: "info",
        }
    }
    if (
        order.nature === "card_voucher" &&
        isSalesOrderApprovalInProgress(order)
    ) {
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
): task is FocusTask & { id: "acceptance" } {
    // 审批入口已由「审批」tab 承接，不再用「去审批」主按钮跳转。
    return task != null && task.id === "acceptance"
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

/** 打开客户往来并锁定本单，不自动进入登记会话。 */
export function customerAccountsForOrderHref(
    order: Pick<SalesOrderListItem, "id">,
    selfReturn: string,
) {
    const params = new URLSearchParams({
        view: "receivable",
        salesOrderId: order.id,
        from: "W05",
        returnTo: selfReturn,
    })
    return `/finance/customer-accounts?${params.toString()}`
}

export function fulfillmentWorkspaceHref(
    _order: SalesOrderListItem,
    _selfReturn: string,
) {
    return fulfillmentTasksHref()
}

export function canCreatePurchaseFromSalesOrder(
    order: SalesOrderListItem,
): boolean {
    if (order.related.purchaseCreationAccess) {
        return order.related.purchaseCreationAccess.allowed
    }
    return false
}

export function purchaseOrdersWorkspaceHref(
    order: SalesOrderListItem,
    selfReturn: string,
) {
    const params = new URLSearchParams({ salesOrderId: order.id })
    if (canCreatePurchaseFromSalesOrder(order)) {
        params.set("mode", "create")
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
            id: "approval",
            label: "审批",
            hint: "审批摘要与历史",
            show: Boolean(order.approval),
        },
        {
            id: "fulfillment",
            label: "采购",
            hint: "本单已创建的采购单",
            show: true,
        },
        {
            id: "acceptance",
            label: "验收",
            hint: "客户验收与历史",
            show: order.nature === "physical_service",
        },
        {
            id: "receivable",
            label: "票款",
            hint: "本单回款和开票",
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
