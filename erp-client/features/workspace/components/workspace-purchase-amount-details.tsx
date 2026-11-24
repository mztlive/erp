import { MoneyValue } from "@/components/business"
import type { PurchaseApprovalMetrics } from "./workspace-purchase-comparison-model"

export function PurchaseAmountDetails({
    estimated,
    difference,
    comparableActual,
}: {
    estimated: string | null
    difference: string | null
    comparableActual?: string
}) {
    const saved = difference?.startsWith("-")
    const isEqual = difference === "0.00"
    const differenceLabel = saved
        ? "低于预估"
        : difference == null || isEqual
          ? "差额"
          : "超出预估"

    return (
        <div className="space-y-0.5 text-xs font-normal text-muted-foreground">
            <div>
                预估采购 <MoneyValue display="exact" value={estimated} />
            </div>
            {comparableActual != null ? (
                <div>
                    对应实际采购{" "}
                    <MoneyValue display="exact" value={comparableActual} />
                </div>
            ) : null}
            <div
                className={
                    saved
                        ? "font-medium text-success-soft-foreground"
                        : difference != null && !isEqual
                          ? "font-medium text-warning-soft-foreground"
                          : undefined
                }
            >
                {differenceLabel}{" "}
                <MoneyValue
                    display="exact"
                    value={saved ? difference?.slice(1) : difference}
                    inheritColor
                />
            </div>
        </div>
    )
}

export function PurchaseOrderAmountDetails({
    metrics,
}: {
    metrics: PurchaseApprovalMetrics
}) {
    const { purchaseAmountComparison, goodsLineCount, feeLineCount } = metrics
    if (!purchaseAmountComparison) {
        return goodsLineCount > 0 ? (
            <div className="text-xs font-normal text-muted-foreground">
                暂无可比的预估采购金额
            </div>
        ) : null
    }

    const { estimated, actual, difference, lineCount } =
        purchaseAmountComparison
    const limitedScope = lineCount < goodsLineCount || feeLineCount > 0

    return (
        <div className="space-y-0.5">
            <PurchaseAmountDetails
                estimated={estimated}
                difference={difference}
                comparableActual={limitedScope ? actual : undefined}
            />
            {limitedScope ? (
                <div className="num text-xs font-normal text-muted-foreground">
                    已对比 {lineCount}/{goodsLineCount} 项商品
                    {feeLineCount > 0 ? "，不含费用" : null}
                </div>
            ) : null}
        </div>
    )
}
