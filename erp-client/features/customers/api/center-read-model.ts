/** 客户对象中心关联业务与应收汇总的运行时 Wire 解码。 */

import { z } from "zod"

import { compactFixed, parseDecimal } from "@/lib/fixed-decimal"

const amountSchema = z.string().transform((value, context) => {
    const normalized = compactFixed(value.trim())
    try {
        parseDecimal(normalized, { maxScale: 2 })
        return normalized
    } catch {
        context.addIssue({
            code: "custom",
            message: "金额必须是非负且有效小数位不超过 2 位的十进制字符串",
        })
        return z.NEVER
    }
})

const businessDateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)

const relatedSchema = z
    .object({
        active_contract_count: z.number().int().nonnegative(),
        in_progress_sales_order_count: z.number().int().nonnegative(),
        contracts: z.array(
            z
                .object({
                    id: z.string().min(1),
                    contract_no: z.string().min(1),
                    status: z.enum(["EFFECTIVE", "TERMINATED", "EXPIRED"]),
                })
                .strict(),
        ),
        sales_orders: z.array(
            z
                .object({
                    id: z.string().min(1),
                    order_no: z.string().min(1),
                    commercial_status: z.enum([
                        "DRAFT",
                        "PENDING_REVIEW",
                        "EFFECTIVE",
                        "VOIDED",
                    ]),
                    close_status: z.enum([
                        "NOT_SATISFIED",
                        "CLOSEABLE",
                        "CLOSED",
                    ]),
                    created_at: z.number().int().nonnegative(),
                })
                .strict(),
        ),
        projected_at: z.number().int().nonnegative(),
    })
    .strict()

const receivableSchema = z
    .object({
        receivable_balance: amountSchema,
        overdue_amount: amountSchema,
        open_invoiceable_total: amountSchema,
        earliest_overdue_date: businessDateSchema.nullable(),
        projected_at: z.number().int().nonnegative(),
    })
    .strict()

export type BackendCustomerCenterRelated = z.infer<typeof relatedSchema>
export type BackendCustomerCenterReceivable = z.infer<typeof receivableSchema>

export function decodeCustomerCenterRelated(
    input: unknown,
): BackendCustomerCenterRelated {
    const result = relatedSchema.safeParse(input)
    if (result.success) return result.data
    throw new Error(
        `客户关联摘要响应契约不匹配：${z.prettifyError(result.error)}`,
    )
}

export function decodeCustomerCenterReceivable(
    input: unknown,
): BackendCustomerCenterReceivable {
    const result = receivableSchema.safeParse(input)
    if (result.success) return result.data
    throw new Error(
        `客户应收摘要响应契约不匹配：${z.prettifyError(result.error)}`,
    )
}
