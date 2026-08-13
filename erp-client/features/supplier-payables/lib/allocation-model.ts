/** W12 供应商往来 · 核销表单 Zod 校验与金额/日期工具（纯函数，无 React）。 */

import { z } from "zod"

export const paymentSchema = z.object({
    paidAt: z.string().min(1, "请填写实际付款时间"),
    amount: z
        .string()
        .trim()
        .min(1, "请填写付款金额")
        .refine((v) => Number(v) > 0, "付款金额必须为正数"),
    bankReference: z.string().trim().min(1, "请填写银行流水引用"),
    note: z.string(),
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
            .refine((v) => Number(v) > 0, "含税金额必须为正数"),
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
        const diff = Math.abs(
            Number(v.netAmount) + Number(v.taxAmount) - Number(v.grossAmount),
        )
        if (diff > 0.011) {
            ctx.addIssue({
                code: "custom",
                path: ["netAmount"],
                message:
                    "不含税金额 + 税额 应等于含税金额（可允许 1 分钱差异）",
            })
        }
    })

export function cents(s: string): number {
    const n = Number(s)
    return Number.isFinite(n) ? Math.round(n * 100) : 0
}

export function fromCents(c: number): string {
    return (c / 100).toFixed(2)
}

export function todayInput(): string {
    const d = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}
