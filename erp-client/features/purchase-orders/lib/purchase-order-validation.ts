import { z } from "zod"
import { compareDecimal } from "@/lib/fixed-decimal"

export const positiveDecimal = (value: string | undefined) => {
    if (value === undefined || value === "") return true
    if (!/^\d+(?:\.\d+)?$/.test(value)) return false
    try {
        return compareDecimal(value, "0", 18) > 0
    } catch {
        return false
    }
}

/**
 * 采购数量校验。采购依据待更新时商品行允许填 0，表示按新销售版本移除该行。
 */
export const purchaseQuantityValid = (
    value: string | undefined,
    allowZero: boolean,
) => {
    if (!allowZero) return positiveDecimal(value)
    if (value === undefined || value === "") return true
    if (!/^\d+(?:\.\d+)?$/.test(value)) return false
    try {
        return compareDecimal(value, "0", 18) >= 0
    } catch {
        return false
    }
}

export const taxRateValid = (value: string) => {
    if (value === "") return true
    if (!/^\d+(?:\.\d+)?$/.test(value)) return false
    try {
        return (
            compareDecimal(value, "0", 18) > 0 &&
            compareDecimal(value, "1", 18) < 0
        )
    } catch {
        return false
    }
}

/** 详情页草稿表单（付款条件 + 备注）校验。 */
export const purchaseOrderDraftFormSchema = z.object({
    paymentTermCode: z.string().min(1),
    note: z.string(),
})
