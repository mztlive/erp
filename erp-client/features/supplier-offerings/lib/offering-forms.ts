import { z } from "zod"
import { getErrorMessage } from "@/lib/api/errors"
import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
} from "@/lib/fixed-decimal"
import type {
    OfferingStatus,
    ReviseSupplierOfferingInput,
    SupplierOfferingView,
} from "@/features/supplier-offerings/types"

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
    validFrom: z.string().trim().min(1, "请选择生效日期"),
    validTo: z.string(),
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

function revisionSchema(allowMissingBulk: boolean) {
    return z
        .object({
            ...termsSchema,
            bulkPrice: z.union([z.literal(""), decimal]),
            minimumQuantity: z.union([
                z.literal(""),
                termsSchema.minimumQuantity,
            ]),
            status: z.enum(["ACTIVE", "PAUSED", "STOPPED"]),
            changeReason: z.string().trim().min(1, "请填写变更原因"),
        })
        .superRefine((value, context) => {
            if (Boolean(value.bulkPrice) !== Boolean(value.minimumQuantity)) {
                context.addIssue({
                    code: "custom",
                    path: ["bulkPrice"],
                    message: "集采价格和起订量须同时填写或同时留空",
                })
            } else if (!allowMissingBulk && !value.bulkPrice) {
                context.addIssue({
                    code: "custom",
                    path: ["bulkPrice"],
                    message: "请填写集采价格和起订量",
                })
            }
        })
}

export const reviseSchema = revisionSchema(false)
/** API 来源可明确未支持集采，不以代发价推导集采条款。 */
export const apiReviseSchema = revisionSchema(true)

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

export const statusRevisionSchema = z.object({
    changeReason: z.string().trim().min(1, "请填写变更原因"),
})

export function idempotencyKey(prefix: string): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
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

function optionalText(value?: string | null): string | null {
    const trimmed = value?.trim()
    return trimmed ? trimmed : null
}

/**
 * 把列表上的当前条款原样带进修订命令。
 * 关系状态变更也必须追加新版本，不得只改状态字段。
 */
export function buildStatusRevisionInput(
    offering: SupplierOfferingView,
    status: OfferingStatus,
    changeReason: string,
    key: string,
): ReviseSupplierOfferingInput | null {
    const expectedRevisionNo = offering.current_revision_no
    const dropship = offering.dropship_supply_price_gross?.trim()
    const bulk = offering.bulk_supply_price_gross?.trim()
    const tax = offering.input_tax_rate?.trim()
    const moq = offering.bulk_minimum_order_quantity?.trim()
    const validFrom = offering.valid_from?.trim()
    if (
        expectedRevisionNo == null ||
        expectedRevisionNo < 1 ||
        !dropship ||
        !tax ||
        !validFrom ||
        Boolean(bulk) !== Boolean(moq) ||
        (offering.source_type !== "API" && !bulk)
    ) {
        return null
    }
    return {
        offeringId: offering.id,
        expected_revision_no: expectedRevisionNo,
        terms: {
            dropship_supply_price_gross: dropship,
            bulk_supply_price_gross: bulk || null,
            input_tax_rate: tax,
            bulk_minimum_order_quantity: moq || null,
            product_capabilities: offering.product_capabilities,
            valid_from: validFrom,
            valid_to: optionalText(offering.valid_to),
        },
        status,
        change_reason: changeReason.trim(),
        idempotency_key: key,
    }
}
