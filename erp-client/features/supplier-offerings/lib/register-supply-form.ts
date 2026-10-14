import { format } from "date-fns"
import { z } from "zod"

import { createSchema } from "./offering-forms"

export const registerSupplySchema = createSchema
    .extend({
        skuId: z.string().min(1, "请选择商品规格"),
        samePrice: z.boolean(),
        quantityMode: z.enum(["unknown", "provided"]),
        validityMode: z.enum(["ongoing", "dated"]),
        supplierSkuCode: z.string().trim().min(1, "请填写供应商订货编码"),
        /** 空表示跟随供应商默认模板。 */
        deliveryTemplateId: z.string(),
    })
    .superRefine((value, context) => {
        if (
            value.quantityMode === "provided" &&
            !value.availableQuantity.trim()
        ) {
            context.addIssue({
                code: "custom",
                path: ["availableQuantity"],
                message: "请填写可供数量，或选择数量未提供",
            })
        }
        if (!z.iso.date().safeParse(value.validFrom).success) {
            context.addIssue({
                code: "custom",
                path: ["validFrom"],
                message: "请选择有效的生效日期",
            })
        }
        if (value.validityMode === "dated") {
            if (!z.iso.date().safeParse(value.validTo).success) {
                context.addIssue({
                    code: "custom",
                    path: ["validTo"],
                    message: "请选择失效日期",
                })
            } else if (value.validTo <= value.validFrom) {
                context.addIssue({
                    code: "custom",
                    path: ["validTo"],
                    message: "失效日期必须晚于生效日期",
                })
            }
        }
    })

export function registerSupplyDefaults(
    skuId = "",
    today = new Date(),
): z.input<typeof registerSupplySchema> {
    return {
        skuId,
        samePrice: false,
        quantityMode: "unknown",
        supplierId: "",
        supplierProductCode: "",
        supplierSkuCode: "",
        dropshipPrice: "",
        bulkPrice: "",
        minimumQuantity: "1",
        inputTaxPercentage: "",
        validFrom: format(today, "yyyy-MM-dd"),
        validTo: "",
        validityMode: "ongoing",
        deliveryTemplateId: "",
        availabilityStatus: "AVAILABLE",
        availableQuantity: "",
        changeReason: "新增供应商供给",
    }
}
