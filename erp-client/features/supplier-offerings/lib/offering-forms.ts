import { z } from "zod"
import { getErrorMessage } from "@/lib/api/errors"
import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
} from "@/lib/fixed-decimal"

function compareSafely(
    left: string,
    right: string,
    maxScale: number,
    predicate: (comparison: -1 | 0 | 1) => boolean,
): boolean {
    try {
        return predicate(compareDecimal(left, right, maxScale))
    } catch {
        return false
    }
}

const decimal = z
    .string()
    .trim()
    .regex(/^\d+(?:\.\d{1,4})?$/, "请输入非负数，最多 4 位小数")

const quantity = z
    .string()
    .trim()
    .regex(/^\d+(?:\.\d{1,6})?$/, "请输入非负数，最多 6 位小数")

const taxPercentage = z
    .string()
    .trim()
    .regex(/^\d+(?:\.\d{1,4})?$/, "请输入 0–100 的税率")
    .refine(
        (value) =>
            compareSafely(value, "100", 4, (comparison) => comparison <= 0),
        "税率不能超过 100%",
    )

const termsSchema = {
    dropshipPrice: decimal,
    bulkPrice: decimal,
    minimumQuantity: quantity.refine(
        (value) => compareSafely(value, "0", 6, (comparison) => comparison > 0),
        "起订量必须大于 0",
    ),
    inputTaxPercentage: taxPercentage,
    supplyRegionText: z.string().trim().min(1, "请填写可供区域"),
    validFrom: z.string().trim().min(1, "请选择生效日期"),
    validTo: z.string(),
    dropshipExpress: z.string(),
    freightAmount: z.union([z.literal(""), decimal]),
    serviceFeeAmount: z.union([z.literal(""), decimal]),
}

export const createSchema = z.object({
    skuId: z.string().min(1, "请选择公司 SKU"),
    supplierId: z.string().min(1, "请选择供应商"),
    supplierProductCode: z.string(),
    supplierSkuCode: z.string().trim().min(1, "请填写供应商 SKU 编码"),
    ...termsSchema,
    availabilityStatus: z.enum([
        "AVAILABLE",
        "UNAVAILABLE",
        "STOPPED",
        "STALE",
    ]),
    availableQuantity: z.union([z.literal(""), quantity]),
    changeReason: z.string().trim().min(1, "请填写登记原因"),
})

export const reviseSchema = z.object({
    ...termsSchema,
    status: z.enum(["ACTIVE", "PAUSED", "STOPPED"]),
    changeReason: z.string().trim().min(1, "请填写变更原因"),
})

export const availabilitySchema = z.object({
    availabilityStatus: z.enum([
        "AVAILABLE",
        "UNAVAILABLE",
        "STOPPED",
        "STALE",
    ]),
    availableQuantity: z.union([z.literal(""), quantity]),
    changeReason: z.string().trim().min(1, "请填写变更原因"),
})

export function idempotencyKey(prefix: string): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

export function splitValues(value: string): readonly string[] {
    return value
        .split(/[，,、]/)
        .map((item) => item.trim())
        .filter(Boolean)
}

export function rateFromPercentage(value: string): string {
    return divideFixed(value, "100", {
        numeratorMaxScale: 4,
        denominatorMaxScale: 0,
        outputScale: 6,
    })
}

export function percentageFromRate(value?: string | null): string {
    if (!value) return ""
    return compactFixed(
        multiplyFixed(value, "100", {
            leftMaxScale: 6,
            rightMaxScale: 0,
            outputScale: 4,
        }),
    )
}

export const errorMessage = (error: unknown, fallback: string): string =>
    getErrorMessage(error, fallback)
