import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import type { SourcingProductLine } from "./purchase-order-create-model"

/** 单位精度缺失时禁止猜测；先由服务端补齐基础单位配置。 */
export function sourcingQuantityStep(
    scale?: number | null,
): string | undefined {
    if (scale == null || !Number.isInteger(scale) || scale < 0 || scale > 6)
        return undefined
    return scale === 0 ? "1" : `0.${"0".repeat(scale - 1)}1`
}

/** 数量尾随零不影响精度；整数单位拒绝半盒等不足最小单位的分配。 */
export function sourcingUnitQuantityError(
    quantity: string,
    scale?: number | null,
): string | undefined {
    if (!sourcingQuantityStep(scale))
        return "数量精度未读取，请刷新或检查计量单位配置"
    try {
        const normalized = quantity
            .trim()
            .replace(/(\.\d*?)0+$/, "$1")
            .replace(/\.$/, "")
        const parsed = parseDecimal(normalized, { maxScale: scale! })
        if (parsed.unscaled <= BigInt(0)) return "本次分配数量必须大于 0"
    } catch {
        return scale === 0
            ? "该单位按整件计量，请填写正整数"
            : `该单位的分配数量最多允许 ${scale} 位小数`
    }
    return undefined
}

/** 每条拆分至少占用一个最小计量单位；一盒整件需求只允许更换来源。 */
export function canSplitSourcingProduct(
    product: SourcingProductLine,
    allocationCount: number,
): boolean {
    const step = sourcingQuantityStep(product.quantityScale)
    if (!step || allocationCount >= product.options.length) return false
    try {
        const quantity = parseDecimal(product.remainingQuantity, {
            maxScale: 6,
        })
        const minimum = parseDecimal(step, { maxScale: 6 })
        const quantityUnits =
            quantity.unscaled * BigInt(10) ** BigInt(6 - quantity.scale)
        const minimumUnits =
            minimum.unscaled * BigInt(10) ** BigInt(6 - minimum.scale)
        return (
            quantityUnits >= minimumUnits * BigInt(allocationCount + 1) &&
            compareDecimal(product.remainingQuantity, "0", 6) > 0
        )
    } catch {
        return false
    }
}
