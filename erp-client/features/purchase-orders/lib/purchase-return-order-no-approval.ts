import type { StatusTone } from "@/components/ui/status-badge"
import type { BackendPurchaseReturnOrder } from "@/features/purchase-orders/api/purchase-return-order-wire-types"
import type { PurchaseReturnOrderRow } from "@/features/purchase-orders/types"

/** 采购退货单作为合同 DocumentType 的固定值。 */
export const PURCHASE_RETURN_ORDER_DOCUMENT_TYPE =
    "PurchaseReturnOrder" as const

/** 工作项上的采购退货对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const PURCHASE_RETURN_ORDER_OBJECT_TYPE =
    "purchase_return_order" as const

/** 合同 §4.3 对 PurchaseReturnOrder 的固定政策。 */
export const PURCHASE_RETURN_ORDER_APPROVAL_REQUIREMENT = "NO_APPROVAL" as const

type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/** 编译期证明：采购退货 HTTP DTO 不得携带审批绑定。 */
export const PURCHASE_RETURN_ORDER_DTO_HAS_NO_APPROVAL: ForbidKey<
    BackendPurchaseReturnOrder,
    "approval"
> = true

/** 编译期证明：采购退货行投影不得嵌入审批区。 */
export const PURCHASE_RETURN_ORDER_ROW_HAS_NO_APPROVAL: ForbidKey<
    PurchaseReturnOrderRow,
    "approval"
> = true

/** 采购退货业务动作白名单；不得混入审批决定或流程写入口。 */
const PURCHASE_RETURN_ORDER_BUSINESS_ACTIONS = new Set([
    "VIEW_DETAIL",
    "SAVE",
    "CONFIRM",
    "EXECUTE",
    "VOID",
])

/**
 * 判断当前任务是否属于采购退货。采购退货为无需审批类型，不得当作审批待办。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @returns 对象类型为 PurchaseReturnOrder / purchase_return_order 时为 true。
 */
export const isPurchaseReturnOrderWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === PURCHASE_RETURN_ORDER_DOCUMENT_TYPE ||
    workItem?.businessObjectType === PURCHASE_RETURN_ORDER_OBJECT_TYPE

/**
 * 判断采购退货状态是否为待执行分工态。该态不是审批复核。
 *
 * @param status 服务端状态码；同时接受 snake_case 与 SCREAMING_SNAKE。
 * @returns `PENDING_EXECUTION` / `pending_execution` 时为 true。
 */
export const isPurchaseReturnExecutionStatus = (status?: string): boolean =>
    status === "PENDING_EXECUTION" || status === "pending_execution"

/**
 * 把采购退货状态映射为用户可见中文，不上屏枚举原值。
 *
 * `PENDING_EXECUTION` 是履约执行分工态，固定译为「待执行」，
 * 不得渲染为「审批中」或「审批复核」。
 *
 * @param status 服务端状态码。
 */
export const purchaseReturnOrderStatusLabel = (status?: string): string => {
    switch (status) {
        case "DRAFT":
        case "draft":
            return "草稿"
        case "PENDING_EXECUTION":
        case "pending_execution":
            return "待执行"
        case "RETURNED":
        case "returned":
            return "已退货"
        case "COMPLETED":
        case "completed":
            return "已完成"
        case "VOIDED":
        case "voided":
        case "VOID":
            return "作废"
        default:
            return "采购退货"
    }
}

/**
 * 采购退货状态对应的列表色调。待执行按进行中处理，不上屏审批复核语义。
 *
 * @param status 服务端状态码。
 */
export const purchaseReturnOrderStatusTone = (status?: string): StatusTone => {
    switch (purchaseReturnOrderStatusLabel(status)) {
        case "已退货":
        case "已完成":
            return "success"
        case "待执行":
            return "warning"
        default:
            return "neutral"
    }
}

/**
 * 把退货模式映射为用户可见中文，不上屏内部码。
 *
 * @param mode 服务端退货模式。
 */
export const purchaseReturnModeLabel = (mode?: string): string => {
    switch (mode) {
        case "company_warehouse_to_supplier":
        case "COMPANY_WAREHOUSE_TO_SUPPLIER":
            return "公司仓退供应商"
        case "direct_to_supplier":
        case "DIRECT_TO_SUPPLIER":
            return "客户直退供应商"
        default:
            return "采购退货"
    }
}

/**
 * 丢弃采购退货 DTO 上误带的审批字段。PurchaseReturnOrder 为 NO_APPROVAL，
 * 禁止把绑定带入投影。
 *
 * @param dto 采购退货 HTTP 载荷。
 * @returns 不含 `approval` 的对象。
 */
export function stripPurchaseReturnApprovalField<T extends object>(
    dto: T,
): Omit<T, "approval"> {
    if (!("approval" in dto)) {
        return dto
    }
    const { approval: _discarded, ...rest } = dto as T & {
        approval?: unknown
    }
    void _discarded
    return rest
}

/**
 * 采购退货允许动作是否只含退货业务入口，不含审批入口。
 *
 * @param actions 行上的允许动作。
 * @returns 全部为采购退货业务动作时为 true。
 */
export const purchaseReturnActionsExcludeApproval = (
    actions: readonly string[],
): boolean =>
    actions.every((action) =>
        PURCHASE_RETURN_ORDER_BUSINESS_ACTIONS.has(action),
    )
