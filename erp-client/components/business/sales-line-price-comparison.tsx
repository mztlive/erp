import { TrendingDownIcon, TrendingUpIcon } from "lucide-react"

import { StatusBadge } from "@/components/ui/status-badge"
import { describeMoney } from "@/lib/money-display"
import {
    salesLinePriceComparison,
    type SalesLinePrices,
} from "@/lib/sales-line-prices"
import { MoneyValue } from "./values"

/** 采购和审批共用的含税价格对比；提醒不改变办理资格。 */
export function SalesLinePriceComparison({
    prices,
    showSalesPrice = true,
    compact = false,
    unit,
}: {
    prices: SalesLinePrices
    showSalesPrice?: boolean
    /** 表头已说明价格含义时，使用主价格与原价两行展示。 */
    compact?: boolean
    unit?: string
}) {
    const { warnings, change, adjustment } = salesLinePriceComparison(prices)
    const original = prices.original_procurement_supply_price_gross
    const known = original != null && original !== ""
    const basis =
        prices.procurement_price_basis === "BULK"
            ? "集采"
            : prices.procurement_price_basis === "DROPSHIP"
              ? "一件代发"
              : null
    if (compact) {
        const hasWarningChange = change != null
        const isUnchanged = adjustment === "unchanged"
        const formattedChange =
            change != null ? (describeMoney(change)?.exact ?? change) : null

        return (
            <div className="min-w-44 space-y-1 whitespace-normal text-xs">
                <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                    <MoneyValue
                        display="exact"
                        value={prices.procurement_supply_price_gross}
                        className="text-body-compact font-semibold"
                    />
                    {hasWarningChange && formattedChange ? (
                        <StatusBadge
                            tone="warning"
                            icon={
                                adjustment === "increased"
                                    ? TrendingUpIcon
                                    : TrendingDownIcon
                            }
                            label={`${adjustment === "increased" ? "调高" : "调低"} ${formattedChange}${unit ? `/${unit}` : ""}`}
                            className="num"
                        />
                    ) : (
                        <span className="text-muted-foreground">
                            {isUnchanged ? (
                                <>{basis ? `${basis} · ` : ""}未调整</>
                            ) : !known ? (
                                <>{basis ? `${basis} · ` : ""}无历史底价记录</>
                            ) : null}
                        </span>
                    )}
                </div>
                {(!isUnchanged && known) || showSalesPrice ? (
                    <div className="flex flex-wrap items-baseline gap-x-1.5 text-muted-foreground">
                        {!isUnchanged && known ? (
                            <span>
                                原底价{" "}
                                <MoneyValue
                                    display="exact"
                                    value={original}
                                    className="font-normal [&_[data-slot=money-value-number]]:text-inherit"
                                />
                                {basis ? ` · ${basis}` : ""}
                            </span>
                        ) : null}
                        {!isUnchanged && known && showSalesPrice ? (
                            <span>·</span>
                        ) : null}
                        {showSalesPrice ? (
                            <span>
                                含税售价{" "}
                                <MoneyValue
                                    display="exact"
                                    value={prices.unit_price_gross}
                                    className="font-normal [&_[data-slot=money-value-number]]:text-inherit"
                                />
                            </span>
                        ) : null}
                    </div>
                ) : null}
                {(hasWarningChange ? warnings.slice(1) : warnings).map(
                    (warning) => (
                        <p
                            key={warning}
                            className="text-warning-soft-foreground"
                        >
                            {warning}
                        </p>
                    ),
                )}
            </div>
        )
    }
    return (
        <div className="min-w-48 space-y-1 whitespace-normal text-xs">
            <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
                <span className="text-muted-foreground">
                    原采购供货价{basis ? `（${basis}）` : ""}
                </span>
                {known ? (
                    <MoneyValue display="exact" value={original} />
                ) : (
                    <span className="text-muted-foreground">无法追溯</span>
                )}
            </div>
            <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
                <span className="text-muted-foreground">销售填写的供货价</span>
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
