"use client"

import {
    ChevronDownIcon,
    PackageCheckIcon,
    TriangleAlertIcon,
} from "lucide-react"
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

/** 默认展示采购决策所需的关键商业事实，配送限制与起订量醒目提示，其余细节按需展开。 */
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
        const available =
            option.sourceAvailableQuantity ?? option.maxCreateQuantity
        return (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border border-border/60 bg-surface-sunken px-3.5 py-2 text-xs text-muted-foreground">
                <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
                    <PackageCheckIcon
                        aria-hidden="true"
                        className="size-3.5 text-info"
                    />
                    现有库存直配
                </span>
                <span aria-hidden="true" className="h-3 w-px bg-border" />
                <span>成本按库存核算</span>
                {available ? (
                    <>
                        <span
                            aria-hidden="true"
                            className="h-3 w-px bg-border"
                        />
                        <span>
                            可用库存{" "}
                            <QuantityValue
                                className="font-medium text-foreground"
                                value={available}
                                unit={product.unitName ?? ""}
                            />
                        </span>
                    </>
                ) : null}
            </div>
        )
    }

    const delivery = deliverySummary(option, product)
    const exception = delivery.startsWith("全部地址") ? "" : delivery
    const taxRatePercent = multiplyFixed(option.inputTaxRate, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })

    return (
        <div className="space-y-2">
            <div className="rounded-lg border border-border/60 bg-surface-sunken px-3.5 py-2">
                <Collapsible>
                    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5">
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                            <span className="inline-flex items-baseline gap-1.5">
                                <span>含税单价</span>
                                <MoneyValue
                                    className="text-body-compact font-semibold text-foreground"
                                    display="exact"
                                    value={option.unitCostGross}
                                />
                            </span>
                            <span
                                aria-hidden="true"
                                className="h-3 w-px bg-border"
                            />
                            <span className="inline-flex items-baseline gap-1">
                                <span>税率</span>
                                <RateValue
                                    className="font-medium text-foreground"
                                    value={taxRatePercent}
                                    precision={2}
                                />
                            </span>
                            {option.paymentTermLabel ? (
                                <>
                                    <span
                                        aria-hidden="true"
                                        className="h-3 w-px bg-border"
                                    />
                                    <span>
                                        付款{" "}
                                        <span className="font-medium text-foreground">
                                            {option.paymentTermLabel}
                                        </span>
                                    </span>
                                </>
                            ) : null}
                        </div>
                        <CollapsibleTrigger
                            id={`${idPrefix}-facts-toggle`}
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className="group px-1.5 text-muted-foreground hover:text-foreground"
                                />
                            }
                        >
                            供给详情
                            <ChevronDownIcon className="size-3.5 transition-transform duration-150 group-data-[panel-open]:rotate-180" />
                        </CollapsibleTrigger>
                    </div>
                    <CollapsibleContent>
                        <div className="mt-2 space-y-1 border-t border-border/60 pt-2 text-xs text-muted-foreground">
                            <p>
                                来源类型{" "}
                                <span className="text-foreground">
                                    {option.inquirySource
                                        ? "询价专项报价"
                                        : "通用供给"}
                                </span>
                                {option.delivery?.templateName
                                    ? ` · 配送模板 ${option.delivery.templateName}`
                                    : ""}
                                {delivery && !exception ? ` · ${delivery}` : ""}
                            </p>
                            {option.inquirySource ? (
                                <p>
                                    沿用询价第{" "}
                                    {option.inquirySource.feedback_round} 轮 ·
                                    内部报价第{" "}
                                    {option.inquirySource.quote_revision_no} 版
                                </p>
                            ) : null}
                            {option.recommendationReason ? (
                                <p>{option.recommendationReason}</p>
                            ) : null}
                        </div>
                    </CollapsibleContent>
                </Collapsible>
            </div>
            {exception || option.delivery?.bulkMinimumOrderQuantity ? (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-warning-border bg-warning-soft/75 px-3 py-1.5 text-xs text-warning-soft-foreground">
                    <TriangleAlertIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0"
                    />
                    {exception ? <span>{exception}</span> : null}
                    {option.delivery?.bulkMinimumOrderQuantity ? (
                        <span>
                            集采起订量{" "}
                            <QuantityValue
                                value={option.delivery.bulkMinimumOrderQuantity}
                                unit={product.unitName ?? ""}
                                tone="warning"
                            />
                        </span>
                    ) : null}
                </div>
            ) : null}
        </div>
    )
}
