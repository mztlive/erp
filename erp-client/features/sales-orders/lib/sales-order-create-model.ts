import {
    compareDecimal,
    multiplyFixed,
    splitGrossByPercentRate,
    sumFixed,
} from "@/lib/fixed-decimal"
import { getErrorMessage } from "@/lib/api/errors"
import type {
    SalesOrderDraftLineInput,
    SalesOrderNature,
} from "@/features/sales-orders/types"

// 校验与 schema 拆到 sales-order-create-validation.ts，这里保持原有导出契约。
export {
    decimalAtMost,
    decimalInput,
    validateSalesOrderContractId,
    validateSalesOrderForm,
} from "@/features/sales-orders/lib/sales-order-create-validation"
export type {
    CreateSalesOrderFormValues,
    SalesOrderFormFieldErrors,
} from "@/features/sales-orders/lib/sales-order-create-validation"

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

export const NATURE_OPTIONS = [
    { value: "physical_service", label: "实物与服务" },
    { value: "card_voucher", label: "卡券" },
] as const

/** 实物/服务单履约方式（数据模型 §6.4；提交按行携带，后端据此形成采购履约责任）。 */
export const FULFILLMENT_MODE_OPTIONS = [
    { value: "公司仓发", label: "公司仓发" },
    { value: "供应商直发", label: "供应商直发" },
    { value: "电子交付", label: "电子交付" },
    { value: "线下服务", label: "线下服务" },
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
         * 履约方式：实物/服务单由建单页单据头选择（默认公司仓发），
         * 提交时按行携带（mapFulfillmentMode），服务端据此形成采购履约责任。
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
