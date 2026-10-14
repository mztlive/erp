import { z } from "zod"
import type { PurchaseChangeDraftLine } from "../api/purchase-change-draft"
import type { PurchaseDeliveryAllocation } from "../types"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import {
    purchaseChangeLineOption,
    purchaseChangeQuantityIssue,
    type PurchaseChangeDeliveryOptions,
} from "./purchase-change-draft"

function decimalWithin(
    value: string,
    minimum: "positive" | "zero",
    maximum?: string,
) {
    try {
        const lower = compareDecimal(value.trim(), "0", 6)
        return (
            (minimum === "positive" ? lower > 0 : lower >= 0) &&
            (maximum == null || compareDecimal(value.trim(), maximum, 6) <= 0)
        )
    } catch {
        return false
    }
}

const deliveryRowSchema = z.object({
    recipient: z.string(),
    phone: z.string(),
    address: z.string(),
    quantity: z.string(),
})

/** 采购改单表单校验；数量范围取自销售当前覆盖与已发生事实。 */
export function purchaseChangeDraftSchema(
    options: PurchaseChangeDeliveryOptions,
) {
    return z.object({
        reason: z
            .string()
            .trim()
            .min(1, "请填写变更原因")
            .max(500, "变更原因最多500个字符"),
        paymentTermCode: z.string().trim().min(1, "请选择付款条件"),
        lines: z
            .array(
                z
                    .object({
                        lineKey: z.string().min(1),
                        source: z.custom<PurchaseChangeDraftLine>(),
                        deliveryRows: z.array(deliveryRowSchema),
                        expectedDeliveryDate: z.string(),
                        quantity: z.string(),
                        unitCostGross: z.string(),
                        inputTaxRate: z
                            .string()
                            .refine(
                                (value) => decimalWithin(value, "zero", "1"),
                                "税率须为 0 至 1 的小数",
                            ),
                        grossAmount: z.string(),
                    })
                    .superRefine((line, context) =>
                        refineLine(line, options, context),
                    ),
            )
            .min(1, "采购明细不能为空"),
    })
}

type LineInput = {
    source: PurchaseChangeDraftLine
    deliveryRows: PurchaseDeliveryAllocation[]
    quantity: string
    unitCostGross: string
    grossAmount: string
}

function refineLine(
    line: LineInput,
    options: PurchaseChangeDeliveryOptions,
    context: z.RefinementCtx,
) {
    if (line.source.line_type === "LOGISTICS_FEE") {
        if (!decimalWithin(line.grossAmount, "zero"))
            context.addIssue({
                code: "custom",
                path: ["grossAmount"],
                message: "费用金额须为非负金额；填 0 表示移除",
            })
        return
    }
    if (!decimalWithin(line.quantity, "zero")) {
        context.addIssue({
            code: "custom",
            path: ["quantity"],
            message: "数量须大于或等于 0，最多六位小数；填 0 表示移除",
        })
        return
    }
    const issue = purchaseChangeQuantityIssue(
        line.quantity,
        purchaseChangeLineOption(line.source, options)?.quantityBound,
    )
    if (issue)
        context.addIssue({ code: "custom", path: ["quantity"], message: issue })
    if (!decimalWithin(line.unitCostGross, "zero"))
        context.addIssue({
            code: "custom",
            path: ["unitCostGross"],
            message: "单价须为非负金额，最多六位小数",
        })
    if (
        line.deliveryRows.length === 0 ||
        !decimalWithin(line.quantity, "positive")
    )
        return
    try {
        const quantities = line.deliveryRows.map(
            (row) => row.quantity.trim() || "0",
        )
        const total = sumFixed(quantities, { maxScale: 6, outputScale: 6 })
        if (
            quantities.some((value) => !decimalWithin(value, "zero")) ||
            compareDecimal(total, line.quantity, 6) !== 0
        )
            throw new Error()
    } catch {
        context.addIssue({
            code: "custom",
            path: ["deliveryRows"],
            message: "各地址交付数量合计必须等于采购数量",
        })
    }
}
