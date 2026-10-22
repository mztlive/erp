"use client"

import { ChevronDownIcon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { deliverySummary } from "../lib/sourcing/delivery-terms"
import type {
    SourcingProductLine,
    SourcingSupplierOption,
} from "../lib/sourcing/types"

/** 默认只展示采购决策所需的价格与付款条件，配送限制始终可见。 */
export function SourcingCompactFacts({
    idPrefix,
    option,
    product,
}: {
    idPrefix: string
    option?: SourcingSupplierOption
    product: SourcingProductLine
}) {
    if (!option) return null
    if (option.sourceType === "EXISTING_STOCK") {
        return <p className="text-xs text-muted-foreground">成本按库存核算</p>
    }
    const delivery = deliverySummary(option, product)
    const exception = delivery.startsWith("全部地址") ? "" : delivery
    return (
        <div className="space-y-2">
            <Collapsible>
                <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>
                            含税单价{" "}
                            <MoneyValue
                                className="text-sm font-semibold text-foreground"
                                display="exact"
                                value={option.unitCostGross}
                            />
                        </span>
                        {option.paymentTermLabel ? (
                            <span>付款 {option.paymentTermLabel}</span>
                        ) : null}
                    </div>
                    <CollapsibleTrigger
                        id={`${idPrefix}-facts-toggle`}
                        render={
                            <Button
                                type="button"
                                variant="ghost"
                                size="sm"
                                className="group h-7 px-1.5 text-xs text-muted-foreground"
                            />
                        }
                    >
                        供给详情
                        <ChevronDownIcon className="size-3.5 group-data-[panel-open]:rotate-180" />
                    </CollapsibleTrigger>
                </div>
                <CollapsibleContent>
                    <div className="mt-2 space-y-1.5 rounded-md bg-muted/40 p-3 text-xs text-muted-foreground">
                        <p>
                            税率{" "}
                            <RateValue
                                value={multiplyFixed(
                                    option.inputTaxRate,
                                    "100",
                                    {
                                        leftMaxScale: 6,
                                        rightMaxScale: 0,
                                        outputScale: 2,
                                    },
                                )}
                                precision={2}
                            />
                            {option.delivery?.templateName
                                ? ` · ${option.delivery.templateName}`
                                : ""}
                            {delivery && !exception ? ` · ${delivery}` : ""}
                        </p>
                        {option.inquirySource ? (
                            <p>
                                询价第 {option.inquirySource.feedback_round} 轮
                                · 报价第{" "}
                                {option.inquirySource.quote_revision_no} 版 ·{" "}
                                {option.inquirySource.valid_to
                                    ? `失效日期 ${option.inquirySource.valid_to}`
                                    : "未限定失效日期"}
                            </p>
                        ) : null}
                        {option.recommendationReason ? (
                            <p>{option.recommendationReason}</p>
                        ) : null}
                    </div>
                </CollapsibleContent>
            </Collapsible>
            {exception || option.delivery?.bulkMinimumOrderQuantity ? (
                <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-warning-soft-foreground">
                    {exception ? <span>{exception}</span> : null}
                    {option.delivery?.bulkMinimumOrderQuantity ? (
                        <span>
                            集采起订量{" "}
                            <QuantityValue
                                value={option.delivery.bulkMinimumOrderQuantity}
                                unit={product.unit}
                            />
                        </span>
                    ) : null}
                </div>
            ) : null}
        </div>
    )
}
