/**
 * 销售单附加费用：类别文案、登记与冲减表单校验。
 * 金额与税率只用定点字符串比较，不转换为 number。
 */

import { z } from "zod"

import {
    EVIDENCE_FILE_MAX_BYTES,
    EVIDENCE_FILE_TYPES,
} from "@/components/form/evidence-upload-field"
import type { ExtraCostCategory } from "@/features/sales-orders/api/sales-order-extra-costs"
import { percentToRate } from "@/features/sales-orders/api/mappers"
import {
    compactFixed,
    compareDecimal,
    multiplyFixed,
} from "@/lib/fixed-decimal"

export const EXTRA_COST_CATEGORY_LABEL: Record<ExtraCostCategory, string> = {
    logistics: "物流",
    printing: "印刷",
    storage: "仓储",
    delivery: "配送",
    platform_tech: "平台技术",
    offline_service: "线下服务",
    rebate: "返点",
    other: "其他",
}

/** 财务登记可选类别（含返点）。 */
export const REGISTER_CATEGORY_OPTIONS = (
    Object.keys(EXTRA_COST_CATEGORY_LABEL) as ExtraCostCategory[]
).map((value) => ({ value, label: EXTRA_COST_CATEGORY_LABEL[value] }))

// SelectField 将空字符串视为未选择，整单归属使用独立的表单值。
export const EXTRA_COST_WHOLE_ORDER = "whole-order"

const amountPattern = /^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/
const percentPattern = /^(?:0|[1-9]\d?)(?:\.\d{1,2})?$/

const positiveAmount = (label: string) =>
    z
        .string()
        .trim()
        .min(1, `请填写${label}`)
        .regex(amountPattern, `${label}最多保留两位小数`)
        .refine((value) => {
            try {
                return compareDecimal(value, "0", 2) > 0
            } catch {
                return false
            }
        }, `${label}必须大于 0`)

const evidenceFile = z
    .custom<File | null>(
        (value) =>
            value === null ||
            (typeof File !== "undefined" && value instanceof File),
        "凭证文件无效",
    )
    .superRefine((file, context) => {
        if (!file) {
            context.addIssue({ code: "custom", message: "请上传凭证" })
            return
        }
        if (!(EVIDENCE_FILE_TYPES as readonly string[]).includes(file.type)) {
            context.addIssue({
                code: "custom",
                message: "凭证仅支持 PDF、JPG、PNG 或 WebP",
            })
        } else if (file.size === 0 || file.size > EVIDENCE_FILE_MAX_BYTES) {
            context.addIssue({
                code: "custom",
                message: "请选择不超过 5 MB 的非空文件",
            })
        }
    })

const description = (label: string) =>
    z
        .string()
        .trim()
        .min(1, `请填写${label}`)
        .max(200, `${label}不能超过 200 字`)

const occurredOn = z
    .string()
    .trim()
    .regex(/^\d{4}-\d{2}-\d{2}$/, "请选择日期")

export const registerExtraCostSchema = z.object({
    category: z.string().min(1, "请选择费用类别"),
    description: description("费用说明"),
    grossAmount: positiveAmount("含税金额"),
    taxRatePercent: z
        .string()
        .trim()
        .regex(percentPattern, "税率填写 0–99 的百分数，最多两位小数"),
    occurredOn,
    supplierId: z.string(),
    salesOrderLineId: z.string(),
    evidence: evidenceFile,
})

export type RegisterExtraCostValues = z.input<typeof registerExtraCostSchema>

export const reduceExtraCostSchema = (remainingGross: string) =>
    z.object({
        description: description("冲减原因"),
        grossAmount: positiveAmount("冲减金额").refine((value) => {
            try {
                return compareDecimal(value, remainingGross, 2) <= 0
            } catch {
                return false
            }
        }, `冲减金额不能超过剩余金额 ${remainingGross}`),
        occurredOn,
        evidence: evidenceFile,
    })

export type ReduceExtraCostValues = z.input<
    ReturnType<typeof reduceExtraCostSchema>
>

/** 本地时区今天（YYYY-MM-DD）。 */
export function todayDate(): string {
    const now = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

export function registerDefaults(): RegisterExtraCostValues {
    return {
        category: "",
        description: "",
        grossAmount: "",
        taxRatePercent: "0",
        occurredOn: todayDate(),
        supplierId: "",
        salesOrderLineId: EXTRA_COST_WHOLE_ORDER,
        evidence: null,
    }
}

export function reduceDefaults(): ReduceExtraCostValues {
    return {
        description: "",
        grossAmount: "",
        occurredOn: todayDate(),
        evidence: null,
    }
}

/** 表单百分数税率转为接口小数税率。 */
export function taxRateFromPercent(percent: string): string {
    return percentToRate(percent.trim() || "0")
}

/** 接口小数税率显示为百分数文本。 */
export function formatTaxRate(rate: string): string {
    try {
        const percent = multiplyFixed(rate, "100", {
            leftMaxScale: 6,
            rightMaxScale: 0,
            outputScale: 4,
        })
        return `${compactFixed(percent)}%`
    } catch {
        return "—"
    }
}

/** 秒级时间戳显示为本地日期。 */
export function formatOccurredDate(secs: number | null): string {
    if (secs == null || secs <= 0) return "—"
    const date = new Date(secs * 1000)
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** 每次打开表单生成一次请求标识，同一表单重试复用。 */
export function newExtraCostRequestKey(action: "register" | "reduce"): string {
    return `extra-cost:${action}:${crypto.randomUUID()}`
}
