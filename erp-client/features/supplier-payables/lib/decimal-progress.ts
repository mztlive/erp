/** 金额进度百分比（0–100），用定点小数计算，不经过 JS number 金额。 */

import { parseDecimal } from "@/lib/fixed-decimal"

const MONEY_MAX_SCALE = 2

/**
 * 已分配金额相对总额的整数百分比。
 * 总额非正则 0；已分配超过总额则封顶 100。
 *
 * @param allocated 已分配金额十进制字符串。
 * @param total 总额十进制字符串。
 */
export function decimalProgressPercent(
    allocated: string,
    total: string,
): number {
    try {
        const part = parseDecimal(allocated, {
            maxScale: MONEY_MAX_SCALE,
            allowNegative: true,
        })
        const whole = parseDecimal(total, {
            maxScale: MONEY_MAX_SCALE,
            allowNegative: true,
        })
        const scale = Math.max(part.scale, whole.scale)
        const partUnscaled =
            part.unscaled * BigInt(10) ** BigInt(scale - part.scale)
        const wholeUnscaled =
            whole.unscaled * BigInt(10) ** BigInt(scale - whole.scale)
        if (wholeUnscaled <= BigInt(0) || partUnscaled <= BigInt(0)) return 0
        if (partUnscaled >= wholeUnscaled) return 100
        return Number((partUnscaled * BigInt(100)) / wholeUnscaled)
    } catch {
        return 0
    }
}
