import { hasAnyPermission, hasPermission } from "@/lib/permissions"

/** 销售单详情页按钮对应的 Casbin 权限码。 */
export const SALES_ORDER_DETAIL_PERMISSIONS = {
    openPurchase: "purchase_order:list",
    previewPurchase: "purchase_order:detail",
    createPurchase: "purchase_order:create",
    openReceivable: "receivable_account:list",
    registerAcceptance: "customer_acceptance:create",
    cancelApproval: "sales_order:cancel_approval",
    startChange: "sales_change_order:create",
} as const

/** 打开履约工作面：与侧栏 W09 采购 lane 入口对齐。 */
export const OPEN_FULFILLMENT_PERMISSIONS = [
    "delivery:list",
    "electronic_delivery:list",
    "service_fulfillment:list",
    "purchase_receipt:list",
] as const

export type SalesOrderDetailPermissionKind =
    | keyof typeof SALES_ORDER_DETAIL_PERMISSIONS
    | "openFulfillment"

export type PermissionGate = {
    enabled: boolean
    reason?: string
}

const REASON: Record<SalesOrderDetailPermissionKind, string> = {
    openPurchase: "当前账号没有采购单列表权限",
    previewPurchase: "当前账号没有采购单详情权限，无法预览",
    createPurchase: "当前账号没有供给分配权限",
    openFulfillment: "当前账号没有履约/交付工作面权限",
    openReceivable: "当前账号没有客户往来权限",
    registerAcceptance: "当前账号没有登记客户验收权限",
    cancelApproval: "当前账号没有撤回销售单审批权限",
    startChange: "当前账号没有发起销售变更权限",
}

export function permissionBlockedReason(
    kind: SalesOrderDetailPermissionKind,
): string {
    return REASON[kind]
}

export function canNavigateWithPermission(
    granted: readonly string[] | undefined | null,
    required: string | readonly string[],
): boolean {
    if (typeof required === "string") {
        return hasPermission(granted, required)
    }
    return hasAnyPermission(granted, required)
}

/**
 * 命令入口：服务端允许 AND 角色有权限。
 * 前端权限不得单独发明服务端未放出的动作。
 */
export function canCommand(
    serverAllows: boolean,
    granted: readonly string[] | undefined | null,
    required: string,
): boolean {
    return serverAllows && hasPermission(granted, required)
}

export function gateNavigate(
    granted: readonly string[] | undefined | null,
    kind: Exclude<
        SalesOrderDetailPermissionKind,
        "registerAcceptance" | "cancelApproval" | "startChange"
    >,
): PermissionGate {
    const required =
        kind === "openFulfillment"
            ? OPEN_FULFILLMENT_PERMISSIONS
            : SALES_ORDER_DETAIL_PERMISSIONS[kind]
    const enabled = canNavigateWithPermission(granted, required)
    return enabled
        ? { enabled: true }
        : { enabled: false, reason: permissionBlockedReason(kind) }
}

export function gateCommand(
    serverAllows: boolean,
    granted: readonly string[] | undefined | null,
    kind: Extract<
        SalesOrderDetailPermissionKind,
        | "createPurchase"
        | "registerAcceptance"
        | "cancelApproval"
        | "startChange"
    >,
    serverReason?: string,
): PermissionGate {
    if (!serverAllows) {
        return {
            enabled: false,
            reason: serverReason || "当前不能执行该操作",
        }
    }
    const code = SALES_ORDER_DETAIL_PERMISSIONS[kind]
    if (!hasPermission(granted, code)) {
        return { enabled: false, reason: permissionBlockedReason(kind) }
    }
    return { enabled: true }
}

/** 服务端已放出撤回，或单据仍处审批中（详情 instance 可能为空）。 */
export function salesOrderAllowsWithdrawApproval(order: {
    approval?: { allowedActions?: readonly string[] } | null
    primaryStatus: { code: string }
}): boolean {
    const actions = order.approval?.allowedActions ?? []
    if (actions.includes("CANCEL") || actions.includes("CANCEL_APPROVAL")) {
        return true
    }
    return (
        order.primaryStatus.code === "in_approval" ||
        order.primaryStatus.code === "awaiting_confirm" ||
        order.primaryStatus.code === "awaiting_sales_lead" ||
        order.primaryStatus.code === "awaiting_ops"
    )
}

/**
 * 撤回未审结销售单：本人负责销售 + cancel_approval 权限。
 * 「自己的销售单在未审批之前可以被自己撤回」。
 */
export function gateCancelSalesOrderApproval(input: {
    order: {
        ownerUserId: string
        approval?: { allowedActions?: readonly string[] } | null
        primaryStatus: { code: string }
    }
    currentUserId?: string | null
    granted: readonly string[] | undefined | null
}): PermissionGate {
    if (!salesOrderAllowsWithdrawApproval(input.order)) {
        return { enabled: false, reason: "当前不在可撤回的审批中" }
    }
    if (
        !input.currentUserId ||
        !input.order.ownerUserId ||
        input.currentUserId !== input.order.ownerUserId
    ) {
        return {
            enabled: false,
            reason: "只有负责销售本人可以撤回未审结的销售单",
        }
    }
    return gateCommand(true, input.granted, "cancelApproval")
}
