import { z } from "zod"

import type { CustomerAssignmentView } from "@/features/customers/types"

export type CustomerAssignmentFormValues = {
    userId: string
    role: "OWNER" | "COLLABORATOR"
    effectiveFrom: string
    effectiveTo: string
    reason: string
}

/** 返回本地业务日期。 */
function todayBusinessDate(): string {
    const date = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** 返回给定业务日的下一天。 */
function nextBusinessDate(value: string): string {
    const date = new Date(`${value}T00:00:00`)
    date.setDate(date.getDate() + 1)
    const pad = (part: number) => String(part).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** 按建立或结束模式生成开窗初值。 */
export function customerAssignmentDefaults(
    target?: CustomerAssignmentView,
): CustomerAssignmentFormValues {
    const today = todayBusinessDate()
    return {
        userId: "",
        role: "COLLABORATOR",
        effectiveFrom: today,
        effectiveTo: target
            ? target.effectiveFrom >= today
                ? nextBusinessDate(today)
                : today
            : "",
        reason: "",
    }
}

/** 归属调整的字段和跨日期约束。 */
export function customerAssignmentSchema(target?: CustomerAssignmentView) {
    return z
        .object({
            userId: z.string(),
            role: z.enum(["OWNER", "COLLABORATOR"]),
            effectiveFrom: z.string(),
            effectiveTo: z.string(),
            reason: z.string().trim().min(1, "请填写调整原因"),
        })
        .superRefine((value, context) => {
            if (target) {
                if (
                    !value.effectiveTo ||
                    value.effectiveTo <= target.effectiveFrom
                ) {
                    context.addIssue({
                        code: "custom",
                        path: ["effectiveTo"],
                        message: `结束日期必须晚于 ${target.effectiveFrom}`,
                    })
                }
                return
            }
            if (!value.userId) {
                context.addIssue({
                    code: "custom",
                    path: ["userId"],
                    message: "请选择销售人员",
                })
            }
            if (
                !value.effectiveFrom ||
                (value.effectiveTo && value.effectiveTo <= value.effectiveFrom)
            ) {
                context.addIssue({
                    code: "custom",
                    path: ["effectiveTo"],
                    message: "结束日期必须晚于生效日期",
                })
            }
        })
}
