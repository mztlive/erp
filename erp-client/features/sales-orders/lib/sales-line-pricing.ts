import { compareDecimal } from "@/lib/fixed-decimal"
import type { SalesSkuReferencePrices } from "@/features/sales-orders/types"

/** 数量达到 SKU 集采起订量时取集采价，其余取一件代发价。 */
export function selectSalesReferencePrice(
    quantity: string,
    prices: SalesSkuReferencePrices | undefined,
): { unitPriceGross: string; tier: "dropship" | "bulk" } | undefined {
    if (!prices) return undefined
    try {
        if (compareDecimal(quantity, "0", 6) <= 0) return undefined
        if (
            prices.bulkPriceGross?.trim() &&
            prices.bulkMinQuantity?.trim() &&
            compareDecimal(prices.bulkMinQuantity, "0", 6) > 0 &&
            compareDecimal(quantity, prices.bulkMinQuantity, 6) >= 0
        ) {
            return { unitPriceGross: prices.bulkPriceGross, tier: "bulk" }
        }
        if (prices.salesVisiblePriceGross?.trim()) {
            return {
                unitPriceGross: prices.salesVisiblePriceGross,
                tier: "dropship",
            }
        }
    } catch {
        // 输入尚未成为有效数量时保留正在编辑的成交价。
    }
    return undefined
}
