/**
 * 供给分配与履约路径视觉色标体系（docs/task-page-design-system.md §3.3.2）：
 * - 4 类履约路径语义色（stock / warehouse / direct_dropship / direct_bulk）
 * - 5 档多源序号色（sourceAccentForIndex）
 *
 * 保持纯结构输入，不依赖任何 `@/features/*` 领域类型。
 */

export type SourceRouteInput = Readonly<{
    sourceType?: string | null
    fulfillmentResponsibility?: string | null
    priceBasis?: string | null
}>

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

export type SourceRouteStyleTokens = Readonly<{
    variant: SourceRouteBadgeVariant
    label: string
    badgeClass: string
}>

const SOURCE_ROUTE_STYLES: Readonly<
    Record<SourceRouteCategory, SourceRouteStyleTokens>
> = {
    stock: {
        variant: "teal",
        label: "现有库存",
        badgeClass: "bg-teal-soft text-teal-soft-foreground border-teal-border",
    },
    warehouse: {
        variant: "orange",
        label: "采购入仓",
        badgeClass:
            "bg-orange-soft text-orange-soft-foreground border-orange-border",
    },
    direct_dropship: {
        variant: "info",
        label: "代发直发",
        badgeClass: "bg-info-soft text-info-soft-foreground border-info-border",
    },
    direct_bulk: {
        variant: "violet",
        label: "集采直发",
        badgeClass:
            "bg-violet-soft text-violet-soft-foreground border-violet-border",
    },
    other: {
        variant: "neutral",
        label: "其他履约",
        badgeClass:
            "bg-neutral-soft text-neutral-soft-foreground border-neutral-border",
    },
}

/** 按履约模式分类：现有库存 / 采购入仓 / 代发直发 / 集采直发 / 其他。 */
export function sourceRouteCategory(
    option: SourceRouteInput | null | undefined,
): SourceRouteCategory {
    if (!option) return "other"
    if (option.sourceType === "EXISTING_STOCK") return "stock"
    if (option.fulfillmentResponsibility === "WAREHOUSE") return "warehouse"
    if (option.fulfillmentResponsibility === "SUPPLIER_DIRECT") {
        return option.priceBasis === "BULK" ? "direct_bulk" : "direct_dropship"
    }
    return "other"
}

/** 根据履约路径分类返回对应的 Badge 变体。 */
export function sourceRouteBadgeVariantForCategory(
    category: SourceRouteCategory,
): SourceRouteBadgeVariant {
    return SOURCE_ROUTE_STYLES[category].variant
}

/** 4 类履约方式采用区分度高的语义标签色，便于多源拆单时一眼识别「仓/库存 vs 直发」。 */
export function sourceRouteBadgeVariant(
    option: SourceRouteInput | null | undefined,
): SourceRouteBadgeVariant {
    return sourceRouteBadgeVariantForCategory(sourceRouteCategory(option))
}

/** 返回履约路径分类对应的完整视觉令牌（变体、默认中文标签与带边框语义类名）。 */
export function sourceRouteStyleForCategory(
    category: SourceRouteCategory,
): SourceRouteStyleTokens {
    return SOURCE_ROUTE_STYLES[category]
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
