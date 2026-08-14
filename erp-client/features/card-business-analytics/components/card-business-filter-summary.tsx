import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

export interface CardBusinessFilterSummaryProps {
    filterSummary: string
    wechatExcludedNote: string
}

/** 当前筛选与数据时间说明（指标条/图表全量口径，明细表按筛选）。 */
export function CardBusinessFilterSummary({
    filterSummary,
    wechatExcludedNote,
}: CardBusinessFilterSummaryProps) {
    return (
        <Alert>
            <AlertTitle>当前筛选与数据时间</AlertTitle>
            <AlertDescription className="text-xs leading-relaxed">
                {filterSummary}
                <span className="mt-1 block">
                    指标条与图表为全量口径（不随客户/销售单/成本口径/履约/覆盖筛选变化）；下钻明细表已按筛选过滤并按分析视角聚合。
                </span>
                <span className="mt-1 block text-muted-foreground">
                    {wechatExcludedNote}
                </span>
            </AlertDescription>
        </Alert>
    )
}
