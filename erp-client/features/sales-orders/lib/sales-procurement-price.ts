import { compareDecimal, multiplyFixed } from "@/lib/fixed-decimal"
import type {
    SalesProcurementPriceBasis,
    SalesSkuReferencePrices,
} from "../types"

/** 只按显式选择取公司商品池价格；缺价不得回退到另一档或零。 */
export function supplyReferencePrice(
    prices: SalesSkuReferencePrices | undefined,
    basis: SalesProcurementPriceBasis,
): string {
    return (
        (basis === "BULK"
            ? prices?.bulkPriceGross
            : prices?.salesVisiblePriceGross) ?? ""
    )
}

export function supplyAmount(
    price: string | undefined,
    quantity: string,
): string | null {
    if (!price?.trim() || !quantity.trim()) return null
    try {
        if (
            compareDecimal(price, "0", 4) < 0 ||
            compareDecimal(quantity, "0", 6) <= 0
        )
            return null
        return multiplyFixed(price, quantity, {
            leftMaxScale: 4,
            rightMaxScale: 6,
            outputScale: 2,
        })
    } catch {
        return null
    }
}
