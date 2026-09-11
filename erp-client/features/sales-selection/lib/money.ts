/**
 * 销售选品金额运算：全程十进制字符串，禁止 Number / parseFloat。
 * 薄封装 lib/fixed-decimal.ts，固定业务口径（两位小数、人民币）。
 */

import {
    compareDecimal,
    formatCurrencyFixed,
    multiplyFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

/** 金额小数位：人民币两位。 */
export const MONEY_SCALE = 2

/** 单价最大小数位（快照销售可见含税价）。 */
export const UNIT_PRICE_MAX_SCALE = 2

/**
 * 后端金额可表示上限（rust_decimal::MAX 按两位小数截断）。
 * 前端 bigint 无界，须显式守卫，溢出整次拒绝。
 */
export const MAX_AMOUNT = "792281625142643375935439503.35"

/**
 * 守卫金额上限，超限抛业务错误。
 * @param value 待校验金额
 * @param message 超限提示
 */
const assertWithinLimit = (value: string, message: string): string => {
    if (compareDecimal(value, MAX_AMOUNT, MONEY_SCALE) > 0) {
        throw new Error(message)
    }
    return value
}

/**
 * 计算陈列项行金额：单价 × 份数。
 * @param priceGross 含税单价十进制字符串
 * @param quantity 份数字符串（正整数）
 * @returns 行金额（两位小数）
 */
export const lineAmount = (priceGross: string, quantity: string): string =>
    assertWithinLimit(
        multiplyFixed(priceGross, quantity, {
            leftMaxScale: UNIT_PRICE_MAX_SCALE,
            rightMaxScale: 0,
            outputScale: MONEY_SCALE,
        }),
        "行金额超出可表示范围",
    )

/**
 * 汇总多行金额。
 * @param amounts 行金额字符串数组
 * @returns 合计（两位小数，空数组返回 0.00）
 */
export const sumAmounts = (amounts: readonly string[]): string =>
    assertWithinLimit(
        sumFixed(amounts, {
            maxScale: MONEY_SCALE,
            outputScale: MONEY_SCALE,
        }),
        "金额合计超出可表示范围",
    )

/**
 * 展示用人民币格式（仅格式化，不参与写入）。
 * @param value 金额字符串
 */
export const formatMoney = (value: string): string =>
    formatCurrencyFixed(value, {
        maxScale: MONEY_SCALE,
        minimumFractionDigits: MONEY_SCALE,
        maximumFractionDigits: MONEY_SCALE,
    })

/**
 * 比较两个金额字符串。
 * @returns -1 | 0 | 1
 */
export const compareMoney = (left: string, right: string): -1 | 0 | 1 =>
    compareDecimal(left, right, MONEY_SCALE)

/** 份数字符串是否为 1–100000 的整数。 */
export const INTEGER_QTY_PATTERN = /^(?:0|[1-9]\d*)$/

/**
 * 校验份数字符串是否在按份采购范围内。
 * @param value 待校验份数
 */
export const isValidQuantity = (value: string): boolean => {
    if (!INTEGER_QTY_PATTERN.test(value.trim())) return false
    const normalized = value.trim().replace(/^0+(?=\d)/, "")
    try {
        const qty = BigInt(normalized || "0")
        return qty >= BigInt(1) && qty <= BigInt(100000)
    } catch {
        return false
    }
}

/**
 * 份数加一（上限 100000，超限保持原值）。
 * @param value 当前份数
 */
export const incrementQuantity = (value: string): string => {
    const current = value.trim() || "1"
    if (!INTEGER_QTY_PATTERN.test(current)) return "1"
    try {
        const next = BigInt(current) + BigInt(1)
        if (next > BigInt(100000)) return "100000"
        return next.toString()
    } catch {
        return "1"
    }
}

/**
 * 份数减一（下限 1，超限保持原值）。
 * @param value 当前份数
 */
export const decrementQuantity = (value: string): string => {
    const current = value.trim() || "1"
    if (!INTEGER_QTY_PATTERN.test(current)) return "1"
    try {
        const next = BigInt(current) - BigInt(1)
        if (next < BigInt(1)) return "1"
        return next.toString()
    } catch {
        return "1"
    }
}

/**
 * 按份数展开 SKU 数量：单价行数量 = 份数；套餐成员数量 = 份数（P0 每成员 1 件）。
 * @param quantity 份数字符串
 */
export const skuQuantityForCopies = (quantity: string): string =>
    quantity.trim().replace(/^0+(?=\d)/, "") || "0"
