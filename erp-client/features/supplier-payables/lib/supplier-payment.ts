import type { PaymentRow } from "@/features/supplier-payables/types"

/**
 * 把供应商付款业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * @param status 服务端状态码。
 */
export const supplierPaymentStatusLabel = (status?: string): string => {
    switch (status) {
        case "DRAFT":
        case "draft":
            return "草稿"
        case "POSTED":
        case "posted":
            return "已过账"
        case "REVERSED":
        case "reversed":
            return "已冲正"
        default:
            return "付款单"
    }
}

/**
 * 供应商付款状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const supplierPaymentStatusTone = (
    status?: string,
): PaymentRow["statusTone"] => {
    switch (supplierPaymentStatusLabel(status)) {
        case "已过账":
            return "success"
        case "已冲正":
            return "destructive"
        default:
            return "neutral"
    }
}
