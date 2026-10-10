/** 公司商品池与选品来源共用的单一价格筛选口径。 */
export type PriceFilterKind = "dropship" | "bulk" | "market"

export const PRICE_FILTER_LABELS: Record<PriceFilterKind, string> = {
    dropship: "一件代发价",
    bulk: "集采价",
    market: "市场价",
}

export const PRICE_FILTER_OPTIONS = Object.entries(PRICE_FILTER_LABELS).map(
    ([value, label]) => ({ value: value as PriceFilterKind, label }),
)

export function parsePriceFilterKind(
    value: string | null | undefined,
): PriceFilterKind {
    return value === "bulk" || value === "market" ? value : "dropship"
}
