import { z } from "zod"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import type { SalesChangeDraftLine } from "../api/sales-change-draft"
import {
    nonnegativeSalesQuantity,
    positiveSalesDecimal,
    validSalesTaxRate,
} from "./sales-change-draft"

export const salesChangeDraftSchema = z.object({
    terms: z.object({
        project_name: z.string().max(200, "项目名称最多200个字符"),
        payment_term_code: z.string().trim().min(1, "请选择付款条件"),
        payment_term_name: z.string().trim().min(1, "请填写付款条件名称"),
        invoice_type: z.string().trim().min(1, "请选择开票要求"),
        tax_point: z.string().refine((value) => {
            try {
                return (
                    compareDecimal(value, "0", 6) >= 0 &&
                    compareDecimal(value, "100", 6) <= 0
                )
            } catch {
                return false
            }
        }, "税点须在0至100之间"),
    }),
    reason: z
        .string()
        .trim()
        .min(1, "请填写变更原因")
        .max(512, "变更原因最多512个字符"),
    remark: z.string().max(512, "业务备注最多512个字符"),
    lines: z.array(
        z
            .object({
                source: z.custom<SalesChangeDraftLine>(),
                dueDate: z.string(),
                serviceRegion: z.string().max(256, "服务地区最多256个字符"),
                deliveryAllocations: z
                    .array(
                        z.object({
                            rowKey: z.string(),
                            recipient: z
                                .string()
                                .trim()
                                .min(1, "请填写收货人")
                                .max(64),
                            phone: z
                                .string()
                                .trim()
                                .min(1, "请填写联系电话")
                                .max(32),
                            address: z
                                .string()
                                .trim()
                                .min(1, "请填写完整收货地址")
                                .max(512),
                            quantity: z
                                .string()
                                .refine(
                                    (value) => positiveSalesDecimal(value, 6),
                                    "分配数量必须大于 0",
                                ),
                        }),
                    )
                    .max(100),
                quantity: z
                    .string()
                    .refine(
                        nonnegativeSalesQuantity,
                        "数量须大于或等于0，最多6位小数",
                    ),
                unitPrice: z
                    .string()
                    .refine(
                        (value) => positiveSalesDecimal(value, 4),
                        "含税单价须大于0，最多4位小数",
                    ),
                taxRate: z
                    .string()
                    .refine(validSalesTaxRate, "税率应在0至1之间，最多6位小数"),
                faceValue: z
                    .string()
                    .refine(
                        (value) => positiveSalesDecimal(value, 2),
                        "面额须大于0，最多2位小数",
                    ),
                cardCount: z
                    .string()
                    .refine(
                        (value) =>
                            /^[1-9]\d*$/.test(value) &&
                            BigInt(value) <= BigInt(4294967295),
                        "卡张数须为有效正整数",
                    ),
            })
            .superRefine((line, context) => {
                if (
                    line.source.goods &&
                    (!/^\d{4}-\d{2}-\d{2}$/.test(line.dueDate) ||
                        !Number.isFinite(Date.parse(line.dueDate)))
                )
                    context.addIssue({
                        code: "custom",
                        path: ["dueDate"],
                        message: "请填写有效交付期限",
                    })
                if (!line.deliveryAllocations.length) return
                try {
                    const total = sumFixed(
                        line.deliveryAllocations.map((row) => row.quantity),
                        { maxScale: 6, outputScale: 6 },
                    )
                    if (compareDecimal(total, line.quantity, 6) !== 0)
                        context.addIssue({
                            code: "custom",
                            path: ["deliveryAllocations"],
                            message: "各地址分配数量合计必须等于商品数量",
                        })
                } catch {
                    /* 字段校验报告非法数量。 */
                }
            }),
    ),
})
