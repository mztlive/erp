/** 分配金额纯函数：UI 草稿提示统一使用两位定点十进制字符串。 */

import {
    clampZeroFixed,
    compareDecimal,
    minFixed,
    normalizeFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

const ZERO_AMOUNT = "0.00"

/** 将可编辑输入规范化为金额；空值或尚未成形的输入按零参与提示计算。 */
export const amountOrZero = (value: string): string => {
    try {
        return normalizeFixed(value.trim() || "0", {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        })
    } catch {
        return ZERO_AMOUNT
    }
}

/** 汇总 UI 草稿金额；不承担服务端正式余额计算。 */
export const sumAmounts = (values: readonly string[]): string =>
    sumFixed(values.map(amountOrZero), {
        maxScale: 2,
        outputScale: 2,
        allowNegative: true,
    })

/** 计算草稿差额，并按需把负结果收敛为零。 */
export const subtractAmounts = (
    left: string,
    right: string,
    clampToZero = false,
): string => {
    const difference = subtractFixed(amountOrZero(left), amountOrZero(right), {
        maxScale: 2,
        outputScale: 2,
    })
    return clampToZero
        ? clampZeroFixed(difference, { maxScale: 2, outputScale: 2 })
        : difference
}

/** 比较两个 UI 草稿金额。 */
export const compareAmounts = (left: string, right: string): -1 | 0 | 1 =>
    compareDecimal(amountOrZero(left), amountOrZero(right), 2)

/** 返回两个 UI 草稿金额中的较小值。 */
export const minAmount = (left: string, right: string): string =>
    minFixed(amountOrZero(left), amountOrZero(right), {
        maxScale: 2,
        outputScale: 2,
        allowNegative: true,
    })

/** 输出统一两位金额字符串。 */
export const money = amountOrZero
