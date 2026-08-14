import { z } from "zod"

export const positiveDecimal = (value: string | undefined) =>
    value === undefined ||
    value === "" ||
    (/^\d+(?:\.\d+)?$/.test(value) && Number(value) > 0)

export const taxRateValid = (value: string) =>
    value === "" ||
    (/^\d+(?:\.\d+)?$/.test(value) && Number(value) > 0 && Number(value) < 1)

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
