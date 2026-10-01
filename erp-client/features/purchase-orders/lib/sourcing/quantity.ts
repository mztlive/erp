/** 模型六位定点运算与供给上限校验；单位精度和拆分资格由 ../sourcing-quantity.ts 负责。 */
import { compareDecimal, formatScaled, parseDecimal } from "@/lib/fixed-decimal"

/** 将最多六位小数的数量转成六位定点整数。 */
export const quantityUnits = (value: string): bigint => {
    const parsed = parseDecimal(value, { maxScale: 6 })
    return parsed.unscaled * BigInt(10) ** BigInt(6 - parsed.scale)
}

/** 将六位定点整数格式化为不带无意义尾零的数量文本。 */
export const formatQuantityUnits = (value: bigint): string => {
    const fixed = formatScaled(value, 6)
    const [integer, fraction = ""] = fixed.split(".")
    const trimmed = fraction.replace(/0+$/, "")
    return trimmed ? `${integer}.${trimmed}` : integer!
}

/** 返回多个非负定点整数中的最小值。 */
export const minimumUnits = (...values: bigint[]): bigint =>
    values.reduce((minimum, value) => (value < minimum ? value : minimum))

/**
 * 校验本次分配数量是否大于 0 且不超过该供给方案最大可分配量。
 *
 * @param quantity 用户输入数量。
 * @param maximum 该供应商最大可创建数量。
 * @returns 合法返回空；否则返回错误文案。
 */
export function sourcingQuantityError(
    quantity: string,
    maximum: string,
): string | undefined {
    try {
        const parsed = parseDecimal(quantity, { maxScale: 6 })
        if (parsed.unscaled <= BigInt(0)) {
            return "本次分配数量必须大于 0"
        }
        if (compareDecimal(quantity, maximum, 6) > 0) {
            return `本次分配数量不能超过 ${maximum}`
        }
        return undefined
    } catch {
        return "本次分配数量必须是大于 0、最多 6 位小数的数值"
    }
}
