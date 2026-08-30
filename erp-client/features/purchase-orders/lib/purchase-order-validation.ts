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

/** 详情页财务审核驳回表单校验。 */
export const purchaseOrderReviewFormSchema = z.object({
    reasonCode: z.string().min(1, "请选择驳回原因"),
    comment: z.string().trim().min(2, "请填写说明"),
})
