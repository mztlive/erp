import { z } from "zod"
import { compareDecimal } from "@/lib/fixed-decimal"

export const positiveDecimal = z
    .string()
    .trim()
    .regex(
        /^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/,
        "请填写大于 0 的数值，最多两位小数",
    )
    .refine((value) => {
        try {
            return compareDecimal(value, "0", 2) > 0
        } catch {
            return false
        }
    }, "数值必须大于 0")
