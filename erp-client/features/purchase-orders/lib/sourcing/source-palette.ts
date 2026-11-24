import type { SourcingSupplierOption } from "./types"

export type SourceRouteCategory =
    | "stock"
    | "warehouse"
    | "direct_dropship"
    | "direct_bulk"
    | "other"

export type SourceRouteBadgeVariant =
    | "teal"
    | "orange"
    | "info"
    | "violet"
    | "neutral"

/** 按履约模式分类：现有库存 / 集采入仓 / 代发直发 / 集采直发 / 其他。 */
export function sourceRouteCategory(
    option: SourcingSupplierOption | undefined,
): SourceRouteCategory {
    if (!option) return "other"
    if (option.sourceType === "EXISTING_STOCK") return "stock"
    if (option.fulfillmentResponsibility === "WAREHOUSE") return "warehouse"
    if (option.fulfillmentResponsibility === "SUPPLIER_DIRECT") {
        return option.priceBasis === "BULK" ? "direct_bulk" : "direct_dropship"
    }
    return "other"
}

/** 4 类履约方式采用区分度高的语义标签色，便于多源拆单时一眼识别「仓/库存 vs 直发」。 */
export function sourceRouteBadgeVariant(
    option: SourcingSupplierOption | undefined,
): SourceRouteBadgeVariant {
    switch (sourceRouteCategory(option)) {
        case "stock":
            return "teal"
        case "warehouse":
            return "orange"
        case "direct_dropship":
            return "info"
        case "direct_bulk":
            return "violet"
        default:
            return "neutral"
    }
}

export type SourceAccentTokens = Readonly<{
    /** 顶部堆叠配平条与圆点实色 */
    barClass: string
    /** 源号角标浅底与深字 */
    chipClass: string
    /** 选中态细边提示色 */
    activeBorderClass: string
}>

const SOURCE_ACCENTS: readonly SourceAccentTokens[] = [
    {
        barClass: "bg-chart-1",
        chipClass: "bg-info-soft text-info-soft-foreground border-info-border",
        activeBorderClass: "border-primary/55 ring-1 ring-primary/15",
    },
    {
        barClass: "bg-chart-2",
        chipClass:
            "bg-orange-soft text-orange-soft-foreground border-orange-border",
        activeBorderClass: "border-orange-border ring-1 ring-orange-border/50",
    },
    {
        barClass: "bg-chart-4",
        chipClass: "bg-teal-soft text-teal-soft-foreground border-teal-border",
        activeBorderClass: "border-teal-border ring-1 ring-teal-border/50",
    },
    {
        barClass: "bg-chart-5",
        chipClass:
            "bg-violet-soft text-violet-soft-foreground border-violet-border",
        activeBorderClass: "border-violet-border ring-1 ring-violet-border/50",
    },
    {
        barClass: "bg-chart-3",
        chipClass: "bg-lime-soft text-lime-soft-foreground border-lime-border",
        activeBorderClass: "border-lime-border ring-1 ring-lime-border/50",
    },
]

/** 为第 N 个供给来源分配稳定的可视化主题色（在堆叠条、来源行与地址表中保持一致）。 */
export function sourceAccentForIndex(index: number): SourceAccentTokens {
    const safe = index >= 0 ? index % SOURCE_ACCENTS.length : 0
    return SOURCE_ACCENTS[safe]!
}
