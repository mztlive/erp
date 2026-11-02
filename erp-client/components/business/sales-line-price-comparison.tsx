import { MoneyValue } from "./values"
import {
    salesLinePriceComparison,
    type SalesLinePrices,
} from "@/lib/sales-line-prices"

/** 采购和审批共用的含税价格对比；提醒不改变办理资格。 */
export function SalesLinePriceComparison({
    prices,
    showSalesPrice = true,
}: {
    prices: SalesLinePrices
    showSalesPrice?: boolean
}) {
    const { warnings, change } = salesLinePriceComparison(prices)
    const original = prices.original_procurement_supply_price_gross
    const known = original != null && original !== ""
    const basis =
        prices.procurement_price_basis === "BULK"
            ? "集采"
            : prices.procurement_price_basis === "DROPSHIP"
              ? "一件代发"
              : null
    return (
        <div className="min-w-48 space-y-1 whitespace-normal text-xs">
            <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
                <span className="text-muted-foreground">
                    原采购供给价{basis ? `（${basis}）` : ""}
                </span>
                {known ? (
                    <MoneyValue display="exact" value={original} />
                ) : (
                    <span className="text-muted-foreground">无法追溯</span>
                )}
            </div>
            <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
                <span className="text-muted-foreground">销售填写的供给价</span>
                <MoneyValue
                    display="exact"
                    value={prices.procurement_supply_price_gross}
                />
            </div>
            {showSalesPrice ? (
                <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
                    <span className="text-muted-foreground">含税销售价</span>
                    <MoneyValue
                        display="exact"
                        value={prices.unit_price_gross}
                    />
                </div>
            ) : null}
            {!known ? (
                <p className="text-muted-foreground">
                    缺少原价依据，无法判断是否调价。
                </p>
            ) : null}
            {warnings.map((warning, index) => (
                <p key={warning} className="text-warning-soft-foreground">
                    {warning}
                    {index === 0 && change ? (
                        <>
                            ，每单位相差{" "}
                            <MoneyValue display="exact" value={change} />
                        </>
                    ) : null}
                </p>
            ))}
        </div>
    )
}
