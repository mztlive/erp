import { z } from "zod"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    percentageFromRate,
    rateFromPercentage,
} from "@/features/supplier-offerings/lib/offering-forms"
export { percentageFromRate } from "@/features/supplier-offerings/lib/offering-forms"
import type { PortalTerms } from "../types"
export const decimal = z
    .string()
    .trim()
    .regex(/^\d+(?:\.\d{1,4})?$/, "请输入非负数，最多4位小数")
export const quantity = z
    .string()
    .trim()
    .regex(/^\d+(?:\.\d{1,6})?$/, "请输入非负数，最多6位小数")
export const optionalQuantity = z.union([z.literal(""), quantity])
export const termsDefaults = {
    orderingCode: "",
    dropshipPrice: "",
    bulkPrice: "",
    taxPercentage: "",
    minimumQuantity: "",
    regions: "",
    validFrom: "",
    validTo: "",
    express: "",
    freight: "",
    serviceFee: "",
    quantity: "",
    availability: "AVAILABLE",
    reason: "",
}
export type TermsValues = typeof termsDefaults
const positive = (value: string) => {
    try {
        return compareDecimal(value, "0", 6) > 0
    } catch {
        return false
    }
}
const taxValid = (value: string) => {
    try {
        return compareDecimal(value, "100", 4) <= 0
    } catch {
        return false
    }
}
export const termsSchema = z.object({
    dropshipPrice: decimal,
    bulkPrice: decimal,
    taxPercentage: decimal.refine(taxValid, "税率不能超过100%"),
    minimumQuantity: quantity.refine(positive, "起订量必须大于0"),
    regions: z.string().trim().min(1, "请填写可供区域"),
    validFrom: z.string().regex(/^\d{4}-\d{2}-\d{2}$/, "请选择生效日期"),
    validTo: z.string(),
    express: z.string(),
    freight: z.union([z.literal(""), decimal]),
    serviceFee: z.union([z.literal(""), decimal]),
    orderingCode: z.string(),
    quantity: optionalQuantity,
    availability: z.enum(["AVAILABLE", "OUT_OF_STOCK"]),
    reason: z.string().trim().min(1, "请填写申请原因"),
})
export function termsFromValues(value: TermsValues): PortalTerms {
    return {
        dropship_supply_price_gross: value.dropshipPrice.trim(),
        bulk_supply_price_gross: value.bulkPrice.trim(),
        input_tax_rate: rateFromPercentage(value.taxPercentage),
        bulk_minimum_order_quantity: value.minimumQuantity.trim(),
        supply_region: value.regions
            .split(/[、，,]/)
            .map((item) => item.trim())
            .filter(Boolean),
        product_capabilities: [],
        valid_from: value.validFrom,
        valid_to: value.validTo || null,
        dropship_express: value.express.trim() || null,
        freight_amount: value.freight.trim() || null,
        service_fee_amount: value.serviceFee.trim() || null,
    }
}
export function valuesFromTerms(terms?: PortalTerms): TermsValues {
    if (!terms) return { ...termsDefaults }
    return {
        ...termsDefaults,
        dropshipPrice: terms.dropship_supply_price_gross,
        bulkPrice: terms.bulk_supply_price_gross,
        taxPercentage: percentageFromRate(terms.input_tax_rate),
        minimumQuantity: terms.bulk_minimum_order_quantity,
        regions: terms.supply_region.join("、"),
        validFrom: terms.valid_from,
        validTo: terms.valid_to ?? "",
        express: terms.dropship_express ?? "",
        freight: terms.freight_amount ?? "",
        serviceFee: terms.service_fee_amount ?? "",
    }
}
export const termsFields = [
    ["dropshipPrice", "代发含税供货价"],
    ["bulkPrice", "集采含税供货价"],
    ["taxPercentage", "税率（%）"],
    ["minimumQuantity", "集采起订量"],
    ["regions", "可供区域"],
    ["freight", "运费"],
    ["serviceFee", "服务费"],
    ["express", "代发快递说明"],
] as const
