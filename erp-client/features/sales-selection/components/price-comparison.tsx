"use client"

import { MoneyValue, RateValue } from "@/components/business"
import type { SelectionReferencePrices } from "../types"
import { compareDecimal } from "@/lib/fixed-decimal"
import { ArrowDownIcon, ArrowUpIcon, MinusIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"

export type PriceComparisonValues = {
    reference_prices?: SelectionReferencePrices | null
    original_price?: string | null
    markup_percent?: string | null
}

export function PriceAdjustment({ markup_percent }: PriceComparisonValues) {
    if (markup_percent == null)
        return <span className="text-muted-foreground">—</span>
    const direction = compareDecimal(markup_percent, "0", 2)
    const Icon =
        direction > 0 ? ArrowUpIcon : direction < 0 ? ArrowDownIcon : MinusIcon
    return (
        <Badge
            variant={
                direction > 0 ? "rose" : direction < 0 ? "teal" : "neutral"
            }
        >
            <Icon data-icon="inline-start" aria-hidden="true" />
            <span className="sr-only">
                {direction > 0
                    ? "涨价 "
                    : direction < 0
                      ? "降价 "
                      : "价格持平 "}
            </span>
            <RateValue value={markup_percent.replace(/^-/, "")} precision={2} />
        </Badge>
    )
}

/** 参考价固定于准备批次，旧册按原修订读取。 */
export function PriceComparison({
    reference_prices,
    original_price,
    markup_percent,
    showAdjustment = true,
}: PriceComparisonValues & { showAdjustment?: boolean }) {
    return (
        <div className="space-y-1.5 text-xs text-muted-foreground">
            <dl className="grid gap-1">
                {(
                    [
                        [
                            "集采价",
                            reference_prices?.bulk_price_gross ??
                                original_price,
                        ],
                        ["一件代发价", reference_prices?.dropship_price_gross],
                        ["市场价", reference_prices?.market_price],
                    ] as const
                ).map(([label, price]) => (
                    <div
                        key={label}
                        className="flex items-baseline justify-between gap-3"
                    >
                        <dt>{label}</dt>
                        <dd className="whitespace-nowrap text-foreground">
                            {price != null ? (
                                <MoneyValue value={price} display="exact" />
                            ) : (
                                "未记录"
                            )}
                        </dd>
                    </div>
                ))}
            </dl>
            {showAdjustment ? (
                <PriceAdjustment markup_percent={markup_percent} />
            ) : null}
        </div>
    )
}
