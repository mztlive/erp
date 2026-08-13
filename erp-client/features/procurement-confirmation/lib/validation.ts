import { z } from "zod"

/** 驳回采购二次确认表单校验。 */
export const rejectSchema = z.object({
    reasonCode: z.string().min(1, "请选择驳回原因"),
    comment: z.string().trim().min(5, "请填写至少 5 个字的补充说明"),
})
