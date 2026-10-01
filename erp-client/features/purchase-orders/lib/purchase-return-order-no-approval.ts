import type { StatusTone } from "@/components/ui/status-badge"
import type { BackendPurchaseReturnOrder } from "@/features/purchase-orders/api/purchase-return-order-wire-types"
import type { PurchaseReturnOrderRow } from "@/features/purchase-orders/types"

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

/**
 * 把采购退货状态映射为用户可见中文，不上屏枚举原值。
 *
 * `pending_execution` 是履约执行分工态，固定译为「待执行」，
 * 不得渲染为「审批中」或「审批复核」。
 *
 * @param status 服务端状态码。
 */
export const purchaseReturnOrderStatusLabel = (status?: string): string => {
    switch (status) {
        case "draft":
            return "草稿"
        case "pending_execution":
            return "待执行"
        case "returned":
            return "已退货"
        case "completed":
            return "已完成"
        case "voided":
            return "作废"
        default:
            return "未知状态"
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
            return "公司仓退供应商"
        case "direct_to_supplier":
            return "客户直退供应商"
        default:
            return "未知退货模式"
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
