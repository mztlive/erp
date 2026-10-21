"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Checkbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import { newFeedbackPrice } from "../lib/feedback-editing"
import type { InquiryFeedbackLine, InquiryLine, InquiryPrice } from "../types"

export function InquiryFeedbackPriceDetails({
    demand,
    value,
    prefix,
    unit,
    onChange,
    onInternalQuote,
}: {
    demand: InquiryLine
    value: InquiryFeedbackLine
    prefix: string
    unit: string
    onChange: (value: InquiryFeedbackLine) => void
    onInternalQuote?: () => void
}) {
    const update = (index: number, patch: Partial<InquiryPrice>) =>
        onChange({
            ...value,
            prices: value.prices.map((price, i) =>
                i === index ? { ...price, ...patch } : price,
            ),
        })
    return (
        <div id={`${prefix}-details-panel`} className="space-y-4 p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <p className="text-sm font-medium">
                    {demand.product_name} · 对销售的报价明细
                </p>
                {onInternalQuote && (
                    <Button
                        id={`${prefix}-internal-quotes`}
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onInternalQuote}
                    >
                        内部供应商报价
                    </Button>
                )}
            </div>
            <div className="space-y-2">
                <Label htmlFor={`${prefix}-public-note`}>
                    {value.conclusion === "unavailable"
                        ? "不可供原因"
                        : "对销售说明"}
                </Label>
                <Input
                    id={`${prefix}-public-note`}
                    value={value.public_note}
                    placeholder="部分供应说明缺口；不可供时填写原因"
                    onChange={(event) =>
                        onChange({ ...value, public_note: event.target.value })
                    }
                />
            </div>
            {value.conclusion !== "unavailable" &&
                value.prices.map((price, index) => {
                    const id = `${prefix}-price-${toAutomationIdSegment(price.price_id)}`
                    const text = (
                        key:
                            | "quantity"
                            | "unit_price_gross"
                            | "tax_rate"
                            | "min_quantity"
                            | "max_quantity"
                            | "included_cost_note",
                        suffix: string,
                        label: string,
                        decimal = true,
                    ) => (
                        <div className="space-y-2">
                            <Label htmlFor={`${id}-${suffix}`}>{label}</Label>
                            <Input
                                id={`${id}-${suffix}`}
                                value={price[key] ?? ""}
                                inputMode={decimal ? "decimal" : "text"}
                                onChange={(event) =>
                                    update(index, {
                                        [key]:
                                            key === "max_quantity"
                                                ? event.target.value || null
                                                : event.target.value,
                                    })
                                }
                            />
                        </div>
                    )
                    return (
                        <section
                            key={price.price_id}
                            className="space-y-4 rounded-md border bg-background p-4"
                        >
                            <div className="flex items-center justify-between gap-3">
                                <h4 className="text-sm font-medium">
                                    报价 {index + 1}
                                </h4>
                                <Button
                                    id={`${id}-remove`}
                                    size="sm"
                                    type="button"
                                    variant="ghost"
                                    onClick={() =>
                                        onChange({
                                            ...value,
                                            prices: value.prices.filter(
                                                (item) =>
                                                    item.price_id !==
                                                    price.price_id,
                                            ),
                                        })
                                    }
                                >
                                    移除报价
                                </Button>
                            </div>
                            <div className="grid gap-4 md:grid-cols-4">
                                {text(
                                    "quantity",
                                    "quantity",
                                    `适用数量（${unit}）`,
                                )}
                                {text(
                                    "unit_price_gross",
                                    "gross-price",
                                    "含税供给单价",
                                )}
                                {text(
                                    "tax_rate",
                                    "tax-rate",
                                    "税率（0.13 表示 13%）",
                                )}
                                <div className="space-y-2">
                                    <Label htmlFor={`${id}-valid-until`}>
                                        报价有效至
                                    </Label>
                                    <DatePicker
                                        id={`${id}-valid-until`}
                                        value={inquiryDateInput(
                                            price.valid_until,
                                        )}
                                        onValueChange={(date) =>
                                            update(index, {
                                                valid_until: inquiryDateSeconds(
                                                    date ?? "",
                                                ),
                                            })
                                        }
                                    />
                                </div>
                                <div className="space-y-2">
                                    <Label htmlFor={`${id}-due`}>
                                        承诺交付日
                                    </Label>
                                    <DatePicker
                                        id={`${id}-due`}
                                        value={inquiryDateInput(
                                            price.delivery_due_at,
                                        )}
                                        onValueChange={(date) =>
                                            update(index, {
                                                delivery_due_at:
                                                    inquiryDateSeconds(
                                                        date ?? "",
                                                    ),
                                            })
                                        }
                                    />
                                </div>
                                {text(
                                    "min_quantity",
                                    "minimum",
                                    "最低适用数量",
                                )}
                                {text(
                                    "max_quantity",
                                    "maximum",
                                    "最高适用数量（可不填）",
                                )}
                                {text(
                                    "included_cost_note",
                                    "included-cost",
                                    "价格已包含的费用",
                                    false,
                                )}
                            </div>
                            <div className="flex items-center gap-2">
                                <Checkbox
                                    id={`${id}-cumulative`}
                                    checked={price.allow_cumulative}
                                    onCheckedChange={(checked) =>
                                        update(index, {
                                            allow_cumulative: Boolean(checked),
                                        })
                                    }
                                />
                                <Label htmlFor={`${id}-cumulative`}>
                                    允许分批累计使用报价数量
                                </Label>
                            </div>
                            {demand.allocations.length > 0 && (
                                <div
                                    id={`${id}-allocations`}
                                    tabIndex={-1}
                                    className="space-y-2"
                                >
                                    <p className="text-sm font-medium">
                                        配送地址与可供数量
                                    </p>
                                    {demand.allocations.map((allocation) => {
                                        const assigned = price.allocations.find(
                                            (item) =>
                                                item.allocation_id ===
                                                allocation.allocation_id,
                                        )
                                        const aid = `${id}-allocation-${toAutomationIdSegment(allocation.allocation_id)}`
                                        return (
                                            <div
                                                key={allocation.allocation_id}
                                                className="flex flex-wrap items-center gap-3 rounded-md bg-muted/40 p-3"
                                            >
                                                <Checkbox
                                                    id={`${aid}-selected`}
                                                    aria-label={`配送至${allocation.recipient}`}
                                                    checked={Boolean(assigned)}
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        update(index, {
                                                            allocations: checked
                                                                ? [
                                                                      ...price.allocations,
                                                                      {
                                                                          allocation_id:
                                                                              allocation.allocation_id,
                                                                          quantity:
                                                                              allocation.quantity,
                                                                      },
                                                                  ]
                                                                : price.allocations.filter(
                                                                      (item) =>
                                                                          item.allocation_id !==
                                                                          allocation.allocation_id,
                                                                  ),
                                                        })
                                                    }
                                                />
                                                <Label
                                                    htmlFor={`${aid}-selected`}
                                                    className="min-w-0 flex-1"
                                                >
                                                    {allocation.recipient} ·{" "}
                                                    {allocation.phone}
                                                    <span className="block text-xs font-normal text-muted-foreground">
                                                        {
                                                            allocation.address
                                                        } ·
                                                        需求{" "}
                                                        {allocation.quantity}{" "}
                                                        {unit}
                                                    </span>
                                                </Label>
                                                {assigned && (
                                                    <Input
                                                        id={`${aid}-quantity`}
                                                        className="w-32"
                                                        aria-label={`${allocation.recipient}可供数量`}
                                                        inputMode="decimal"
                                                        value={
                                                            assigned.quantity
                                                        }
                                                        onChange={(event) =>
                                                            update(index, {
                                                                allocations:
                                                                    price.allocations.map(
                                                                        (
                                                                            item,
                                                                        ) =>
                                                                            item.allocation_id ===
                                                                            allocation.allocation_id
                                                                                ? {
                                                                                      ...item,
                                                                                      quantity:
                                                                                          event
                                                                                              .target
                                                                                              .value,
                                                                                  }
                                                                                : item,
                                                                    ),
                                                            })
                                                        }
                                                    />
                                                )}
                                            </div>
                                        )
                                    })}
                                </div>
                            )}
                        </section>
                    )
                })}
            {value.conclusion !== "unavailable" && (
                <Button
                    id={`${prefix}-add-price`}
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() =>
                        onChange({
                            ...value,
                            prices: [
                                ...value.prices,
                                newFeedbackPrice(
                                    demand,
                                    value.prices.length
                                        ? ""
                                        : value.available_quantity,
                                ),
                            ],
                        })
                    }
                >
                    添加不同价格或交期
                </Button>
            )}
        </div>
    )
}
