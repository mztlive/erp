"use client"

import { MoneyValue, RateValue } from "@/components/business"
import type { SelectionReferencePrices } from "../types"
import { compareDecimal } from "@/lib/fixed-decimal"

export type PriceComparisonValues = {
    reference_prices?: SelectionReferencePrices | null
    original_price?: string | null
    markup_percent?: string | null
}

export function PriceAdjustment({ markup_percent }: PriceComparisonValues) {
    if (markup_percent == null)
        return <span className="text-muted-foreground">—</span>
    const direction = compareDecimal(markup_percent, "0", 2)
    return (
        <span className="whitespace-nowrap text-xs">
            {direction < 0 ? "降价 " : "加价 "}
            <RateValue value={markup_percent.replace(/^-/, "")} precision={2} />
        </span>
    )
}

/** 参考价固定于准备批次，旧册按原修订读取。 */
export function PriceComparison({
    reference_prices,
    original_price,
    markup_percent,
}: PriceComparisonValues) {
    return (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            {(
                [
                    [
                        "集采价",
                        reference_prices?.bulk_price_gross ?? original_price,
                    ],
                    ["一件代发价", reference_prices?.dropship_price_gross],
                    ["市场价", reference_prices?.market_price],
                ] as const
            ).map(([label, price]) => (
                <span key={label}>
                    {label}{" "}
                    {price != null ? (
                        <MoneyValue value={price} display="exact" />
                    ) : (
                        "未记录"
                    )}
                </span>
            ))}
            <PriceAdjustment markup_percent={markup_percent} />
        </div>
    )
}
