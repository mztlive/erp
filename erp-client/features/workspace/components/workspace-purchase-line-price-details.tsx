import { MoneyValue, RateValue } from "@/components/business"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { PurchaseProfitValue } from "./workspace-purchase-amount-details"
import type { PurchaseComparison } from "./workspace-purchase-comparison-model"

export function PurchaseLinePriceDetails({
    line,
    comparison,
}: {
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
}) {
    const sales = line.source
    const originalSupplyPrice =
        sales?.sales_prices?.original_procurement_supply_price_gross ??
        sales?.original_procurement_supply_price_gross
    const inputTaxRate =
        line.input_tax_rate != null
            ? multiplyFixed(line.input_tax_rate, "100", {
                  leftMaxScale: 6,
                  rightMaxScale: 0,
                  outputScale: 2,
              })
            : null

    return (
        <div className="space-y-3 text-xs">
            <dl className="grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-4 gap-y-1 [&>dd]:text-right [&>dt]:text-muted-foreground">
                <dt>实际采购含税单价</dt>
                <dd>
                    <MoneyValue display="exact" value={line.unit_cost_gross} />
                </dd>
                {sales ? (
                    <>
                        <dt>采购供给含税单价</dt>
                        <dd>
                            <MoneyValue
                                display="exact"
                                value={sales.procurement_supply_price_gross}
                            />
                        </dd>
                        <dt>销售单含税成交单价</dt>
                        <dd>
                            <MoneyValue
                                display="exact"
                                value={sales.unit_price_gross}
                            />
                        </dd>
                    </>
                ) : null}
                {originalSupplyPrice != null ? (
                    <>
                        <dt>原采购底价</dt>
                        <dd>
                            <MoneyValue
                                display="exact"
                                value={originalSupplyPrice}
                            />
                        </dd>
                    </>
                ) : null}
                {inputTaxRate != null ? (
                    <>
                        <dt>采购进项税率</dt>
                        <dd>
                            <RateValue value={inputTaxRate} precision={2} />
                        </dd>
                    </>
                ) : null}
                {line.tax_amount != null ? (
                    <>
                        <dt>采购行税额</dt>
                        <dd>
                            <MoneyValue
                                display="exact"
                                value={line.tax_amount}
                            />
                        </dd>
                    </>
                ) : null}
            </dl>
            {sales ? (
                <dl className="space-y-2 border-t border-border/60 pt-2">
                    <div>
                        <dt className="text-muted-foreground">
                            采购毛利（含税）
                        </dt>
                        <dd className="flex flex-wrap items-baseline gap-x-1.5">
                            <span>
                                供给金额{" "}
                                <MoneyValue
                                    display="exact"
                                    value={comparison.supplyGrossAmount}
                                />
                            </span>
                            <span aria-hidden="true">−</span>
                            <span>
                                采购成本{" "}
                                <MoneyValue
                                    display="exact"
                                    value={line.gross_amount}
                                />
                            </span>
                            <span aria-hidden="true">=</span>
                            <PurchaseProfitValue
                                amount={
                                    comparison.purchaseMargin?.margin ?? null
                                }
                            />
                        </dd>
                    </div>
                    <div>
                        <dt className="text-muted-foreground">
                            公司对外毛利（含税）
                        </dt>
                        <dd className="flex flex-wrap items-baseline gap-x-1.5">
                            <span>
                                销售单成交金额{" "}
                                <MoneyValue
                                    display="exact"
                                    value={comparison.salesLineGross}
                                />
                            </span>
                            <span aria-hidden="true">−</span>
                            <span>
                                采购成本{" "}
                                <MoneyValue
                                    display="exact"
                                    value={line.gross_amount}
                                />
                            </span>
                            <span aria-hidden="true">=</span>
                            <PurchaseProfitValue
                                amount={comparison.externalGrossMargin}
                            />
                        </dd>
                    </div>
                </dl>
            ) : null}
            {comparison.salesPriceWarnings.length > 0 ? (
                <ul className="space-y-0.5 border-t border-border/60 pt-2 text-warning-soft-foreground">
                    {comparison.salesPriceWarnings.map((warning) => (
                        <li key={warning}>{warning}</li>
                    ))}
                </ul>
            ) : null}
        </div>
    )
}
