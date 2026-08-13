import { z } from "zod"

const changeReasonSchema = z.object({
    reasonCode: z.string().min(1, "请选择变更原因"),
    comment: z.string().trim().max(200),
})

export { changeReasonSchema }
