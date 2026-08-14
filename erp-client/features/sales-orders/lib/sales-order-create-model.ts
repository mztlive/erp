import { z } from "zod"
import type { StandardSchemaV1Issue } from "@tanstack/react-form"

import { WELFARE_SCENARIO_OPTIONS } from "@/lib/business-options"
import {
    compareDecimal,
    multiplyFixed,
    splitGrossByPercentRate,
    sumFixed,
} from "@/lib/fixed-decimal"
import { getErrorMessage } from "@/lib/api/errors"
import type {
    SalesOrderCreateIntent,
    SalesOrderDraftLineInput,
    SalesOrderNature,
} from "@/features/sales-orders/types"

/**
 * 配赠由后端公式推导（§6.4）：
 * gift_amount = face_value × qty − unit_price × qty
 * gift_rate = gift_amount / transaction_amount
 * 建单页只读预览，禁止手输。
 */
export function deriveVoucherGiftPreview(
    faceValue: string,
    unitPriceGross: string,
    quantity: string,
): { giftAmount: string; giftRatePercent: string } | null {
    try {
        if (!faceValue.trim() || !unitPriceGross.trim() || !quantity.trim()) {
            return null
        }
        const faceTotal = multiplyFixed(faceValue, quantity, {
            leftMaxScale: 2,
            rightMaxScale: 6,
            outputScale: 2,
        })
        const transaction = multiplyFixed(unitPriceGross, quantity, {
            leftMaxScale: 4,
            rightMaxScale: 6,
            outputScale: 2,
        })
        if (compareDecimal(transaction, "0", 2) <= 0) return null
        const giftAmount = sumFixed([faceTotal, `-${transaction}`], {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        })
        // 预览用百分数，正式配赠率由服务端按成交金额分母落库
        const gift = Number(giftAmount)
        const txn = Number(transaction)
        if (!Number.isFinite(gift) || !Number.isFinite(txn) || txn === 0) {
            return null
        }
        return {
            giftAmount,
            giftRatePercent: ((gift / txn) * 100).toFixed(2),
        }
    } catch {
        return null
    }
}

export const decimalInput = (
    label: string,
    maxScale: number,
    options: { positive?: boolean } = {},
) =>
    z
        .string()
        .trim()
        .regex(
            new RegExp(`^\\d+(?:\\.\\d{1,${maxScale}})?$`),
            `${label}最多保留 ${maxScale} 位小数`,
        )
        .refine(
            (value) => !options.positive || /[1-9]/.test(value),
            `${label}必须大于 0`,
        )

export function decimalAtMost(
    value: string,
    maximum: string,
    maxScale: number,
) {
    try {
        return compareDecimal(value, maximum, maxScale) <= 0
    } catch {
        return false
    }
}

const draftLineSchema = z.object({
    rowKey: z.string().min(1),
    name: z.string().trim().min(1, "请输入销售项目"),
    sku: z.string(),
    skuRevisionId: z.string(),
    quantity: decimalInput("数量", 6, { positive: true }),
    unit: z.string().trim().min(1, "请输入单位"),
    unitPriceGross: decimalInput("含税单价", 4, { positive: true }),
    fulfillmentMode: z.string(),
    dueDate: z.string(),
    faceValue: z.string(),
    giftRate: z.string(),
    cardForm: z.string(),
})

/** 草稿只要求「已选合同 + 至少一行明细」，明细内容允许不完整。 */
const draftRowSchema = z.object({
    rowKey: z.string().min(1),
    name: z.string(),
    sku: z.string(),
    skuRevisionId: z.string(),
    quantity: z.string(),
    unit: z.string(),
    unitPriceGross: z.string(),
    fulfillmentMode: z.string(),
    dueDate: z.string(),
    faceValue: z.string(),
    giftRate: z.string(),
    cardForm: z.string(),
})

const createSalesOrderSchema = z
    .object({
        contractId: z.string(),
        requestedContractRevisionId: z.string(),
        contractRevisionLabel: z.string(),
        customerId: z.string(),
        customerName: z.string(),
        settlementPartyId: z.string(),
        settlementEntity: z.string(),
        nature: z.enum(["physical_service", "card_voucher"]),
        ownerUserId: z.string().trim().min(1, "负责销售未就绪，请刷新后重试"),
        ownerName: z.string().trim().min(1, "负责销售未就绪，请刷新后重试"),
        welfareScene: z
            .string()
            .trim()
            .min(1, "请选择福利场景")
            .refine(
                (value) =>
                    WELFARE_SCENARIO_OPTIONS.some((o) => o.value === value),
                "请选择有效的福利场景",
            ),
        paymentTerms: z.string().trim().min(1, "请选择付款条件"),
        fulfillmentDeadline: z.string().min(1, "请选择履约期限"),
        targetMallId: z.string(),
        receivableDueDate: z.string(),
        taxRatePercent: decimalInput("税率", 6).refine(
            (value) => decimalAtMost(value, "100", 6),
            "税率不能超过 100%",
        ),
        remark: z.string(),
        lineItems: z.array(draftLineSchema).min(1, "至少需要一条销售明细"),
    })
    .superRefine((value, context) => {
        if (!value.contractId.trim()) {
            context.addIssue({
                code: "custom",
                path: ["contractId"],
                message: "请选择已有有效合同",
            })
        } else if (
            !value.requestedContractRevisionId ||
            !value.contractRevisionLabel
        ) {
            context.addIssue({
                code: "custom",
                path: ["contractId"],
                message: "正在同步合同信息，请稍后再提交",
            })
        } else {
            if (!value.customerName.trim()) {
                context.addIssue({
                    code: "custom",
                    path: ["customerName"],
                    message: "正在同步客户信息，请稍后再提交",
                })
            }
            if (!value.settlementEntity.trim()) {
                context.addIssue({
                    code: "custom",
                    path: ["settlementEntity"],
                    message: "正在同步结算主体信息，请稍后再提交",
                })
            }
        }
        if (value.nature === "card_voucher" && value.lineItems.length !== 1) {
            context.addIssue({
                code: "custom",
                path: ["lineItems"],
                message: "卡券销售单必须恰好只有一条明细",
            })
        }
        if (value.nature === "card_voucher" && !value.targetMallId.trim()) {
            context.addIssue({
                code: "custom",
                path: ["targetMallId"],
                message: "请选择目标商城",
            })
        }
        if (
            value.nature === "card_voucher" &&
            !value.receivableDueDate.trim()
        ) {
            context.addIssue({
                code: "custom",
                path: ["receivableDueDate"],
                message: "请选择应收到期日",
            })
        }
        value.lineItems.forEach((line, index) => {
            if (value.nature === "card_voucher") {
                if (!line.sku.trim()) {
                    context.addIssue({
                        code: "custom",
                        path: ["lineItems", index, "sku"],
                        message: "请选择卡券类目",
                    })
                }
                if (
                    !/^\d+(?:\.\d{1,2})?$/.test(line.faceValue) ||
                    !/[1-9]/.test(line.faceValue)
                ) {
                    context.addIssue({
                        code: "custom",
                        path: ["lineItems", index, "faceValue"],
                        message: "请输入大于 0 的卡券面值",
                    })
                }
                if (!line.cardForm.trim()) {
                    context.addIssue({
                        code: "custom",
                        path: ["lineItems", index, "cardForm"],
                        message: "请选择卡形态",
                    })
                }
            } else {
                if (!line.sku.trim()) {
                    context.addIssue({
                        code: "custom",
                        path: ["lineItems", index, "sku"],
                        message: "请选择商品/SKU",
                    })
                }
                if (!line.fulfillmentMode.trim()) {
                    context.addIssue({
                        code: "custom",
                        path: ["lineItems", index, "fulfillmentMode"],
                        message: "请选择履约方式",
                    })
                }
                if (!line.dueDate) {
                    context.addIssue({
                        code: "custom",
                        path: ["lineItems", index, "dueDate"],
                        message: "请选择明细交付日期",
                    })
                }
            }
        })
    })

export type CreateSalesOrderFormValues = z.input<typeof createSalesOrderSchema>

/** 保存草稿：宽松校验（合同已选 + 至少一行明细），提交才走全量 schema。 */
const draftSalesOrderSchema = z
    .object({
        contractId: z.string(),
        requestedContractRevisionId: z.string(),
        contractRevisionLabel: z.string(),
        customerId: z.string(),
        customerName: z.string(),
        settlementPartyId: z.string(),
        settlementEntity: z.string(),
        nature: z.enum(["physical_service", "card_voucher"]),
        ownerUserId: z.string(),
        ownerName: z.string(),
        welfareScene: z.string(),
        paymentTerms: z.string(),
        fulfillmentDeadline: z.string(),
        targetMallId: z.string(),
        receivableDueDate: z.string(),
        taxRatePercent: z.string(),
        remark: z.string(),
        lineItems: z.array(draftRowSchema).min(1, "至少需要一条销售明细"),
    })
    .superRefine((value, context) => {
        if (!value.contractId.trim()) {
            context.addIssue({
                code: "custom",
                path: ["contractId"],
                message: "请选择已有有效合同",
            })
        }
    })

/** 合同字段提交校验。未选合同时给出可读错误；已选则交给整单 schema 检查修订同步。 */
export function validateSalesOrderContractId(
    contractId: string,
): string | undefined {
    if (!contractId.trim()) return "请选择已有有效合同"
    return undefined
}

export type SalesOrderFormFieldErrors = {
    fields: Record<string, StandardSchemaV1Issue[]>
}

/**
 * 将 Zod issue.path 转成 TanStack Form 字段名（如 `lineItems[0].sku`）。
 * 必须返回 `{ fields }`：直接丢回 issue 数组会被当成整表错误，字段上看不到。
 */
function fieldErrorsFromZodIssues(
    issues: readonly z.core.$ZodIssue[],
    formValue: CreateSalesOrderFormValues,
): Record<string, StandardSchemaV1Issue[]> {
    const fields: Record<string, StandardSchemaV1Issue[]> = {}
    for (const issue of issues) {
        const path = tanstackFieldPath(issue.path, formValue)
        if (!path) continue
        const list = fields[path] ?? []
        list.push({ message: issue.message, path: issue.path })
        fields[path] = list
    }
    return fields
}

function tanstackFieldPath(path: readonly PropertyKey[], formValue: unknown) {
    let current = formValue
    let result = ""
    for (let index = 0; index < path.length; index += 1) {
        const segment = path[index]
        if (segment === undefined) continue
        const key =
            typeof segment === "object" && segment !== null && "key" in segment
                ? (segment as { key: PropertyKey }).key
                : segment
        const asNumber = Number(key)
        if (Array.isArray(current) && !Number.isNaN(asNumber)) {
            result += `[${asNumber}]`
        } else {
            result += (result ? "." : "") + String(key)
        }
        if (typeof current === "object" && current !== null) {
            current = (current as Record<PropertyKey, unknown>)[key]
        } else {
            current = undefined
        }
    }
    return result
}

export function validateSalesOrderForm(
    value: CreateSalesOrderFormValues,
    intent: SalesOrderCreateIntent,
): SalesOrderFormFieldErrors | undefined {
    const schema =
        intent === "SAVE_DRAFT" ? draftSalesOrderSchema : createSalesOrderSchema
    const result = schema.safeParse(value)
    if (result.success) return undefined
    return { fields: fieldErrorsFromZodIssues(result.error.issues, value) }
}

export const NATURE_OPTIONS = [
    { value: "physical_service", label: "实物与服务" },
    { value: "card_voucher", label: "卡券" },
] as const

export const CARD_FORM_OPTIONS = [
    { value: "电子卡", label: "电子卡" },
    { value: "实体卡", label: "实体卡" },
] as const

let draftLineSequence = 0

export function createEmptyLine(
    nature: SalesOrderNature,
): SalesOrderDraftLineInput {
    draftLineSequence += 1
    return {
        rowKey: `draft-line-${draftLineSequence}`,
        name: "",
        sku: "",
        skuRevisionId: "",
        quantity: "1",
        /** 非卡券单位随 SKU 基础单位带出；卡券固定为张。建单页不可改。 */
        unit: nature === "card_voucher" ? "张" : "",
        unitPriceGross: "0.00",
        /**
         * 建单页不提供仓发/直发选择；履约方式由后续采购二次确认写入正式结论。
         * 提交仍带占位值以满足契约，服务端以确认结果为准。
         */
        fulfillmentMode: nature === "physical_service" ? "公司仓发" : "",
        dueDate: "",
        faceValue: "",
        /** 配赠只读推导，不作为输入；保留字段供兼容提交快照。 */
        giftRate: "",
        cardForm: nature === "card_voucher" ? "电子卡" : "",
    }
}

/** 明细行是否已有实质内容（用于切换业务性质前的防丢失确认）。 */
export function hasMeaningfulLines(
    lineItems: readonly SalesOrderDraftLineInput[],
): boolean {
    return lineItems.some(
        (line) =>
            line.name.trim() !== "" ||
            line.sku.trim() !== "" ||
            line.quantity !== "1" ||
            line.unitPriceGross !== "0.00" ||
            line.faceValue !== "" ||
            line.dueDate !== "",
    )
}

export function calculateTotals(
    lineItems: readonly SalesOrderDraftLineInput[],
    taxRatePercent: string,
) {
    try {
        const lines = lineItems.map((line) => {
            const gross = multiplyFixed(
                line.quantity || "0",
                line.unitPriceGross || "0",
                {
                    leftMaxScale: 6,
                    rightMaxScale: 4,
                    outputScale: 2,
                },
            )
            return splitGrossByPercentRate(gross, taxRatePercent || "0")
        })
        return {
            gross: sumFixed(
                lines.map((line) => line.gross),
                { maxScale: 2, outputScale: 2 },
            ),
            net: sumFixed(
                lines.map((line) => line.net),
                { maxScale: 2, outputScale: 2 },
            ),
            tax: sumFixed(
                lines.map((line) => line.tax),
                { maxScale: 2, outputScale: 2 },
            ),
        }
    } catch {
        return { gross: "0.00", net: "0.00", tax: "0.00" }
    }
}

export function errorMessage(error: unknown): string {
    const message = getErrorMessage(error, "创建失败，请重试。")
    const messages: Record<string, string> = {
        CONTRACT_NOT_SELECTABLE: "所选合同已不可用于新建销售单，请刷新后重选。",
        CONTRACT_REVISION_NOT_FOUND: "所选合同修订不存在，请刷新合同后重试。",
        CONTRACT_REVISION_NOT_CURRENT: "新销售单只能引用合同当前有效修订。",
        LINE_ITEM_REQUIRED: "至少需要一条销售明细。",
        LINE_ITEM_INVALID: "销售明细不完整，请检查项目、数量、单位和价格。",
        VOUCHER_REQUIRES_EXACTLY_ONE_LINE: "卡券销售单必须恰好一条明细。",
    }
    return messages[message] ?? message
}
