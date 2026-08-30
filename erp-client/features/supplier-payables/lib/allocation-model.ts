/** W12 供应商往来 · 核销表单 Zod 校验与金额/日期工具（纯函数，无 React）。 */

import { z } from "zod"
import {
    compareDecimal,
    formatScaled,
    normalizeFixed,
    parseDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

export const BANK_RECEIPT_PENDING_REFERENCE = "pending-file:bank-receipt"

const BANK_RECEIPT_MAX_BYTES = 5 * 1024 * 1024
const BANK_RECEIPT_TYPES = new Set(["image/jpeg", "image/png", "image/webp"])

/** 锁定单一应付目标时，以付款金额覆盖该目标核销金额，确保界面与提交只有一个金额真源。 */
export function withLockedPaymentAmount(
    amounts: Readonly<Record<string, string>>,
    payableAccountId: string | undefined,
    paymentAmount: string,
): Readonly<Record<string, string>> {
    if (!payableAccountId || amounts[payableAccountId] === paymentAmount) {
        return amounts
    }
    return {
        ...amounts,
        [payableAccountId]: paymentAmount,
    }
}

export const paymentSchema = z
    .object({
        paidAt: z.string().min(1, "请填写实际付款时间"),
        amount: z
            .string()
            .trim()
            .min(1, "请填写付款金额")
            .refine(isPositiveAmount, "付款金额必须为正数"),
        bankReference: z
            .string()
            .trim()
            .max(256, "银行流水号不能超过 256 个字符"),
        bankReceiptAssetId: z.string(),
        bankReceipt: z
            .custom<File | null>(
                (value) =>
                    value === null ||
                    (typeof File !== "undefined" && value instanceof File),
                "银行回单文件无效",
            )
            .refine(
                (file) => !file || BANK_RECEIPT_TYPES.has(file.type),
                "银行回单仅支持 JPG、PNG 或 WebP 图片",
            )
            .refine(
                (file) => !file || file.size <= BANK_RECEIPT_MAX_BYTES,
                "银行回单图片不能超过 5 MB",
            ),
        note: z.string(),
    })
    .superRefine((value, context) => {
        if (!value.bankReceiptAssetId.trim()) {
            context.addIssue({
                code: "custom",
                path: ["bankReceipt"],
                message: "请上传银行回单图片",
            })
        }
    })

export const invoiceSchema = z
    .object({
        invoiceCode: z.string(),
        invoiceNo: z.string().trim().min(1, "请填写发票号码"),
        invoiceDate: z.string().min(1, "请填写开票日期"),
        grossAmount: z
            .string()
            .trim()
            .min(1, "请填写含税金额")
            .refine(isPositiveAmount, "含税金额必须为正数"),
        netAmount: z.string().trim().min(1, "请填写不含税金额"),
        taxAmount: z.string().trim().min(1, "请填写税额"),
    })
    .superRefine((v, ctx) => {
        if (
            !v.netAmount.trim() ||
            !v.taxAmount.trim() ||
            !v.grossAmount.trim()
        ) {
            return
        }
        let difference: string
        try {
            const parts = sumFixed([v.netAmount, v.taxAmount], {
                maxScale: 2,
                outputScale: 2,
            })
            difference = subtractFixed(parts, v.grossAmount, {
                maxScale: 2,
                outputScale: 2,
            })
            if (difference.startsWith("-")) difference = difference.slice(1)
        } catch {
            difference = "999999999999.99"
        }
        if (compareDecimal(difference, "0.01", 2) > 0) {
            ctx.addIssue({
                code: "custom",
                path: ["netAmount"],
                message:
                    "不含税金额 + 税额 应等于含税金额（可允许 1 分钱差异）",
            })
        }
    })

export function cents(value: string): bigint {
    try {
        const normalized = normalizeFixed(value.trim() || "0", {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        })
        return parseDecimal(normalized, {
            maxScale: 2,
            allowNegative: true,
        }).unscaled
    } catch {
        return BigInt(0)
    }
}

export function fromCents(value: bigint): string {
    return formatScaled(value, 2)
}

/** 校验业务金额为大于零的两位十进制字符串。 */
function isPositiveAmount(value: string): boolean {
    try {
        return compareDecimal(value, "0", 2) > 0
    } catch {
        return false
    }
}

export function todayInput(): string {
    const d = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}
