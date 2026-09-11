import { format } from "date-fns"
import { z } from "zod"

import { createSchema, splitValues } from "./offering-forms"

export const registerSupplySchema = createSchema
    .extend({
        validityMode: z.enum(["ongoing", "dated"]),
        supplierSkuCode: z.string().trim().min(1, "请填写供应商订货编码"),
        supplyRegionText: z.string(),
    })
    .superRefine((value, context) => {
        if (!splitValues(value.supplyRegionText).length) {
            context.addIssue({
                code: "custom",
                path: ["supplyRegionText"],
                message: "请选择或添加可供区域",
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
        supplierId: "",
        supplierProductCode: "",
        supplierSkuCode: "",
        dropshipPrice: "",
        bulkPrice: "",
        minimumQuantity: "1",
        inputTaxPercentage: "",
        supplyRegionText: "",
        validFrom: format(today, "yyyy-MM-dd"),
        validTo: "",
        validityMode: "ongoing",
        dropshipExpress: "",
        freightAmount: "",
        serviceFeeAmount: "",
        availabilityStatus: "AVAILABLE",
        availableQuantity: "",
        changeReason: "新增供应商供给",
    }
}
