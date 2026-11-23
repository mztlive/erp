import { compactFixed, sumFixed } from "@/lib/fixed-decimal"
import type { InquiryCustomerWorkbookProduct } from "@/lib/inquiry-workbook"
import type { InquiryLine } from "../types"

export * from "@/lib/inquiry-workbook"

export function withSavedDemand(
    products: readonly InquiryCustomerWorkbookProduct[],
    lines: readonly InquiryLine[],
): InquiryCustomerWorkbookProduct[] {
    const linesBySku = new Map<string, InquiryLine[]>()
    for (const line of lines) {
        const skuLines = linesBySku.get(line.sku_id)
        if (skuLines) skuLines.push(line)
        else linesBySku.set(line.sku_id, [line])
    }

    return products.map((product) => {
        const skuLines = linesBySku.get(product.sku_id) ?? []
        const quantities = skuLines.map((line) =>
            compactFixed(line.quantity.trim()),
        )
        const quantity =
            quantities.length > 0 &&
            skuLines.every((line) => line.unit_code === product.unit)
                ? compactFixed(
                      sumFixed(quantities, { maxScale: 6, outputScale: 6 }),
                  )
                : null
        // 模板每个 SKU 只有一个价格，需求行之间不一致时留空。
        const prices = skuLines.map((line) =>
            compactFixed(line.sales_reference_price_gross?.trim() ?? ""),
        )
        const price = prices[0] ?? ""

        return {
            ...product,
            quantity,
            sales_reference_price:
                price && prices.every((linePrice) => linePrice === price)
                    ? price
                    : null,
        }
    })
}
