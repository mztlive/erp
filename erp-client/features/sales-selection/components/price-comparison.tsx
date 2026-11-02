"use client"

import { MoneyValue, RateValue } from "@/components/business"
import { compareDecimal } from "@/lib/fixed-decimal"

export type PriceComparisonValues = {
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

/** 原价固定于选品册引用的商品修订，缺失时明确显示未知。 */
export function PriceComparison({
    original_price,
    markup_percent,
}: PriceComparisonValues) {
    return (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            <span>
                原价{" "}
                {original_price != null ? (
                    <MoneyValue value={original_price} display="exact" />
                ) : (
                    "未记录"
                )}
            </span>
            <PriceAdjustment markup_percent={markup_percent} />
        </div>
    )
}
