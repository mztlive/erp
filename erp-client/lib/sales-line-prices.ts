import { compareDecimal, subtractFixed } from "@/lib/fixed-decimal"

/** 同一销售版本行的含税单价，缺失原价保持未知。 */
export type SalesLinePrices = {
    original_procurement_supply_price_gross?: string | null
    procurement_supply_price_gross?: string | null
    unit_price_gross?: string | null
    procurement_price_basis?: "BULK" | "DROPSHIP" | null
}

export function salesLinePriceComparison(prices: SalesLinePrices) {
    const original = prices.original_procurement_supply_price_gross
    const supply = prices.procurement_supply_price_gross
    const sales = prices.unit_price_gross
    const warnings: string[] = []
    let change: string | null = null
    try {
        if (
            original != null &&
            original !== "" &&
            supply != null &&
            supply !== ""
        ) {
            const order = compareDecimal(supply, original, 4)
            if (order !== 0) {
                change = subtractFixed(
                    order > 0 ? supply : original,
                    order > 0 ? original : supply,
                    { maxScale: 4, outputScale: 4 },
                )
                warnings.push(`销售已${order > 0 ? "调高" : "调低"}采购供给价`)
            }
        }
        if (sales != null && sales !== "") {
            if (
                original != null &&
                original !== "" &&
                compareDecimal(sales, original, 4) < 0
            ) {
                warnings.push("销售价低于原采购底价")
            }
            if (
                supply != null &&
                supply !== "" &&
                compareDecimal(sales, supply, 4) < 0
            ) {
                warnings.push("销售价低于销售填写的供给价")
            }
        }
    } catch {
        warnings.push("价格格式需核对")
    }
    return { warnings, change }
}
