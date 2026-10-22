"use client"

import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    feedbackCommonTerms,
    type FeedbackTerms,
} from "../lib/feedback-editing"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import type { InquiryFeedbackLine } from "../types"

export function InquiryFeedbackCommonTerms({
    lines,
    disabled,
    onChange,
}: {
    lines: InquiryFeedbackLine[]
    disabled: boolean
    onChange: (patch: Partial<FeedbackTerms>) => void
}) {
    const common = feedbackCommonTerms(lines)
    const inactive =
        disabled || lines.every((line) => line.conclusion === "unavailable")
    return (
        <div className="flex flex-wrap items-end gap-x-6 gap-y-3 rounded-lg border bg-muted/20 p-3">
            <div className="space-y-1">
                <p className="text-sm font-medium">统一报价条件</p>
                <p className="text-xs text-muted-foreground">
                    修改后应用到全部可供及待确认商品；个别差异在行内“调整”。
                </p>
            </div>
            <div className="w-32 space-y-1.5">
                <Label htmlFor="procurement-feedback-common-tax">税率</Label>
                <Input
                    id="procurement-feedback-common-tax"
                    disabled={inactive}
                    inputMode="decimal"
                    aria-describedby="procurement-feedback-common-tax-hint"
                    value={common.tax_rate ?? ""}
                    placeholder={
                        common.tax_rate === null ? "各商品不同" : "如 0.13"
                    }
                    onChange={(event) =>
                        onChange({ tax_rate: event.target.value || null })
                    }
                />
                <p
                    id="procurement-feedback-common-tax-hint"
                    className="text-xs text-muted-foreground"
                >
                    0.13 表示 13%
                </p>
            </div>
            <div className="w-52 space-y-1.5">
                <Label htmlFor="procurement-feedback-common-valid">
                    报价有效至
                </Label>
                <DatePicker
                    id="procurement-feedback-common-valid"
                    disabled={inactive}
                    size="default"
                    value={
                        inquiryDateInput(common.valid_until ?? 0) || undefined
                    }
                    placeholder={
                        common.valid_until === null
                            ? "各商品不同"
                            : "统一选择一次"
                    }
                    onValueChange={(date) =>
                        onChange({
                            valid_until: inquiryDateSeconds(date ?? ""),
                        })
                    }
                />
                <p className="text-xs text-muted-foreground">
                    {common.valid_until === null
                        ? "已保留各商品的有效期"
                        : "按供应商确认的有效期填写"}
                </p>
            </div>
        </div>
    )
}
