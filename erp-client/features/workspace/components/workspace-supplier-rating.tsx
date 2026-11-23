import { Badge } from "@/components/ui/badge"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"

/** 评价来自最新登记档案，与提交时冻结的供应商名称分别标注。 */
export function WorkspaceSupplierRating({
    rating,
    compact = false,
}: {
    rating: ApprovalMaterials["latest_supplier_rating"]
    /** 紧凑行内徽章/细条展示，与供应商名称同行不拉高首屏。 */
    compact?: boolean
}) {
    if (!rating) {
        return compact ? (
            <Badge variant="neutral">评估暂不可用</Badge>
        ) : (
            <p className="text-xs text-muted-foreground">
                供应商评估资料暂不可用，请刷新后重试。
            </p>
        )
    }

    if (compact) {
        const ratingVariant = rating.rating_label ? "info" : "neutral"
        const scoreParts = [
            rating.current_score != null
                ? `合作中 ${rating.current_score}/100`
                : null,
            rating.initial_score != null
                ? `期初 ${rating.initial_score}/100`
                : null,
        ].filter(Boolean)

        return (
            <div
                data-slot="supplier-rating-compact"
                className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs"
            >
                <Badge variant={ratingVariant}>
                    评级：{rating.rating_label ?? "未评级"}
                </Badge>
                {scoreParts.length > 0 ? (
                    <span className="num text-muted-foreground">
                        {scoreParts.join(" · ")}
                    </span>
                ) : (
                    <span className="text-muted-foreground">未评分</span>
                )}
                {rating.valid_from ? (
                    <span className="num text-muted-foreground">
                        · 有效期 {rating.valid_from} 至{" "}
                        {rating.valid_to ?? "长期"}
                    </span>
                ) : (
                    <span className="text-muted-foreground">
                        · 尚未登记评估
                    </span>
                )}
            </div>
        )
    }

    const fields = [
        { label: "供应商评级", value: rating.rating_label ?? "未评级" },
        {
            label: "合作期初评分",
            value:
                rating.initial_score == null
                    ? "未评分"
                    : `${rating.initial_score} / 100`,
        },
        {
            label: "合作中评分",
            value:
                rating.current_score == null
                    ? "未评分"
                    : `${rating.current_score} / 100`,
        },
    ]
    return (
        <div className="space-y-2">
            <dl className="flex flex-wrap gap-x-6 gap-y-2 text-body-compact">
                {fields.map((field) => (
                    <div
                        key={field.label}
                        className="flex items-baseline gap-2"
                    >
                        <dt className="text-muted-foreground">{field.label}</dt>
                        <dd className="num font-medium">{field.value}</dd>
                    </div>
                ))}
            </dl>
            <p className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                <span>最新登记评估</span>
                {rating.valid_from ? (
                    <span>
                        有效期：{rating.valid_from} 至{" "}
                        {rating.valid_to ?? "长期"}
                    </span>
                ) : (
                    <span>尚未登记评估</span>
                )}
            </p>
        </div>
    )
}
