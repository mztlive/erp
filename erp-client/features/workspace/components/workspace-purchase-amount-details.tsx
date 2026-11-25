import { MoneyValue } from "@/components/business"
import type { PurchaseMarginTotals } from "./workspace-purchase-comparison-model"

export {
    ProfitValue,
    PurchaseProfitValue,
    type ProfitValueProps,
} from "@/components/business/task/profit-value"

export function PurchaseAmountDetails({
    supply,
    margin,
    cost,
    costLabel = "实际采购金额",
}: {
    supply: string | null
    margin: string | null
    cost?: string | null
    costLabel?: string
}) {
    return (
        <div className="space-y-0.5 text-xs font-normal text-muted-foreground">
            <div>
                采购供给金额 <MoneyValue display="exact" value={supply} />
            </div>
            {cost != null ? (
                <div>
                    {costLabel} <MoneyValue display="exact" value={cost} />
                </div>
            ) : null}
            <div
                className={
                    margin?.startsWith("-")
                        ? "font-medium text-destructive-soft-foreground"
                        : margin != null && margin !== "0.00"
                          ? "font-medium text-success-soft-foreground"
                          : undefined
                }
            >
                采购毛利（含税）{" "}
                <MoneyValue display="exact" value={margin} inheritColor />
            </div>
        </div>
    )
}

export function PurchaseOrderAmountDetails({
    totals,
    goodsLineCount,
    hasFees = false,
    showCost = false,
}: {
    totals: PurchaseMarginTotals | null
    goodsLineCount: number
    hasFees?: boolean
    showCost?: boolean
}) {
    if (!totals) {
        return goodsLineCount > 0 ? (
            <div className="text-xs font-normal text-muted-foreground">
                暂无法核算采购毛利
            </div>
        ) : null
    }

    const { supply, cost, margin, lineCount } = totals
    const limitedScope = lineCount < goodsLineCount || hasFees

    return (
        <div className="space-y-0.5">
            <PurchaseAmountDetails
                supply={supply}
                margin={margin}
                cost={limitedScope || showCost ? cost : undefined}
                costLabel={limitedScope ? "对应实际采购金额" : undefined}
            />
            {limitedScope ? (
                <div className="num text-xs font-normal text-muted-foreground">
                    已核算 {lineCount}/{goodsLineCount} 条采购明细
                    {hasFees ? "，不含费用" : null}
                </div>
            ) : null}
        </div>
    )
}
