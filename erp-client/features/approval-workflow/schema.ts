import { z } from "zod"

/** 决定表单。驳回原因必填，通过原因可选。 */
export const decisionFormSchema = z
    .object({
        decision: z.enum(["APPROVE", "REJECT"]),
        reason: z.string(),
    })
    .superRefine((value, ctx) => {
        if (value.decision === "REJECT" && value.reason.trim().length === 0) {
            ctx.addIssue({
                code: "custom",
                path: ["reason"],
                message: "驳回时必须填写原因",
            })
        }
    })

export type DecisionFormValues = z.input<typeof decisionFormSchema>

/** 受阻取消与撤回共用的原因表单。 */
export const reasonFormSchema = z.object({
    reason: z.string().trim().min(1, "请填写原因"),
})

export type ReasonFormValues = z.input<typeof reasonFormSchema>

/** 升级未提交绑定。不允许选择任意历史定义。 */
export const upgradeBindingFormSchema = z.object({
    reason: z.string().trim().min(1, "请填写更新原因"),
})

export type UpgradeBindingFormValues = z.input<typeof upgradeBindingFormSchema>
