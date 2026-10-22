"use client"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { DatePicker } from "@/components/ui/date-picker"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import type { InquiryLine, InquiryPrice } from "../types"

export function InquiryFeedbackInlinePrice({
    kind,
    prefix,
    demand,
    price,
    onChange,
}: {
    kind: "price" | "dates"
    prefix: string
    demand: InquiryLine
    price?: InquiryPrice
    onChange: (patch: Partial<InquiryPrice>) => void
}) {
    if (kind === "price")
        return (
            <div className="space-y-2">
                <Input
                    id={`${prefix}-gross-price`}
                    className="text-right tabular-nums"
                    aria-label={`${demand.product_name}含税供给单价`}
                    inputMode="decimal"
                    placeholder="填写单价"
                    value={price?.unit_price_gross ?? ""}
                    onChange={(event) =>
                        onChange({ unit_price_gross: event.target.value })
                    }
                />
                <div className="flex items-center gap-2">
                    <Label
                        className="shrink-0 text-xs text-muted-foreground"
                        htmlFor={`${prefix}-tax-rate`}
                    >
                        税率
                    </Label>
                    <Input
                        id={`${prefix}-tax-rate`}
                        className="h-8 text-right text-xs tabular-nums"
                        aria-label={`${demand.product_name}税率，0.13 表示 13%`}
                        title="0.13 表示 13%"
                        inputMode="decimal"
                        placeholder="如 0.13"
                        value={price?.tax_rate ?? ""}
                        onChange={(event) =>
                            onChange({ tax_rate: event.target.value })
                        }
                    />
                </div>
            </div>
        )
    return (
        <div className="space-y-2">
            <DatePicker
                id={`${prefix}-due`}
                value={inquiryDateInput(
                    price?.delivery_due_at ?? demand.delivery_due_at,
                )}
                onValueChange={(date) =>
                    onChange({
                        delivery_due_at: inquiryDateSeconds(date ?? ""),
                    })
                }
            />
            <div className="space-y-1">
                <Label
                    className="text-xs text-muted-foreground"
                    htmlFor={`${prefix}-valid-until`}
                >
                    报价有效至
                </Label>
                <DatePicker
                    id={`${prefix}-valid-until`}
                    size="sm"
                    placeholder="选择有效期"
                    value={
                        inquiryDateInput(price?.valid_until ?? 0) || undefined
                    }
                    onValueChange={(date) =>
                        onChange({
                            valid_until: inquiryDateSeconds(date ?? ""),
                        })
                    }
                />
            </div>
        </div>
    )
}
