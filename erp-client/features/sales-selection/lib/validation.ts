/**
 * 销售选品 Zod 校验：创建、档位、会话保存与提交。
 * 金额比较走 compareDecimal，份数走字符串 + BigInt，不使用 Number / parseFloat。
 */

import { z } from "zod"
import { compareDecimal } from "@/lib/fixed-decimal"
import { isValidQuantity } from "@/features/sales-selection/lib/money"

/** 十进制金额字符串（最多两位小数）。 */
const decimalAmountPattern = /^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/

/** 金额字符串基础校验。 */
const amountString = (message: string) =>
    z
        .string()
        .trim()
        .min(1, message)
        .regex(decimalAmountPattern, "金额格式不正确，最多保留两位小数")

/**
 * 校验目标金额必须大于 0。
 */
const targetAmountString = amountString("请填写目标金额").refine(
    (value) => {
        try {
            return compareDecimal(value, "0", 2) > 0
        } catch {
            return false
        }
    },
    { message: "目标金额必须大于 0" },
)

/**
 * 校验容差必须大于或等于 0。
 */
const toleranceString = amountString("请填写容差").refine(
    (value) => {
        try {
            return compareDecimal(value, "0", 2) >= 0
        } catch {
            return false
        }
    },
    { message: "容差必须大于或等于 0" },
)

/** 份数字符串：1–100000 整数。 */
export const quantityStringSchema = z
    .string()
    .trim()
    .min(1, "请填写份数")
    .regex(/^(?:0|[1-9]\d*)$/, "份数必须为整数")
    .refine((value) => isValidQuantity(value), {
        message: "份数必须为 1 到 100000 的整数",
    })

/** 单个档位规则。 */
export const tierRuleSchema = z.object({
    tier_id: z.string().trim().optional(),
    name: z.string().trim().min(1, "请填写档位名称").max(64, "档位名称过长"),
    target_amount: targetAmountString,
    tolerance: toleranceString,
    expected_count: z
        .number()
        .int("套餐数量必须为整数")
        .min(1, "每档至少生成 1 个套餐")
        .max(20, "每档最多生成 20 个套餐"),
    sku_count: z
        .number()
        .int("每套餐件数必须为整数")
        .min(2, "每套餐至少包含 2 个商品")
        .max(8, "每套餐最多包含 8 个商品"),
})

export type TierRuleFormValue = z.input<typeof tierRuleSchema>

/** 创建选品册表单。 */
export const createBookSchema = z
    .object({
        customer_id: z.string().trim().min(1, "请选择客户"),
        selection_form: z.enum(["SINGLE_SKU", "PACKAGE"], {
            message: "请选择选品形态",
        }),
        submit_mode: z.enum(["BY_QUANTITY", "MALL_REDEEM"], {
            message: "请选择提交方式",
        }),
        source_kind: z.enum(["FILTER", "SELECTION"], {
            message: "请选择商品来源",
        }),
        filter_q: z.string().trim().optional(),
        sku_ids: z.array(z.string().trim().min(1)).optional(),
        tiers: z.array(tierRuleSchema).optional(),
    })
    .superRefine((value, ctx) => {
        if (value.selection_form === "PACKAGE") {
            const tiers = value.tiers ?? []
            if (tiers.length < 1 || tiers.length > 10) {
                ctx.addIssue({
                    code: "custom",
                    path: ["tiers"],
                    message: "套餐形态需要填写 1 到 10 个档位",
                })
            }
            const names = tiers.map((tier) => tier.name.trim())
            if (new Set(names).size !== names.length) {
                ctx.addIssue({
                    code: "custom",
                    path: ["tiers"],
                    message: "档位名称在册内不得重复",
                })
            }
        }
        if (value.source_kind === "SELECTION") {
            const ids = (value.sku_ids ?? []).filter(Boolean)
            if (ids.length < 1 || ids.length > 500) {
                ctx.addIssue({
                    code: "custom",
                    path: ["sku_ids"],
                    message: "勾选商品数量必须在 1 到 500 之间",
                })
            }
        }
    })

export type CreateBookFormValue = z.input<typeof createBookSchema>

/** 会话单项选择。 */
const sessionSelectionSchema = z.object({
    item_id: z.string().trim().min(1, "请选择陈列项"),
    quantity: quantityStringSchema.optional(),
})

/**
 * 构建保存会话校验：按份必须带份数，兑换禁止带份数。
 * @param submitMode 选品册提交方式
 */
export const buildSaveSelectionSchema = (
    submitMode: "BY_QUANTITY" | "MALL_REDEEM",
) =>
    z
        .object({
            selections: z
                .array(sessionSelectionSchema)
                .min(1, "请至少选择 1 项后再提交"),
        })
        .superRefine((value, ctx) => {
            const ids = value.selections.map((item) => item.item_id)
            if (new Set(ids).size !== ids.length) {
                ctx.addIssue({
                    code: "custom",
                    path: ["selections"],
                    message: "存在重复选择的陈列项",
                })
            }
            value.selections.forEach((item, index) => {
                if (submitMode === "BY_QUANTITY" && !item.quantity) {
                    ctx.addIssue({
                        code: "custom",
                        path: ["selections", index, "quantity"],
                        message: "请填写该项份数",
                    })
                }
                if (submitMode === "MALL_REDEEM" && item.quantity) {
                    ctx.addIssue({
                        code: "custom",
                        path: ["selections", index, "quantity"],
                        message: "商城兑换不需要填写份数",
                    })
                }
            })
        })

/** 生成请求幂等键（浏览器随机，断网重试不另生键由调用方缓存）。 */
export const createIdempotencyKey = (): string => {
    const bytes = new Uint8Array(16)
    crypto.getRandomValues(bytes)
    return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
        "",
    )
}
