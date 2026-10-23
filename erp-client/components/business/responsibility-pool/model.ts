import { z } from "zod"

export const assignmentStrategySchema = z.enum(["ROUND_ROBIN"])
export type AssignmentStrategy = z.infer<typeof assignmentStrategySchema>
export const ASSIGNMENT_STRATEGY_LABEL: Record<AssignmentStrategy, string> = {
    ROUND_ROBIN: "轮询",
}
export const ownerPoolSchema = z
    .array(z.string().trim().min(1, "请选择负责人"))
    .min(1, "请至少选择一名负责人")
    .max(100, "最多选择 100 名负责人")
    .refine(
        (owners) => new Set(owners).size === owners.length,
        "不能重复选择同一负责人",
    )
