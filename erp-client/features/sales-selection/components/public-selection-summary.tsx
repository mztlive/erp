import type { PublicDisplayItemView, PublicPageView } from "../types"

/** 使用快照名称和后端金额核对所选内容。 */
export const ChoiceSummary = ({ page }: { page: PublicPageView }) => {
    const choices = page.choices
    const total = page.total_amount
    return (
        <div className="space-y-3 rounded-2xl border border-border/80 bg-card p-4 shadow-xs">
            {choices.map((choice) => {
                const item = page.items.find(
                    (item) => item.item_id === choice.item_id,
                )
                return (
                    <div
                        key={choice.item_id}
                        className="border-b border-grid pb-3 last:border-b-0 last:pb-0"
                    >
                        <div className="flex items-baseline justify-between gap-2">
                            <p className="font-medium text-foreground text-sm leading-snug">
                                {item?.name ??
                                    "商品资料暂不可用，请联系销售核对"}
                                {choice.quantity != null
                                    ? ` × ${choice.quantity} 份`
                                    : ""}
                            </p>
                            {choice.line_amount != null && (
                                <p className="font-semibold text-primary text-sm shrink-0">
                                    ¥ {choice.line_amount}
                                </p>
                            )}
                        </div>
                        {item?.specification &&
                            item.specification.length > 0 && (
                                <div className="mt-1 flex flex-wrap gap-1">
                                    {item.specification.map((spec) => (
                                        <span
                                            key={spec.name}
                                            className="rounded-md bg-muted px-1.5 py-0.5 text-tiny text-muted-foreground"
                                        >
                                            {spec.name}：{spec.value}
                                        </span>
                                    ))}
                                </div>
                            )}
                        {item?.members && item.members.length > 0 && (
                            <div className="mt-1.5 space-y-0.5 pl-2 border-l-2 border-border text-xs text-muted-foreground">
                                {item.members.map((member, index) => (
                                    <p
                                        key={`${member.name}-${index}`}
                                        className="text-tiny"
                                    >
                                        {member.name} ·{" "}
                                        {member.specification
                                            .map((s) => `${s.name}：${s.value}`)
                                            .join(" / ")}{" "}
                                        · 1 {member.unit}
                                    </p>
                                ))}
                            </div>
                        )}
                    </div>
                )
            })}
            {!choices.length && (
                <p className="py-2 text-center text-xs text-muted-foreground">
                    尚未选择商品
                </p>
            )}
            {total != null && (
                <div className="flex items-center justify-between pt-2 border-t border-grid">
                    <span className="text-xs font-medium text-muted-foreground">
                        方案合计金额（含税）
                    </span>
                    <p className="font-bold text-base text-primary">
                        合计 ¥ {total}
                    </p>
                </div>
            )}
        </div>
    )
}

/** 分类推断辅助函数（在单品形态无档位时，根据名称与规格智能分类）。 */
export const inferItemCategory = (item: PublicDisplayItemView): string => {
    if (item.tier_name) return item.tier_name
    for (const spec of item.specification) {
        if (
            (spec.name === "分类" || spec.name === "品类") &&
            spec.value.trim()
        ) {
            return spec.value.trim()
        }
    }
    const name = item.name.toLowerCase()
    if (
        name.includes("茶") ||
        name.includes("普洱") ||
        name.includes("龙井") ||
        name.includes("滋补")
    )
        return "茗茶冲饮"
    if (
        name.includes("卡") ||
        name.includes("券") ||
        name.includes("密") ||
        name.includes("通")
    )
        return "礼品卡券"
    if (
        name.includes("安装") ||
        name.includes("派送") ||
        name.includes("服务") ||
        name.includes("保洁")
    )
        return "生活服务"
    if (name.includes("数码") || name.includes("家电") || name.includes("电"))
        return "数码家电"
    if (
        name.includes("生鲜") ||
        name.includes("礼包") ||
        name.includes("果") ||
        name.includes("食品") ||
        name.includes("粮油")
    )
        return "节庆生鲜"
    return "精选好物"
}
